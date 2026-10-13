//! Calendar page: a Week, Month, or Year view of the Period containing the Focused date.

mod grids;
mod period;

use super::NativeView;
use super::controls::{app_icon, button};
use chrono::{Local, NaiveDate};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{Sizable, StyledExt, button::ButtonVariants, h_flex};
use gpui_kit::{
    AnyElement, Context, Div, InteractiveElement, IntoElement, ParentElement, Styled, Window, div,
    px,
};
use period::CalendarView;

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

/// Calendar view and Focused date, kept in memory while the app runs.
pub(super) struct CalendarPage {
    view: CalendarView,
    focused: NaiveDate,
}

impl Default for CalendarPage {
    fn default() -> Self {
        Self {
            view: CalendarView::Month,
            focused: today(),
        }
    }
}

/// Read on every use, so a window left open past midnight catches up on its next render.
fn today() -> NaiveDate {
    Local::now().date_naive()
}

impl NativeView {
    fn step_period(&mut self, forward: bool, cx: &mut Context<Self>) {
        let calendar = &mut self.calendar;
        calendar.focused = calendar.view.step(calendar.focused, forward);
        cx.notify();
    }

    fn show_today(&mut self, cx: &mut Context<Self>) {
        self.calendar.focused = today();
        cx.notify();
    }

    fn show_calendar_view(&mut self, view: CalendarView, cx: &mut Context<Self>) {
        self.calendar.view = view;
        cx.notify();
    }

    /// Opens Month view on the month starting at `first`.
    fn open_month(&mut self, first: NaiveDate, cx: &mut Context<Self>) {
        self.calendar = CalendarPage {
            view: CalendarView::Month,
            focused: first,
        };
        cx.notify();
    }

    /// Calendar shortcuts, registered only while the Calendar page is open.
    pub(super) fn calendar_actions(&self, root: Div, cx: &mut Context<Self>) -> Div {
        root.on_action(cx.listener(|view, _: &ShowToday, _, cx| view.show_today(cx)))
            .on_action(cx.listener(|view, _: &PreviousPeriod, _, cx| view.step_period(false, cx)))
            .on_action(cx.listener(|view, _: &NextPeriod, _, cx| view.step_period(true, cx)))
    }

    /// Today, Previous, Next, the Period title, and the Calendar view switcher.
    pub(super) fn render_calendar_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let CalendarPage { view, focused } = self.calendar;
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
        let CalendarPage { view, focused } = self.calendar;
        let first_day = self.settings.first_day_of_week;
        // One device pixel, matching the sidebar's hairline.
        let hairline = px(1.0 / window.scale_factor());
        let today = today();
        match view {
            CalendarView::Week => grids::week(focused, first_day, today, hairline, cx),
            CalendarView::Month => grids::month(focused, first_day, today, hairline, cx),
            CalendarView::Year => grids::year(focused, first_day, today, hairline, cx),
        }
    }
}
