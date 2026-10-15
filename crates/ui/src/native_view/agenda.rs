//! Agenda page: every Event that has started, newest first, in a list that reads further into
//! the past as it scrolls toward its end.

mod rows;

use super::NativeView;
use super::account_dot::tint;
use super::calendar::{local_midnight, today};
use super::route::Route;
use chrono::{DateTime, Days, Local, Months, NaiveDate, Timelike};
use crumble_eventkit::{self as eventkit, Access, Event};
use crumble_theme::CONTENT_PADDING;
use gpui_kit::component::{ActiveTheme, StyledExt, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext, Context, Hsla, IntoElement, ListAlignment, ListState,
    ParentElement, Styled, Task, Window, div, list, px,
};
use rows::{Row, day_label, start_day, time_label};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

/// Width of the Agenda list.
const LIST_WIDTH: f32 = 320.0;
/// Days each read further into the past covers.
const CHUNK_DAYS: u64 = 30;
/// How far back the Agenda reads.
const HORIZON_MONTHS: u32 = 5 * 12;
/// EventKit searches at most four years at a time, so longer reads are split.
const READ_DAYS: u64 = 365;
/// The list reads further back until it holds this many rows, enough to fill a tall window.
const MIN_ROWS: usize = 40;
/// Scrolling within this many rows of the end reads further back.
const PREFETCH_ROWS: usize = 20;
/// Thickness of the Now line, and the size of the dot that starts it.
const NOW_LINE: f32 = 2.0;
const NOW_DOT: f32 = 8.0;

/// The days and Calendars an Agenda read covers.
#[derive(Clone, PartialEq, Eq)]
struct Span {
    from: NaiveDate,
    /// The day after the last.
    to: NaiveDate,
    calendar_ids: Vec<String>,
}

/// The Events read so far and the list showing them; kept in memory while the app runs, so
/// the Agenda reopens where it was left.
pub(super) struct AgendaPage {
    events: Vec<Rc<Event>>,
    /// The days `events` cover once `read` finishes; `None` before the first read.
    span: Option<Span>,
    /// Set when EventKit reports a change, so the covered days are read again.
    stale: bool,
    reading: bool,
    read: Task<()>,
    /// Finished reads, so the rows are built again after each.
    reads: u64,
    rows: Rc<Vec<Row>>,
    /// The minute and read count `rows` were built for.
    built: Option<(DateTime<Local>, u64)>,
    list: ListState,
    /// Whether the last scroll came near the end of the list.
    near_end: Rc<Cell<bool>>,
    /// Whether the minute tick that keeps the rows current is running.
    ticking: bool,
}

impl Default for AgendaPage {
    fn default() -> Self {
        let list = ListState::new(0, ListAlignment::Top, px(200.0));
        let near_end = Rc::new(Cell::new(false));
        let near = near_end.clone();
        list.set_scroll_handler(move |event, _, _| {
            near.set(event.visible_range.end + PREFETCH_ROWS >= event.count);
        });
        Self {
            events: Vec::new(),
            span: None,
            stale: false,
            reading: false,
            read: Task::ready(()),
            reads: 0,
            rows: Rc::default(),
            built: None,
            list,
            near_end,
            ticking: false,
        }
    }
}

impl AgendaPage {
    /// Reads the covered days again the next time the Agenda shows.
    pub(super) fn mark_stale(&mut self) {
        self.stale = true;
    }

