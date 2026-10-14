//! How Events are drawn: a block in Week view, a line or bar in Month view, and the details
//! popover any of them opens.

use super::events::{columns, day_order, minutes_on, on_day, when_text};
use super::grids::HOUR_HEIGHT;
use chrono::NaiveDate;
use crumble_eventkit::{Calendar, Event, When};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{
    ActiveTheme, Selectable, Sizable, StyledExt, button::Button, button::ButtonVariants, h_flex,
    v_flex,
};
use gpui_kit::{
    App, Div, ElementId, Hsla, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StyleRefinement, Styled, Window, div, prelude::FluentBuilder, px, relative,
};

/// Height of a Month view line and an all-day bar.
const LINE_HEIGHT: f32 = 18.0;
/// Lines a Month view day shows before "+N more".
const MONTH_LINES: usize = 3;
/// All-day bars a Week view day shows before "+N more".
const ALL_DAY_LINES: usize = 2;
/// The shortest a Week view block is drawn, in minutes, so short Events stay clickable.
const MIN_BLOCK_MINUTES: f32 = 22.0;
/// Blocks shorter than this, in points, show their title on one line and leave out the time.
const TWO_LINE_HEIGHT: f32 = 36.0;
/// Line height inside a Week view block, tighter than the theme's so two lines fit in 45 minutes.
const BLOCK_LINE_HEIGHT: f32 = 15.0;
const POPOVER_WIDTH: f32 = 280.0;

/// An Event with what drawing it needs from its Calendar and Account.
#[derive(Clone)]
pub(super) struct Shown {
    pub(super) event: Event,
    /// The Account's color.
    pub(super) color: Hsla,
    pub(super) calendar: SharedString,
}

