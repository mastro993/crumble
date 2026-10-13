mod calendar;
mod calendars_section;
mod controls;
mod route;
mod settings_page;
mod shell;
mod sidebar;
mod sidebar_fold;
mod sidebar_row;
mod sidebar_scroll;

pub use calendar::{NextPeriod, PreviousPeriod, ShowToday};
pub use shell::{GoBack, GoForward, ToggleSidebar};

use crate::calendar_sync::CalendarSync;
use crate::settings::{Settings, SidebarSection};
use calendar::CalendarPage;
use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, ScrollHandle, Styled, Window, div, prelude::FluentBuilder,
};
use route::{History, Route};
use settings_page::WeekdaySelect;
use std::collections::{HashMap, HashSet};

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
    settings: Settings,
    first_day_select: WeekdaySelect,
    calendar: CalendarPage,
    calendar_sync: Entity<CalendarSync>,
    /// Scroll position of the sidebar's sections.
    sidebar_scroll: ScrollHandle,
    /// Whether the pointer is over the sidebar, which shows the sections' scrollbar.
    sidebar_hovered: bool,
    /// Sidebar hover count; zero renders the scrollbar without a fade.
    sidebar_hover_motion: u64,
    /// Fold toggle count per section, keying its slide.
    section_motion: HashMap<SidebarSection, u64>,
    /// Sections whose rows are still sliding open or shut.
    sections_moving: HashSet<SidebarSection>,
}

impl NativeView {
    /// Creates the root view and focuses it.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Window-level shortcuts such as ToggleSidebar dispatch through the focused view.
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let settings = Settings::load();
        let first_day_select = settings_page::first_day_select(&settings, window, cx);
        let calendar_sync = cx.new(CalendarSync::new);
        cx.observe(&calendar_sync, |_, _, cx| cx.notify()).detach();
        Self {
            focus,
            route: Route::Home,
            history: History::default(),
            sidebar_width: sidebar::DEFAULT_WIDTH,
            sidebar_collapsed: false,
            sidebar_motion: 0,
            sidebar_closing: false,
            settings,
            first_day_select,
            calendar: CalendarPage::default(),
            calendar_sync,
            sidebar_scroll: ScrollHandle::new(),
            sidebar_hovered: false,
            sidebar_hover_motion: 0,
            section_motion: HashMap::new(),
            sections_moving: HashSet::new(),
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
        let page = match self.route {
            Route::Calendar => self.render_calendar(window, cx),
            Route::Settings => self.render_settings(cx).into_any_element(),
            route => div().text_xl().child(route.title()).into_any_element(),
        };
        let colors = cx.theme().colors;
        div()
            .track_focus(&self.focus)
            .when(self.route == Route::Calendar, |root| {
                self.calendar_actions(root, cx)
            })
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
