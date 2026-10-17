//! The Note actions in the page header: a "…" button opening a menu with Delete recording when
//! the Note has one, and Delete note.

use super::super::NativeView;
use super::super::controls::{app_icon, button};
use super::super::menu::{MenuItem, OpenMenu};
use gpui_kit::component::button::{ButtonVariant, ButtonVariants};
use gpui_kit::component::{Sizable, WindowExt};
use gpui_kit::{
    AnyElement, App, Context, IntoElement, ParentElement, Styled, WeakEntity, Window, canvas, div,
    point, px,
};

/// The menu's element owner, so only one Note actions menu is open.
const OWNER: &str = "note-actions";
/// Space between the button and its menu.
const MENU_GAP: f32 = 4.0;

impl NativeView {
    /// The Note actions button; `None` while there is no Note, as the card then offers New note
    /// itself.
    pub(in super::super) fn render_note_actions(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let detail = &self.note_detail;
        detail.note.as_ref()?;
        let bounds = detail.actions.clone();
        let trigger = button(OWNER)
            .ghost()
            .small()
            .icon(app_icon("more").size_4())
            .tooltip("Note actions")
            .on_click(cx.listener(|view, _, window, cx| view.open_note_menu(window, cx)));
        Some(
            div()
                .relative()
                .flex_shrink_0()
                .child(trigger)
                .child(
                    canvas(move |drawn, _, _| bounds.set(drawn), |_, _, _, _| {})
                        .absolute()
                        .inset_0(),
                )
                .into_any_element(),
        )
    }

    /// Opens the Note actions menu below its button: Delete recording when the shown Note has a
    /// finished one, and Delete note.
    fn open_note_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let detail = &self.note_detail;
        let Some(shown) = detail.note.as_ref().map(|note| note.id.clone()) else {
            return;
        };
        let view = cx.entity().downgrade();
        let mut items = Vec::new();
        let recorded = detail
            .note
            .as_ref()
            .is_some_and(|note| note.recording.is_some() && !self.is_recording(&note.id));
        if recorded {
            let id = shown.clone();
            items.push(
                MenuItem::new("note-delete-recording", "Delete recording")
                    .icon("delete")
                    .on_select(act(&view, move |view, window, cx| {
                        view.confirm_delete_recording(id.clone(), window, cx)
                    })),
            );
        }
        items.push(
            MenuItem::new("note-delete", "Delete note")
                .icon("delete")
                .on_select(act(&view, move |view, window, cx| {
                    view.confirm_delete_note(shown.clone(), window, cx)
                })),
        );
        let bounds = detail.actions.get();
        self.menu = Some(OpenMenu::new(
            OWNER,
            items,
            point(bounds.left(), bounds.bottom() + px(MENU_GAP)),
            self.focus.clone(),
            window,
            cx,
            |view: &mut NativeView| &mut view.menu,
        ));
    }

    /// Asks before deleting the Note `id`, as nothing brings it back.
    fn confirm_delete_note(
        &mut self,
        id: crumble_notes::NoteId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (view, id) = (view.clone(), id.clone());
            alert
                .title("Delete this note?")
                .description("Its title, Memo and Recording are deleted. This can't be undone.")
                .show_cancel(true)
                .ok_text("Delete")
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| view.delete_note(&id, window, cx));
                    true
                })
        });
    }
}

/// A menu row's handler running `action` on the view, if it is still open.
fn act(
    view: &WeakEntity<NativeView>,
    action: impl Fn(&mut NativeView, &mut Window, &mut Context<NativeView>) + 'static,
) -> impl Fn(&mut Window, &mut App) + 'static {
    let view = view.clone();
    move |window, cx| {
        let _ = view.update(cx, |view, cx| action(view, window, cx));
    }
}
