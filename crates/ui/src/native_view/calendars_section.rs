//! The sidebar's Calendars section: one row per Account, or the step that grants access.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use super::NativeView;
use super::account_dot::{dot, name};
use super::controls::app_icon;
use super::sidebar_reorder::{RowDrag, RowDragPreview, grab_cursor};
use super::sidebar_row::{activate, row, row_with};
use crate::settings::{self, AccountColor, SidebarSection};
use crumble_eventkit::{Access, Account};
use crumble_theme::sidebar::{DETAIL_SIZE, ICON_GAP, INSET, ROW_GAP, ROW_PADDING};
use gpui_kit::component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme, Side, h_flex};
use gpui_kit::{
    AnyElement, App, AppContext, Context, Div, DragMoveEvent, InteractiveElement, IntoElement,
    MouseButton, ParentElement, SharedString, Stateful, StatefulInteractiveElement, Styled, Window,
    div, prelude::FluentBuilder, px,
};

const PRIVACY_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Calendars";
const INTERNET_ACCOUNTS: &str =
    "x-apple.systempreferences:com.apple.Internet-Accounts-Settings.extension";

/// A Disabled Account's row is dimmed to this opacity.
const DISABLED_OPACITY: f32 = 0.5;
/// Muted copy and menu icons sit a step below their label, as in SkillBinder.
const MUTED_OPACITY: f32 = 0.65;

/// What the Calendars section shows.
#[derive(Debug, PartialEq, Eq)]
enum Section {
    /// No EventKit on this platform.
    Hidden,
    /// One row that asks for access.
    Connect,
    /// One row that opens Privacy settings, the only place a refusal can be undone.
    AllowAccess,
    /// A hint that opens Internet Accounts settings.
    AddAccounts,
    /// One row per Account; none until the first read finishes.
    Accounts,
}

fn section(access: Access, loaded: bool, accounts: usize) -> Section {
    match access {
        Access::Unsupported => Section::Hidden,
        Access::NotDetermined => Section::Connect,
        Access::Denied => Section::AllowAccess,
        Access::Granted if loaded && accounts == 0 => Section::AddAccounts,
        Access::Granted => Section::Accounts,
    }
}

/// What an Account row's context menu asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccountAction {
    /// Disable an enabled Account, or Enable a Disabled one.
    ToggleDisabled,
    /// Give the dot this color.
    Recolor(AccountColor),
    /// Move the row to this index.
    MoveTo(usize),
}

/// An Account's label: its email when EventKit gives it away, else its System Settings name.
pub(super) fn account_label(account: &Account) -> &str {
    account.email.as_deref().unwrap_or(&account.name)
}

