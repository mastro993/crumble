//! The Note detail view's state: the Note of the selected Agenda row or of the opened Event,
//! and its title and Memo fields. Edits are written through `Notes`: the title on Enter or blur,
//! the Memo once typing pauses, on blur, and before another Note shows.
//! The Recording, its player and the header pill are in `recording`.

mod actions;
mod detail;
mod recording;

use super::NativeView;
use super::agenda::Selected;
use chrono::Utc;
use crumble_eventkit::Event;
use crumble_notes::{Note, NoteId, RecordingInfo};
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::component::notification::Notification;
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
            _subscriptions: subscriptions,
        }
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
                // The same occurrence, maybe with new details, such as a new Event title.
                let renamed = placeholder(shown) != placeholder(subject);
                *shown = subject.clone();
                if renamed {
                    let placeholder = placeholder(subject);
                    self.note_detail.title.update(cx, |input, cx| {
                        input.set_placeholder(placeholder, window, cx);
                    });
                }
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
            Some(Subject::Event(event)) => match self.notes.for_event(event) {
                Ok(note) => note,
                Err(error) => {
                    eprintln!("Could not read the Event's Note: {error}");
                    // Shows nothing rather than "No note yet", and reads again on a later
                    // render; not redrawing now keeps a failing read from repeating each frame.
                    let detail = &mut self.note_detail;
                    detail.subject = None;
                    detail.note = None;
                    return;
                }
            },
        };
        self.show_note(note, window, cx);
    }

    /// Saves the shown Note and stops a running Recording before the window closes. While a
    /// title or Memo still fails to save, asks first instead, as closing drops the text kept in
    /// memory.
    fn may_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.save_note(cx);
        let detail = &self.note_detail;
        if detail.unsaved_titles.is_empty() && detail.unsaved_memos.is_empty() {
            self.stop_recording(cx);
            return true;
        }
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let view = view.clone();
            alert
                .title("Close without saving?")
                .description("Some notes couldn't be saved. Closing loses their changes.")
                .show_cancel(true)
                .ok_text("Close")
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| view.stop_recording(cx));
                    window.remove_window();
                    true
                })
        });
        false
    }

    /// Shows `note` and fills the fields with it, with a title, Memo or Recording length that
    /// failed to save in place of the stored one.
    fn show_note(&mut self, mut note: Option<Note>, window: &mut Window, cx: &mut Context<Self>) {
        let detail = &mut self.note_detail;
        if let Some(note) = &mut note {
            if let Some(title) = detail.unsaved_titles.get(&note.id) {
                note.title.clone_from(title);
            }
            if let Some(memo) = detail.unsaved_memos.get(&note.id) {
                note.memo.clone_from(memo);
            }
            if let Some(recording) = detail.unsaved_recordings.get(&note.id) {
                note.recording = Some(*recording);
            }
        }
        detail.memo_dirty = false;
        detail.note = note;
        detail.memo_save = Task::ready(());
        let (title, memo) = detail
            .note
            .as_ref()
            .map(|note| (note.title.clone().unwrap_or_default(), note.memo.clone()))
            .unwrap_or_default();
        let placeholder = detail.subject.as_ref().map(placeholder).unwrap_or_default();
        detail.title.update(cx, |input, cx| {
            input.set_value(title, window, cx);
            input.set_placeholder(placeholder, window, cx);
        });
        detail
            .memo
            .update(cx, |input, cx| input.set_value(memo, window, cx));
        self.show_player(cx);
        cx.notify();
    }

    /// Saves the shown Note's title and Memo where they changed, and tries again the ones that
    /// failed to save.
    fn save_note(&mut self, cx: &mut Context<Self>) {
        self.save_title(cx);
        self.save_memo(cx);
        self.retry_unsaved();
    }

    /// Writes again the titles, Memos and Recording lengths that failed to save, and says
    /// "Couldn't save" while any still fails.
    fn retry_unsaved(&mut self) {
        let detail = &mut self.note_detail;
        let notes = &mut self.notes;
        let now = Utc::now();
        let mut saved_titles = Vec::new();
        detail.unsaved_titles.retain(|id, title| {
            let failed = notes.set_title(id, title.as_deref(), now).is_err();
            if !failed {
                saved_titles.push((id.clone(), title.clone()));
            }
            failed
        });
        let mut saved_memos = Vec::new();
        detail.unsaved_memos.retain(|id, memo| {
            let failed = notes.set_memo(id, memo, now).is_err();
            if !failed {
                saved_memos.push((id.clone(), memo.clone()));
            }
            failed
        });
        detail.unsaved_recordings.retain(|id, recording| {
            recording
                .duration
                .is_some_and(|duration| notes.set_recording_finished(id, duration, now).is_err())
        });
        detail.save_failed = !detail.unsaved_titles.is_empty()
            || !detail.unsaved_memos.is_empty()
            || !detail.unsaved_recordings.is_empty();
        // A rebuilt Agenda listed a Quick note as stored before, so it takes the saved edits.
        for (id, title) in saved_titles {
            self.update_quick_note(&id, |note| note.title = title);
        }
        for (id, memo) in saved_memos {
            self.update_quick_note(&id, |note| note.memo = memo);
        }
    }

    /// Changes the Agenda's listed copy of the Quick note `id`, if it is listed.
    fn update_quick_note(&mut self, id: &NoteId, change: impl FnOnce(&mut Note)) {
        if let Some(mut note) = self.agenda.quick_note(id).cloned() {
            change(&mut note);
            self.agenda.note_changed(&note);
        }
    }

    fn memo_changed(&mut self, cx: &mut Context<Self>) {
        let detail = &mut self.note_detail;
        detail.memo_dirty = true;
        detail.memo_save = cx.spawn(async move |view, cx| {
            cx.background_executor().timer(MEMO_SAVE_DELAY).await;
            let _ = view.update(cx, |view, cx| view.save_memo(cx));
        });
    }

    /// Writes the Memo field to the shown Note if it changed, then tries again what failed to
    /// save; a failure keeps the text and says so under the card.
    fn save_memo(&mut self, cx: &mut Context<Self>) {
        let detail = &mut self.note_detail;
        // A Memo kept after a failed save is written again, even unchanged since.
        let kept =
            (detail.note.as_ref()).is_some_and(|note| detail.unsaved_memos.contains_key(&note.id));
        if !detail.memo_dirty && !kept {
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
        if let Err(error) = self.notes.set_memo(&note.id, &memo, Utc::now()) {
            eprintln!("Could not save the Memo: {error}");
            detail.unsaved_memos.insert(note.id.clone(), memo);
        } else {
            detail.unsaved_memos.remove(&note.id);
        }
        self.agenda.note_changed(&note);
        self.retry_unsaved();
        cx.notify();
    }

    /// Writes the title field to the shown Note if it changed, then tries again what failed to
    /// save; an empty title is none, and a failure keeps the title as the Memo's does.
    fn save_title(&mut self, cx: &mut Context<Self>) {
        let detail = &mut self.note_detail;
        let typed = detail.title.read(cx).value().trim().to_owned();
        let title = (!typed.is_empty()).then_some(typed);
        let Some(note) = &mut detail.note else {
            return;
        };
        // A title kept after a failed save is written again, even unchanged since.
        if note.title == title && !detail.unsaved_titles.contains_key(&note.id) {
            return;
        }
        note.title.clone_from(&title);
        let note = note.clone();
        if let Err(error) = self.notes.set_title(&note.id, title.as_deref(), Utc::now()) {
            eprintln!("Could not save the title: {error}");
            detail.unsaved_titles.insert(note.id.clone(), title);
        } else {
            detail.unsaved_titles.remove(&note.id);
        }
        self.agenda.note_changed(&note);
        self.retry_unsaved();
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
                // A toast, as no Note may show to say it under.
                window.push_notification(Notification::error("Couldn't make a note"), cx);
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
        let detail = &mut self.note_detail;
        if let Err(error) = self.notes.delete(id, Utc::now()) {
            eprintln!("Could not delete the Note: {error}");
            detail.save_failed = true;
            cx.notify();
            return;
        }
        detail.unsaved_titles.remove(id);
        detail.unsaved_memos.remove(id);
        detail.unsaved_recordings.remove(id);
        detail.memo_dirty = false;
        if matches!(detail.subject, Some(Subject::QuickNote(_))) {
            self.agenda.quick_note_deleted(id);
        }
        // Clears "Couldn't save" from an earlier failed attempt.
        self.retry_unsaved();
        self.show_note(None, window, cx);
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
fn event_title(event: &Event) -> String {
    if event.title.is_empty() {
        "(No title)".to_owned()
    } else {
        event.title.clone()
    }
}
