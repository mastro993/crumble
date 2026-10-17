//! A Note's Recording in the detail view: Record and Stop on the card header's trailing side,
//! hints under the card when a track stays silent, the player once it is recorded, Delete
//! recording, and the header pill that leads back to the Note being recorded. One Recording
//! runs at a time in the whole app, and it keeps running while other pages show.

use super::super::controls::{Inactive, app_icon, button};
use super::super::{NativeView, Route};
use super::{Subject, event_title};
use crate::native_view::agenda::Selected;
use chrono::{DateTime, Utc};
use crumble_audio::{Access, Player, Recorder};
use crumble_notes::{NoteId, RecordingInfo};
use gpui_kit::component::button::{ButtonVariant, ButtonVariants};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState, SliderValue};
use gpui_kit::component::{ActiveTheme, Sizable, WindowExt, h_flex};
use gpui_kit::{
    AnyElement, AppContext, Context, Entity, IntoElement, ParentElement, Styled, Subscription,
    Task, Window, div, px,
};
use std::time::Duration;

/// How long a track may stay silent before the hint shows.
const SILENCE_HINT_AFTER: Duration = Duration::from_secs(10);
/// How often the player's position redraws while it plays.
const PLAYBACK_TICK: Duration = Duration::from_millis(200);
/// The scrubber's width.
const SCRUBBER_WIDTH: f32 = 140.0;
/// System Settings › Privacy & Security › Screen & System Audio Recording.
const SYSTEM_AUDIO_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";
/// System Settings › Privacy & Security › Microphone.
const MICROPHONE_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";

/// The Note to record, kept from the click until recording starts, as macOS may ask for the
/// microphone first.
pub(super) struct Target {
    note: NoteId,
    /// The Note's title, or its Event's, for the tooltip of every other Record button.
    name: String,
    /// Where the Note shows, so the header pill can open it.
    subject: Subject,
}

/// The Recording running now.
pub(super) struct Active {
    target: Target,
    started_at: DateTime<Utc>,
    recorder: Recorder,
    /// Redraws the elapsed time each second.
    _tick: Task<()>,
}

/// The shown Note's finished Recording, ready to play.
pub(super) struct Playback {
    note: NoteId,
    player: Player,
    scrubber: Entity<SliderState>,
    _seek: Subscription,
    /// Moves the scrubber while playing.
    tick: Task<()>,
}

impl NativeView {
    /// Records the shown Note, asking for the microphone first if macOS has not yet. A denied
    /// microphone still records the Mac's sound; its silence brings up the hint.
    pub(super) fn record(&mut self, cx: &mut Context<Self>) {
        let detail = &self.note_detail;
        let (Some(note), Some(subject)) = (detail.note.as_ref(), detail.subject.clone()) else {
            return;
        };
        if detail.recording.is_some() || note.recording.is_some() {
            return;
        }
        let name = match (&note.title, &subject) {
            (Some(title), _) if !title.trim().is_empty() => title.clone(),
            (_, Subject::Event(event)) => event_title(event),
            (_, Subject::QuickNote(_)) => "Quick note".to_owned(),
        };
        let target = Target {
            note: note.id.clone(),
            name,
            subject,
        };
        if crumble_audio::mic_access() != Access::NotDetermined {
            self.begin_recording(target, cx);
            return;
        }
        let (answered, answer) = async_channel::bounded(1);
        crumble_audio::request_mic_access(move |granted| {
            let _ = answered.try_send(granted);
        });
        cx.spawn(async move |view, cx| {
            let _ = answer.recv().await;
            let _ = view.update(cx, |view, cx| view.begin_recording(target, cx));
        })
        .detach();
    }

    fn begin_recording(&mut self, target: Target, cx: &mut Context<Self>) {
        if self.note_detail.recording.is_some() {
            return;
        }
        let id = target.note.clone();
        let info = match self.notes.set_recording_started(&id, Utc::now()) {
            Ok(info) => info,
            Err(error) => return self.recording_failed(&id, &format!("{error:#}"), cx),
        };
        let recorder = match Recorder::start(&self.notes.recording_dir(&id)) {
            Ok(recorder) => recorder,
            Err(error) => {
                if let Err(error) = self.notes.delete_recording(&id, Utc::now()) {
                    eprintln!("Could not forget the Recording that did not start: {error:#}");
                }
                return self.recording_failed(&id, &format!("{error:#}"), cx);
            }
        };
        let tick = cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if view.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        self.note_detail.recording = Some(Active {
            target,
            started_at: info.started_at,
            recorder,
            _tick: tick,
        });
        self.note_detail.recording_failed = None;
        self.recording_changed(&id, Some(info));
        cx.notify();
    }

