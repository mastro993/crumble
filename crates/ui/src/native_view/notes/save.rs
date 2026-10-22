//! Saving the shown Note: the title on Enter or blur, the Memo once typing pauses or on blur,
//! and again what failed to save, which closing the window asks about first.

use super::super::NativeView;
use chrono::Utc;
use crumble_notes::{Note, NoteId};
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::dialog::DialogEntrance;
use gpui_kit::{Context, Task, Window};
use std::time::Duration;

/// How long typing pauses before the Memo is saved.
const MEMO_SAVE_DELAY: Duration = Duration::from_millis(500);

impl NativeView {
    /// Saves the shown Note and stops a running Recording before the window closes. While a
    /// title or Memo still fails to save, asks first instead, as closing drops the text kept in
    /// memory.
    pub(super) fn may_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.save_note(cx);
        let detail = &self.note_detail;
        if detail.unsaved_titles.is_empty() && detail.unsaved_memos.is_empty() {
            let _ = self.stop_recording(cx);
            return true;
        }
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let view = view.clone();
            alert
                .entrance(DialogEntrance::Fade)
                .title("Close without saving?")
                .description("Some notes couldn't be saved. Closing loses their changes.")
                .show_cancel(true)
                .ok_text("Close")
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| {
                        let _ = view.stop_recording(cx);
                    });
                    window.remove_window();
                    true
                })
        });
        false
    }

    /// Saves the shown Note's title and Memo where they changed, and tries again the ones that
    /// failed to save.
    pub(super) fn save_note(&mut self, cx: &mut Context<Self>) {
        self.save_title(cx);
        self.save_memo(cx);
        self.retry_unsaved(cx);
    }

    /// Writes again the titles, Memos and Recording lengths that failed to save, and says
    /// "Couldn't save" while any still fails. A Recording whose length saves is transcribed.
    pub(super) fn retry_unsaved(&mut self, cx: &mut Context<Self>) {
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
        let mut saved_recordings = Vec::new();
        detail.unsaved_recordings.retain(|id, recording| {
            let failed = recording
                .duration
                .is_some_and(|duration| notes.finish_recording(id, duration, now).is_err());
            if !failed {
                saved_recordings.push(id.clone());
            }
            failed
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
        for id in saved_recordings {
            self.transcribe(id, None, None, cx);
        }
    }

    /// Changes the Agenda's listed copy of the Quick note `id`, if it is listed.
    fn update_quick_note(&mut self, id: &NoteId, change: impl FnOnce(&mut Note)) {
        if let Some(mut note) = self.agenda.quick_note(id).cloned() {
            change(&mut note);
            self.agenda.note_changed(&note);
        }
    }

    pub(super) fn memo_changed(&mut self, cx: &mut Context<Self>) {
        let detail = &mut self.note_detail;
        detail.memo_dirty = true;
        detail.memo_save = cx.spawn(async move |view, cx| {
            cx.background_executor().timer(MEMO_SAVE_DELAY).await;
            let _ = view.update(cx, |view, cx| view.save_memo(cx));
        });
    }

    /// Writes the Memo field to the shown Note if it changed, then tries again what failed to
    /// save; a failure keeps the text and says so under the card.
    pub(super) fn save_memo(&mut self, cx: &mut Context<Self>) {
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
        self.retry_unsaved(cx);
        cx.notify();
    }

    /// Writes the title field to the shown Note if it changed, then tries again what failed to
    /// save; an empty title is none, and a failure keeps the title as the Memo's does.
    pub(super) fn save_title(&mut self, cx: &mut Context<Self>) {
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
        self.retry_unsaved(cx);
        cx.notify();
    }
}