impl NativeView {
    /// The section title, which folds the rows, and the rows; `None` without EventKit.
    pub(super) fn render_calendars_section(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.account_reorder.moving(Instant::now()) {
            window.request_animation_frame();
        }
        let sync = self.calendar_sync.read(cx);
        let rows: Vec<AnyElement> =
            match section(sync.access(), sync.loaded(), sync.accounts().len()) {
                Section::Hidden => return None,
                Section::Connect => vec![
                    activate(
                        row(
                            "connect-calendars",
                            app_icon("calendar"),
                            "Connect calendars",
                            false,
                            cx,
                        ),
                        "Connect calendars",
                        cx,
                        |view, _, cx| view.calendar_sync.read(cx).request_access(),
                    )
                    .into_any_element(),
                ],
                Section::AllowAccess => vec![
                    activate(
                        row(
                            "allow-calendar-access",
                            app_icon("calendar"),
                            "Allow calendar access",
                            false,
                            cx,
                        ),
                        "Allow calendar access",
                        cx,
                        |_, _, cx| cx.open_url(PRIVACY_SETTINGS),
                    )
                    .into_any_element(),
                ],
                Section::AddAccounts => vec![self.add_accounts_hint(cx).into_any_element()],
                Section::Accounts => {
                    let accounts = self.settings.ordered_accounts(sync.accounts());
                    let stored: Vec<String> =
                        accounts.iter().map(|account| account.id.clone()).collect();
                    let (order, floating) = self
                        .account_reorder
                        .displayed(&stored, cx.has_active_drag());
                    let now = Instant::now();
                    order
                        .iter()
                        .filter_map(|id| accounts.iter().find(|account| &account.id == id))
                        .enumerate()
                        .map(|(index, account)| {
                            let placement = Placement {
                                index,
                                len: order.len(),
                                offset: self.account_reorder.offset(&account.id, now),
                                floating: floating.as_ref() == Some(&account.id),
                            };
                            self.account_row(account, placement, cx)
                        })
                        .collect()
                }
            };
        let items = rows.len().max(1);
        Some(
            div()
                .flex()
                .flex_col()
                .child(self.section_header(SidebarSection::Calendars, "Calendars", cx))
                .children(
                    self.section_rows(
                        SidebarSection::Calendars,
                        items,
                        div()
                            .id("sidebar-calendars")
                            .flex()
                            .flex_col()
                            .gap(px(ROW_GAP))
                            // Drag moves are measured from the first row's slot.
                            .on_drag_move(cx.listener(
                                |view, event: &DragMoveEvent<RowDrag<String>>, window, cx| {
                                    let drag = event.drag(cx);
                                    let top = drag.top(event.event.position, event.bounds);
                                    view.drag_account(drag.id.clone(), top, window, cx)
                                },
                            ))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|view, _, _, cx| view.finish_account_drag(cx)),
                            )
                            .on_mouse_up_out(
                                MouseButton::Left,
                                cx.listener(|view, _, _, cx| view.finish_account_drag(cx)),
                            )
                            .children(rows),
                    ),
                )
                .into_any_element(),
        )
    }

    fn add_accounts_hint(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        const HINT: &str = "Add accounts in System Settings › Internet Accounts";
        let theme = cx.theme();
        let (muted, hover) = (
            theme.muted_foreground.opacity(MUTED_OPACITY),
            theme.foreground,
        );
        activate(
            div()
                .id("add-accounts")
                .px(px(ROW_PADDING))
                .py_1()
                .cursor_pointer()
                .text_size(px(DETAIL_SIZE))
                .text_color(muted)
                .hover(move |style| style.text_color(hover))
                .child(HINT),
            HINT,
            cx,
            |_, _, cx| cx.open_url(INTERNET_ACCOUNTS),
        )
    }

    /// Clicking the row does nothing; dragging it reorders the Accounts, and right-clicking it
    /// opens a menu that Disables or Enables the Account, picks its dot color, and moves it.
    fn account_row(
        &self,
        account: &Account,
        placement: Placement,
        cx: &Context<Self>,
    ) -> AnyElement {
        let disabled = self.settings.disabled_accounts.contains(&account.id);
        let color = self.settings.account_color(&account.id);
        let label: SharedString = account_label(account).to_owned().into();
        let (view, id) = (cx.entity().downgrade(), account.id.clone());
        let act = move |action, cx: &mut App| {
            let _ = view.update(cx, |view, cx| match action {
                AccountAction::ToggleDisabled => view.toggle_account(&id, cx),
                AccountAction::Recolor(color) => view.set_account_color(&id, color, cx),
                AccountAction::MoveTo(index) => view.move_account(&id, index, cx),
            });
        };
        let menu = account_menu(disabled, color, placement.index, placement.len, act);
        row_with(
            format!("account-{}", account.id),
            dot(color, cx),
            label.clone(),
            false,
            cx,
        )
        .cursor_default()
        .when(disabled, |row| row.opacity(DISABLED_OPACITY))
        .relative()
        .top(px(placement.offset))
        // The floating preview stands in for the dragged row.
        .when(placement.floating, |row| row.opacity(0.0))
        .on_drag(
            RowDrag {
                id: account.id.clone(),
                color,
                label,
                width: self.sidebar_width - 2.0 * INSET,
                grab: Cell::new(0.0),
            },
            |drag, offset, _, cx| {
                drag.grab.set(f32::from(offset.y));
                cx.new(|_| RowDragPreview::new(drag))
            },
        )
        .context_menu(menu)
        .into_any_element()
    }

    /// The Account identifiers in their saved order.
    fn account_ids(&self, cx: &App) -> Vec<String> {
        self.settings
            .ordered_accounts(self.calendar_sync.read(cx).accounts())
            .into_iter()
            .map(|account| account.id.clone())
            .collect()
    }

    fn drag_account(&mut self, id: String, top: f32, window: &mut Window, cx: &mut Context<Self>) {
        grab_cursor(window, cx);
        let stored = self.account_ids(cx);
        if self
            .account_reorder
            .drag(&stored, &id, top, !cx.reduce_motion())
        {
            cx.notify();
        }
    }

    /// Saves the order shown when the pointer is released.
    fn finish_account_drag(&mut self, cx: &mut Context<Self>) {
        let stored = self.account_ids(cx);
        let (released, animate) = (cx.has_active_drag(), !cx.reduce_motion());
        if let Some(order) = self.account_reorder.finish(&stored, released, animate) {
            self.save_account_order(order);
        }
        cx.notify();
    }

    /// Moves `id` to `target`, sliding the rows it passes.
    fn move_account(&mut self, id: &str, target: usize, cx: &mut Context<Self>) {
        let stored = self.account_ids(cx);
        let animate = !cx.reduce_motion();
        if let Some(order) = self
            .account_reorder
            .move_to(&stored, &id.to_owned(), target, animate)
        {
            self.save_account_order(order);
        }
        cx.notify();
    }

    fn save_account_order(&mut self, order: Vec<String>) {
        self.settings.account_order = order;
        self.settings.save();
    }

    fn set_account_color(&mut self, id: &str, color: AccountColor, cx: &mut Context<Self>) {
        self.settings.account_colors.insert(id.to_owned(), color);
        self.settings.save();
        cx.notify();
    }

    pub(super) fn toggle_account(&mut self, id: &str, cx: &mut Context<Self>) {
        settings::toggle(&mut self.settings.disabled_accounts, id);
        self.settings.save();
        self.refresh_events(false, cx);
        cx.notify();
    }

    pub(super) fn toggle_calendar(&mut self, id: &str, cx: &mut Context<Self>) {
        settings::toggle(&mut self.settings.disabled_calendars, id);
        self.settings.save();
        self.refresh_events(false, cx);
        cx.notify();
    }
}