    fn recording_failed(&mut self, id: &NoteId, error: &str, cx: &mut Context<Self>) {
        eprintln!("Could not record: {error}");
        self.note_detail.recording_failed = Some(id.clone());
        cx.notify();
    }

    /// Whether "Couldn't record" shows: the shown Note's Recording last failed.
    pub(super) fn shown_recording_failed(&self) -> bool {
        let detail = &self.note_detail;
        detail
            .note
            .as_ref()
            .is_some_and(|note| detail.recording_failed.as_ref() == Some(&note.id))
    }

    /// Stops the Recording and keeps its length; on quit and window close too. A stop that
    /// fails keeps what the files hold, and a Recording with no audio is deleted.
    pub(super) fn stop_recording(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.note_detail.recording.take() else {
            return;
        };
        let id = active.target.note;
        let dir = self.notes.recording_dir(&id);
        let stopped = active.recorder.stop();
        let clean = stopped.is_ok();
        let length = stopped.or_else(|error| {
            eprintln!("The Recording did not stop cleanly: {error:#}");
            crumble_audio::recorded_length(&dir)
        });
        let now = Utc::now();
        let info = match length {
            Ok(duration) => self
                .notes
                .set_recording_finished(&id, duration, now)
                .map(|()| {
                    Some(RecordingInfo {
                        started_at: active.started_at,
                        duration: Some(duration),
                    })
                }),
            Err(error) => {
                eprintln!("Deleting the Recording, as none of it is readable: {error:#}");
                self.notes.delete_recording(&id, now).map(|()| None)
            }
        };
        match info {
            // What was kept stays, but part of it may be missing.
            Ok(info) if !clean => {
                self.recording_changed(&id, info);
                self.recording_failed(&id, "it did not stop cleanly", cx);
            }
            Ok(info) => {
                self.recording_changed(&id, info);
                self.note_detail.recording_failed = None;
            }
            Err(error) => self.recording_failed(&id, &format!("{error:#}"), cx),
        }
        self.show_player(cx);
        cx.notify();
    }

    /// Keeps every in-memory copy of the Note `id` in step with its Recording.
    fn recording_changed(&mut self, id: &NoteId, info: Option<RecordingInfo>) {
        let detail = &mut self.note_detail;
        if let Some(note) = &mut detail.note
            && &note.id == id
        {
            note.recording = info;
        }
        if let Some(Subject::QuickNote(note)) = &mut detail.subject
            && &note.id == id
        {
            note.recording = info;
        }
        if let Some(mut note) = self.agenda.quick_note(id).cloned() {
            note.recording = info;
            self.agenda.note_changed(&note);
        }
    }

    /// Opens the shown Note's finished Recording in the player, replacing any other.
    pub(super) fn show_player(&mut self, cx: &mut Context<Self>) {
        let detail = &mut self.note_detail;
        detail.playback = None;
        let Some(note) = detail.note.as_ref() else {
            return;
        };
        if note
            .recording
            .is_none_or(|recording| recording.duration.is_none())
        {
            return;
        }
        let player = match Player::open(&self.notes.recording_dir(&note.id)) {
            Ok(player) => player,
            Err(error) => {
                eprintln!("Could not open the Recording: {error:#}");
                return;
            }
        };
        let length = player.duration().as_secs_f32().max(0.1);
        let scrubber = cx.new(|_| SliderState::new().min(0.0).max(length).step(0.1));
        let seek = cx.subscribe(&scrubber, |view, _, event: &SliderEvent, cx| {
            let (SliderEvent::Change(SliderValue::Single(to))
            | SliderEvent::Release(SliderValue::Single(to))) = *event
            else {
                return;
            };
            if let Some(playback) = &mut view.note_detail.playback {
                playback.player.seek(Duration::from_secs_f32(to.max(0.0)));
                cx.notify();
            }
        });
        self.note_detail.playback = Some(Playback {
            note: note.id.clone(),
            player,
            scrubber,
            _seek: seek,
            tick: Task::ready(()),
        });
    }

