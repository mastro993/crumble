//! A Note's Transcript in the detail view: the background job that makes one once a Recording
//! finishes, one Note at a time, and the Transcript tab that shows it. The job reads the audio
//! off the main thread; its result is stored through `Notes` on the main thread when it lands.

use super::super::NativeView;
use super::super::controls::button;
use super::recording::clock;
use chrono::Utc;
use crumble_notes::{NoteId, Track, TranscriptSegment};
use crumble_transcribe::{ModelStatus, Segment};
use gpui_kit::component::button::{ButtonVariant, ButtonVariants};
use gpui_kit::component::progress::Progress;
use gpui_kit::component::{ActiveTheme, Sizable, WindowExt, h_flex, v_flex};
use gpui_kit::{AnyElement, Context, IntoElement, ParentElement, Styled, Task, Window, div, px};
use std::collections::{HashMap, VecDeque};
use std::path::Path;

/// The width of the time column, so the columns after it line up whatever the time's digits.
const TIME_WIDTH: f32 = 44.0;
/// The width of the "You" and "Others" column.
const SPEAKER_WIDTH: f32 = 52.0;

/// The Transcripts being made, for whichever Note shows.
#[derive(Default)]
pub(super) struct Transcription {
    /// Notes waiting their turn, first come first.
    queue: VecDeque<NoteId>,
    running: Option<Job>,
    /// Why each Note's last attempt failed, until it is tried again.
    failed: HashMap<NoteId, Failure>,
}

impl Transcription {
    /// Whether the Note `id` is waiting, being transcribed, or failed: its Transcript tab shows.
    pub(super) fn involves(&self, id: &NoteId) -> bool {
        self.busy(id) || self.failed.contains_key(id)
    }

    /// Whether the Note `id` is waiting or being transcribed.
    pub(super) fn busy(&self, id: &NoteId) -> bool {
        self.job(id).is_some() || self.queue.contains(id)
    }

    /// The job running for the Note `id`, unless it was cancelled.
    fn job(&self, id: &NoteId) -> Option<&Job> {
        self.running
            .as_ref()
            .filter(|job| &job.note == id && !job.cancelled)
    }
}

struct Job {
    note: NoteId,
    stage: Stage,
    /// Whether its Note no longer wants it. A blocking call into Swift cannot be stopped, so a
    /// cancelled job still holds the slot until it ends, and its result is dropped.
    cancelled: bool,
    /// Receives the job's updates; dropping it drops the job's result.
    _updates: Task<()>,
}

#[derive(Clone)]
enum Stage {
    Starting,
    /// Apple's speech model for `language` is downloading.
    Downloading {
        language: String,
        fraction: f32,
    },
    Transcribing(f32),
}

enum Failure {
    /// This Mac cannot transcribe `language`.
    Unsupported {
        language: String,
    },
    /// Apple's speech model did not download.
    Download,
    Transcribe,
}

/// What the job sends back to the main thread.
enum Update {
    Stage(Stage),
    Done {
        locale: String,
        result: Result<Vec<Segment>, Failure>,
    },
}

impl NativeView {
    /// Queues the Notes whose Recording finished without a Transcript, as when the app quit while
    /// transcribing. Run once at launch.
    pub(in super::super) fn transcribe_untranscribed(&mut self, cx: &mut Context<Self>) {
        match self.notes.untranscribed() {
            Ok(ids) => ids.into_iter().for_each(|id| self.transcribe(id, cx)),
            Err(error) => eprintln!("Could not find the Recordings to transcribe: {error:#}"),
        }
    }

    /// Queues the Note `id` to be transcribed, unless it already waits or runs.
    pub(super) fn transcribe(&mut self, id: NoteId, cx: &mut Context<Self>) {
        let transcription = &mut self.note_detail.transcription;
        if transcription.busy(&id) {
            return;
        }
        transcription.failed.remove(&id);
        transcription.queue.push_back(id);
        self.next_transcription(cx);
        cx.notify();
    }

    /// Forgets the Note `id`'s Transcript work, as when its Recording is deleted: a job running
    /// for it finishes unseen and its result is dropped.
    pub(super) fn cancel_transcription(&mut self, id: &NoteId) {
        let transcription = &mut self.note_detail.transcription;
        transcription.queue.retain(|queued| queued != id);
        transcription.failed.remove(id);
        if let Some(job) = transcription.running.as_mut().filter(|job| &job.note == id) {
            job.cancelled = true;
        }
    }

