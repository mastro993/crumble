//! The sidebar's Calendars section: one row per Account, or the step that grants access.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use super::NativeView;
use super::account_alias::{Rename, account_name};
use super::account_dot::{check, dot, name, ring, srgb, tint};
use super::controls::app_icon;
use super::custom_color::CustomColor;
use super::menu::{MenuItem, OpenMenu};
use super::route::Route;
use super::sidebar_reorder::{RowDrag, RowDragPreview, grab_cursor};
use super::sidebar_row::{activate, hover_fill, row, row_with};
use crate::settings::{self, AccountColor, SidebarSection};
use crumble_eventkit::{Access, Account};
use crumble_theme::sidebar::{DETAIL_SIZE, ICON_GAP, INSET, ROW_GAP, ROW_HEIGHT, ROW_PADDING};
use gpui_kit::component::input::Input;
use gpui_kit::component::{ActiveTheme, Sizable, h_flex};
use gpui_kit::{
    AnyElement, App, AppContext, Context, Div, DragMoveEvent, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, ParentElement, Role, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Toggled, Window, div, prelude::FluentBuilder, px,
};

const PRIVACY_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Calendars";
const INTERNET_ACCOUNTS: &str =
    "x-apple.systempreferences:com.apple.Internet-Accounts-Settings.extension";

/// Muted copy sits a step below its label, as in SkillBinder.
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
    /// A hint that opens Settings, where every Account is Disabled.
    AllDisabled,
    /// One row per Account that is not Disabled; none until the first read finishes.
    Accounts,
}

/// `listed` counts the `accounts` that are not Disabled.
fn section(access: Access, loaded: bool, accounts: usize, listed: usize) -> Section {
    match access {
        Access::Unsupported => Section::Hidden,
        Access::NotDetermined => Section::Connect,
        Access::Denied => Section::AllowAccess,
        Access::Granted if loaded && accounts == 0 => Section::AddAccounts,
        Access::Granted if listed == 0 && accounts > 0 => Section::AllDisabled,
        Access::Granted => Section::Accounts,
    }
}