    fn toggle_playback(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(playback) = &mut self.note_detail.playback else {
            return;
        };
        if playback.player.is_playing() {
            playback.player.pause();
            playback.tick = Task::ready(());
            cx.notify();
            return;
        }
        playback.player.play();
        playback.tick = cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor().timer(PLAYBACK_TICK).await;
                let playing = view.update_in(cx, |view, window, cx| {
                    let Some(playback) = &view.note_detail.playback else {
                        return false;
                    };
                    let at = playback.player.position().as_secs_f32();
                    playback
                        .scrubber
                        .update(cx, |scrubber, cx| scrubber.set_value(at, window, cx));
                    cx.notify();
                    playback.player.is_playing()
                });
                if !matches!(playing, Ok(true)) {
                    break;
                }
            }
        });
        cx.notify();
    }

    /// Asks before deleting the Note `id`'s Recording, as nothing brings it back.
    pub(super) fn confirm_delete_recording(
        &mut self,
        id: NoteId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (view, id) = (view.clone(), id.clone());
            alert
                .title("Delete this recording?")
                .description("Its audio is deleted from this Mac. This can't be undone.")
                .show_cancel(true)
                .ok_text("Delete")
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, _, cx| {
                    let _ = view.update(cx, |view, cx| view.delete_recording(&id, cx));
                    true
                })
        });
    }

    fn delete_recording(&mut self, id: &NoteId, cx: &mut Context<Self>) {
        if self.is_recording(id) {
            return;
        }
        // Closes the files before they go.
        self.note_detail.playback = None;
        match self.notes.delete_recording(id, Utc::now()) {
            Ok(()) => {
                self.recording_changed(id, None);
                self.note_detail.recording_failed = None;
            }
            Err(error) => self.recording_failed(id, &format!("{error:#}"), cx),
        }
        cx.notify();
    }

    /// Whether the Note `id` is being recorded now.
    pub(super) fn is_recording(&self, id: &NoteId) -> bool {
        self.note_detail
            .recording
            .as_ref()
            .is_some_and(|active| &active.target.note == id)
    }

    /// Goes to the page of the Note being recorded and shows it.
    fn open_recording(&mut self, cx: &mut Context<Self>) {
        let Some(active) = &self.note_detail.recording else {
            return;
        };
        match active.target.subject.clone() {
            Subject::QuickNote(note) => {
                self.agenda.selected = Some(Selected::QuickNote(note.id));
                self.navigate(Route::Agenda, cx);
            }
            Subject::Event(event) => self.navigate(Route::Event(event), cx),
        }
    }

    /// The card header's trailing side: Record, the running Recording with Stop, or the player.
    pub(super) fn render_recording_controls(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let detail = &self.note_detail;
        let note = detail.note.as_ref()?;
        let theme = cx.theme();
        if let Some(active) = detail
            .recording
            .as_ref()
            .filter(|active| active.target.note == note.id)
        {
            let elapsed = active.recorder.status().elapsed;
            return Some(
                h_flex()
                    .gap_2()
                    .text_sm()
                    .child(red_dot(cx))
                    .child(
                        div()
                            .text_color(theme.muted_foreground)
                            .child(clock(elapsed)),
                    )
                    .child(
                        button("recording-stop")
                            .ghost()
                            .small()
                            .icon(app_icon("stop").size_4())
                            .label("Stop")
                            .on_click(cx.listener(|view, _, _, cx| view.stop_recording(cx))),
                    )
                    .into_any_element(),
            );
        }
        if let Some(playback) = detail
            .playback
            .as_ref()
            .filter(|playback| playback.note == note.id)
        {
            let (playing, position, length) = (
                playback.player.is_playing(),
                playback.player.position(),
                playback.player.duration(),
            );
            return Some(
                h_flex()
                    .gap_2()
                    .text_sm()
                    .child(
                        button("recording-play")
                            .ghost()
                            .small()
                            .icon(app_icon(if playing { "pause" } else { "play" }).size_4())
                            .accessibility_label(if playing { "Pause" } else { "Play" })
                            .on_click(
                                cx.listener(|view, _, window, cx| view.toggle_playback(window, cx)),
                            ),
                    )
                    .child(
                        div()
                            .w(px(SCRUBBER_WIDTH))
                            .child(Slider::new(&playback.scrubber)),
                    )
                    .child(div().text_color(theme.muted_foreground).child(format!(
                        "{} / {}",
                        clock(position),
                        clock(length)
                    )))
                    .into_any_element(),
            );
        }
        if note.recording.is_some() {
            return None;
        }
        Some(record_button("recording-record", self.recording_elsewhere(), cx).into_any_element())
    }

    /// The name of the Note being recorded, when one is.
    fn recording_elsewhere(&self) -> Option<String> {
        self.note_detail
            .recording
            .as_ref()
            .map(|active| active.target.name.clone())
    }

    /// The empty state's Record button: makes a Note for the Event and records it at once.
    pub(super) fn render_empty_record_button(&self, cx: &mut Context<Self>) -> AnyElement {
        record_button("note-empty-record", self.recording_elsewhere(), cx)
            .outline()
            .on_click(cx.listener(|view, _, window, cx| {
                view.new_note(false, window, cx);
                view.record(cx);
            }))
            .into_any_element()
    }

    /// Under the card, while the shown Note records: a line for each track that has stayed
    /// silent since the start, with a button to the System Settings pane that allows it.
    pub(super) fn render_recording_hints(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let detail = &self.note_detail;
        let Some(active) = detail.recording.as_ref().filter(|active| {
            detail
                .note
                .as_ref()
                .is_some_and(|note| note.id == active.target.note)
        }) else {
            return Vec::new();
        };
        let status = active.recorder.status();
        if status.elapsed < SILENCE_HINT_AFTER {
            return Vec::new();
        }
        let hint = |id: &'static str, text: &'static str, url: &'static str| {
            h_flex()
                .w_full()
                .gap_2()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(div().flex_1().min_w_0().child(text))
                .child(
                    button(id)
                        .flex_shrink_0()
                        .link()
                        .small()
                        .label("Open System Settings")
                        .on_click(move |_, _, cx| cx.open_url(url)),
                )
                .into_any_element()
        };
        let mut hints = Vec::new();
        if !status.system_heard {
            hints.push(hint(
                "recording-hint-system",
                "No sound from the Mac yet. If a call is playing, allow Crumble in System Settings › \
                 Privacy & Security › Screen & System Audio Recording.",
                SYSTEM_AUDIO_SETTINGS,
            ));
        }
        if !status.mic_heard {
            hints.push(hint(
                "recording-hint-mic",
                "No sound from the microphone yet. Allow Crumble in System Settings › Privacy & \
                 Security › Microphone.",
                MICROPHONE_SETTINGS,
            ));
        }
        hints
    }

    /// The header's "● Recording · 12:03" pill while a Recording runs; it opens that Note.
    pub(in super::super) fn render_recording_pill(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let active = self.note_detail.recording.as_ref()?;
        let elapsed = active.recorder.status().elapsed;
        Some(
            button("recording-pill")
                .ghost()
                .small()
                .tooltip(format!("Open “{}”", active.target.name))
                .child(
                    h_flex()
                        .gap_1p5()
                        .child(red_dot(cx))
                        .child(format!("Recording · {}", clock(elapsed))),
                )
                .on_click(cx.listener(|view, _, _, cx| view.open_recording(cx)))
                .into_any_element(),
        )
    }
}