    /// Replaces the rows, splicing only those that changed, so the list keeps its measured
    /// rows and its scroll position when older days are added below or Today changes above.
    fn set_rows(&mut self, rows: Vec<Row>) {
        let old = &self.rows;
        let kept = old.iter().zip(&rows).take_while(|(a, b)| a == b).count();
        let tail = old[kept..]
            .iter()
            .rev()
            .zip(rows[kept..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        if old.is_empty() {
            // Splicing into an empty list would count the new rows as above the scroll top.
            self.list.reset(rows.len());
        } else {
            self.list
                .splice(kept..old.len() - tail, rows.len() - kept - tail);
        }
        self.rows = Rc::new(rows);
    }
}

/// The oldest day the Agenda reads.
fn horizon(today: NaiveDate) -> NaiveDate {
    today
        .checked_sub_months(Months::new(HORIZON_MONTHS))
        .unwrap_or(today)
}

/// The Events starting from `from` up to the day before `to`, in pieces EventKit accepts.
/// Each piece keeps only the Events starting inside it, so none is read twice.
fn read_span(from: NaiveDate, to: NaiveDate, ids: &[String]) -> anyhow::Result<Vec<Event>> {
    let mut events = Vec::new();
    let mut first = from;
    while first < to {
        let end = first
            .checked_add_days(Days::new(READ_DAYS))
            .map_or(to, |end| end.min(to));
        if let (Some(start), Some(stop)) = (local_midnight(first), local_midnight(end)) {
            events.extend(
                eventkit::events(start, stop, ids)?
                    .into_iter()
                    .filter(|event| (first..end).contains(&start_day(event))),
            );
        }
        first = end;
    }
    Ok(events)
}

impl NativeView {
    /// Starts reading `span`; `older` adds its Events to those already read instead of
    /// replacing them. A failed read is logged and the last Events stay.
    fn read_agenda(&mut self, span: Span, older: bool, cx: &mut Context<Self>) {
        let agenda = &mut self.agenda;
        let previous = agenda.span.replace(span.clone());
        agenda.reading = true;
        // An older read covers only the days before those already read.
        let to = match &previous {
            Some(previous) if older => previous.from,
            _ => span.to,
        };
        // Apple advises reading off the main thread.
        let read = cx.background_spawn(async move { read_span(span.from, to, &span.calendar_ids) });
        agenda.read = cx.spawn(async move |this, cx| {
            let result = read.await;
            let _ = this.update(cx, |view, cx| {
                let agenda = &mut view.agenda;
                agenda.reading = false;
                match result {
                    Ok(events) => {
                        let events: Vec<Rc<Event>> = events.into_iter().map(Rc::new).collect();
                        if older {
                            // An empty stretch reads on while the end is still in view.
                            agenda
                                .near_end
                                .set(agenda.near_end.get() && events.is_empty());
                            agenda.events.extend(events);
                        } else {
                            agenda.events = events;
                        }
                        agenda.reads += 1;
                    }
                    Err(error) => {
                        eprintln!("Could not read the Agenda: {error}");
                        if older {
                            agenda.span = previous;
                        }
                    }
                }
                cx.notify();
            });
        });
    }

    /// Keeps the Agenda current while it shows: reads it the first time, again after the day,
    /// the shown Calendars, or EventKit change, and further back near the end of the list;
    /// rebuilds the rows each minute so Events join as they start.
    pub(super) fn maintain_agenda(&mut self, cx: &mut Context<Self>) {
        let sync = self.calendar_sync.read(cx);
        if sync.access() != Access::Granted {
            return;
        }
        let ids = self.settings.shown_calendar_ids(sync.calendars());
        let today = today();
        let to = today.checked_add_days(Days::new(1)).unwrap_or(today);
        let horizon = horizon(today);
        let agenda = &mut self.agenda;
        let wanted = match &agenda.span {
            None => Some((
                Span {
                    from: to.checked_sub_days(Days::new(CHUNK_DAYS)).unwrap_or(to),
                    to,
                    calendar_ids: ids,
                },
                false,
            )),
            Some(span) if agenda.stale || span.to != to || span.calendar_ids != ids => Some((
                Span {
                    from: span.from,
                    to,
                    calendar_ids: ids,
                },
                false,
            )),
            Some(span)
                if !agenda.reading
                    && span.from > horizon
                    && (agenda.rows.len() < MIN_ROWS || agenda.near_end.get()) =>
            {
                let from = span
                    .from
                    .checked_sub_days(Days::new(CHUNK_DAYS))
                    .map_or(horizon, |from| from.max(horizon));
                Some((
                    Span {
                        from,
                        ..span.clone()
                    },
                    true,
                ))
            }
            Some(_) => None,
        };
        if let Some((span, older)) = wanted {
            agenda.stale = false;
            self.read_agenda(span, older, cx);
        }

        let agenda = &mut self.agenda;
        let now = Local::now();
        let minute = now
            .with_second(0)
            .and_then(|now| now.with_nanosecond(0))
            .unwrap_or(now);
        if agenda.built != Some((minute, agenda.reads)) {
            agenda.built = Some((minute, agenda.reads));
            let end = !agenda.reading
                && agenda
                    .span
                    .as_ref()
                    .is_some_and(|span| span.from <= horizon);
            let rows = rows::rows(&agenda.events, minute, end);
            agenda.set_rows(rows);
        }

        if !agenda.ticking {
            agenda.ticking = true;
            cx.spawn(async move |this, cx| {
                loop {
                    // Wakes on the minute, when Events start and end.
                    let wait = 60 - u64::from(Local::now().second());
                    cx.background_executor()
                        .timer(Duration::from_secs(wait))
                        .await;
                    let shown = this.update(cx, |view, cx| {
                        let shown = view.route == Route::Agenda;
                        if shown {
                            cx.notify();
                        } else {
                            view.agenda.ticking = false;
                        }
                        shown
                    });
                    if !matches!(shown, Ok(true)) {
                        break;
                    }
                }
            })
            .detach();
        }
    }

    /// The Agenda list beside an empty content area.
    pub(super) fn render_agenda(&self, window: &Window, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let sync = self.calendar_sync.read(cx);
        let column = div()
            .flex_shrink_0()
            .w(px(LIST_WIDTH))
            .h_full()
            // One device pixel, matching the sidebar's hairline.
            .border_r(px(1.0 / window.scale_factor()))
            .border_color(theme.border);
        let column = if sync.access() == Access::Granted {
            let colors: Rc<HashMap<String, Hsla>> = Rc::new(
                sync.calendars()
                    .iter()
                    .map(|calendar| {
                        let color = self.settings.account_color(&calendar.account_id);
                        (calendar.id.clone(), tint(color, cx))
                    })
                    .collect(),
            );
            let rows = self.agenda.rows.clone();
            let today = today();
            column.child(
                list(self.agenda.list.clone(), move |index, _, cx| {
                    rows.get(index).map_or_else(
                        || div().into_any_element(),
                        |row| render_row(row, rows.len() == 1, today, &colors, cx),
                    )
                })
                .size_full(),
            )
        } else {
            column.child(
                div()
                    .p(px(CONTENT_PADDING))
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("Connect your calendars from the sidebar to see past Events."),
            )
        };
        h_flex().size_full().child(column).into_any_element()
    }
}

/// One item of the Agenda list; `alone` says the list holds nothing else.
fn render_row(
    row: &Row,
    alone: bool,
    today: NaiveDate,
    colors: &HashMap<String, Hsla>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let item = div().px(px(CONTENT_PADDING));
    match row {
        Row::Day(day) => item
            .pt_4()
            .pb_1()
            .text_sm()
            .font_semibold()
            .child(day_label(*day, today))
            .into_any_element(),
        Row::Now => h_flex()
            .w_full()
            .py_1()
            // The dot hangs into the padding so the line starts level with the Events.
            .pl(px(CONTENT_PADDING - NOW_DOT / 2.0))
            .pr(px(CONTENT_PADDING))
            .child(
                div()
                    .flex_shrink_0()
                    .size(px(NOW_DOT))
                    .rounded_full()
                    .bg(theme.primary),
            )
            .child(div().flex_1().h(px(NOW_LINE)).bg(theme.primary))
            .into_any_element(),
        Row::Event(event) => {
            let color = colors
                .get(&event.calendar_id)
                .copied()
                .unwrap_or(theme.muted_foreground);
            let title = if event.title.is_empty() {
                "(No title)".to_owned()
            } else {
                event.title.clone()
            };
            h_flex()
                .w_full()
                .px(px(CONTENT_PADDING))
                .py_1p5()
                .gap_3()
                .items_stretch()
                .child(div().flex_shrink_0().w(px(3.0)).rounded_full().bg(color))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_sm().truncate().child(title))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(time_label(&event.when)),
                        ),
                )
                .into_any_element()
        }
        Row::End => item
            .py_4()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(if alone {
                "No past Events"
            } else {
                "End of Agenda"
            })
            .into_any_element(),
    }
}
