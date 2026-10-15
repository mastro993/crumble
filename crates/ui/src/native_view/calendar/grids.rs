//! Week, Month, and Year grids. Weekends are shaded in every Calendar view.

use super::NativeView;
use super::event_views::{Shown, all_day_strip, day_blocks, month_day, open_in};
use super::period::{
    CalendarView, is_weekend, month_weeks, week_days, week_start, weekdays, year_months,
};
use chrono::{Datelike, Days, NaiveDate, Weekday};
use crumble_theme::SURFACE_RADIUS;
use gpui_kit::component::{ActiveTheme, StyledExt, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Corners, Div, Hsla, InteractiveElement, IntoElement, ParentElement,
    Pixels, ScrollHandle, StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};

/// Width of the Week view's hour labels.
const HOUR_GUTTER: f32 = 56.0;
pub(super) const HOUR_HEIGHT: f32 = 48.0;
/// Week rows in every Year view month, so the months in each grid row end level.
const YEAR_WEEKS: u64 = 6;
const YEAR_DAY_HEIGHT: f32 = 20.0;
/// Keeps Year view day columns compact on wide windows.
const YEAR_MAX_WIDTH: f32 = 1200.0;

fn weekend_fill(cx: &App) -> Hsla {
    cx.theme().muted
}

/// Day number, circled in the accent color when it is today.
fn day_number(date: NaiveDate, today: NaiveDate, size: f32, muted: bool, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .size(px(size))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .when(muted, |number| number.text_color(theme.muted_foreground))
        .when(date == today, |number| {
            number
                .bg(theme.primary)
                .text_color(theme.primary_foreground)
                .font_semibold()
        })
        .child(date.day().to_string())
}

/// Weekday names in column order.
fn weekday_header(first_day: Weekday, cx: &App) -> Div {
    h_flex().children(weekdays(first_day).map(|day| {
        div()
            .flex_1()
            .px_2()
            .pb_1()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(day.to_string())
    }))
}

/// Corner radii for a fill that touches the given sides of a rounded shape, so the fill
/// follows that shape's corners. GPUI does not clip children to a parent's radius.
fn frame_corners(
    radius: Pixels,
    top: bool,
    bottom: bool,
    left: bool,
    right: bool,
) -> Corners<Pixels> {
    let at = |touches: bool| if touches { radius } else { px(0.) };
    Corners {
        top_left: at(top && left),
        top_right: at(top && right),
        bottom_left: at(bottom && left),
        bottom_right: at(bottom && right),
    }
}

/// Rounded, bordered surface holding a Week or Month grid.
fn frame(hairline: Pixels, cx: &App) -> Div {
    v_flex()
        .flex_1()
        .min_h_0()
        .border(hairline)
        .border_color(cx.theme().border)
        .rounded(px(SURFACE_RADIUS))
}

pub(super) fn month(
    focused: NaiveDate,
    first_day: Weekday,
    today: NaiveDate,
    hairline: Pixels,
    shown: &[Shown],
    cx: &mut Context<NativeView>,
) -> AnyElement {
    let (border, radius) = (cx.theme().border, px(SURFACE_RADIUS));
    let open = open_in(cx);
    let weeks = month_weeks(focused, first_day);
    let last_row = weeks.len() - 1;
    let rows = weeks.into_iter().enumerate().map(|(row, start)| {
        h_flex()
            .flex_1()
            .min_h_0()
            .children(week_days(start).enumerate().map(|(column, date)| {
                let (top, bottom) = (row == 0, row == last_row);
                let (left, right) = (column == 0, column == 6);
                // Clicking a day opens its week; clicking an Event opens its own page.
                v_flex()
                    .id(("month-day", row * 7 + column))
                    .cursor_pointer()
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.open_period(CalendarView::Week, date, cx)
                    }))
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .gap_1()
                    .p_1()
                    .overflow_hidden()
                    .text_sm()
                    .border_color(border)
                    .when(!right, |cell| cell.border_r(hairline))
                    .when(!bottom, |cell| cell.border_b(hairline))
                    .corner_radii(frame_corners(radius, top, bottom, left, right))
                    .when(is_weekend(date), |cell| cell.bg(weekend_fill(cx)))
                    .child(day_number(
                        date,
                        today,
                        24.0,
                        date.month() != focused.month(),
                        cx,
                    ))
                    .child(month_day(date, shown, &open, cx))
            }))
    });
    v_flex()
        .size_full()
        .child(weekday_header(first_day, cx))
        .child(frame(hairline, cx).children(rows))
        .into_any_element()
}

