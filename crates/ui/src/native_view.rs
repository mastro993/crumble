mod account_alias;
mod account_dot;
mod agenda;
mod calendar;
mod calendars_popover;
mod calendars_section;
mod card;
mod controls;
mod custom_color;
mod event_details;
mod keyboard;
mod list_rows;
mod local_models;
mod menu;
mod notes;
mod rail;
mod route;
mod select;
mod settings_models;
mod settings_page;
mod shell;
mod sidebar;
mod sidebar_reorder;
mod sidebar_row;
mod suggestions;

pub use calendar::{NextPeriod, PreviousPeriod, ShowToday};
pub use shell::{GoBack, GoForward, ToggleSidebar};
#[cfg(debug_assertions)]
pub use suggestions::SuggestSample;

use account_alias::{AliasFields, Rename};
use agenda::AgendaPage;
use calendar::CalendarPage;
use calendars_popover::CalendarsPopover;
use chrono::Utc;
use crumble_notes::Notes;
use crumble_store::Store;
use crumble_sync::{CalendarEvents, CalendarSync};
use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, Styled, Task, Window, div, prelude::FluentBuilder,
};
use local_models::LocalModels;
use menu::OpenMenu;
use notes::NoteDetail;
use route::{History, Route};
use settings_models::ModelSettings;
use settings_page::{SettingsSection, WeekdaySelect};
use sidebar_reorder::RowReorder;
use std::time::Instant;
use suggestions::Suggestions;

/// GPUI owner of the application shell and navigation.
pub struct NativeView {
    focus: FocusHandle,
    route: Route,
    history: History,
    /// Width every second-level sidebar shares: the Agenda list and the Settings sections.
    sidebar_width: f32,
    sidebar_collapsed: bool,
    /// Sidebar toggle count, keying each slide.
    sidebar_motion: u64,
    /// When the sidebar was last toggled; it slides only until the slide's length has passed,
    /// so launch does not animate and a page that mounts it again does not replay the slide.
    sidebar_toggled: Option<Instant>,
    /// Collapsed sidebar still mounted for its closing motion.
    sidebar_closing: bool,
    /// The Calendars popover beside the rail.
    calendars_popover: CalendarsPopover,
    store: Store,
    notes: Notes,
    /// The Note the Agenda content area shows.
    note_detail: NoteDetail,
    first_day_select: WeekdaySelect,
    /// The Settings section on screen.
    settings_section: SettingsSection,
    /// The fields of Settings › Transcription, Summary and Providers.
    models: ModelSettings,
    /// Settings › Local Models, and the downloads running.
    local_models: LocalModels,
    calendar: CalendarPage,
    events: Entity<CalendarEvents>,
    agenda: AgendaPage,
    calendar_sync: Entity<CalendarSync>,
    /// Drag and slide state of the Account rows.
    account_reorder: RowReorder<String>,
    /// The Alias field of each Account in Settings.
    alias_fields: AliasFields,
    /// The Account row being renamed in the Calendars popover.
    renaming: Option<Rename>,
    /// The open context menu.
    menu: Option<OpenMenu>,
    /// Today's Events and what was suggested of them.
    suggestions: Suggestions,
    /// Redraws Week view each minute for its current time line, and posts the Suggestions due.
    _minute_tick: Task<()>,
}

impl NativeView {
    /// Creates the root view and focuses it.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Window-level shortcuts such as ToggleSidebar dispatch through the focused view.
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let store = Store::open();
        let mut notes = Notes::open();
        // Recordings a crash cut short keep what their files hold.
        if let Err(error) = notes.recover_recordings(crumble_audio::recorded_length, Utc::now()) {
            eprintln!("Could not recover the Recordings left unfinished: {error:#}");
        }
        let note_detail = NoteDetail::new(window, cx);
        let first_day_select = settings_page::first_day_select(&store, cx);
        let local_models = LocalModels::new(window, cx);
        let models = settings_models::model_settings(&store, local_models.downloaded(), window, cx);
        let calendar = CalendarPage::new(store.calendar_view());
        let calendar_sync = cx.new(CalendarSync::new);
        let events = cx.new(|_| CalendarEvents::default());
        cx.observe(&events, |_, _, cx| cx.notify()).detach();
        let agenda = AgendaPage::new(cx);
        let suggestions = Suggestions::new(window, cx);
        // A sync change can also mean EventKit changed its Events or Accounts.
        cx.observe_in(&calendar_sync, window, |view, _, window, cx| {
            view.refresh_events(true, cx);
            view.agenda
                .events
                .update(cx, |events, _| events.mark_stale());
            view.sync_alias_fields(window, cx);
            cx.notify();
        })
        .detach();
        let mut view = Self {
            focus,
            route: Route::Home,
            history: History::default(),
            sidebar_width: sidebar::DEFAULT_WIDTH,
            sidebar_collapsed: false,
            sidebar_motion: 0,
            sidebar_toggled: None,
            sidebar_closing: false,
            calendars_popover: CalendarsPopover::default(),
            store,
            notes,
            note_detail,
            first_day_select,
            settings_section: SettingsSection::default(),
            models,
            local_models,
            calendar,
            events,
            agenda,
            calendar_sync,
            account_reorder: RowReorder::default(),
            alias_fields: AliasFields::default(),
            renaming: None,
            menu: None,
            suggestions,
            _minute_tick: calendar::tick_minutes(cx),
        };
        // Transcripts left without a Summary, as when the app quit while making them. Recordings
        // without a Transcript wait for the user (ADR 0010).
        view.summarize_unsummarized(cx);
        view.read_notification_status(cx);
        view
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
        if self.route == Route::Agenda {
            self.maintain_agenda(window, cx);
        }
        self.maintain_note_detail(window, cx);
        // Notices a held popover's menu, Rename, or drag ending.
        self.settle_calendars(window, cx);
        let page = match &self.route {
            Route::Agenda => self.render_agenda(window, cx),
            Route::Calendar => self.render_calendar(window, cx),
            Route::Settings => self.render_settings(window, cx),
            route => div().text_xl().child(route.title()).into_any_element(),
        };
        let colors = cx.theme().colors;
        let root = div().track_focus(&self.focus);
        #[cfg(debug_assertions)]
        let root =
            root.on_action(cx.listener(|view, _: &SuggestSample, _, cx| view.suggest_sample(cx)));
        root.when(self.route == Route::Calendar, |root| {
            self.calendar_actions(root, cx)
        })
        .on_action(cx.listener(|view, _: &ToggleSidebar, _, cx| view.toggle_sidebar(cx)))
        .on_action(cx.listener(|view, _: &GoBack, _, cx| view.go_back(cx)))
        .on_action(cx.listener(|view, _: &GoForward, _, cx| view.go_forward(cx)))
        .size_full()
        .font_family(crumble_theme::system_font())
        // The header and the rail sit on the sidebar color around the page's panel.
        .bg(colors.sidebar)
        .text_color(colors.foreground)
        .child(self.render_shell(page, window, cx))
        .children(self.menu.as_ref().map(OpenMenu::render))
    }
}
