//! Calendar page: a Week, Month, or Year view of the Period containing the Focused date.

mod event_details;
mod event_views;
mod events;
mod grids;
mod period;

use super::NativeView;
use super::account_dot::tint;
use super::controls::{app_icon, button};
use super::route::Route;
use chrono::{DateTime, Local, NaiveDate, NaiveTime, TimeDelta, TimeZone, Timelike};
use crumble_eventkit::{self as eventkit, Access, Event};
pub(super) use events::when_text;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{Sizable, StyledExt, button::ButtonVariants, h_flex};
use gpui_kit::{
    AnyElement, App, AppContext, Context, Div, InteractiveElement, IntoElement, ParentElement,
    ScrollHandle, Styled, Task, Window, div, point, px,
};
pub(crate) use period::CalendarView;
use std::rc::Rc;
use std::time::Duration;

gpui_kit::actions!(
    crumble,
    [
        /// Shows the Period containing today.
        ShowToday,
        /// Moves the Calendar back one Period.
        PreviousPeriod,
        /// Moves the Calendar forward one Period.
        NextPeriod
    ]
);

/// Calendar view and Focused date. The Calendar view is also saved in the settings; the Focused
/// date is kept in memory while the app runs.
pub(super) struct CalendarPage {
    view: CalendarView,
    focused: NaiveDate,
    /// Week view's scrolling hours, kept across Periods and Calendar views.
    week_hours: ScrollHandle,
}

impl CalendarPage {
    /// Today in `view`.
    pub(super) fn new(view: CalendarView) -> Self {
        let week_hours = ScrollHandle::new();
        // Week view opens on the working day rather than on midnight.
        week_hours.set_offset(point(px(0.0), px(-WEEK_OPENS_AT * grids::HOUR_HEIGHT)));
        Self {
            view,
            focused: today(),
            week_hours,
        }
    }
}

/// The hour Week view shows at the top when the app starts.
const WEEK_OPENS_AT: f32 = 8.0;

/// What an Event read covers: a range of dates and the shown Calendars.
#[derive(Clone, PartialEq, Eq)]
struct EventsRead {
    first: NaiveDate,
    /// The day after the last.
    end: NaiveDate,
    calendar_ids: Vec<String>,
}

/// The Events of the dates on screen, read again when those dates, the shown Calendars, or
/// EventKit change.
pub(super) struct CalendarEvents {
    events: Vec<Event>,
    /// The read the shown Events answer, or will once `read` finishes.
    wanted: Option<EventsRead>,
    read: Task<()>,
}

impl Default for CalendarEvents {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            wanted: None,
            read: Task::ready(()),
        }
    }
}

/// The start of `date`. Where clocks skip midnight for daylight saving, as in Chile, the day
/// starts an hour later; the extra Events read are drawn only on their own dates.
pub(super) fn local_midnight(date: NaiveDate) -> Option<DateTime<Local>> {
    let midnight = date.and_time(NaiveTime::MIN);
    Local.from_local_datetime(&midnight).earliest().or_else(|| {
        Local
            .from_local_datetime(&(midnight + TimeDelta::hours(1)))
            .earliest()
    })
}

/// Read on every use, so a window left open past midnight catches up on its next render.
pub(super) fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// Redraws Week view at the start of every minute, so its current time line keeps up.
pub(super) fn tick_minutes(cx: &mut Context<NativeView>) -> Task<()> {
    cx.spawn(async move |view, cx| {
        loop {
            let wait = 60 - u64::from(Local::now().second());
            cx.background_executor()
                .timer(Duration::from_secs(wait))
                .await;
            let redrawn = view.update(cx, |view, cx| {
                if view.route == Route::Calendar && view.calendar.view == CalendarView::Week {
                    cx.notify();
                }
            });
            if redrawn.is_err() {
                break;
            }
        }
    })
}

impl NativeView {
    /// Reads the Events of the dates on screen in the shown Calendars, unless they are already
    /// read; `force` reads again anyway, as after EventKit reports a change. A failed read is
    /// logged and the last Events stay.
    pub(super) fn refresh_events(&mut self, force: bool, cx: &mut Context<Self>) {
        let CalendarPage { view, focused, .. } = self.calendar;
        let sync = self.calendar_sync.read(cx);
        let wanted = (sync.access() == Access::Granted)
            .then(|| view.event_range(focused, self.settings.first_day_of_week))
            .flatten()
            .map(|(first, end)| EventsRead {
                first,
                end,
                calendar_ids: self.settings.shown_calendar_ids(sync.calendars()),
            });
        if !force && wanted == self.events.wanted {
            return;
        }
        self.events.wanted = wanted.clone();
        let Some(range) = wanted.and_then(|read| {
            Some((
                local_midnight(read.first)?,
                local_midnight(read.end)?,
                read.calendar_ids,
            ))
        }) else {
            self.events.events.clear();
            self.events.read = Task::ready(());
            cx.notify();
            return;
        };
        // Apple advises reading off the main thread.
        let read = cx.background_spawn(async move {
            let (start, end, calendar_ids) = range;
            eventkit::events(start, end, &calendar_ids)
        });
        self.events.read = cx.spawn(async move |this, cx| {
            let result = read.await;
            let _ = this.update(cx, |view, cx| match result {
                Ok(events) => {
                    view.events.events = events;
                    cx.notify();
                }
                Err(error) => eprintln!("Could not read events: {error}"),
            });
        });
    }

