//! A Note's Transcript in the detail view: the background job that makes one once a Recording
//! finishes, or when the user asks, one Note at a time; `transcript_tab` shows it. Nothing else
//! is transcribed on its own (ADR 0010). The job reads the audio
//! off the main thread; its result is stored through `Notes` on the main thread when it lands.
//! A remote Model gets audio only once `consent` says the user agreed (ADR 0008).

use super::super::NativeView;
use chrono::Utc;
use crumble_notes::{NoteId, Track, TranscriptSegment};
use crumble_remote::{Provider, runs_locally};
use crumble_transcribe::{ModelStatus, Segment, Transcriber};
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::{Context, Task, Window};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The Transcripts being made, for whichever Note shows.
#[derive(Default)]
pub(super) struct Transcription {
    /// Notes waiting their turn, first come first, each with the Provider to transcribe with.
    queue: VecDeque<(NoteId, Provider)>,
    running: Option<Job>,
    /// Why each Note's last attempt failed, until it is tried again.
    pub(super) failed: HashMap<NoteId, Failure>,
}

impl Transcription {
    /// Whether the Note `id` is waiting, being transcribed, or failed: its Transcript tab shows.
    pub(super) fn involves(&self, id: &NoteId) -> bool {
        self.busy(id) || self.failed.contains_key(id)
    }

    /// Whether the Note `id` is waiting or being transcribed.
    pub(super) fn busy(&self, id: &NoteId) -> bool {
        self.job(id).is_some() || self.queue.iter().any(|(queued, _)| queued == id)
    }

    /// The job running for the Note `id`, unless it was cancelled.
    pub(super) fn job(&self, id: &NoteId) -> Option<&Job> {
        self.running
            .as_ref()
            .filter(|job| &job.note == id && !job.cancel.load(Ordering::Relaxed))
    }
}

pub(super) struct Job {
    note: NoteId,
    pub(super) stage: Stage,
    /// Set once its Note no longer wants it, which stops a Local Model's worker. A blocking call
    /// into Swift or a server's answer cannot be stopped, so a cancelled job still holds the slot
    /// until it ends, and its result is dropped.
    cancel: Arc<AtomicBool>,
    /// Receives the job's updates; dropping it drops the job's result.
    _updates: Task<()>,
}

#[derive(Clone)]
pub(super) enum Stage {
    Starting,
    /// Apple's speech model for `language` is downloading.
    Downloading {
        language: String,
        fraction: f32,
    },
    Transcribing(f32),
}

pub(super) enum Failure {
    /// This Mac cannot transcribe `language`.
    Unsupported { language: String },
    /// Apple's speech model did not download.
    Download,
    /// The Model failed, saying why when it is a remote one; trying again uses `provider`.
    Transcribe {
        message: Option<String>,
        provider: Provider,
    },
    /// The Transcript `provider` made could not be stored.
    Save { provider: Provider },
}

impl Failure {
    /// The Provider that failed, which Try again uses.
    pub(super) fn provider(&self) -> Provider {
        match self {
            Failure::Unsupported { .. } | Failure::Download => Provider::Apple,
            Failure::Transcribe { provider, .. } | Failure::Save { provider } => *provider,
        }
    }

    /// What the Transcript tab says about it.
    pub(super) fn message(&self) -> String {
        match self {
            Failure::Unsupported { language } => {
                format!("Transcription isn't available for {language} on this Mac.")
            }
            Failure::Download => "Couldn't download the speech model.".to_owned(),
            Failure::Transcribe {
                message: Some(message),
                ..
            } => message.clone(),
            Failure::Transcribe { message: None, .. } => {
                "Couldn't transcribe this recording.".to_owned()
            }
            Failure::Save { .. } => "Couldn't save the Transcript.".to_owned(),
        }
    }
}

/// What the job sends back to the main thread.
enum Update {
    Stage(Stage),
    Done {
        provider: Provider,
        model: String,
        locale: String,
        result: Result<Vec<Segment>, Failure>,
    },
}

impl NativeView {
    /// How many finished Recordings have no Transcript and wait for no job, for Settings to
    /// offer to transcribe them all.
    pub(in super::super) fn untranscribed_count(&mut self) -> usize {
        self.untranscribed().len()
    }