/// The Events paired with their Calendars, each in the color `account_color` gives its
/// Account; an Event whose Calendar is gone is dropped.
pub(super) fn shown(
    events: &[Event],
    calendars: &[Calendar],
    account_color: impl Fn(&str) -> Hsla,
) -> Vec<Shown> {
    events
        .iter()
        .filter_map(|event| {
            let calendar = calendars
                .iter()
                .find(|calendar| calendar.id == event.calendar_id)?;
            Some(Shown {
                event: event.clone(),
                color: account_color(&calendar.account_id),
                calendar: calendar.name.clone().into(),
            })
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChipKind {
    /// A Week view block for a timed Event; a short one shows only its title.
    Block { one_line: bool },
    /// A Month view line for a timed Event: dot, start time, title.
    Line,
    /// A tinted bar for an all-day Event.
    Bar,
}

/// One Event's clickable shape; it is the trigger of the details popover.
#[derive(IntoElement)]
struct EventChip {
    id: ElementId,
    kind: ChipKind,
    title: SharedString,
    time: Option<SharedString>,
    color: Hsla,
    selected: bool,
}

impl Selectable for EventChip {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for EventChip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let tint = self.color.opacity(if self.selected { 0.4 } else { 0.22 });
        let title = if self.title.is_empty() {
            "(No title)".into()
        } else {
            self.title
        };
        let chip = div()
            .id(self.id)
            .cursor_pointer()
            .overflow_hidden()
            .text_xs()
            .text_color(theme.foreground);
        match self.kind {
            ChipKind::Block { one_line } => chip
                .size_full()
                .line_height(px(BLOCK_LINE_HEIGHT))
                .px_1()
                .rounded(px(4.0))
                .border_l(px(3.0))
                .border_color(self.color)
                .bg(tint)
                .when(one_line, |block| block.flex().items_center())
                .when(!one_line, |block| block.py(px(2.0)))
                .child(div().min_w_0().font_semibold().truncate().child(title))
                // A short block has room for the title only; the details say when.
                .when(!one_line, |block| {
                    block.children(self.time.map(|time| {
                        div()
                            .truncate()
                            .text_color(theme.muted_foreground)
                            .child(time)
                    }))
                }),
            ChipKind::Bar => chip
                .h(px(LINE_HEIGHT))
                .px_1()
                .flex()
                .items_center()
                .rounded(px(4.0))
                .bg(tint)
                .child(div().truncate().child(title)),
            ChipKind::Line => chip
                .h(px(LINE_HEIGHT))
                .px_1()
                .flex()
                .items_center()
                .gap_1()
                .rounded(px(4.0))
                .when(self.selected, |line| line.bg(tint))
                .when(!self.selected, |line| {
                    let hover = theme.muted;
                    line.hover(move |style| style.bg(hover))
                })
                .child(
                    div()
                        .size(px(6.0))
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(self.color),
                )
                .children(self.time.map(|time| {
                    div()
                        .flex_shrink_0()
                        .text_color(theme.muted_foreground)
                        .child(time)
                }))
                .child(div().min_w_0().truncate().child(title)),
        }
    }
}

/// The chip of `shown` on `date`, opening the Event's details when clicked.
fn event(shown: &Shown, date: NaiveDate, kind: ChipKind) -> Popover {
    let event = &shown.event;
    let id = format!("event-{}-{date}", event.id);
    let time = match (kind, event.when) {
        (ChipKind::Bar, _) | (_, When::AllDay { .. }) => None,
        (_, When::Timed { start, .. }) => Some(start.format("%H:%M").to_string().into()),
    };
    let chip = EventChip {
        id: ElementId::Name(format!("{id}-chip").into()),
        kind,
        title: event.title.clone().into(),
        time,
        color: shown.color,
        selected: false,
    };
    let details = shown.clone();
    Popover::new(ElementId::Name(id.into()))
        .trigger_style(StyleRefinement::default().size_full())
        .trigger(chip)
        .content(move |_, _, cx| details_view(&details, cx))
}

/// Title, when, Calendar, location, and a Join button for a meeting link. Read-only: Crumble
/// never changes Events.
fn details_view(shown: &Shown, cx: &App) -> Div {
    let theme = cx.theme();
    let event = &shown.event;
    let join = event.join_url.clone();
    v_flex()
        .w(px(POPOVER_WIDTH))
        .gap_2()
        .text_sm()
        .child(
            div()
                .text_base()
                .font_semibold()
                .child(if event.title.is_empty() {
                    "(No title)".to_owned()
                } else {
                    event.title.clone()
                }),
        )
        .child(
            div()
                .text_color(theme.muted_foreground)
                .child(when_text(&event.when)),
        )
        .child(
            h_flex()
                .gap_2()
                .child(
                    div()
                        .size(px(8.0))
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(shown.color),
                )
                .child(div().min_w_0().truncate().child(shown.calendar.clone())),
        )
        .children(
            event
                .location
                .clone()
                .map(|location| div().text_color(theme.muted_foreground).child(location)),
        )
        .children(join.map(|url| {
            h_flex().child(
                Button::new("event-join")
                    .primary()
                    .small()
                    .label("Join")
                    .cursor_pointer()
                    .on_click(move |_, _, cx| cx.open_url(&url)),
            )
        }))
}

/// The all-day Events of each of `days`, in Week view's strip above the hours; `None` when the
/// week has none.
pub(super) fn all_day_strip(days: &[NaiveDate], shown: &[Shown], cx: &App) -> Option<Vec<Div>> {
    let all_day: Vec<&Shown> = shown
        .iter()
        .filter(|shown| matches!(shown.event.when, When::AllDay { .. }))
        .collect();
    if !days
        .iter()
        .any(|&date| all_day.iter().any(|shown| on_day(&shown.event, date)))
    {
        return None;
    }
    Some(
        days.iter()
            .map(|&date| {
                let day = day_order(all_day.iter().map(|shown| &shown.event), date);
                stacked(day, &all_day, date, ChipKind::Bar, ALL_DAY_LINES, cx)
            })
            .collect(),
    )
}

/// A Month view day's Events, all-day first.
pub(super) fn month_day(date: NaiveDate, shown: &[Shown], cx: &App) -> Div {
    let day = day_order(shown.iter().map(|shown| &shown.event), date);
    let all: Vec<&Shown> = shown.iter().collect();
    stacked(day, &all, date, ChipKind::Line, MONTH_LINES, cx)
}

/// The first `limit` of `day` as chips, then "+N more" for the rest.
fn stacked(
    day: Vec<&Event>,
    shown: &[&Shown],
    date: NaiveDate,
    timed_kind: ChipKind,
    limit: usize,
    cx: &App,
) -> Div {
    // ponytail: a fixed number of lines per day, whatever the cell's height; measure the
    // cell if tall windows look sparse.
    let more = day.len().saturating_sub(limit);
    v_flex()
        .gap(px(1.0))
        .min_w_0()
        .children(day.into_iter().take(limit).filter_map(|event| {
            let shown = shown
                .iter()
                .find(|shown| std::ptr::eq(&shown.event, event))?;
            let kind = match event.when {
                When::AllDay { .. } => ChipKind::Bar,
                When::Timed { .. } => timed_kind,
            };
            Some(
                div()
                    .h(px(LINE_HEIGHT))
                    .child(self::event(shown, date, kind)),
            )
        }))
        .when(more > 0, |list| {
            list.child(
                div()
                    .px_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("+{more} more")),
            )
        })
}

/// Week view's timed Events of one day, placed by time over the hour grid. Overlapping ones
/// share the width side by side.
pub(super) fn day_blocks(date: NaiveDate, shown: &[Shown]) -> Div {
    let timed: Vec<(&Shown, (f32, f32))> = shown
        .iter()
        .filter_map(|shown| Some((shown, minutes_on(&shown.event, date)?)))
        .collect();
    let spans: Vec<(f32, f32)> = timed
        .iter()
        .map(|&(_, (start, end))| (start, end.max(start + MIN_BLOCK_MINUTES)))
        .collect();
    let placed = columns(&spans);
    let to_px = |minutes: f32| minutes / 60.0 * HOUR_HEIGHT;
    div()
        .relative()
        .flex_1()
        .h_full()
        .children(timed.iter().zip(spans).zip(placed).map(
            |((&(shown, _), (start, end)), (column, width))| {
                div()
                    .absolute()
                    .top(px(to_px(start)))
                    .h(px(to_px(end - start) - 1.0))
                    .left(relative(column as f32 / width as f32))
                    .w(relative(1.0 / width as f32))
                    .pr(px(2.0))
                    .child(event(
                        shown,
                        date,
                        ChipKind::Block {
                            one_line: to_px(end - start) < TWO_LINE_HEIGHT,
                        },
                    ))
            },
        ))
}

#[cfg(test)]
mod tests {
    use super::shown;
    use chrono::NaiveDate;
    use crumble_eventkit::{Calendar, Event, When};
    use gpui_kit::{Hsla, rgb};

    fn calendar(id: &str, account_id: &str, color: u32) -> Calendar {
        Calendar {
            id: id.into(),
            account_id: account_id.into(),
            name: id.into(),
            color,
        }
    }

    fn event(id: &str, calendar_id: &str) -> Event {
        let day = NaiveDate::from_ymd_opt(2026, 10, 14).unwrap_or_default();
        Event {
            id: id.into(),
            calendar_id: calendar_id.into(),
            title: id.into(),
            when: When::AllDay {
                first: day,
                last: day,
            },
            location: None,
            join_url: None,
        }
    }

    #[test]
    fn events_take_their_account_color_not_their_calendar_color() {
        let calendars = [
            calendar("work", "google", 0x00ff00),
            calendar("holidays", "google", 0x0000ff),
            calendar("family", "icloud", 0x00ff00),
        ];
        let events = [
            event("standup", "work"),
            event("party", "family"),
            event("ferragosto", "holidays"),
            event("orphan", "deleted"),
        ];
        let account_color = |account: &str| -> Hsla {
            rgb(if account == "google" {
                0xff0000
            } else {
                0x333333
            })
            .into()
        };
        let shown = shown(&events, &calendars, account_color);
        let colors: Vec<(&str, Hsla)> = shown
            .iter()
            .map(|shown| (shown.event.id.as_str(), shown.color))
            .collect();
        let red: Hsla = rgb(0xff0000).into();
        let gray: Hsla = rgb(0x333333).into();
        assert_eq!(
            colors,
            [("standup", red), ("party", gray), ("ferragosto", red)]
        );
    }
}
