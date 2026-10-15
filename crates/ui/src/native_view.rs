mod account_alias;
mod account_dot;
mod calendar;
mod calendars_section;
mod card;
mod controls;
mod custom_color;
mod menu;
mod route;
mod settings_page;
mod shell;
mod sidebar;
mod sidebar_fold;
mod sidebar_reorder;
mod sidebar_row;
mod sidebar_scroll;

pub(crate) use calendar::CalendarView;
pub use calendar::{NextPeriod, PreviousPeriod, ShowToday};
pub use shell::{GoBack, GoForward, ToggleSidebar};

use crate::calendar_sync::CalendarSync;
use crate::settings::{Settings, SidebarSection};
use account_alias::{AliasFields, Rename};
use calendar::{CalendarEvents, CalendarPage};
use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, ScrollHandle, Styled, Window, div, prelude::FluentBuilder,
};
use menu::OpenMenu;
use route::{History, Route};
use settings_page::WeekdaySelect;
use sidebar_reorder::RowReorder;
use std::collections::{BTreeSet, HashMap, HashSet};

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
    /// EventKit identifiers of the Hidden Accounts; never saved, so they show again next launch.
    hidden_accounts: BTreeSet<String>,
    first_day_select: WeekdaySelect,
    calendar: CalendarPage,
    events: CalendarEvents,
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
    /// Drag and slide state of the Account rows.
    account_reorder: RowReorder<String>,
    /// The Alias field of each Account in Settings.
    alias_fields: AliasFields,
    /// The Account row being renamed in the sidebar.
    renaming: Option<Rename>,
    /// The open context menu.
    menu: Option<OpenMenu>,
}

impl NativeView {
    /// Creates the root view and focuses it.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Window-level shortcuts such as ToggleSidebar dispatch through the focused view.
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let settings = Settings::load();
        let first_day_select = settings_page::first_day_select(&settings, window, cx);
        let calendar = CalendarPage::new(settings.calendar_view);
        let calendar_sync = cx.new(CalendarSync::new);
        // A sync change can also mean EventKit changed its Events or Accounts.
        cx.observe_in(&calendar_sync, window, |view, _, window, cx| {
            view.refresh_events(true, cx);
            view.sync_alias_fields(window, cx);
            cx.notify();
        })
        .detach();
        Self {
            focus,
            route: Route::Home,
            history: History::default(),
            sidebar_width: sidebar::DEFAULT_WIDTH,
            sidebar_collapsed: false,
            sidebar_motion: 0,
            sidebar_closing: false,
            settings,
            hidden_accounts: BTreeSet::new(),
            first_day_select,
            calendar,
            events: CalendarEvents::default(),
            calendar_sync,
            sidebar_scroll: ScrollHandle::new(),
            sidebar_hovered: false,
            sidebar_hover_motion: 0,
            section_motion: HashMap::new(),
            sections_moving: HashSet::new(),
            account_reorder: RowReorder::default(),
            alias_fields: AliasFields::default(),
            renaming: None,
            menu: None,
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
        if let Some(route) = self.history.back(self.route.clone()) {
            self.route = route;
            cx.notify();
        }
    }

    fn go_forward(&mut self, cx: &mut Context<Self>) {
        if let Some(route) = self.history.forward(self.route.clone()) {
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
        let page = match &self.route {
            Route::Calendar => self.render_calendar(window, cx),
            Route::Settings => self.render_settings(cx).into_any_element(),
            // The Event's details are in the header for now.
            Route::Event(_) => div().into_any_element(),
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
            .children(self.menu.as_ref().map(OpenMenu::render))
    }
}
