//! The Note detail view's state: the Note of the selected Agenda row or of the opened Event,
//! and its title and Memo fields. Edits are written through `Notes`: the title on Enter or blur,
//! the Memo once typing pauses, on blur, and before another Note shows.
//! The Recording, its player and the header pill are in `recording`; the Transcript and the job
//! that makes it in `transcript`; the Summary and its job in `summary`.

mod actions;
mod detail;
mod recording;
mod summary;
mod tabs;
mod transcript;

use super::NativeView;
use super::agenda::Selected;
use chrono::Utc;
use crumble_eventkit::Event;
use crumble_notes::{Note, NoteId, TranscriptSegment};
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::{App, AppContext, Bounds, Context, Entity, Pixels, Subscription, Task, Window};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

/// How long typing pauses before the Memo is saved.
const MEMO_SAVE_DELAY: Duration = Duration::from_millis(500);

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
    /// Memos whose save failed, shown again instead of the stored ones so no text is lost; each
    /// later save tries them again.
    unsaved: HashMap<NoteId, String>,
    /// Whether the last save failed.
    save_failed: bool,
    /// Where the Note actions button was last drawn; its menu opens below it.
    actions: Rc<Cell<Bounds<Pixels>>>,
    /// The one Recording running in the app, whichever Note shows.
    recording: Option<recording::Active>,
    /// The shown Note's finished Recording.
    playback: Option<recording::Playback>,
    /// Whether the last Recording failed to start, stop or delete.
    recording_failed: bool,
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
                view.stop_recording(cx);
                async {}
            }),
        ];
        // So does closing the window, which drops the view before the app quits.
        let view = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            let _ = view.update(cx, |view, cx| {
                view.save_note(cx);
                view.stop_recording(cx);
            });
            true
        });
        Self {
            subject: None,
            note: None,
            title,
            memo,
            memo_dirty: false,
            memo_save: Task::ready(()),
            unsaved: HashMap::new(),
            save_failed: false,
            actions: Rc::default(),
            recording: None,
            playback: None,
            recording_failed: false,
            tab: None,
            transcript: Vec::new(),
            transcription: transcript::Transcription::default(),
            summarization: summary::Summarization::default(),
            _subscriptions: subscriptions,
        }
    }

    /// The shown Note's tabs, and the one that shows.
    fn tabs(&self) -> (Vec<tabs::NoteTab>, tabs::NoteTab) {
        let (summary, transcript) = self.note.as_ref().map_or((false, false), |note| {
            (
                // Also a spoken Transcript no job took, which the tab offers to summarize.
                note.summary.is_some()
                    || self.summarization.involves(&note.id)
                    || (note.transcript.is_some() && !self.transcript.is_empty()),
                note.transcript.is_some() || self.transcription.involves(&note.id),
            )
        });
        let tabs = tabs::tabs(summary, transcript);
        let shown = tabs::shown(self.tab, &tabs);
        (tabs, shown)
    }
}

impl NativeView {
    /// What the current page shows the Note of: the selected Agenda row, or the opened Event.
    fn note_subject(&self, cx: &App) -> Option<Subject> {
        match &self.route {
            super::Route::Agenda => match self.agenda.selected.as_ref()? {
                Selected::Event(event) => Some(Subject::Event(event.clone())),
                Selected::QuickNote(id) => self
                    .agenda
                    .quick_note(id)
                    .map(|note| Subject::QuickNote(Box::new(note.clone()))),
            },
            super::Route::Event(opened) => {
                let event = crumble_calendar::events::latest(self.events.read(cx).events(), opened);
                Some(Subject::Event(Rc::new(event.clone())))
            }
            _ => None,
        }
    }