/// Record, or Record disabled with the recording Note's name when `elsewhere` is one.
fn record_button(
    id: &'static str,
    elsewhere: Option<String>,
    cx: &mut Context<NativeView>,
) -> gpui_kit::component::button::Button {
    let button = button(id)
        .ghost()
        .small()
        .icon(app_icon("mic").size_4())
        .label("Record")
        .inactive(elsewhere.is_some());
    match elsewhere {
        Some(name) => button.tooltip(format!("Recording “{name}”")),
        None => button.on_click(cx.listener(|view, _, _, cx| view.record(cx))),
    }
}

fn red_dot(cx: &Context<NativeView>) -> impl IntoElement {
    div()
        .size(px(8.0))
        .flex_shrink_0()
        .rounded_full()
        .bg(cx.theme().danger)
}

/// `0:07`, `12:03`, or `1:02:03`.
fn clock(time: Duration) -> String {
    let seconds = time.as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::clock;
    use std::time::Duration;

    #[test]
    fn clock_shows_minutes_and_seconds_then_hours() {
        assert_eq!(clock(Duration::ZERO), "0:00");
        assert_eq!(clock(Duration::from_millis(7_900)), "0:07");
        assert_eq!(clock(Duration::from_secs(12 * 60 + 3)), "12:03");
        assert_eq!(clock(Duration::from_secs(3600 + 2 * 60 + 3)), "1:02:03");
    }
}