    fn step_period(&mut self, forward: bool, cx: &mut Context<Self>) {
        let calendar = &mut self.calendar;
        calendar.focused = calendar.view.step(calendar.focused, forward);
        self.refresh_events(false, cx);
        cx.notify();
    }

    fn show_today(&mut self, cx: &mut Context<Self>) {
        self.calendar.focused = today();
        self.refresh_events(false, cx);
        cx.notify();
    }

    /// Switches to `view` and saves it, so the Calendar page opens on it after a restart.
    fn set_calendar_view(&mut self, view: CalendarView) {
        self.calendar.view = view;
        if self.settings.calendar_view != view {
            self.settings.calendar_view = view;
            self.settings.save();
        }
    }

    fn show_calendar_view(&mut self, view: CalendarView, cx: &mut Context<Self>) {
        self.set_calendar_view(view);
        self.refresh_events(false, cx);
        cx.notify();
    }

    /// Opens `view` on the Period containing `date`.
    fn open_period(&mut self, view: CalendarView, date: NaiveDate, cx: &mut Context<Self>) {
        self.set_calendar_view(view);
        self.calendar.focused = date;
        self.refresh_events(false, cx);
        cx.notify();
    }

    /// Opens the page of `event`, recording the Calendar page for Back.
    fn open_event(&mut self, event: Event, cx: &mut Context<Self>) {
        self.navigate(Route::Event(Rc::new(event)), cx);
    }

    /// The Event page's header for `event`, as EventKit last read it when it, or its calendar
    /// item after a move, is still in the Calendar page's dates.
    pub(super) fn render_event_toolbar(&self, event: &Event, cx: &mut Context<Self>) -> AnyElement {
        let open_calendar = cx.listener(|view, _, _, cx| view.navigate(Route::Calendar, cx));
        let event = events::latest(&self.events.events, event);
        event_details::toolbar(event, open_calendar, cx).into_any_element()
    }

    /// Calendar shortcuts, registered only while the Calendar page is open.
    pub(super) fn calendar_actions(&self, root: Div, cx: &mut Context<Self>) -> Div {
        root.on_action(cx.listener(|view, _: &ShowToday, _, cx| view.show_today(cx)))
            .on_action(cx.listener(|view, _: &PreviousPeriod, _, cx| view.step_period(false, cx)))
            .on_action(cx.listener(|view, _: &NextPeriod, _, cx| view.step_period(true, cx)))
    }

    /// Today, Previous, Next, the Period title, and the Calendar view switcher.
    pub(super) fn render_calendar_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let CalendarPage { view, focused, .. } = self.calendar;
        let arrow = |id: &'static str, icon: &str, label: &'static str| {
            button(id)
                .ghost()
                .small()
                .icon(app_icon(icon).size_4())
                .accessibility_label(label)
        };
        h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(
                button("calendar-today")
                    .outline()
                    .small()
                    .label("Today")
                    .on_click(cx.listener(|view, _, _, cx| view.show_today(cx))),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .child(
                        arrow("calendar-previous", "chevron-left", "Previous period")
                            .on_click(cx.listener(|view, _, _, cx| view.step_period(false, cx))),
                    )
                    .child(
                        arrow("calendar-next", "chevron-right", "Next period")
                            .on_click(cx.listener(|view, _, _, cx| view.step_period(true, cx))),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_base()
                    .font_semibold()
                    .child(view.title(focused, self.settings.first_day_of_week)),
            )
            .child(
                TabBar::new("calendar-views")
                    .segmented()
                    .small()
                    .flex_shrink_0()
                    .selected_index(
                        CalendarView::ALL
                            .iter()
                            .position(|v| *v == view)
                            .unwrap_or(0),
                    )
                    .on_click(cx.listener(|view, index: &usize, _, cx| {
                        if let Some(calendar_view) = CalendarView::ALL.get(*index) {
                            view.show_calendar_view(*calendar_view, cx);
                        }
                    }))
                    .children(
                        CalendarView::ALL.map(|v| Tab::new().label(v.label()).cursor_pointer()),
                    ),
            )
    }

    pub(super) fn render_calendar(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let CalendarPage { view, focused, .. } = self.calendar;
        let first_day = self.settings.first_day_of_week;
        // One device pixel, matching the sidebar's hairline.
        let hairline = px(1.0 / window.scale_factor());
        let now = Local::now().naive_local();
        let today = now.date();
        let app: &App = cx;
        let shown = event_views::shown(
            &self.events.events,
            self.calendar_sync.read(app).calendars(),
            |account| tint(self.settings.account_color(account), app),
        );
        match view {
            CalendarView::Week => grids::week(
                focused,
                first_day,
                now,
                hairline,
                &self.calendar.week_hours,
                &shown,
                cx,
            ),
            CalendarView::Month => grids::month(focused, first_day, today, hairline, &shown, cx),
            CalendarView::Year => grids::year(focused, first_day, today, hairline, cx),
        }
    }
}