/// What an Account row's context menu asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccountAction {
    /// Edit the Account's Alias in place.
    Rename,
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
        let accounts = self.listed_accounts(sync.accounts());
        let rows: Vec<AnyElement> = match section(
            sync.access(),
            sync.loaded(),
            sync.accounts().len(),
            accounts.len(),
        ) {
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
            Section::AddAccounts => vec![
                hint(
                    "add-accounts",
                    "Add accounts in System Settings › Internet Accounts",
                    cx,
                    |_, _, cx| cx.open_url(INTERNET_ACCOUNTS),
                )
                .into_any_element(),
            ],
            Section::AllDisabled => vec![
                hint(
                    "all-accounts-disabled",
                    "All accounts disabled · Open Settings",
                    cx,
                    |view, _, cx| view.navigate(Route::Settings, cx),
                )
                .into_any_element(),
            ],
            Section::Accounts => {
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

    /// `accounts` that are not Disabled, in the user's order.
    fn listed_accounts<'a>(&self, accounts: &'a [Account]) -> Vec<&'a Account> {
        let mut listed = self.settings.ordered_accounts(accounts);
        listed.retain(|account| !self.settings.disabled_accounts.contains(&account.id));
        listed
    }

    /// The row shows a checkbox in the Account's color, checked unless the Account is Hidden,
    /// then its Alias, else its label. Clicking the row, or pressing Enter or Space while it has
    /// focus, Hides or Shows the Account. Dragging the row reorders the Accounts, and right-clicking it opens a
    /// menu that renames the Account, picks its color, and moves it. The row
    /// keeps its hover fill while its menu is open.
    fn account_row(
        &self,
        account: &Account,
        placement: Placement,
        cx: &Context<Self>,
    ) -> AnyElement {
        let color = self.settings.account_color(&account.id);
        let hidden = self.settings.hidden_accounts.contains(&account.id);
        if let Some(rename) = self
            .renaming
            .as_ref()
            .filter(|rename| rename.id == account.id)
        {
            return rename_row(rename, check(color, !hidden, cx), cx);
        }
        let label: SharedString = account_name(&self.settings, account).to_owned().into();
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

    /// The listed Account identifiers in their saved order.
    fn account_ids(&self, cx: &App) -> Vec<String> {
        self.listed_accounts(self.calendar_sync.read(cx).accounts())
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
            self.save_account_order(order, cx);
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
            self.save_account_order(order, cx);
        }
        cx.notify();
    }

    fn save_account_order(&mut self, order: Vec<String>, cx: &App) {
        let accounts = self.calendar_sync.read(cx).accounts();
        self.settings.set_listed_order(accounts, order);
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

    fn toggle_hidden(&mut self, id: &str, cx: &mut Context<Self>) {
        settings::toggle(&mut self.settings.hidden_accounts, id);
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

/// A muted line of copy that runs `action` when clicked.
fn hint(
    id: &'static str,
    text: &'static str,
    cx: &Context<NativeView>,
    action: impl Fn(&mut NativeView, &mut Window, &mut Context<NativeView>) + 'static,
) -> Stateful<Div> {
    let theme = cx.theme();
    let (muted, hover) = (
        theme.muted_foreground.opacity(MUTED_OPACITY),
        theme.foreground,
    );
    activate(
        div()
            .id(id)
            .px(px(ROW_PADDING))
            .py_1()
            .cursor_pointer()
            .text_size(px(DETAIL_SIZE))
            .text_color(muted)
            .hover(move |style| style.text_color(hover))
            .child(text),
        text,
        cx,
        action,
    )
}

/// An Account row's context menu, for the row at `index` of `len`; each choice runs `act`, and
/// the Color submenu's Custom row opens `custom` in a panel.
fn account_menu(
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
            MenuItem::new(format!("color-{}", name(color).to_lowercase()), name(color))
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

#[cfg(test)]
mod tests {
    use super::super::account_dot::{srgb, tint};
    use super::super::custom_color::CustomColor;
    use super::super::menu::SUBMENU_CLOSE_DELAY;
    use super::AccountAction::Recolor;
    use super::{Access, AccountAction, AccountColor, OpenMenu, Section, account_menu, section};
    use crumble_theme::menu::PADDING;
    use crumble_theme::menu::SUBMENU_OVERLAP;
    use gpui_kit::component::ColorName;
    use gpui_kit::test::{TestSupportExt, TestWindowExt};
    use gpui_kit::{
        AnyWindowHandle, AppContext, Bounds, Context, FocusHandle, FrameObserver,
        InteractiveElement, IntoElement, KeyBinding, MouseButton, MouseDownEvent, ParentElement,
        Render, StatefulInteractiveElement, Styled, Subscription, TestAppContext, Window,
        WindowBounds, WindowOptions, div, point, px, size,
    };
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    gpui_kit::actions!(
        test,
        [
            /// Stands in for a window shortcut such as Toggle Sidebar.
            Shortcut
        ]
    );

    #[test]
    fn each_access_state_picks_its_rows() {
        assert_eq!(section(Access::Unsupported, false, 0, 0), Section::Hidden);
        assert_eq!(
            section(Access::NotDetermined, false, 0, 0),
            Section::Connect
        );
        assert_eq!(section(Access::Denied, true, 2, 2), Section::AllowAccess);
        assert_eq!(section(Access::Granted, true, 0, 0), Section::AddAccounts);
        assert_eq!(section(Access::Granted, true, 2, 1), Section::Accounts);
        assert_eq!(section(Access::Granted, true, 2, 0), Section::AllDisabled);
        // No hint flashes before the first read finishes.
        assert_eq!(section(Access::Granted, false, 0, 0), Section::Accounts);
    }

    /// Stands in for the sidebar: the first of two Account rows, recording its menu's choices
    /// and the clicks that reach the row, and a field that goes away when it loses focus, as a
    /// Rename field does.
    struct Sidebar {
        focus: FocusHandle,
        menu: Option<OpenMenu>,
        /// The row's color, which its menu checks.
        color: AccountColor,
        actions: Rc<RefCell<Vec<AccountAction>>>,
        row_clicks: usize,
        /// Runs of a window shortcut.
        shortcuts: usize,
        field: Option<(FocusHandle, Subscription)>,
    }

    impl Render for Sidebar {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let (actions, color) = (self.actions.clone(), self.color);
            div()
                .id("sidebar")
                .test_support()
                .track_focus(&self.focus)
                .on_action(cx.listener(|sidebar, _: &Shortcut, _, _| sidebar.shortcuts += 1))
                .size_full()
                .child(
                    div()
                        .id("row")
                        .test_support()
                        .tab_index(0)
                        .h(px(32.))
                        .w(px(200.))
                        .child("Work")
                        .on_click(cx.listener(|sidebar, _, _, _| sidebar.row_clicks += 1))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |sidebar, event: &MouseDownEvent, window, cx| {
                                let actions = actions.clone();
                                // Picks as the sidebar's own Custom panel does.
                                let custom = CustomColor::new(
                                    tint(color, cx),
                                    window,
                                    cx,
                                    |sidebar: &mut Sidebar, picked, done, window, cx| {
                                        if let Some(picked) = picked {
                                            let color = AccountColor::Custom(srgb(picked));
                                            sidebar.actions.borrow_mut().push(Recolor(color));
                                        }
                                        if done && let Some(menu) = sidebar.menu.as_ref() {
                                            menu.close(window, cx);
                                        }
                                    },
                                );
                                let items =
                                    account_menu(color, custom, 0, 2, move |action, _, _| {
                                        actions.borrow_mut().push(action)
                                    });
                                sidebar.menu = Some(OpenMenu::new(
                                    "row",
                                    items,
                                    event.position,
                                    sidebar.focus.clone(),
                                    window,
                                    cx,
                                    |sidebar: &mut Sidebar| &mut sidebar.menu,
                                ));
                            }),
                        ),
                )
                .children(self.field.as_ref().map(|(focus, _)| {
                    div()
                        .id("field")
                        .test_support()
                        .track_focus(focus)
                        .h(px(32.))
                        .w(px(200.))
                }))
                .children(self.menu.as_ref().map(OpenMenu::render))
        }
    }

    /// Turns on the window's accessibility tree, which panics in debug builds when two elements
    /// claim focus, or two rows claim the highlight, in one frame.
    struct Accessibility;
    impl FrameObserver for Accessibility {}

    struct Harness {
        handle: AnyWindowHandle,
        sidebar: gpui_kit::Entity<Sidebar>,
        _accessibility: Arc<Accessibility>,
    }

    impl Harness {
        fn new(cx: &mut TestAppContext) -> Self {
            cx.update(|cx| {
                gpui_kit::init(cx);
                // The opening fade would otherwise stop at its first frame, as tests draw no
                // frames on their own.
                cx.set_reduce_motion(true);
                cx.bind_keys([KeyBinding::new("cmd-b", Shortcut, None)]);
            });
            let (handle, sidebar) = cx
                .update(|cx| {
                    gpui_kit::open_window(
                        WindowOptions {
                            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                                Default::default(),
                                size(px(900.), px(700.)),
                            ))),
                            ..Default::default()
                        },
                        cx,
                        |window, cx| {
                            cx.new(|cx| {
                                let focus = cx.focus_handle();
                                window.focus(&focus, cx);
                                let field = cx.focus_handle();
                                let ends =
                                    cx.on_blur(&field, window, |sidebar: &mut Sidebar, _, cx| {
                                        sidebar.field = None;
                                        cx.notify();
                                    });
                                Sidebar {
                                    focus,
                                    menu: None,
                                    color: AccountColor::Blue,
                                    actions: Rc::default(),
                                    row_clicks: 0,
                                    shortcuts: 0,
                                    field: Some((field, ends)),
                                }
                            })
                        },
                    )
                })
                .unwrap_or_else(|error| panic!("open test window: {error}"));
            let accessibility = Arc::new(Accessibility);
            let harness = Self {
                handle,
                sidebar,
                _accessibility: accessibility.clone(),
            };
            // Focus events only reach an active window.
            harness.step(cx, move |window, _| {
                window.observe_frames(&accessibility);
                window.activate_window();
            });
            harness
        }

        /// Draws a frame, runs `act`, then lets deferred work such as the menu's focus finish.
        fn step(&self, cx: &mut TestAppContext, act: impl FnOnce(&mut Window, &mut gpui_kit::App)) {
            cx.update_window(self.handle, |_, window, cx| {
                window.render_frame(cx);
                act(window, cx);
            })
            .unwrap_or_else(|error| panic!("update test window: {error}"));
            cx.run_until_parked();
            cx.update_window(self.handle, |_, window, cx| window.render_frame(cx))
                .unwrap_or_else(|error| panic!("update test window: {error}"));
        }

        fn check(&self, cx: &mut TestAppContext, check: impl FnOnce(&mut Window)) {
            cx.update_window(self.handle, |_, window, _| check(window))
                .unwrap_or_else(|error| panic!("read test window: {error}"));
        }

        fn actions(&self, cx: &mut TestAppContext) -> Vec<AccountAction> {
            cx.update(|cx| self.sidebar.read(cx).actions.borrow().clone())
        }

        fn wait(&self, cx: &mut TestAppContext) {
            cx.executor().advance_clock(SUBMENU_CLOSE_DELAY);
            self.step(cx, |_, _| {});
        }

        fn submenu_open(&self, cx: &mut TestAppContext) -> bool {
            let mut open = false;
            self.check(cx, |window| {
                open = window.try_find("menu-submenu").is_some()
            });
            open
        }

        fn open(&self, cx: &mut TestAppContext) {
            self.step(cx, |window, cx| window.right_click("row", cx));
            self.check(cx, |window| {
                assert_eq!(window.find("menu").focused(), Some(true));
            });
        }

        /// The menu is gone and focus is back where it was before the menu opened.
        fn assert_closed(&self, cx: &mut TestAppContext) {
            self.check(cx, |window| {
                assert!(window.try_find("menu").is_none());
                assert_eq!(window.find("sidebar").focused(), Some(true));
            });
        }
    }

    #[gpui_kit::test]
    fn choosing_a_row_closes_the_menu_and_runs_it(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        for item in ["rename", "move-down"] {
            harness.open(cx);
            harness.step(cx, |window, cx| {
                window.click(format!("menu-item-{item}"), cx)
            });
            harness.assert_closed(cx);
        }

        // Move Up is disabled on the first row, so choosing it does nothing.
        harness.open(cx);
        harness.check(cx, |window| {
            assert_eq!(window.find("menu-item-move-up").disabled(), Some(true));
        });
        harness.step(cx, |window, cx| window.click("menu-item-move-up", cx));
        harness.check(cx, |window| assert!(window.try_find("menu").is_some()));

        assert_eq!(
            harness.actions(cx),
            [AccountAction::Rename, AccountAction::MoveTo(1)]
        );
    }

    #[gpui_kit::test]
    fn the_color_submenu_opens_on_hover_and_checks_the_current_color(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.open(cx);
        harness.check(cx, |window| {
            assert!(window.try_find("menu-submenu").is_none())
        });

        harness.step(cx, |window, cx| window.hover("menu-item-color", cx));
        harness.check(cx, |window| {
            assert_eq!(window.find("menu-item-color").expanded(), Some(true));
            assert_eq!(window.find("menu-item-color-blue").checked(), Some(true));
            assert_eq!(window.find("menu-item-color-red").checked(), Some(false));
            // It opens to the right, overlapping the menu's edge, with its first row lined up
            // with the row it opened from.
            let (menu, submenu) = (
                window.find("menu").bounds(),
                window.find("menu-submenu").bounds(),
            );
            assert_eq!(submenu.left(), menu.right() - px(SUBMENU_OVERLAP));
            assert_eq!(
                window.find("menu-item-color-gray").bounds().top(),
                window.find("menu-item-color").bounds().top()
            );
        });
        harness.step(cx, |window, cx| window.click("menu-item-color-red", cx));

        harness.assert_closed(cx);
        assert_eq!(
            harness.actions(cx),
            [AccountAction::Recolor(AccountColor::Red)]
        );
    }

    #[gpui_kit::test]
    fn custom_opens_a_panel_beside_the_color_submenu_and_a_shade_picks_it(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.open(cx);
        harness.step(cx, |window, cx| window.hover("menu-item-color", cx));
        harness.step(cx, |window, cx| window.hover("menu-item-color-custom", cx));
        harness.check(cx, |window| {
            let custom = window.find("menu-item-color-custom");
            assert_eq!(custom.checked(), Some(false));
            assert_eq!(custom.expanded(), Some(true));
            // It opens to the right of the submenu, overlapping its edge, level with its row.
            let (submenu, panel) = (
                window.find("menu-submenu").bounds(),
                window.find("menu-panel").bounds(),
            );
            assert_eq!(panel.left(), submenu.right() - px(SUBMENU_OVERLAP));
            assert_eq!(panel.top() + px(PADDING), custom.bounds().top());
        });
        // Red's row, the second, at its sixth shade, 500: swatches are 20 pt apart by 4.
        let red = ColorName::Red.scale(500);
        let swatch = |index: f32| px(PADDING + index * 24. + 10.);
        harness.step(cx, |window, cx| {
            window.click_at("menu-panel", point(swatch(5.), swatch(1.)), cx)
        });

        harness.assert_closed(cx);
        assert_eq!(
            harness.actions(cx),
            [Recolor(AccountColor::Custom(srgb(red)))]
        );

        // Once picked, Custom is the checked color.
        let custom = AccountColor::Custom(0x3366ff);
        cx.update(|cx| {
            harness
                .sidebar
                .update(cx, |sidebar, _| sidebar.color = custom)
        });
        harness.open(cx);
        harness.step(cx, |window, cx| window.hover("menu-item-color", cx));
        harness.check(cx, |window| {
            assert_eq!(window.find("menu-item-color-custom").checked(), Some(true));
            assert_eq!(window.find("menu-item-color-blue").checked(), Some(false));
        });
    }

    #[gpui_kit::test]
    fn the_keyboard_opens_the_panel_and_a_hex_value_picks_it(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.open(cx);
        // Rename, Color, into the submenu on Gray, up to Custom, the last row, and into
        // its panel, which focuses the hex field.
        for key in ["down", "down", "right", "up", "right"] {
            harness.step(cx, |window, cx| window.press(key, cx));
        }
        harness.check(cx, |window| {
            assert!(window.try_find("menu-panel").is_some())
        });
        // The field's own keys stay in it, and editing does not close the menu.
        for key in ["cmd-a", "backspace", "left", "up"] {
            harness.step(cx, |window, cx| window.press(key, cx));
        }
        harness.step(cx, |window, cx| window.input("#3366FF", cx));
        harness.check(cx, |window| {
            assert!(window.try_find("menu-panel").is_some())
        });
        harness.step(cx, |window, cx| window.press("enter", cx));

        harness.assert_closed(cx);
        assert_eq!(
            harness.actions(cx),
            [Recolor(AccountColor::Custom(0x3366ff))]
        );
    }

    #[gpui_kit::test]
    fn escape_closes_the_panel_then_the_submenu_then_the_menu(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.open(cx);
        for key in ["down", "down", "right", "up", "right"] {
            harness.step(cx, |window, cx| window.press(key, cx));
        }
        harness.step(cx, |window, cx| window.press("escape", cx));
        harness.check(cx, |window| {
            assert!(window.try_find("menu-panel").is_none());
            assert!(window.try_find("menu-submenu").is_some());
            assert_eq!(window.find("menu").focused(), Some(true));
        });
        harness.step(cx, |window, cx| window.press("escape", cx));
        harness.check(cx, |window| {
            assert!(window.try_find("menu-submenu").is_none());
            assert!(window.try_find("menu").is_some());
        });
        harness.step(cx, |window, cx| window.press("escape", cx));
        harness.assert_closed(cx);
        assert!(harness.actions(cx).is_empty());
    }

    #[gpui_kit::test]
    fn the_menu_keeps_focus_when_the_panel_closes_while_its_field_has_it(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.open(cx);
        for key in ["down", "down", "right", "up", "right"] {
            harness.step(cx, |window, cx| window.press(key, cx));
        }
        harness.step(cx, |window, cx| window.hover("menu-item-color-pink", cx));
        harness.wait(cx);
        harness.check(cx, |window| {
            assert!(window.try_find("menu-panel").is_none());
            assert_eq!(window.find("menu").focused(), Some(true));
        });
    }

    #[gpui_kit::test]
    fn the_panel_closes_a_moment_after_the_pointer_leaves_its_row(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.open(cx);
        harness.step(cx, |window, cx| window.hover("menu-item-color", cx));
        harness.step(cx, |window, cx| window.hover("menu-item-color-custom", cx));
        harness.step(cx, |window, cx| window.hover("menu-item-color-pink", cx));
        harness.check(cx, |window| {
            assert!(window.try_find("menu-panel").is_some())
        });
        harness.wait(cx);
        harness.check(cx, |window| {
            assert!(window.try_find("menu-panel").is_none());
            assert!(window.try_find("menu-submenu").is_some());
        });

        // Reaching the panel in time keeps it open.
        harness.step(cx, |window, cx| window.hover("menu-item-color-custom", cx));
        harness.step(cx, |window, cx| window.hover("menu-item-color-pink", cx));
        harness.step(cx, |window, cx| window.hover("menu-panel", cx));
        harness.wait(cx);
        harness.check(cx, |window| {
            assert!(window.try_find("menu-panel").is_some())
        });
    }

    #[gpui_kit::test]
    fn the_keyboard_walks_the_menu_and_its_submenu(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.open(cx);
        // Rename, Color, then into the submenu, which highlights Gray; one down is Red.
        for key in ["down", "down", "right", "down", "enter"] {
            harness.step(cx, |window, cx| window.press(key, cx));
        }
        harness.assert_closed(cx);

        // Going up wraps to Move Down and skips the disabled Move Up to reach Color. Left and
        // Escape close the submenu, and Escape then the menu, choosing nothing.
        harness.open(cx);
        for key in ["up", "up", "right"] {
            harness.step(cx, |window, cx| window.press(key, cx));
        }
        harness.check(cx, |window| {
            assert!(window.try_find("menu-submenu").is_some())
        });
        // Escape closes only the submenu while one is open.
        for key in ["left", "escape"] {
            harness.step(cx, |window, cx| window.press(key, cx));
            harness.check(cx, |window| {
                assert!(window.try_find("menu-submenu").is_none());
                assert!(window.try_find("menu").is_some());
            });
            harness.step(cx, |window, cx| window.press("right", cx));
        }
        for _ in 0..2 {
            harness.step(cx, |window, cx| window.press("escape", cx));
        }
        harness.assert_closed(cx);

        assert_eq!(
            harness.actions(cx),
            [AccountAction::Recolor(AccountColor::Red)]
        );
    }

    #[gpui_kit::test]
    fn a_press_outside_the_menu_only_closes_it(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.open(cx);
        // Over the row, left of the menu, which opened at the row's centre.
        harness.step(cx, |window, cx| {
            window.click_at("menu-backdrop", point(px(20.), px(16.)), cx)
        });

        harness.assert_closed(cx);
        assert!(harness.actions(cx).is_empty());
        let row_clicks = cx.update(|cx| harness.sidebar.read(cx).row_clicks);
        assert_eq!(row_clicks, 0);
    }

    #[gpui_kit::test]
    fn the_submenu_closes_a_moment_after_the_pointer_leaves_its_row(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.open(cx);
        // Onto an enabled row, then onto the disabled Move Up.
        for row in ["menu-item-rename", "menu-item-move-up"] {
            harness.step(cx, |window, cx| window.hover("menu-item-color", cx));
            harness.step(cx, |window, cx| window.hover(row, cx));
            assert!(harness.submenu_open(cx));
            harness.wait(cx);
            assert!(!harness.submenu_open(cx));
        }

        // Reaching the submenu in time keeps it open.
        harness.step(cx, |window, cx| window.hover("menu-item-color", cx));
        harness.step(cx, |window, cx| window.hover("menu-item-rename", cx));
        harness.step(cx, |window, cx| window.hover("menu-item-color-red", cx));
        harness.wait(cx);
        assert!(harness.submenu_open(cx));
    }

    #[gpui_kit::test]
    fn a_shortcut_runs_and_closes_the_menu(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.open(cx);
        harness.step(cx, |window, cx| window.press("cmd-b", cx));

        harness.assert_closed(cx);
        let shortcuts = cx.update(|cx| harness.sidebar.read(cx).shortcuts);
        assert_eq!(shortcuts, 1);
    }

    #[gpui_kit::test]
    fn tab_closes_the_menu(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.open(cx);
        harness.step(cx, |window, cx| window.press("tab", cx));
        harness.check(cx, |window| assert!(window.try_find("menu").is_none()));
    }

    #[gpui_kit::test]
    fn focus_returns_to_the_view_when_what_had_it_is_gone(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.step(cx, |window, cx| window.click("field", cx));
        harness.check(cx, |window| {
            assert_eq!(window.find("field").focused(), Some(true))
        });
        // Taking focus ends the field.
        harness.open(cx);
        harness.check(cx, |window| assert!(window.try_find("field").is_none()));

        harness.step(cx, |window, cx| window.press("escape", cx));
        harness.assert_closed(cx);
    }
}
