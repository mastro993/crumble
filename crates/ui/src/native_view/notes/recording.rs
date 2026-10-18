//! A Note's Recording in the detail view: Record and Stop on the card header's trailing side,
//! hints under the card when a track stays silent, the player once it is recorded, Delete
//! recording, and the header pill that leads back to the Note being recorded. One Recording
//! runs at a time in the whole app, and it keeps running while other pages show.
//! The player is in `player`; the controls, hints and pill are drawn in `render`.

mod player;
mod render;

pub(super) use render::clock;

use super::super::{NativeView, Route};
use super::{Subject, event_title};
use crate::native_view::agenda::Selected;
use chrono::{DateTime, Utc};
use crumble_audio::{Access, Player, Recorder};
use crumble_eventkit::Event;
use crumble_notes::{NoteId, RecordingInfo};
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::slider::SliderState;
use gpui_kit::{Context, Entity, Subscription, Task, Window};
use std::time::Duration;

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
        self.note_detail
            .recording_failed
            .take_if(|failed| *failed == id);
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

    /// Stops the Recording and keeps it as [`Notes::finish_recording`] does; on quit, window
    /// close and Note deletion too. A stop that fails keeps what the files hold. One whose
    /// files cannot be read stays unfinished, for the next launch to read again. Returns the
    /// Note when its finished Recording is stored, so Stop can transcribe it.
    ///
    /// [`Notes::finish_recording`]: crumble_notes::Notes::finish_recording
    pub(super) fn stop_recording(&mut self, cx: &mut Context<Self>) -> Option<NoteId> {
        let active = self.note_detail.recording.take()?;
        let id = active.target.note;
        let dir = self.notes.recording_dir(&id);
        let stopped = active.recorder.stop();
        let clean = stopped.is_ok();
        let length = stopped.or_else(|error| {
            eprintln!("The Recording did not stop cleanly: {error:#}");
            crumble_audio::recorded_length(&dir)
        });
        let finished = length.and_then(|duration| {
            let finished = self.notes.finish_recording(&id, duration, Utc::now());
            match finished {
                // The files are complete, so the Recording shows as finished; its length is
                // kept in memory and written again with every later save, as a failed Memo is.
                Err(error) if !duration.is_zero() => {
                    eprintln!("Could not save the Recording's length: {error:#}");
                    let info = RecordingInfo {
                        started_at: active.started_at,
                        duration: Some(duration),
                    };
                    self.note_detail.unsaved_recordings.insert(id.clone(), info);
                    self.note_detail.save_failed = true;
                    Ok(Some(info))
                }
                finished => finished,
            }
        });
        let stored = (finished.as_ref().ok()).is_some_and(|info| {
            info.is_some() && !self.note_detail.unsaved_recordings.contains_key(&id)
        });
        match finished {
            Ok(info) => {
                self.recording_changed(&id, info);
                if clean {
                    self.note_detail
                        .recording_failed
                        .take_if(|failed| *failed == id);
                } else {
                    // What was kept stays, but part of it may be missing.
                    self.recording_failed(&id, "it did not stop cleanly", cx);
                }
            }
            Err(error) => self.recording_failed(&id, &format!("{error:#}"), cx),
        }
        // Another Note's player stays as it is.
        if (self.note_detail.note.as_ref()).is_some_and(|note| note.id == id) {
            self.show_player(cx);
        }
        cx.notify();
        stored.then_some(id)
    }

    /// Keeps every in-memory copy of the Note `id` in step with its Recording.
    fn recording_changed(&mut self, id: &NoteId, info: Option<RecordingInfo>) {
        self.note_changed(id, |note| note.recording = info);
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
        // Closes the files before they go, and the Transcript and Summary go with them.
        self.note_detail.playback = None;
        self.cancel_transcription(id);
        match self.notes.delete_recording(id, Utc::now()) {
            Ok(()) => {
                // Only now: a Recording that stays keeps its Summary work.
                self.cancel_summary(id);
                self.note_detail.unsaved_recordings.remove(id);
                self.note_changed(id, |note| {
                    note.recording = None;
                    note.transcript = None;
                    note.summary = None;
                });
                self.show_transcript();
                self.note_detail
                    .recording_failed
                    .take_if(|failed| failed == id);
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

    /// Makes the shown Event's Note, or takes the one it has, and records it unless a Recording
    /// runs or the Note has one: the empty state's Record, and a Suggestion's.
    pub(in super::super) fn record_new_note(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.note_detail.recording.is_some() {
            return;
        }
        self.new_note(false, window, cx);
        self.record(cx);
    }

    /// The Event being recorded, when a Recording of an Event's Note runs.
    pub(in super::super) fn recording_event(&self) -> Option<&Event> {
        match &self.note_detail.recording.as_ref()?.target.subject {
            Subject::Event(event) => Some(event),
            Subject::QuickNote(_) => None,
        }
    }
}
