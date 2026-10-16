//! How Events are drawn: a block in Week view, a line or bar in Month view. Hovering one shows
//! its details card; clicking one opens its page.

use super::NativeView;
use super::event_details;
use super::grids::HOUR_HEIGHT;
use crate::native_view::card::hover_card;
use chrono::NaiveDate;
use crumble_calendar::events::{columns, day_order, minutes_on, on_day};
use crumble_eventkit::{Calendar, Event, When};
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::{ActiveTheme, StyledExt, v_flex};
use gpui_kit::{
    App, Context, Div, ElementId, Hsla, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
    relative,
};
use std::rc::Rc;

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
/// Space after a Week view block, before the block beside it or its column's edge.
const BLOCK_GAP_X: f32 = 6.0;
/// Space between Week view blocks one under another; half of it sits under the hour line.
const BLOCK_GAP_Y: f32 = 4.0;
/// Line height inside a Week view block, tighter than the theme's so two lines fit in 45 minutes.
const BLOCK_LINE_HEIGHT: f32 = 15.0;

/// Opens an Event's page when its chip is clicked.
pub(super) type OpenEvent = Rc<dyn Fn(&Event, &mut App)>;

/// Opens the clicked Event's page in `cx`'s view.
pub(super) fn open_in(cx: &Context<NativeView>) -> OpenEvent {
    let view = cx.weak_entity();
    Rc::new(move |event, cx| {
        let _ = view.update(cx, |view, cx| view.open_event(event.clone(), cx));
    })
}

/// An Event with what drawing it needs from its Calendar and Account.
#[derive(Clone)]
pub(super) struct Shown {
    pub(super) event: Event,
    /// The Account's color.
    pub(super) color: Hsla,
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
            })
        })
        .collect()
}

#[derive(Clone, Copy)]
enum ChipKind {
    /// A Week view block for a timed Event; a short one shows only its title.
    Block { one_line: bool },
    /// A Month view line for a timed Event: dot, start time, title.
    Line,
    /// A tinted bar for an all-day Event.
    Bar,
}

/// One Event's clickable shape, showing its details card while the pointer rests on it.
#[derive(IntoElement)]
struct EventChip {
    id: ElementId,
    kind: ChipKind,
    time: Option<SharedString>,
    shown: Shown,
    open: Box<dyn Fn(&mut App)>,
}

impl RenderOnce for EventChip {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let color = self.shown.color;
        let tint = color.opacity(0.22);
        let title: SharedString = if self.shown.event.title.is_empty() {
            "(No title)".into()
        } else {
            self.shown.event.title.clone().into()
        };
        let chip = div()
            .id(self.id.clone())
            .test_support()
            // Focusable, so Tab reaches it and Enter or Space opens it as a click does.
            .tab_index(0)
            .aria_label(title.clone())
            .cursor_pointer()
            .overflow_hidden()
            .text_xs()
            .text_color(theme.foreground)
            // The day under a Month view Event opens its week; the Event opens its own page.
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                (self.open)(cx);
            });
        let chip = match self.kind {
            ChipKind::Block { one_line } => chip
                .size_full()
                .line_height(px(BLOCK_LINE_HEIGHT))
                .px_1()
                .rounded(px(4.0))
                .border_l(px(3.0))
                .border_color(color)
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
                .hover({
                    let hover = theme.muted;
                    move |style| style.bg(hover)
                })
                .child(div().size(px(6.0)).flex_shrink_0().rounded_full().bg(color))
                .children(self.time.map(|time| {
                    div()
                        .flex_shrink_0()
                        .text_color(theme.muted_foreground)
                        .child(time)
                }))
                .child(div().min_w_0().truncate().child(title)),
        };
        let shown = self.shown;
        hover_card(
            self.id,
            chip,
            move |_, cx| event_details::card(&shown, cx).into_any_element(),
            window,
            cx,
        )
    }
}

/// The chip of `shown` on `date`: hovering shows its details card, clicking opens its page.
fn event(shown: &Shown, date: NaiveDate, kind: ChipKind, open: &OpenEvent) -> EventChip {
    let event = &shown.event;
    let id = format!("event-{}-{date}", event.id);
    let time = match (kind, event.when) {
        (ChipKind::Bar, _) | (_, When::AllDay { .. }) => None,
        (_, When::Timed { start, .. }) => Some(start.format("%H:%M").to_string().into()),
    };
    let (open, opened) = (open.clone(), event.clone());
    EventChip {
        id: ElementId::Name(format!("{id}-chip").into()),
        kind,
        time,
        shown: shown.clone(),
        open: Box::new(move |cx| open(&opened, cx)),
    }
}

/// The all-day Events of each of `days`, in Week view's strip above the hours; `None` when the
/// week has none.
pub(super) fn all_day_strip(
    days: &[NaiveDate],
    shown: &[Shown],
    open: &OpenEvent,
    cx: &App,
) -> Option<Vec<Div>> {
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
                stacked(day, &all_day, date, ChipKind::Bar, ALL_DAY_LINES, open, cx)
            })
            .collect(),
    )
}

