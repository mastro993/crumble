mod controls;
mod route;
mod shell;
mod sidebar;
mod sidebar_row;

pub use shell::{GoBack, GoForward, ToggleSidebar};

use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    Context, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render,
    Styled, Window, div,
};
use route::{History, Route};

/// GPUI owner of the application shell and navigation.
pub struct NativeView {
    focus: FocusHandle,
    route: Route,
    history: History,
    sidebar_width: f32,
    sidebar_collapsed: bool,
    /// Sidebar toggle count; zero renders without motion, so launch does not animate.
    sidebar_motion: u64,
    /// Collapsed sidebar still mounted for its closing motion.
    sidebar_closing: bool,
}

impl NativeView {
    /// Creates the root view and focuses it.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Window-level shortcuts such as ToggleSidebar dispatch through the focused view.
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            focus,
            route: Route::Home,
            history: History::default(),
            sidebar_width: sidebar::DEFAULT_WIDTH,
            sidebar_collapsed: false,
            sidebar_motion: 0,
            sidebar_closing: false,
        }
    }

    /// Opens `route` and records the current one for Back.
    fn navigate(&mut self, route: Route, cx: &mut Context<Self>) {
        if self.route != route {
            self.history.push(std::mem::replace(&mut self.route, route));
        }
        cx.notify();
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        if let Some(route) = self.history.back(self.route) {
            self.route = route;
            cx.notify();
        }
    }

    fn go_forward(&mut self, cx: &mut Context<Self>) {
        if let Some(route) = self.history.forward(self.route) {
            self.route = route;
            cx.notify();
        }
    }
}

impl Focusable for NativeView {
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for NativeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = div().text_xl().child(self.route.title()).into_any_element();
        let colors = cx.theme().colors;
        div()
            .track_focus(&self.focus)
            .on_action(cx.listener(|view, _: &ToggleSidebar, _, cx| view.toggle_sidebar(cx)))
            .on_action(cx.listener(|view, _: &GoBack, _, cx| view.go_back(cx)))
            .on_action(cx.listener(|view, _: &GoForward, _, cx| view.go_forward(cx)))
            .size_full()
            .font_family(crumble_theme::system_font())
            .bg(colors.background)
            .text_color(colors.foreground)
            .child(self.render_shell(page, window, cx))
    }
}
