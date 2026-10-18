//! A stand-in sidebar that opens an Account row's context menu, for the menu tests in `menu`.

mod menu;

use super::super::account_dot::{srgb, tint};
use super::super::custom_color::CustomColor;
use super::super::menu::{OpenMenu, SUBMENU_CLOSE_DELAY};
use super::AccountAction::Recolor;
use super::account_row::account_menu;
use super::{AccountAction, AccountColor};
use gpui_kit::test::{TestSupportExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, Context, FocusHandle, FrameObserver, InteractiveElement,
    IntoElement, KeyBinding, MouseButton, MouseDownEvent, ParentElement, Render,
    StatefulInteractiveElement, Styled, Subscription, TestAppContext, Window, WindowBounds,
    WindowOptions, div, px, size,
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
                            let items = account_menu(color, custom, 0, 2, move |action, _, _| {
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
