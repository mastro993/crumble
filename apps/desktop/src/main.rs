//! Native desktop lifecycle: menus, key bindings, and the main window.

use crumble_ui::{GoBack, GoForward, ToggleSidebar};
use gpui_kit::component::TitleBar;
use gpui_kit::{
    App, AppContext, Bounds, KeyBinding, Menu, MenuItem, TitlebarOptions, WindowBounds,
    WindowOptions, actions, point, px, size,
};

actions!(
    crumble,
    [
        /// Quits the native application.
        Quit
    ]
);

/// macOS draws the app under a transparent titlebar; other platforms keep native decorations.
fn window_chrome() -> WindowOptions {
    if cfg!(target_os = "macos") {
        let base = TitleBar::window_options();
        WindowOptions {
            titlebar: base.titlebar.map(|titlebar| {
                let (x, y) = crumble_theme::TRAFFIC_LIGHTS_ORIGIN;
                TitlebarOptions {
                    title: Some("Crumble".into()),
                    traffic_light_position: Some(point(px(x), px(y))),
                    ..titlebar
                }
            }),
            ..base
        }
    } else {
        WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Crumble".into()),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}

fn main() -> std::process::ExitCode {
    if let Err(error) = run() {
        eprintln!("Could not start Crumble: {error}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

fn run() -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    anyhow::ensure!(
        gpui_kit::guess_compositor() != "Headless",
        "Crumble requires a Wayland or X11 desktop session."
    );
    gpui_kit::application()
        .with_assets(crumble_ui::Assets)
        .run(|cx: &mut App| {
            crumble_ui::init(cx);
            cx.on_action(|_: &Quit, cx| cx.quit());
            let (quit_key, sidebar_key, back_key, forward_key) = if cfg!(target_os = "macos") {
                ("cmd-q", "cmd-b", "cmd-[", "cmd-]")
            } else {
                ("ctrl-q", "ctrl-b", "ctrl-[", "ctrl-]")
            };
            cx.bind_keys([
                KeyBinding::new(quit_key, Quit, None),
                KeyBinding::new(sidebar_key, ToggleSidebar, None),
                KeyBinding::new(back_key, GoBack, None),
                KeyBinding::new(forward_key, GoForward, None),
            ]);
            cx.set_menus(vec![
                Menu {
                    name: "Crumble".into(),
                    disabled: false,
                    items: vec![MenuItem::action("Quit Crumble", Quit)],
                },
                Menu {
                    name: "View".into(),
                    disabled: false,
                    items: vec![MenuItem::action("Toggle Sidebar", ToggleSidebar)],
                },
                Menu {
                    name: "Go".into(),
                    disabled: false,
                    items: vec![
                        MenuItem::action("Back", GoBack),
                        MenuItem::action("Forward", GoForward),
                    ],
                },
            ]);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let mut bounds = Bounds::centered(None, size(px(1180.0), px(728.0)), cx);
            bounds.origin.x = px(f32::from(bounds.origin.x).floor());
            bounds.origin.y = px(f32::from(bounds.origin.y).floor());
            let options = WindowOptions {
                app_id: Some("crumble".into()),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(860.0), px(588.0))),
                ..window_chrome()
            };
            if let Err(error) = gpui_kit::open_window(options, cx, |window, cx| {
                crumble_theme::follow_window(window, cx);
                cx.new(|cx| crumble_ui::NativeView::new(window, cx))
            }) {
                eprintln!("Could not open Crumble: {error}");
                cx.quit();
            }
            cx.activate(true);
        });
    Ok(())
}