    /// Transcribes every finished Recording without a Transcript that waits for no job, with
    /// the Transcription Model picked in Settings, asking once first when it is remote and the
    /// user has not agreed to send it audio.
    pub(in super::super) fn transcribe_untranscribed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let provider = self.transcription_provider();
        if !self.can_transcribe(provider) {
            return;
        }
        let ids = self.untranscribed();
        match self.consent_needed(provider) {
            None => ids
                .into_iter()
                .for_each(|id| self.queue_transcription(id, provider, cx)),
            Some(consent) if !ids.is_empty() => self.ask_consent(ids, consent, window, cx),
            Some(_) => {}
        }
    }

    /// The Notes whose Recording finished without a Transcript and wait for no job.
    fn untranscribed(&mut self) -> Vec<NoteId> {
        match self.notes.untranscribed() {
            Ok(ids) => ids
                .into_iter()
                .filter(|id| {
                    !self.note_detail.transcription.busy(id)
                        && !self.note_detail.unsaved_recordings.contains_key(id)
                })
                .collect(),
            Err(error) => {
                eprintln!("Could not find the Recordings to transcribe: {error:#}");
                Vec::new()
            }
        }
    }

    /// Transcribes the Note `id` with `provider`, else the Transcription Model picked in
    /// Settings. A remote Model the user has not agreed to send audio to asks first, in
    /// `window`; without one, as on quit, the Note waits for the user to ask.
    pub(super) fn transcribe(
        &mut self,
        id: NoteId,
        provider: Option<Provider>,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        // A Transcript needs the Recording's length stored; it is transcribed once that saves.
        if self.note_detail.unsaved_recordings.contains_key(&id) {
            return;
        }
        let provider = provider.unwrap_or_else(|| self.transcription_provider());
        if !self.can_transcribe(provider) {
            // Its Transcript tab points to Settings instead.
            self.note_detail.transcription.failed.remove(&id);
            cx.notify();
            return;
        }
        match (self.consent_needed(provider), window) {
            (None, _) => self.queue_transcription(id, provider, cx),
            (Some(consent), Some(window)) => self.ask_consent(vec![id], consent, window, cx),
            (Some(_), None) => {}
        }
    }

    /// Queues the Note `id` to be transcribed by `provider`, as the user asked or its
    /// Recording just stopped, unless it already waits or runs.
    pub(super) fn queue_transcription(
        &mut self,
        id: NoteId,
        provider: Provider,
        cx: &mut Context<Self>,
    ) {
        let transcription = &mut self.note_detail.transcription;
        if transcription.busy(&id) {
            return;
        }
        transcription.failed.remove(&id);
        transcription.queue.push_back((id, provider));
        self.next_transcription(cx);
        cx.notify();
    }

    /// Forgets what `provider` failed at, so its Notes can be transcribed again once its
    /// settings change.
    pub(in super::super) fn forget_transcription_failures(&mut self, provider: Provider) {
        (self.note_detail.transcription.failed).retain(|_, failure| failure.provider() != provider);
    }

    /// Forgets the Note `id`'s Transcript work, as when its Recording is deleted: a job running
    /// for it finishes unseen and its result is dropped.
    pub(super) fn cancel_transcription(&mut self, id: &NoteId) {
        let transcription = &mut self.note_detail.transcription;
        transcription.queue.retain(|(queued, _)| queued != id);
        transcription.failed.remove(id);
        if let Some(job) = transcription.running.as_mut().filter(|job| &job.note == id) {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Starts the first queued job that can run, unless one runs.
    fn next_transcription(&mut self, cx: &mut Context<Self>) {
        if self.note_detail.transcription.running.is_some() {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let Some((id, transcriber)) = self.next_transcriber(&cancel) else {
            return;
        };
        let dir = self.notes.recording_dir(&id);
        let locale = self.transcription_locale();
        let (provider, model) = (transcriber.provider(), transcriber.model().to_owned());
        let (send, updates) = async_channel::unbounded();
        let work = cx.background_executor().spawn(async move {
            let result = make_transcript(transcriber.as_ref(), &dir, &locale, &send);
            let _ = send.try_send(Update::Done {
                provider,
                model,
                locale,
                result,
            });
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
            cancel,
            _updates: updates,
        });
    }

    /// Takes the first queued Note that can run, with its Transcriber. The Note is skipped, and
    /// its Transcript tab offers to transcribe it, when its Model is remote and the user has not
    /// agreed to send audio to it, as when its Model or host changed since they did, or when the
    /// server was local as it was queued.
    fn next_transcriber(
        &mut self,
        cancel: &Arc<AtomicBool>,
    ) -> Option<(NoteId, Box<dyn Transcriber + Send>)> {
        while let Some((id, provider)) = self.note_detail.transcription.queue.pop_front() {
            let local = runs_locally(provider, self.store.openai_base_url());
            if !local && self.consent_needed(provider).is_some() {
                continue;
            }
            match self.transcriber(provider, cancel) {
                Ok(transcriber) => return Some((id, transcriber)),
                Err(message) => {
                    let failure = Failure::Transcribe {
                        message: Some(message),
                        provider,
                    };
                    self.note_detail.transcription.failed.insert(id, failure);
                }
            }
        }
        None
    }

    fn transcription_update(&mut self, id: &NoteId, update: Update, cx: &mut Context<Self>) {
        let transcription = &mut self.note_detail.transcription;
        let Some(job) = transcription.running.as_mut().filter(|job| &job.note == id) else {
            return;
        };
        match update {
            Update::Stage(stage) => job.stage = stage,
            Update::Done {
                provider,
                model,
                locale,
                result,
            } => {
                let cancelled = job.cancel.load(Ordering::Relaxed);
                transcription.running = None;
                match result {
                    _ if cancelled => {}
                    Ok(segments) => {
                        self.store_transcript(id, (provider, &model, &locale), &segments, cx);
                    }
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

    /// Keeps `segments` as the Note `id`'s Transcript, made by the Provider, Model and locale in
    /// `made`, shows it if the Note shows, and queues its Summary in place of the one the old
    /// Transcript had.
    fn store_transcript(
        &mut self,
        id: &NoteId,
        made: (Provider, &str, &str),
        segments: &[Segment],
        cx: &mut Context<Self>,
    ) {
        let (provider, model, locale) = made;
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
                .set_transcript(id, provider.id(), model, locale, &segments, Utc::now());
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
            Err(error) => {
                eprintln!("Could not keep the Transcript: {error:#}");
                let failure = Failure::Save { provider };
                (self.note_detail.transcription.failed).insert(id.clone(), failure);
            }
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

    /// Asks before transcribing the Note `id` again with `provider` when it has a Transcript,
    /// as the new one replaces it; then asks for consent when `provider` needs it.
    pub(super) fn confirm_transcribe_again(
        &mut self,
        id: NoteId,
        provider: Provider,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let replaces = self
            .note_detail
            .note
            .iter()
            .any(|note| note.id == id && note.transcript.is_some());
        if !replaces {
            self.transcribe(id, Some(provider), Some(window), cx);
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
                .on_ok(move |_, window, cx| {
                    // After this dialog closes, so a consent dialog can open in its place.
                    let (view, id) = (view.clone(), id.clone());
                    window.defer(cx, move |window, cx| {
                        let _ = view.update(cx, |view, cx| {
                            view.transcribe(id, Some(provider), Some(window), cx);
                        });
                    });
                    true
                })
        });
    }
}

/// Makes the Transcript of the Recording in `dir` with `transcriber`, getting Apple's model for
/// `locale` first if this Mac lacks it, and sends each stage to `send`. Blocks, on a background
/// thread.
fn make_transcript(
    transcriber: &dyn Transcriber,
    dir: &Path,
    locale: &str,
    send: &async_channel::Sender<Update>,
) -> Result<Vec<Segment>, Failure> {
    let stage = |stage| {
        let _ = send.try_send(Update::Stage(stage));
    };
    let provider = transcriber.provider();
    if provider == Provider::Apple {
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
                .map_err(|error| {
                    eprintln!("Could not download the speech model: {error:#}");
                    Failure::Download
                })?;
            }
        }
    }
    stage(Stage::Transcribing(0.0));
    transcriber
        .transcribe(dir, locale, &mut |fraction| {
            stage(Stage::Transcribing(fraction));
        })
        .map_err(|error| {
            eprintln!("Could not transcribe the Recording: {error:#}");
            Failure::Transcribe {
                // Apple's errors are for developers; a remote Model's say what to fix.
                message: (provider != Provider::Apple).then(|| error.to_string()),
                provider,
            }
        })
}
