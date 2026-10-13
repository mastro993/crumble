//! Week, Month, and Year grids. Weekends are shaded in Week and Month and muted in Year.

use super::NativeView;
use super::period::{is_weekend, month_weeks, week_days, week_start, weekdays, year_months};
use crate::native_view::controls::button;
use chrono::{Datelike, NaiveDate, Weekday};
use crumble_theme::SURFACE_RADIUS;
use gpui_kit::component::{
    ActiveTheme, Sizable, StyledExt, button::ButtonVariants, h_flex, v_flex,
};
use gpui_kit::{
    AnyElement, App, Context, Corners, Div, Hsla, InteractiveElement, IntoElement, ParentElement,
    Pixels, StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};

/// Width of the Week view's hour labels.
const HOUR_GUTTER: f32 = 56.0;
const HOUR_HEIGHT: f32 = 48.0;

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

/// Corner radii for a cell that touches the given sides of a rounded frame, so its fill
/// follows the frame's corners. GPUI does not clip children to a parent's radius.
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
    cx: &App,
) -> AnyElement {
    let (border, radius) = (cx.theme().border, px(SURFACE_RADIUS));
    let weeks = month_weeks(focused, first_day);
    let last_row = weeks.len() - 1;
    let rows = weeks.into_iter().enumerate().map(|(row, start)| {
        h_flex()
            .flex_1()
            .min_h_0()
            .children(week_days(start).enumerate().map(|(column, date)| {
                let (top, bottom) = (row == 0, row == last_row);
                let (left, right) = (column == 0, column == 6);
                div()
                    .flex_1()
                    .h_full()
                    .p_1()
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
    cx: &App,
) -> AnyElement {
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
    v_flex()
        .size_full()
        .child(
            frame(hairline, cx).child(header).child(
                div().relative().flex_1().min_h_0().child(columns).child(
                    div()
                        .id("week-hours")
                        .size_full()
                        .overflow_y_scroll()
                        .children(hours),
                ),
            ),
        )
        .into_any_element()
}

pub(super) fn year(
    focused: NaiveDate,
    first_day: Weekday,
    today: NaiveDate,
    cx: &mut Context<NativeView>,
) -> AnyElement {
    let months: Vec<AnyElement> = year_months(focused)
        .map(|first| mini_month(first, first_day, today, cx))
        .collect();
    div()
        .id("year-months")
        .size_full()
        .overflow_y_scroll()
        .child(
            div()
                .grid()
                .grid_cols(4)
                .gap_x_8()
                .gap_y_6()
                .children(months),
        )
        .into_any_element()
}

/// One month of the Year view; its name opens the month in Month view.
fn mini_month(
    first: NaiveDate,
    first_day: Weekday,
    today: NaiveDate,
    cx: &mut Context<NativeView>,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let weeks = month_weeks(first, first_day).into_iter().map(|start| {
        h_flex().children(week_days(start).map(|date| {
            div()
                .flex_1()
                .flex()
                .justify_center()
                .text_xs()
                .when(date.month() == first.month(), |cell| {
                    cell.child(day_number(date, today, 20.0, is_weekend(date), cx))
                })
        }))
    });
    v_flex()
        .gap_1()
        .child(
            h_flex().child(
                button(("year-month", first.month()))
                    .ghost()
                    .small()
                    .label(first.format("%B").to_string())
                    .on_click(cx.listener(move |view, _, _, cx| view.open_month(first, cx))),
            ),
        )
        .child(h_flex().children(weekdays(first_day).map(|day| {
            div()
                .flex_1()
                .text_center()
                .text_xs()
                .text_color(muted)
                .child(day.to_string().chars().take(2).collect::<String>())
        })))
        .children(weeks)
        .into_any_element()
}
