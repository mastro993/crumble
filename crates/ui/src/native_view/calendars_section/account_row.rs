//! An Account row: its checkbox, label, drag, and context menu, or the field that renames it.

use std::cell::Cell;
use std::rc::Rc;

use super::super::NativeView;
use super::super::account_alias::Rename;
use super::super::account_dot::{check, dot, ring, srgb, tint};
use super::super::custom_color::CustomColor;
use super::super::menu::{MenuItem, OpenMenu};
use super::super::sidebar_reorder::{RowDrag, RowDragPreview};
use super::super::sidebar_row::{activate, hover_fill, row_with};
use super::{AccountAction, Placement};
use crumble_eventkit::Account;
use crumble_store::AccountColor;
use crumble_theme::sidebar::{ICON_GAP, INSET, ROW_HEIGHT, ROW_PADDING};
use gpui_kit::component::input::Input;
use gpui_kit::component::{Sizable, h_flex};
use gpui_kit::{
    AnyElement, App, AppContext, Context, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, Role, SharedString, StatefulInteractiveElement, Styled, Toggled,
    Window, prelude::FluentBuilder, px,
};

impl NativeView {
    /// The row shows a checkbox in the Account's color, checked unless the Account is Hidden,
    /// then its Alias, else its label. Clicking the row, or pressing Enter or Space while it has
    /// focus, Hides or Shows the Account. Dragging the row reorders the Accounts, and right-clicking it opens a
    /// menu that renames the Account, picks its color, and moves it. The row
    /// keeps its hover fill while its menu is open.
    pub(super) fn account_row(
        &self,
        account: &Account,
        placement: Placement,
        cx: &Context<Self>,
    ) -> AnyElement {
        let color = self.store.account_color(&account.id);
        let hidden = self.store.is_account_hidden(&account.id);
        if let Some(rename) = self
            .renaming
            .as_ref()
            .filter(|rename| rename.id == account.id)
        {
            return rename_row(rename, check(color, !hidden, cx), cx);
        }
        let label: SharedString = self.store.account_name(account).to_owned().into();
        let (view, id) = (cx.entity().downgrade(), account.id.clone());
        let act = move |action: AccountAction, window: &mut Window, cx: &mut App| {
            let _ = view.update(cx, |view, cx| match action {
                AccountAction::Rename => view.start_rename(&id, window, cx),
                AccountAction::Recolor(color) => view.set_account_color(&id, color, cx),
                AccountAction::MoveTo(index) => view.move_account(&id, index, cx),
            });
        };
        let owner: SharedString = format!("account-{}", account.id).into();
        let menu_open = self.menu.as_ref().is_some_and(|menu| menu.is_for(&owner));
        let (index, len) = (placement.index, placement.len);
        let recolored = account.id.clone();
        let id = account.id.clone();
        // A drag never ends in a click.
        activate(
            row_with(
                owner.clone(),
                check(color, !hidden, cx),
                label.clone(),
                false,
                cx,
            ),
            label.clone(),
            cx,
            move |view, _, cx| view.toggle_hidden(&id, cx),
        )
        .role(Role::CheckBox)
        .aria_toggled(if hidden {
            Toggled::False
        } else {
            Toggled::True
        })
        .when(menu_open, |row| row.bg(hover_fill(cx)))
        .relative()
        .top(px(placement.offset))
        // The floating preview stands in for the dragged row.
        .when(placement.floating, |row| row.opacity(0.0))
        .on_drag(
            RowDrag {
                id: account.id.clone(),
                color,
                checked: !hidden,
                label,
                width: self.sidebar_width - 2.0 * INSET,
                grab: Cell::new(0.0),
            },
            |drag, offset, _, cx| {
                drag.grab.set(f32::from(offset.y));
                cx.new(|_| RowDragPreview::new(drag))
            },
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                let id = recolored.clone();
                let custom = CustomColor::new(
                    tint(color, cx),
                    window,
                    cx,
                    move |view: &mut NativeView, picked, done, window, cx| {
                        if let Some(picked) = picked {
                            let color = AccountColor::Custom(srgb(picked));
                            view.set_account_color(&id, color, cx);
                        }
                        if done && let Some(menu) = view.menu.as_ref() {
                            menu.close(window, cx);
                        }
                    },
                );
                let items = account_menu(color, custom, index, len, act.clone());
                view.menu = Some(OpenMenu::new(
                    owner.clone(),
                    items,
                    event.position,
                    view.focus.clone(),
                    window,
                    cx,
                    |view: &mut NativeView| &mut view.menu,
                ));
            }),
        )
        .into_any_element()
    }
}

/// An Account row turned into a field for its Alias; Enter or leaving it saves, Escape cancels.
fn rename_row(rename: &Rename, leading: impl IntoElement, cx: &Context<NativeView>) -> AnyElement {
    h_flex()
        .id(SharedString::from(format!("rename-{}", rename.id)))
        .flex_shrink_0()
        .gap(px(ICON_GAP))
        .h(px(ROW_HEIGHT))
        .px(px(ROW_PADDING))
        .capture_key_down(cx.listener(NativeView::rename_key_down))
        .child(leading)
        .child(Input::new(&rename.input).small())
        .into_any_element()
}

/// An Account row's context menu, for the row at `index` of `len`; each choice runs `act`, and
/// the Color submenu's Custom row opens `custom` in a panel.
pub(super) fn account_menu(
    current: AccountColor,
    custom: CustomColor,
    index: usize,
    len: usize,
    act: impl Fn(AccountAction, &mut Window, &mut App) + 'static,
) -> Vec<MenuItem> {
    let act = Rc::new(act);
    let on = |action| {
        let act = act.clone();
        move |window: &mut Window, cx: &mut App| act(action, window, cx)
    };
    // Each color leads with its dot, so the current one is checked at the trailing edge. Custom
    // leads with the custom color once there is one.
    let mut colors: Vec<MenuItem> = AccountColor::PALETTE
        .into_iter()
        .map(|color| {
            MenuItem::new(
                format!("color-{}", color.name().to_lowercase()),
                color.name(),
            )
            .leading(move |cx| dot(color, cx).into_any_element())
            .checked(color == current)
            .on_select(on(AccountAction::Recolor(color)))
        })
        .collect();
    let is_custom = matches!(current, AccountColor::Custom(_));
    colors.extend([
        MenuItem::separator(),
        MenuItem::new("color-custom", "Custom")
            .leading(move |cx| {
                if is_custom {
                    dot(current, cx)
                } else {
                    ring(cx)
                }
                .into_any_element()
            })
            .checked(is_custom)
            .panel(custom.focus().clone(), move |window, cx| {
                custom.panel(window, cx)
            }),
    ]);
    vec![
        MenuItem::new("rename", "Rename…")
            .icon("pencil")
            .on_select(on(AccountAction::Rename)),
        MenuItem::new("color", "Color")
            .icon("paint-board")
            .submenu(colors),
        MenuItem::separator(),
        MenuItem::new("move-up", "Move Up")
            .icon("arrow-up")
            .disabled(index == 0)
            .on_select(on(AccountAction::MoveTo(index.saturating_sub(1)))),
        MenuItem::new("move-down", "Move Down")
            .icon("arrow-down")
            .disabled(index + 1 >= len)
            .on_select(on(AccountAction::MoveTo(index + 1))),
    ]
}