pub(super) fn week(
    focused: NaiveDate,
    first_day: Weekday,
    today: NaiveDate,
    hairline: Pixels,
    hours_scroll: &ScrollHandle,
    shown: &[Shown],
    cx: &mut Context<NativeView>,
) -> AnyElement {
    let open = &open_in(cx);
    let cx: &App = cx;
    let theme = cx.theme();
    let radius = px(SURFACE_RADIUS);
    let days: Vec<NaiveDate> = week_days(week_start(focused, first_day)).collect();
    let header = h_flex()
        .pl(px(HOUR_GUTTER))
        .border_b(hairline)
        .border_color(theme.border)
        .children(days.iter().enumerate().map(|(column, &date)| {
            h_flex()
                .flex_1()
                .justify_center()
                .gap_1()
                .py_1()
                .text_sm()
                .corner_radii(frame_corners(radius, true, false, false, column == 6))
                .when(is_weekend(date), |cell| cell.bg(weekend_fill(cx)))
                .child(
                    div()
                        .text_color(theme.muted_foreground)
                        .child(date.weekday().to_string()),
                )
                .child(day_number(date, today, 24.0, false, cx))
        }));
    // Day columns and weekend shading stay put behind the scrolling hours, so the shading
    // keeps the frame's bottom corners while the hours scroll.
    let columns = h_flex()
        .absolute()
        .size_full()
        .pl(px(HOUR_GUTTER))
        .children(days.iter().enumerate().map(|(column, &date)| {
            div()
                .flex_1()
                .h_full()
                .border_l(hairline)
                .border_color(theme.border)
                .corner_radii(frame_corners(radius, false, true, false, column == 6))
                .when(is_weekend(date), |cell| cell.bg(weekend_fill(cx)))
        }));
    let hours = (0..24).map(|hour| {
        h_flex()
            .h(px(HOUR_HEIGHT))
            .flex_shrink_0()
            .child(
                div()
                    .w(px(HOUR_GUTTER))
                    .h_full()
                    .flex_shrink_0()
                    .pr_2()
                    .text_right()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("{hour:02}:00")),
            )
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .border_color(theme.border)
                    .when(hour < 23, |line| line.border_b(hairline)),
            )
    });
    // All-day Events sit in a strip between the day names and the hours.
    let all_day = all_day_strip(&days, shown, open, cx).map(|cells| {
        // A plain flex row, so every cell stretches to the tallest one.
        div()
            .flex()
            .pl(px(HOUR_GUTTER))
            .border_b(hairline)
            .border_color(theme.border)
            .children(cells.into_iter().zip(&days).map(|(cell, &date)| {
                div()
                    .flex_1()
                    .min_w_0()
                    .p(px(2.0))
                    .border_l(hairline)
                    .border_color(theme.border)
                    .when(is_weekend(date), |cell| cell.bg(weekend_fill(cx)))
                    .child(cell)
            }))
    });
    // Timed Events scroll with the hours, over their day's column.
    let blocks = h_flex()
        .absolute()
        .top_0()
        .left(px(HOUR_GUTTER))
        .right_0()
        .h(px(24.0 * HOUR_HEIGHT))
        .children(days.iter().map(|&date| day_blocks(date, shown, open)));
    v_flex()
        .size_full()
        .child(
            frame(hairline, cx).child(header).children(all_day).child(
                div().relative().flex_1().min_h_0().child(columns).child(
                    div()
                        .id("week-hours")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(hours_scroll)
                        .child(div().relative().children(hours).child(blocks)),
                ),
            ),
        )
        .into_any_element()
}

pub(super) fn year(
    focused: NaiveDate,
    first_day: Weekday,
    today: NaiveDate,
    hairline: Pixels,
    cx: &mut Context<NativeView>,
) -> AnyElement {
    let months: Vec<AnyElement> = year_months(focused)
        .map(|first| month_card(first, first_day, today, hairline, cx))
        .collect();
    div()
        .id("year-months")
        .size_full()
        .overflow_y_scroll()
        .child(
            div()
                .grid()
                .grid_cols(4)
                .gap_4()
                .max_w(px(YEAR_MAX_WIDTH))
                .mx_auto()
                .children(months),
        )
        .into_any_element()
}

/// One month of the Year view in a rounded card; clicking it opens the month in Month view.
fn month_card(
    first: NaiveDate,
    first_day: Weekday,
    today: NaiveDate,
    hairline: Pixels,
    cx: &mut Context<NativeView>,
) -> AnyElement {
    let theme = cx.theme();
    let (muted, border, column_radius) = (theme.muted_foreground, theme.border, theme.radius);
    let current = (first.year(), first.month()) == (today.year(), today.month());
    let grid_start = week_start(first, first_day);
    // Columns rather than rows, so each weekend column is one shaded, rounded strip.
    let columns: Vec<_> = (0..7)
        .map(|column| {
            let top = grid_start + Days::new(column);
            // Neighbouring weekend columns share one strip: only its outer corners are round.
            let weekend_at = |offset: i64| {
                (0..7).contains(&(column as i64 + offset))
                    && is_weekend(top + chrono::Duration::days(offset))
            };
            let (open_left, open_right) = (!weekend_at(-1), !weekend_at(1));
            v_flex()
                .flex_1()
                .items_center()
                .py_1()
                .when(is_weekend(top), |strip| {
                    strip.bg(weekend_fill(cx)).corner_radii(frame_corners(
                        column_radius,
                        true,
                        true,
                        open_left,
                        open_right,
                    ))
                })
                .child(
                    div()
                        .h(px(YEAR_DAY_HEIGHT))
                        .flex()
                        .items_center()
                        .text_xs()
                        .text_color(muted)
                        .child(
                            top.weekday()
                                .to_string()
                                .chars()
                                .take(2)
                                .collect::<String>(),
                        ),
                )
                .children((0..YEAR_WEEKS).map(|week| {
                    let date = top + Days::new(week * 7);
                    div()
                        .h(px(YEAR_DAY_HEIGHT))
                        .flex()
                        .items_center()
                        .text_xs()
                        .when(date.month() == first.month(), |slot| {
                            slot.child(day_number(date, today, 20.0, false, cx))
                        })
                }))
        })
        .collect();
    v_flex()
        .id(("year-month", first.month() as usize))
        .cursor_pointer()
        .on_click(
            cx.listener(move |view, _, _, cx| view.open_period(CalendarView::Month, first, cx)),
        )
        .gap_1()
        .p_3()
        .border(hairline)
        .border_color(border)
        .rounded(px(SURFACE_RADIUS))
        .child(
            div()
                .px_1()
                .text_sm()
                .font_medium()
                .when(current, |name| {
                    name.text_color(cx.theme().primary).font_semibold()
                })
                .child(first.format("%B").to_string()),
        )
        .child(h_flex().children(columns))
        .into_any_element()
}