    /// Shows the Note of the current page's subject, saving the shown Note first when the
    /// subject changed. Called on each render, as the Agenda keeps its rows.
    pub(super) fn maintain_note_detail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let subject = self.note_subject(cx);
        match (&mut self.note_detail.subject, &subject) {
            (Some(shown), Some(subject)) if shown.same(subject) => {
                // The same occurrence, maybe with new details.
                *shown = subject.clone();
                return;
            }
            (None, None) => return,
            _ => {}
        }
        self.save_note(cx);
        self.note_detail.subject = subject;
        let note = match &self.note_detail.subject {
            None => None,
            Some(Subject::QuickNote(note)) => Some((**note).clone()),
            Some(Subject::Event(event)) => self.notes.for_event(event).unwrap_or_else(|error| {
                eprintln!("Could not read the Event's Note: {error}");
                None
            }),
        };
        self.show_note(note, window, cx);
    }

    /// Shows `note` and fills the fields with it, with a Memo that failed to save in place of
    /// the stored one.
    fn show_note(&mut self, mut note: Option<Note>, window: &mut Window, cx: &mut Context<Self>) {
        let detail = &mut self.note_detail;
        if let Some(note) = &mut note
            && let Some(memo) = detail.unsaved.get(&note.id)
        {
            note.memo.clone_from(memo);
        }
        detail.memo_dirty = note
            .as_ref()
            .is_some_and(|note| detail.unsaved.contains_key(&note.id));
        detail.note = note;
        detail.memo_save = Task::ready(());
        let (title, memo) = detail
            .note
            .as_ref()
            .map(|note| (note.title.clone().unwrap_or_default(), note.memo.clone()))
            .unwrap_or_default();
        let placeholder = match &detail.subject {
            Some(Subject::Event(event)) => event_title(event),
            _ => "Quick note".to_owned(),
        };
        detail.title.update(cx, |input, cx| {
            input.set_value(title, window, cx);
            input.set_placeholder(placeholder, window, cx);
        });
        detail
            .memo
            .update(cx, |input, cx| input.set_value(memo, window, cx));
        self.show_player(cx);
        self.show_transcript();
        cx.notify();
    }

    /// Changes every in-memory copy of the Note `id`: the shown Note, the Quick note shown,
    /// and its Agenda row.
    fn note_changed(&mut self, id: &NoteId, change: impl Fn(&mut Note)) {
        let detail = &mut self.note_detail;
        if let Some(note) = &mut detail.note
            && &note.id == id
        {
            change(note);
        }
        if let Some(Subject::QuickNote(note)) = &mut detail.subject
            && &note.id == id
        {
            change(note);
        }
        if let Some(mut note) = self.agenda.quick_note(id).cloned() {
            change(&mut note);
            self.agenda.note_changed(&note);
        }
    }

    /// Saves the shown Note's title and Memo where they changed, and tries again the Memos
    /// that failed to save.
    fn save_note(&mut self, cx: &mut Context<Self>) {
        self.save_title(cx);
        self.save_memo(cx);
        let detail = &mut self.note_detail;
        let notes = &mut self.notes;
        detail
            .unsaved
            .retain(|id, memo| notes.set_memo(id, memo, Utc::now()).is_err());
    }

    fn memo_changed(&mut self, cx: &mut Context<Self>) {
        let detail = &mut self.note_detail;
        detail.memo_dirty = true;
        detail.memo_save = cx.spawn(async move |view, cx| {
            cx.background_executor().timer(MEMO_SAVE_DELAY).await;
            let _ = view.update(cx, |view, cx| view.save_memo(cx));
        });
    }

    /// Writes the Memo field to the shown Note if it changed; a failure keeps the text and
    /// says so under the card.
    fn save_memo(&mut self, cx: &mut Context<Self>) {
        let detail = &mut self.note_detail;
        if !detail.memo_dirty {
            return;
        }
        detail.memo_dirty = false;
        detail.memo_save = Task::ready(());
        let memo = detail.memo.read(cx).value().to_string();
        let Some(note) = &mut detail.note else {
            return;
        };
        note.memo.clone_from(&memo);
        let note = note.clone();
        match self.notes.set_memo(&note.id, &memo, Utc::now()) {
            Ok(()) => {
                detail.unsaved.remove(&note.id);
                detail.save_failed = false;
            }
            Err(error) => {
                eprintln!("Could not save the Memo: {error}");
                detail.unsaved.insert(note.id.clone(), memo);
                detail.memo_dirty = true;
                detail.save_failed = true;
            }
        }
        self.agenda.note_changed(&note);
        cx.notify();
    }

    /// Writes the title field to the shown Note if it changed; an empty title is none.
    fn save_title(&mut self, cx: &mut Context<Self>) {
        let detail = &mut self.note_detail;
        let typed = detail.title.read(cx).value().trim().to_owned();
        let title = (!typed.is_empty()).then_some(typed);
        let Some(note) = &mut detail.note else {
            return;
        };
        if note.title == title {
            return;
        }
        match self.notes.set_title(&note.id, title.as_deref(), Utc::now()) {
            Ok(()) => {
                note.title = title;
                detail.save_failed = false;
                let note = note.clone();
                self.agenda.note_changed(&note);
            }
            Err(error) => {
                eprintln!("Could not save the title: {error}");
                detail.save_failed = true;
            }
        }
        cx.notify();
    }

    /// Makes a Quick note when `quick`, else the shown Event's Note, or a Quick note when no
    /// Event shows; shows it and focuses its Memo. An Event that has a Note keeps it.
    pub(super) fn new_note(&mut self, quick: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.save_note(cx);
        let event = match &self.note_detail.subject {
            Some(Subject::Event(event)) if !quick => Some(event.clone()),
            _ => None,
        };
        let note = match self.notes.create(event.as_deref(), Utc::now()) {
            Ok(note) => note,
            Err(error) => {
                eprintln!("Could not make a Note: {error}");
                self.note_detail.save_failed = true;
                cx.notify();
                return;
            }
        };
        if event.is_some() {
            self.show_note(Some(note), window, cx);
        } else {
            self.agenda.select_new_quick_note(note);
            self.maintain_note_detail(window, cx);
        }
        let memo = self.note_detail.memo.clone();
        memo.update(cx, |input, cx| input.focus(window, cx));
    }

    /// Deletes the shown Note; a Quick note's row leaves the Agenda.
    fn delete_note(&mut self, id: &NoteId, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .note_detail
            .note
            .as_ref()
            .is_none_or(|note| &note.id != id)
        {
            return;
        }
        // The Recording goes with its Note: stop it, and close its files, before they go.
        if self.is_recording(id) {
            self.note_detail.recording = None;
        }
        self.note_detail.playback = None;
        self.cancel_transcription(id);
        self.cancel_summary(id);
        let detail = &mut self.note_detail;
        if let Err(error) = self.notes.delete(id, Utc::now()) {
            eprintln!("Could not delete the Note: {error}");
            detail.save_failed = true;
            cx.notify();
            return;
        }
        detail.unsaved.remove(id);
        detail.memo_dirty = false;
        if matches!(detail.subject, Some(Subject::QuickNote(_))) {
            self.agenda.quick_note_deleted(id);
        }
        self.show_note(None, window, cx);
    }
}

/// An Event's title, or `(No title)`.
fn event_title(event: &Event) -> String {
    if event.title.is_empty() {
        "(No title)".to_owned()
    } else {
        event.title.clone()
    }
}
