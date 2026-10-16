//! Agenda page: every Event that has started, newest first, in a list that reads further into
//! the past as it scrolls toward its end.

use super::NativeView;
use super::account_dot::tint;
use super::controls::{app_icon, button};
use super::route::Route;
use chrono::{DateTime, Local, NaiveDate, Timelike};
use crumble_calendar::agenda::{self, Row, Span, day_label, horizon, time_label};
use crumble_calendar::{local_midnight, today};
use crumble_eventkit::{self as eventkit, Access, Event};
use crumble_theme::CONTENT_PADDING;
use gpui_kit::component::{ActiveTheme, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext, Context, Hsla, IntoElement, ListAlignment, ListOffset, ListState,
    ParentElement, Pixels, Styled, Task, Window, div, ease_in_out, linear_color_stop,
    linear_gradient, list, prelude::FluentBuilder, px,
};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Width of the Agenda list.
const LIST_WIDTH: f32 = 320.0;
/// The list reads further back until it holds this many rows, enough to fill a tall window.
const MIN_ROWS: usize = 40;
/// Scrolling within this many rows of the end reads further back.
const PREFETCH_ROWS: usize = 20;
/// Thickness of the Now line, and the size of the dot that starts it.
const NOW_LINE: f32 = 2.0;
const NOW_DOT: f32 = 8.0;
/// Height of the fade that rows scroll away under at the list's top.
const TOP_FADE: f32 = 32.0;
/// How long the list takes to scroll back to its top.
const SCROLL_TO_TOP: Duration = Duration::from_millis(300);

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
    /// The last read that failed and whether it was older; it is not tried again until
    /// EventKit reports a change or another read is wanted.
    failed: Option<(Span, bool)>,
    /// Finished reads, so the rows are built again after each.
    reads: u64,
    rows: Rc<Vec<Row>>,
    /// The minute and read count `rows` were built for.
    built: Option<(DateTime<Local>, u64)>,
    list: ListState,
    /// Whether the last scroll came near the end of the list.
    near_end: Rc<Cell<bool>>,
    /// When a scroll back to the top began and how far down it began; the user scrolling
    /// stops it.
    scrolling_up: Rc<Cell<Option<(Instant, Pixels)>>>,
    /// Whether the minute tick that keeps the rows current is running.
    ticking: bool,
}

impl Default for AgendaPage {
    fn default() -> Self {
        let list = ListState::new(0, ListAlignment::Top, px(200.0));
        let near_end = Rc::new(Cell::new(false));
        let scrolling_up = Rc::new(Cell::new(None));
        let (near, up) = (near_end.clone(), scrolling_up.clone());
        list.set_scroll_handler(move |event, _, _| {
            near.set(event.visible_range.end + PREFETCH_ROWS >= event.count);
            up.set(None);
        });
        Self {
            events: Vec::new(),
            span: None,
            stale: false,
            reading: false,
            read: Task::ready(()),
            failed: None,
            reads: 0,
            rows: Rc::default(),
            built: None,
            list,
            near_end,
            scrolling_up,
            ticking: false,
        }
    }
}

impl AgendaPage {
    /// Reads the covered days again the next time the Agenda shows.
    pub(super) fn mark_stale(&mut self) {
        self.stale = true;
        self.failed = None;
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

    /// How far the list is scrolled from its top.
    fn scroll_offset(&self) -> Pixels {
        -self.list.scroll_px_offset_for_scrollbar().y
    }

    /// Moves a scroll back to the top one frame on, or straight there when the system asks
    /// for less motion.
    fn step_scroll_to_top(&self, window: &Window, cx: &App) {
        let Some((started, from)) = self.scrolling_up.get() else {
            return;
        };
        let progress = started.elapsed().as_secs_f32() / SCROLL_TO_TOP.as_secs_f32();
        if progress < 1.0 && !cx.reduce_motion() {
            let goal = from * (1.0 - ease_in_out(progress));
            self.list.scroll_by(goal - self.scroll_offset());
            window.request_animation_frame();
        } else {
            self.scrolling_up.set(None);
            self.list.scroll_to(ListOffset::default());
        }
    }
}

/// The Events starting from `from` up to the day before `to`, read in the pieces EventKit
/// accepts and kept as [`agenda::span_events`] says.
fn read_span(from: NaiveDate, to: NaiveDate, ids: &[String]) -> anyhow::Result<Vec<Event>> {
    let now = Local::now();
    let mut pieces = Vec::new();
    for piece in agenda::read_pieces(from, to) {
        if let (Some(start), Some(stop)) = (local_midnight(piece.start), local_midnight(piece.end))
        {
            let read = eventkit::events(start, stop, ids)?;
            pieces.push((piece, read));
        }
    }
    Ok(agenda::span_events(pieces, now))
}

impl NativeView {
    /// Starts reading `span`; `older` adds its Events to those already read instead of
    /// replacing them. A failed read is logged, and the last Events and the days they cover
    /// stay.
    fn read_agenda(&mut self, span: Span, older: bool, cx: &mut Context<Self>) {
        let agenda = &mut self.agenda;
        let previous = agenda.span.replace(span.clone());
        agenda.reading = true;
        // An older read covers only the days before those already read.
        let to = match &previous {
            Some(previous) if older => previous.from,
            _ => span.to,
        };
        let requested = span.clone();
        // Apple advises reading off the main thread.
        let read = cx.background_spawn(async move { read_span(span.from, to, &span.calendar_ids) });
        agenda.read = cx.spawn(async move |this, cx| {
            let result = read.await;
            let _ = this.update(cx, |view, cx| {
                let agenda = &mut view.agenda;
                agenda.reading = false;
                match result {
                    Ok(mut events) => {
                        agenda.failed = None;
                        if older {
                            // A long In progress Event was read with Today already.
                            let known: HashSet<&str> = agenda
                                .events
                                .iter()
                                .map(|event| event.id.as_str())
                                .collect();
                            events.retain(|event| !known.contains(event.id.as_str()));
                        }
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
                        agenda.span = previous;
                        agenda.failed = Some((requested, older));
                    }
                }
                cx.notify();
            });
        });
    }