/// Where an Account row sits in the list, and how it is drawn while rows move.
struct Placement {
    index: usize,
    len: usize,
    /// How far the row is drawn from its slot while it slides.
    offset: f32,
    /// The row is being dragged, so its floating preview stands in for it.
    floating: bool,
}

/// An Account row's context menu, for the row at `index` of `len`; each choice runs `act`.
fn account_menu(
    disabled: bool,
    current: AccountColor,
    index: usize,
    len: usize,
    act: impl Fn(AccountAction, &mut App) + 'static,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    let act = Rc::new(act);
    move |menu, window, cx| {
        let on = |action| {
            let act = act.clone();
            move |_: &_, _: &mut Window, cx: &mut App| act(action, cx)
        };
        let (label, icon) = if disabled {
            ("Enable", "view")
        } else {
            ("Disable", "view-off")
        };
        let plain = cx.theme().muted_foreground.opacity(MUTED_OPACITY);
        let menu_icon = |name| app_icon(name).text_color(plain).mr(px(ICON_GAP - 4.0));
        let recolor = act.clone();
        menu.item(
            PopupMenuItem::new(label)
                .icon(menu_icon(icon))
                .on_click(on(AccountAction::ToggleDisabled)),
        )
        .submenu_with_icon(
            Some(menu_icon("paint-board")),
            "Color",
            window,
            cx,
            move |menu, _, _| {
                // Each choice shows its dot, so the check moves to the right.
                AccountColor::ALL
                    .into_iter()
                    .fold(menu.check_side(Side::Right), |menu, color| {
                        let recolor = recolor.clone();
                        menu.item(
                            PopupMenuItem::element(move |_, cx| {
                                h_flex()
                                    .gap(px(ICON_GAP - 4.0))
                                    .child(dot(color, cx))
                                    .child(name(color))
                            })
                            .checked(color == current)
                            .on_click(move |_, _, cx| recolor(AccountAction::Recolor(color), cx)),
                        )
                    })
            },
        )
        .separator()
        .item(
            PopupMenuItem::new("Move Up")
                .icon(menu_icon("arrow-up"))
                .disabled(index == 0)
                .on_click(on(AccountAction::MoveTo(index.saturating_sub(1)))),
        )
        .item(
            PopupMenuItem::new("Move Down")
                .icon(menu_icon("arrow-down"))
                .disabled(index + 1 >= len)
                .on_click(on(AccountAction::MoveTo(index + 1))),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Access, Account, AccountAction, AccountColor, Section, account_menu, section};
    use crumble_eventkit::Provider;
    use gpui_kit::component::menu::ContextMenuExt;
    use gpui_kit::test::{TestSupportExt, TestWindowExt};
    use gpui_kit::{
        AppContext, Bounds, Context, InteractiveElement, IntoElement, ParentElement, Render,
        Styled, TestAppContext, Window, WindowBounds, WindowOptions, div, px, size,
    };
    use std::rc::Rc;

    #[test]
    fn each_access_state_picks_its_rows() {
        assert_eq!(section(Access::Unsupported, false, 0), Section::Hidden);
        assert_eq!(section(Access::NotDetermined, false, 0), Section::Connect);
        assert_eq!(section(Access::Denied, true, 2), Section::AllowAccess);
        assert_eq!(section(Access::Granted, true, 0), Section::AddAccounts);
        assert_eq!(section(Access::Granted, true, 2), Section::Accounts);
        // No hint flashes before the first read finishes.
        assert_eq!(section(Access::Granted, false, 0), Section::Accounts);
    }

    /// Stands in for the sidebar: the first of two Account rows, recording its menu's choices.
    struct Sidebar {
        account: Account,
        actions: Rc<std::cell::RefCell<Vec<AccountAction>>>,
    }

    impl Render for Sidebar {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let actions = self.actions.clone();
            div()
                .id("sidebar")
                .size_full()
                .child(
                    // Fills the window: wrapped in its context menu, the row is no longer
                    // observable by ID, so the test right-clicks the window's centre.
                    div()
                        .id("row")
                        .size_full()
                        .child(self.account.name.clone())
                        .context_menu(account_menu(
                            false,
                            AccountColor::Blue,
                            0,
                            2,
                            move |action, _| actions.borrow_mut().push(action),
                        )),
                )
                .test_support()
        }
    }

    #[gpui_kit::test]
    fn the_account_menu_disables_and_moves_the_account(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let actions = Rc::new(std::cell::RefCell::new(Vec::new()));
        let account = Account {
            id: "source-1".into(),
            name: "Work".into(),
            email: None,
            provider: Provider::Google,
        };
        let sidebar_actions = actions.clone();
        let (handle, _) = cx
            .update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            Default::default(),
                            size(px(400.), px(300.)),
                        ))),
                        ..Default::default()
                    },
                    cx,
                    |_, cx| {
                        cx.new(|_| Sidebar {
                            account,
                            actions: sidebar_actions,
                        })
                    },
                )
            })
            .unwrap_or_else(|error| panic!("open test window: {error}"));

        let step = |cx: &mut TestAppContext, act: &dyn Fn(&mut Window, &mut gpui_kit::App)| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                act(window, cx);
            })
            .unwrap_or_else(|error| panic!("update test window: {error}"));
            // The menu is built in a deferred callback.
            cx.run_until_parked();
        };
        // Items: Disable, Color, a separator, Move Up, Move Down.
        for item in [0usize, 3, 4] {
            step(cx, &|window, cx| window.right_click("sidebar", cx));
            step(cx, &|window, cx| window.click(item, cx));
        }

        // Move Up is disabled on the first row, so it records nothing.
        assert_eq!(
            *actions.borrow(),
            [AccountAction::ToggleDisabled, AccountAction::MoveTo(1)]
        );
    }
}
