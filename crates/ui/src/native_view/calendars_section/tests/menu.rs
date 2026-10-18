//! The Account row's context menu: choosing, submenus, the Custom panel, the keyboard, and focus.

use super::super::super::account_dot::srgb;
use super::super::AccountAction::Recolor;
use super::super::{AccountAction, AccountColor};
use super::Harness;
use crumble_theme::menu::PADDING;
use crumble_theme::menu::SUBMENU_OVERLAP;
use gpui_kit::component::ColorName;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{TestAppContext, point, px};

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
