//! Which Note the detail view shows: the current page's subject's, read from `Notes`, and
//! making and deleting one.

use super::super::agenda::Selected;
use super::super::{NativeView, Route};
use super::{Subject, placeholder};
use chrono::Utc;
use crumble_notes::{Note, NoteId};
use gpui_kit::component::WindowExt;
use gpui_kit::component::notification::Notification;
use gpui_kit::{App, Context, Task, Window};
use std::rc::Rc;

impl NativeView {
    /// What the current page shows the Note of: the selected Agenda row, or the opened Event.
    fn note_subject(&self, cx: &App) -> Option<Subject> {
        match &self.route {
            Route::Agenda => match self.agenda.selected.as_ref()? {
                Selected::Event(event) => Some(Subject::Event(event.clone())),
                Selected::QuickNote(id) => self
                    .agenda
                    .quick_note(id)
                    .map(|note| Subject::QuickNote(Box::new(note.clone()))),
            },
            Route::Event(opened) => {
                let event = crumble_calendar::events::latest(self.events.read(cx).events(), opened);
                Some(Subject::Event(Rc::new(event.clone())))
            }
            _ => None,
        }
    }

    /// Shows the Note of the current page's subject, saving the shown Note first when the
    /// subject changed. Called on each render, as the Agenda keeps its rows.
    pub(in super::super) fn maintain_note_detail(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        self.show_transcript();
        cx.notify();
    }

    /// Changes every in-memory copy of the Note `id`: the shown Note, the Quick note shown,
    /// and its Agenda row.
    pub(super) fn note_changed(&mut self, id: &NoteId, change: impl Fn(&mut Note)) {
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

    /// Makes a Quick note when `quick`, else the shown Event's Note, or a Quick note when no
    /// Event shows; shows it and focuses its Memo. An Event that has a Note keeps it.
    pub(in super::super) fn new_note(
        &mut self,
        quick: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
    pub(super) fn delete_note(&mut self, id: &NoteId, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .note_detail
            .note
            .as_ref()
            .is_none_or(|note| &note.id != id)
        {
            return;
        }
        // The Recording goes with its Note: stop and keep it, so it stays whole if the delete
        // fails, and close its files before they go. It is not transcribed, as it is going.
        if self.is_recording(id) {
            let _ = self.stop_recording(cx);
        }
        self.note_detail.playback = None;
        self.cancel_transcription(id);
        if let Err(error) = self.notes.delete(id, Utc::now()) {
            eprintln!("Could not delete the Note: {error}");
            self.note_detail.save_failed = true;
            cx.notify();
            return;
        }
        // Only now: a Note that stays keeps its Summary work.
        self.cancel_summary(id);
        let detail = &mut self.note_detail;
        detail.unsaved_titles.remove(id);
        detail.unsaved_memos.remove(id);
        detail.unsaved_recordings.remove(id);
        detail.memo_dirty = false;
        if matches!(detail.subject, Some(Subject::QuickNote(_))) {
            self.agenda.quick_note_deleted(id);
        }
        // Clears "Couldn't save" from an earlier failed attempt.
        self.retry_unsaved(cx);
        self.show_note(None, window, cx);
    }
}