/// A Month view day's Events, all-day first.
pub(super) fn month_day(date: NaiveDate, shown: &[Shown], open: &OpenEvent, cx: &App) -> Div {
    let day = day_order(shown.iter().map(|shown| &shown.event), date);
    let all: Vec<&Shown> = shown.iter().collect();
    stacked(day, &all, date, ChipKind::Line, MONTH_LINES, open, cx)
}

/// The first `limit` of `day` as chips, then "+N more" for the rest.
fn stacked(
    day: Vec<&Event>,
    shown: &[&Shown],
    date: NaiveDate,
    timed_kind: ChipKind,
    limit: usize,
    open: &OpenEvent,
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
                    .child(self::event(shown, date, kind, open)),
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
pub(super) fn day_blocks(date: NaiveDate, shown: &[Shown], open: &OpenEvent) -> Div {
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
                    .top(px(to_px(start) + BLOCK_GAP_Y / 2.0))
                    .h(px(to_px(end - start) - BLOCK_GAP_Y))
                    .left(relative(column as f32 / width as f32))
                    .w(relative(1.0 / width as f32))
                    .pr(px(BLOCK_GAP_X))
                    .child(event(
                        shown,
                        date,
                        ChipKind::Block {
                            one_line: to_px(end - start) < TWO_LINE_HEIGHT,
                        },
                        open,
                    ))
            },
        ))
}

#[cfg(test)]
mod tests {
    use super::{OpenEvent, Shown, month_day, shown};
    use chrono::NaiveDate;
    use crumble_eventkit::{Calendar, Event, Join, Participant, Response, When};
    use gpui_kit::base::TestSupportExt;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Bounds, Context, Hsla, InteractiveElement, IntoElement, ParentElement, Render,
        StatefulInteractiveElement, Styled, TestAppContext, Window, WindowBounds, WindowOptions,
        div, px, rgb, size,
    };
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

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
            description: None,
            join: None,
            participants: Vec::new(),
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

    /// A Month view day holding one Event, recording which Events open and the clicks that
    /// reach the day.
    struct Day {
        shown: Vec<Shown>,
        opened: Rc<RefCell<Vec<String>>>,
        day_clicks: usize,
    }

    impl Render for Day {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let opened = self.opened.clone();
            let open: OpenEvent =
                Rc::new(move |event, _| opened.borrow_mut().push(event.id.clone()));
            let date = NaiveDate::from_ymd_opt(2026, 10, 14).unwrap_or_default();
            div()
                .id("day")
                .test_support()
                // As wide as a Month view day, so the card has room beside it.
                .w(px(160.))
                .h_full()
                .on_click(cx.listener(|day, _, _, _| day.day_clicks += 1))
                .child(month_day(date, &self.shown, &open, cx))
        }
    }

    #[gpui_kit::test]
    fn an_event_shows_its_details_on_hover_and_opens_on_click(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let standup = Event {
            description: Some("Agenda".into()),
            join: Some(Join {
                url: "https://meet.google.com/abc-defg-hij".into(),
                service: "Google Meet",
            }),
            participants: vec![Participant {
                name: "Ada".into(),
                response: Response::Accepted,
                organizer: true,
                you: false,
            }],
            ..event("standup", "work")
        };
        let opened = Rc::new(RefCell::new(Vec::new()));
        let (handle, day) = cx
            .update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            Default::default(),
                            size(px(800.), px(600.)),
                        ))),
                        ..Default::default()
                    },
                    cx,
                    |_, cx| {
                        cx.new(|_| Day {
                            shown: vec![Shown {
                                event: standup,
                                color: rgb(0xff0000).into(),
                            }],
                            opened: opened.clone(),
                            day_clicks: 0,
                        })
                    },
                )
            })
            .unwrap_or_else(|error| panic!("open test window: {error}"));
        let chip = "event-standup-2026-10-14-chip";

        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.hover(chip, cx);
            window.render_frame(cx);
            assert!(window.try_find("hover-card").is_none(), "opened at once");
        })
        .unwrap_or_else(|error| panic!("update test window: {error}"));
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.render_frame(cx);
            // Beside the Event, never over it.
            let (card, trigger) = (
                window.find("hover-card").bounds(),
                window.find(chip).bounds(),
            );
            assert!(card.left() > trigger.right(), "{card:?} covers {trigger:?}");
            window.click(chip, cx);
            // The click also focuses the Event, which the keyboard then opens.
            window.press("enter", cx);
            window.press("space", cx);
        })
        .unwrap_or_else(|error| panic!("update test window: {error}"));

        // Each opens the Event and stops before the day, which would open its week.
        assert_eq!(*opened.borrow(), ["standup"; 3]);
        cx.update(|cx| assert_eq!(day.read(cx).day_clicks, 0));
    }
}
