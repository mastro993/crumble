//! The Note detail view's state: the Note of the selected Agenda row or Event,
//! and its title and Memo fields. Edits are written through `Notes`: the title on Enter or blur,
//! the Memo once typing pauses, on blur, and before another Note shows.
//! The Recording, its player and the header pill are in `recording`; the Transcript and the job
//! that makes it in `transcript`, its tab in `transcript_tab`, with the Transcription Model and the consent a remote one
//! needs in `consent`; the Summary and its job in `summary`. Which Note shows, and making and
//! deleting one, are in `show`; saving it in `save`.

mod actions;
mod consent;
mod detail;
mod recording;
mod save;
mod show;
mod summary;
mod tabs;
mod transcript;
mod transcript_tab;

use super::NativeView;
use crumble_eventkit::Event;
use crumble_notes::{Note, NoteId, RecordingInfo, TranscriptSegment};
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::{AppContext, Bounds, Context, Entity, Pixels, Subscription, Task, Window};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
/// Whose Note the detail view shows.
#[derive(Clone)]
pub(super) enum Subject {
    QuickNote(Box<Note>),
    Event(Rc<Event>),
}

impl Subject {
    /// Whether `self` and `other` show the same Note: the same Quick note, or the same Event
    /// occurrence, found by its series UID and occurrence once it has a UID, as `Event::id`
    /// changes when the Event moves (ADR 0004).
    fn same(&self, other: &Subject) -> bool {
        match (self, other) {
            (Subject::QuickNote(a), Subject::QuickNote(b)) => a.id == b.id,
            (Subject::Event(a), Subject::Event(b)) if a.series_uid.is_some() => {
                a.series_uid == b.series_uid && a.occurrence == b.occurrence
            }
            (Subject::Event(a), Subject::Event(b)) => a.id == b.id,
            _ => false,
        }
    }
}

pub(super) struct NoteDetail {
    /// What the Note below was read for.
    subject: Option<Subject>,
    /// The subject's Note, with the edits made since; `None` for an Event with no Note.
    note: Option<Note>,
    title: Entity<InputState>,
    memo: Entity<TextareaState>,
    /// Whether the Memo field changed since it was last saved.
    memo_dirty: bool,
    /// Saves the Memo once typing pauses.
    memo_save: Task<()>,
    /// Titles and Memos whose save failed, shown again instead of the stored ones so no text is
    /// lost; each later save tries them again.
    unsaved_titles: HashMap<NoteId, Option<String>>,
    unsaved_memos: HashMap<NoteId, String>,
    /// Finished Recordings whose length failed to save, shown as finished and written again
    /// with every later save. A restart's crash recovery reads the length from the files too.
    unsaved_recordings: HashMap<NoteId, RecordingInfo>,
    /// Whether the last save failed.
    save_failed: bool,
    /// Where the Note actions button was last drawn; its menu opens below it.
    actions: Rc<Cell<Bounds<Pixels>>>,
    /// The one Recording running in the app, whichever Note shows.
    recording: Option<recording::Active>,
    /// The shown Note's finished Recording.
    playback: Option<recording::Playback>,
    /// The Note whose Recording last failed to start, stop in full, or delete.
    recording_failed: Option<NoteId>,
    /// The tab the user picked, kept across Notes; `None` shows the first.
    tab: Option<tabs::NoteTab>,
    /// The shown Note's Transcript.
    transcript: Vec<TranscriptSegment>,
    /// The Transcripts being made, in the background.
    transcription: transcript::Transcription,
    /// The Summaries being made, in the background.
    summarization: summary::Summarization,
    _subscriptions: [Subscription; 3],
}

impl NoteDetail {
    pub(super) fn new(window: &mut Window, cx: &mut Context<NativeView>) -> Self {
        let title = cx.new(|cx| InputState::new(window, cx));
        let memo = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(6, 1000)
                .placeholder("Write something…")
        });
        let subscriptions = [
            cx.subscribe_in(&title, window, |view, _, event, _, cx| {
                if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    view.save_title(cx);
                }
            }),
            cx.subscribe_in(&memo, window, |view, _, event, _, cx| match event {
                InputEvent::Change => view.memo_changed(cx),
                InputEvent::Blur => view.save_memo(cx),
                _ => {}
            }),
            // Quitting within the pause still keeps what was typed, and a running Recording.
            cx.on_app_quit(|view, cx| {
                view.save_note(cx);
                // The next launch transcribes it with a local Model; a remote one waits to be asked.
                let _ = view.stop_recording(cx);
                async {}
            }),
        ];
        // So does closing the window, which drops the view before the app quits.
        let view = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            view.update(cx, |view, cx| view.may_close(window, cx))
                .unwrap_or(true)
        });
        Self {
            subject: None,
            note: None,
            title,
            memo,
            memo_dirty: false,
            memo_save: Task::ready(()),
            unsaved_titles: HashMap::new(),
            unsaved_memos: HashMap::new(),
            unsaved_recordings: HashMap::new(),
            save_failed: false,
            actions: Rc::default(),
            recording: None,
            playback: None,
            recording_failed: None,
            tab: None,
            transcript: Vec::new(),
            transcription: transcript::Transcription::default(),
            summarization: summary::Summarization::default(),
            _subscriptions: subscriptions,
        }
    }

    /// Whether the shown Transcript holds speech, which a Summary can be made from.
    pub(super) fn spoken(&self) -> bool {
        self.transcript
            .iter()
            .any(|segment| !segment.text.trim().is_empty())
    }

    /// The shown Note's tabs, and the one that shows.
    fn tabs(&self) -> (Vec<tabs::NoteTab>, tabs::NoteTab) {
        let (summary, transcript) = self.note.as_ref().map_or((false, false), |note| {
            (
                // Also a spoken Transcript no job took, which the tab offers to summarize.
                note.summary.is_some()
                    || self.summarization.involves(&note.id)
                    || (note.transcript.is_some() && self.spoken()),
                // Also a finished Recording no job took, which the tab offers to transcribe,
                // once its length is stored, as a Transcript needs.
                note.transcript.is_some()
                    || self.transcription.involves(&note.id)
                    || (note
                        .recording
                        .is_some_and(|recording| recording.duration.is_some())
                        && !self.unsaved_recordings.contains_key(&note.id)),
            )
        });
        let tabs = tabs::tabs(summary, transcript);
        let shown = tabs::shown(self.tab, &tabs);
        (tabs, shown)
    }
}

/// The title field's placeholder while the Note has no title: the Event's title, or
/// `Quick note`.
fn placeholder(subject: &Subject) -> String {
    match subject {
        Subject::Event(event) => event_title(event),
        Subject::QuickNote(_) => "Quick note".to_owned(),
    }
}

/// An Event's title, or `(No title)`.
pub(super) fn event_title(event: &Event) -> String {
    if event.title.is_empty() {
        "(No title)".to_owned()
    } else {
        event.title.clone()
    }
}
