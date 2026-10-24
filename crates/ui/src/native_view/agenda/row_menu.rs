//! The context menu of an Agenda row that has a Note: Open folder, which shows the Note's
//! folder in Finder with its audio selected, and Delete note, which asks first.

use super::super::NativeView;
use super::super::menu::{MenuItem, OpenMenu};
use super::row::row_id;
use crumble_calendar::agenda::Row;
use crumble_notes::{FolderEntry, NoteId};
use gpui_kit::component::WindowExt;
use gpui_kit::component::notification::Notification;
use gpui_kit::{Context, Pixels, Point, Window};

impl NativeView {
    /// Opens the context menu of `row` at `position`, when it is a Quick note's or an Event's
    /// that has a Note; an Event without one has none.
    pub(super) fn open_agenda_row_menu(
        &mut self,
        row: &Row,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(owner), Some(id)) = (row_id(row), self.row_note(row)) else {
            return;
        };
        let view = cx.entity().downgrade();
        let shown = view.clone();
        let deleted = id.clone();
        let items = vec![
            MenuItem::new("agenda-open-folder", "Open folder")
                .icon("folder")
                .on_select(move |window, cx| {
                    let _ = shown.update(cx, |view, cx| view.show_note_folder(&id, window, cx));
                }),
            MenuItem::separator(),
            MenuItem::new("agenda-delete-note", "Delete note")
                .icon("delete")
                .destructive()
                .on_select(move |window, cx| {
                    let _ = view.update(cx, |view, cx| {
                        view.confirm_delete_note(deleted.clone(), window, cx);
                    });
                }),
        ];
        self.menu = Some(OpenMenu::new(
            owner,
            items,
            position,
            self.focus.clone(),
            window,
            cx,
            |view: &mut NativeView| &mut view.menu,
        ));
    }

    /// The Note of `row`: its Quick note, or its Event occurrence's Note, if it has one. A
    /// failed read is logged.
    fn row_note(&mut self, row: &Row) -> Option<NoteId> {
        match row {
            Row::QuickNote(listed) => self
                .agenda
                .quick_notes
                .iter()
                .find(|note| note.id.to_string() == listed.id)
                .map(|note| note.id.clone()),
            Row::Event(event) | Row::Current(event) => self
                .notes
                .for_event(event)
                .inspect_err(|error| eprintln!("Could not read the Event's Note: {error}"))
                .ok()
                .flatten()
                .map(|note| note.id),
            _ => None,
        }
    }

    /// Shows the Note `id`'s folder in Finder, its first audio file selected, or opens it empty
    /// when the Note has none; a failure is logged and says so in a toast.
    fn show_note_folder(&mut self, id: &NoteId, window: &mut Window, cx: &mut Context<Self>) {
        match self.notes.folder_entry(id) {
            Ok(FolderEntry::File(file)) => cx.reveal_path(&file),
            Ok(FolderEntry::Folder(folder)) => cx.open_with_system(&folder),
            Err(error) => {
                eprintln!("Could not open the Note's folder: {error}");
                window.push_notification(Notification::error("Couldn't open the folder"), cx);
            }
        }
    }
}