    /// Starts the first queued job unless one runs.
    fn next_transcription(&mut self, cx: &mut Context<Self>) {
        let transcription = &mut self.note_detail.transcription;
        if transcription.running.is_some() {
            return;
        }
        let Some(id) = transcription.queue.pop_front() else {
            return;
        };
        let dir = self.notes.recording_dir(&id);
        let (send, updates) = async_channel::unbounded();
        let work = cx.background_executor().spawn(async move {
            let locale = crumble_transcribe::current_locale();
            let result = make_transcript(&dir, &locale, &send);
            let _ = send.try_send(Update::Done { locale, result });
        });
        let note = id.clone();
        let updates = cx.spawn(async move |view, cx| {
            let _work = work;
            while let Ok(update) = updates.recv().await {
                if view
                    .update(cx, |view, cx| view.transcription_update(&note, update, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        self.note_detail.transcription.running = Some(Job {
            note: id,
            stage: Stage::Starting,
            cancelled: false,
            _updates: updates,
        });
    }

    fn transcription_update(&mut self, id: &NoteId, update: Update, cx: &mut Context<Self>) {
        let transcription = &mut self.note_detail.transcription;
        let Some(job) = transcription.running.as_mut().filter(|job| &job.note == id) else {
            return;
        };
        match update {
            Update::Stage(stage) => job.stage = stage,
            Update::Done { locale, result } => {
                let cancelled = job.cancelled;
                transcription.running = None;
                match result {
                    _ if cancelled => {}
                    Ok(segments) => self.store_transcript(id, &locale, &segments, cx),
                    Err(failure) => {
                        self.note_detail
                            .transcription
                            .failed
                            .insert(id.clone(), failure);
                    }
                }
                self.next_transcription(cx);
            }
        }
        cx.notify();
    }

    /// Keeps `segments` as the Note `id`'s Transcript, shows it if the Note shows, and queues
    /// its Summary in place of the one the old Transcript had.
    fn store_transcript(
        &mut self,
        id: &NoteId,
        locale: &str,
        segments: &[Segment],
        cx: &mut Context<Self>,
    ) {
        let segments: Vec<TranscriptSegment> = segments
            .iter()
            .map(|segment| TranscriptSegment {
                track: match segment.track {
                    crumble_transcribe::Track::Mic => Track::Mic,
                    crumble_transcribe::Track::System => Track::System,
                },
                start: segment.start,
                end: segment.end,
                text: segment.text.clone(),
            })
            .collect();
        let stored =
            self.notes
                .set_transcript(id, crumble_transcribe::MODEL, locale, &segments, Utc::now());
        match stored {
            Ok(info) => {
                self.note_changed(id, |note| {
                    note.transcript = Some(info.clone());
                    note.summary = None;
                });
                if self
                    .note_detail
                    .note
                    .as_ref()
                    .is_some_and(|note| &note.id == id)
                {
                    self.note_detail.transcript = segments;
                }
                self.cancel_summary(id);
                self.summarize(id.clone(), None, cx);
            }
            // As when the Recording or its Note went while the job ran.
            Err(error) => eprintln!("Could not keep the Transcript: {error:#}"),
        }
    }

    /// Reads the shown Note's Transcript into the view.
    pub(super) fn show_transcript(&mut self) {
        let detail = &mut self.note_detail;
        detail.transcript = match detail.note.as_ref() {
            Some(note) if note.transcript.is_some() => {
                self.notes.transcript(&note.id).unwrap_or_else(|error| {
                    eprintln!("Could not read the Transcript: {error:#}");
                    Vec::new()
                })
            }
            _ => Vec::new(),
        };
    }

    /// Asks before transcribing the Note `id` again when it has a Transcript, as the new one
    /// replaces it.
    pub(super) fn confirm_transcribe_again(
        &mut self,
        id: NoteId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let replaces = self
            .note_detail
            .note
            .iter()
            .any(|note| note.id == id && note.transcript.is_some());
        if !replaces {
            self.transcribe(id, cx);
            return;
        }
        let view = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (view, id) = (view.clone(), id.clone());
            alert
                .title("Transcribe this recording again?")
                .description("The new Transcript replaces the current one.")
                .show_cancel(true)
                .ok_text("Transcribe")
                .ok_variant(ButtonVariant::Primary)
                .on_ok(move |_, _, cx| {
                    let _ = view.update(cx, |view, cx| view.transcribe(id.clone(), cx));
                    true
                })
        });
    }

    /// The Transcript tab: the job's progress, why it failed, or the Transcript, each line with
    /// its time, which seeks the player, and who spoke.
    pub(super) fn render_transcript(&self, cx: &mut Context<Self>) -> AnyElement {
        let detail = &self.note_detail;
        let Some(note) = detail.note.as_ref() else {
            return div().into_any_element();
        };
        let muted = cx.theme().muted_foreground;
        let transcription = &detail.transcription;
        if let Some(job) = transcription.job(&note.id) {
            let (text, fraction) = match &job.stage {
                Stage::Starting => ("Transcribing…".to_owned(), None),
                Stage::Downloading { language, fraction } => (
                    format!(
                        "Downloading the speech model for {language}… {}",
                        percent(*fraction)
                    ),
                    Some(*fraction),
                ),
                Stage::Transcribing(fraction) => (
                    format!("Transcribing… {}", percent(*fraction)),
                    Some(*fraction),
                ),
            };
            return v_flex()
                .gap_2()
                .text_sm()
                .text_color(muted)
                .child(text)
                .child(
                    Progress::new("transcript-progress")
                        .small()
                        .loading(fraction.is_none())
                        .value(fraction.unwrap_or(0.0) * 100.0),
                )
                .into_any_element();
        }
        if transcription.queue.contains(&note.id) {
            return div()
                .text_sm()
                .text_color(muted)
                .child("Waiting to transcribe…")
                .into_any_element();
        }
        if let Some(failure) = transcription.failed.get(&note.id) {
            let id = note.id.clone();
            return match failure {
                Failure::Unsupported { language } => div()
                    .text_sm()
                    .text_color(muted)
                    .child(format!(
                        "Transcription isn't available for {language} on this Mac."
                    ))
                    .into_any_element(),
                Failure::Download | Failure::Transcribe => h_flex()
                    .gap_2()
                    .text_sm()
                    .text_color(muted)
                    .child(div().flex_1().min_w_0().child(
                        if matches!(failure, Failure::Download) {
                            "Couldn't download the speech model."
                        } else {
                            "Couldn't transcribe this recording."
                        },
                    ))
                    .child(
                        button("transcript-retry")
                            .outline()
                            .small()
                            .label("Try again")
                            .on_click(
                                cx.listener(move |view, _, _, cx| view.transcribe(id.clone(), cx)),
                            ),
                    )
                    .into_any_element(),
            };
        }
        let lines = detail
            .transcript
            .iter()
            .enumerate()
            .map(|(index, segment)| {
                let start = segment.start;
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(
                        div().w(px(TIME_WIDTH)).flex_shrink_0().child(
                            button(("transcript-time", index))
                                .link()
                                .xsmall()
                                .label(clock(start))
                                .accessibility_label(format!("Play from {}", clock(start)))
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    view.seek(start, window, cx)
                                })),
                        ),
                    )
                    .child(
                        div()
                            .w(px(SPEAKER_WIDTH))
                            .flex_shrink_0()
                            .text_color(muted)
                            .child(match segment.track {
                                Track::Mic => "You",
                                Track::System => "Others",
                            }),
                    )
                    .child(div().flex_1().min_w_0().child(segment.text.clone()))
                    .into_any_element()
            });
        // ponytail: every line renders at once; a list that renders only what shows, once
        // hour-long Transcripts feel slow.
        v_flex()
            .gap_2()
            .text_sm()
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("Transcribed by Apple, on this Mac"),
            )
            .children(
                detail
                    .transcript
                    .is_empty()
                    .then(|| div().text_color(muted).child("No speech was heard.")),
            )
            .children(lines)
            .into_any_element()
    }
}