    /// Keeps the Agenda current while it shows: reads it the first time, again after the day,
    /// the shown Calendars, or EventKit change, and further back near the end of the list;
    /// rebuilds the rows each minute so Events join as they start.
    pub(super) fn maintain_agenda(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.agenda.step_scroll_to_top(window, cx);
        let sync = self.calendar_sync.read(cx);
        if sync.access() != Access::Granted {
            return;
        }
        let ids = self.settings.shown_calendar_ids(sync.calendars());
        let today = today();
        let horizon = horizon(today);
        let agenda = &mut self.agenda;
        let wants_more = agenda.rows.len() < MIN_ROWS || agenda.near_end.get();
        let wanted = agenda::next_read(
            agenda.span.as_ref(),
            agenda.stale,
            agenda.reading,
            wants_more,
            today,
            ids,
        );
        if let Some((span, older)) = wanted.filter(|wanted| agenda.failed.as_ref() != Some(wanted))
        {
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
            let from = agenda.span.as_ref().map_or(today, |span| span.from);
            let end = !agenda.reading && from <= horizon;
            let rows = agenda::rows(&agenda.events, now, from, end);
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

    /// The Agenda column's surface from the window's top to its bottom, behind the header and
    /// the list.
    pub(super) fn render_agenda_backdrop(window: &Window, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .w(px(LIST_WIDTH))
            .bg(surface(cx))
            // One device pixel, matching the sidebar's hairline.
            .border_r(px(1.0 / window.scale_factor()))
            .border_color(theme.border)
    }

    /// The Agenda list beside an empty content area, with a button back to the top once the
    /// list is scrolled.
    pub(super) fn render_agenda(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let sync = self.calendar_sync.read(cx);
        let column = div().relative().flex_shrink_0().w(px(LIST_WIDTH)).h_full();
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
            column
                .child(
                    list(self.agenda.list.clone(), move |index, _, cx| {
                        rows.get(index).map_or_else(
                            || div().into_any_element(),
                            |row| render_row(row, index == 0, rows.len() == 1, today, &colors, cx),
                        )
                    })
                    .size_full(),
                )
                .child({
                    // Rows fade away under the list's top; the fade grows in over its first
                    // scrolled points so the top row isn't dimmed at rest.
                    let surface = surface(cx);
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        // Clear of the backdrop's hairline.
                        .right(px(1.0 / window.scale_factor()))
                        .h(self.agenda.scroll_offset().min(px(TOP_FADE)))
                        .bg(linear_gradient(
                            180.0,
                            linear_color_stop(surface, 0.0),
                            linear_color_stop(surface.opacity(0.0), 1.0),
                        ))
                })
                .when(self.agenda.scroll_offset() > px(0.0), |column| {
                    column.child(
                        h_flex()
                            .absolute()
                            .top_3()
                            .left_0()
                            .right_0()
                            .justify_center()
                            .child(
                                button("agenda-top")
                                    .outline()
                                    .small()
                                    .rounded(px(999.0))
                                    .shadow_md()
                                    .icon(app_icon("scroll-to-top"))
                                    .accessibility_label("Scroll to top")
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        let agenda = &view.agenda;
                                        agenda
                                            .scrolling_up
                                            .set(Some((Instant::now(), agenda.scroll_offset())));
                                        cx.notify();
                                    })),
                            ),
                    )
                })
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

/// The Agenda column's opaque surface: half the sidebar over the page, so this second-level
/// column sits between them.
fn surface(cx: &App) -> Hsla {
    let theme = cx.theme();
    theme.background.blend(theme.sidebar.opacity(0.5))
}

/// One item of the Agenda list; `first` says it opens the list, and `alone` says the list
/// holds nothing else.
fn render_row(
    row: &Row,
    first: bool,
    alone: bool,
    today: NaiveDate,
    colors: &HashMap<String, Hsla>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let item = div().px(px(CONTENT_PADDING));
    match row {
        Row::Day(day) => item
            // The header strip already spaces the list from the window's top.
            .map(|item| if first { item.pt_1() } else { item.pt_4() })
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
                    .bg(theme.danger),
            )
            .child(div().flex_1().h(px(NOW_LINE)).bg(theme.danger))
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