/// `42%`.
pub(super) fn percent(fraction: f32) -> String {
    format!("{:.0}%", (fraction * 100.0).clamp(0.0, 100.0))
}

/// Makes the Transcript of the Recording in `dir`, getting `locale`'s model first if this Mac
/// lacks it, and sends each stage to `send`. Blocks, on a background thread.
fn make_transcript(
    dir: &Path,
    locale: &str,
    send: &async_channel::Sender<Update>,
) -> Result<Vec<Segment>, Failure> {
    let stage = |stage| {
        let _ = send.try_send(Update::Stage(stage));
    };
    let failed = |failure: Failure| {
        move |error: anyhow::Error| {
            eprintln!("Could not transcribe the Recording: {error:#}");
            failure
        }
    };
    match crumble_transcribe::model_status(locale) {
        ModelStatus::Installed => {}
        ModelStatus::Unsupported => {
            return Err(Failure::Unsupported {
                language: crumble_transcribe::language_name(locale),
            });
        }
        ModelStatus::Downloadable | ModelStatus::Downloading => {
            let language = crumble_transcribe::language_name(locale);
            crumble_transcribe::install_model(locale, |fraction| {
                stage(Stage::Downloading {
                    language: language.clone(),
                    fraction,
                });
            })
            .map_err(failed(Failure::Download))?;
        }
    }
    stage(Stage::Transcribing(0.0));
    crumble_transcribe::transcribe(dir, locale, |fraction| {
        stage(Stage::Transcribing(fraction));
    })
    .map_err(failed(Failure::Transcribe))
}

#[cfg(test)]
mod tests {
    use super::percent;

    #[test]
    fn percent_rounds_and_stays_in_range() {
        assert_eq!(percent(0.0), "0%");
        assert_eq!(percent(0.424), "42%");
        assert_eq!(percent(1.2), "100%");
    }
}
