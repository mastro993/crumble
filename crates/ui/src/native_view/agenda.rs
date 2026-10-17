//! Agenda page: every Event that has started and every Quick note, newest first, in a list
//! that reads further into the past as it scrolls toward its end. Selecting a row shows its
//! Note beside the list.

use super::NativeView;
use super::account_dot::tint;
use super::controls::{app_icon, button};
use super::route::Route;
use super::sidebar_row::hover_fill;
use chrono::{DateTime, Local, NaiveDate, Timelike, Utc};
use crumble_calendar::agenda::{self, QuickNote, Row, day_label, horizon, time_label};
use crumble_calendar::{events, local_midnight, today};
use crumble_eventkit::{Access, Event};
use crumble_notes::{Note, NoteId, Notes};
use crumble_sync::{AgendaEvents, OlderEventsRead};
use crumble_theme::CONTENT_PADDING;
use crumble_theme::sidebar::{RADIUS, ROW_PADDING};
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::{
    ActiveTheme, Sizable, StyledExt, button::ButtonVariants, h_flex, v_flex,
};
use gpui_kit::{
    AnyElement, App, AppContext, Context, ElementId, Entity, Hsla, InteractiveElement, IntoElement,
    ListAlignment, ListOffset, ListState, ParentElement, Pixels, StatefulInteractiveElement,
    Styled, Window, div, ease_in_out, linear_color_stop, linear_gradient, list,
    prelude::FluentBuilder, px,
};
use std::cell::Cell;
use std::collections::HashMap;
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

/// The selected row of the Agenda list.
pub(super) enum Selected {
    Event(Rc<Event>),
    QuickNote(NoteId),
}

/// The Events and Quick notes read so far and the list showing them; kept in memory while the
/// app runs, so the Agenda reopens where it was left.
pub(super) struct AgendaPage {
    pub(super) events: Entity<AgendaEvents>,
    /// The Quick notes of the days the Events cover, newest first.
    quick_notes: Vec<Note>,
    pub(super) selected: Option<Selected>,
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

impl AgendaPage {
    /// An empty list; nothing is read until the Agenda shows.
    pub(super) fn new(cx: &mut Context<NativeView>) -> Self {
        let events = cx.new(|_| AgendaEvents::default());
        cx.observe(&events, |_, _, cx| cx.notify()).detach();
        let list = ListState::new(0, ListAlignment::Top, px(200.0));
        let near_end = Rc::new(Cell::new(false));
        let scrolling_up = Rc::new(Cell::new(None));
        let (near, up) = (near_end.clone(), scrolling_up.clone());
        list.set_scroll_handler(move |event, _, _| {
            near.set(event.visible_range.end + PREFETCH_ROWS >= event.count);
            up.set(None);
        });
        // An empty stretch reads on while the end is still in view; Events read further back
        // wait for the next scroll to come near the end.
        let near = near_end.clone();
        cx.subscribe(&events, move |_, _, _: &OlderEventsRead, _| near.set(false))
            .detach();
        Self {
            events,
            quick_notes: Vec::new(),
            selected: None,
            rows: Rc::default(),
            built: None,
            list,
            near_end,
            scrolling_up,
            ticking: false,
        }
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

    pub(super) fn quick_note(&self, id: &NoteId) -> Option<&Note> {
        self.quick_notes.iter().find(|note| &note.id == id)
    }

    /// Keeps the listed copy of a Quick note current, and its row when its title changed.
    pub(super) fn note_changed(&mut self, note: &Note) {
        if let Some(listed) = self
            .quick_notes
            .iter_mut()
            .find(|listed| listed.id == note.id)
        {
            if listed.title != note.title {
                self.built = None;
            }
            listed.clone_from(note);
        }
    }

    /// Lists a Quick note just made and selects its row.
    pub(super) fn select_new_quick_note(&mut self, note: Note) {
        self.selected = Some(Selected::QuickNote(note.id.clone()));
        self.quick_notes.insert(0, note);
        self.built = None;
    }

    /// Drops a deleted Quick note's row and clears the selection.
    pub(super) fn quick_note_deleted(&mut self, id: &NoteId) {
        self.quick_notes.retain(|note| &note.id != id);
        self.selected = None;
        self.built = None;
    }

    /// Selects the row of `row`, unless it is a day header or a line.
    fn select(&mut self, row: &Row) {
        self.selected = match row {
            Row::Event(event) => Some(Selected::Event(event.clone())),
            Row::QuickNote(listed) => self
                .quick_notes
                .iter()
                .find(|note| note.id.to_string() == listed.id)
                .map(|note| Selected::QuickNote(note.id.clone())),
            _ => return,
        };
    }

    /// Whether `row` is the selected one.
    fn is_selected(&self, row: &Row) -> bool {
        match (&self.selected, row) {
            (Some(Selected::Event(selected)), Row::Event(event)) => selected.id == event.id,
            (Some(Selected::QuickNote(id)), Row::QuickNote(note)) => id.to_string() == note.id,
            _ => false,
        }
    }

    /// Keeps the selection on the same row after the rows are built again: an Event is found
    /// again even after it moved; a row that is gone clears it.
    fn reselect(&mut self) {
        self.selected = match self.selected.take() {
            Some(Selected::Event(selected)) => {
                let events: Vec<Rc<Event>> = self
                    .rows
                    .iter()
                    .filter_map(|row| match row {
                        Row::Event(event) => Some(event.clone()),
                        _ => None,
                    })
                    .collect();
                let id = events::latest(&events, &selected).id.clone();
                events
                    .into_iter()
                    .find(|event| event.id == id)
                    .map(|event| Selected::Event(event.clone()))
            }
            Some(Selected::QuickNote(id)) => self
                .quick_note(&id)
                .is_some()
                .then_some(Selected::QuickNote(id)),
            None => None,
        };
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

impl NativeView {
    /// Keeps the Agenda current while it shows: reads it the first time, again after the day,
    /// the shown Calendars, or EventKit change, and further back near the end of the list;
    /// rebuilds the rows each minute so Events join as they start.
    pub(super) fn maintain_agenda(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.agenda.step_scroll_to_top(window, cx);
        let sync = self.calendar_sync.read(cx);
        if sync.access() != Access::Granted {
            return;
        }
        let ids = self.store.shown_calendar_ids(sync.calendars());
        let today = today();
        let horizon = horizon(today);
        let notes = &mut self.notes;
        let agenda = &mut self.agenda;
        let wants_more = agenda.rows.len() < MIN_ROWS || agenda.near_end.get();
        agenda.events.update(cx, |events, cx| {
            events.read_next(today, ids, wants_more, cx);
        });

        let loaded = agenda.events.read(cx);
        let now = Local::now();
        let minute = now
            .with_second(0)
            .and_then(|now| now.with_nanosecond(0))
            .unwrap_or(now);
        if agenda.built != Some((minute, loaded.reads())) {
            agenda.built = Some((minute, loaded.reads()));
            let from = loaded.span().map_or(today, |span| span.from);
            let end = !loaded.reading() && from <= horizon;
            agenda.quick_notes = read_quick_notes(notes, from, today);
            let quick_notes: Vec<Rc<QuickNote>> = agenda
                .quick_notes
                .iter()
                .map(|note| {
                    Rc::new(QuickNote {
                        id: note.id.to_string(),
                        at: note.created_at.with_timezone(&Local),
                        label: quick_note_label(note),
                    })
                })
                .collect();
            let rows = agenda::rows(loaded.events(), &quick_notes, now, from, end);
            agenda.set_rows(rows);
            agenda.reselect();
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

    /// Selects the row of `row`, showing its Note beside the list.
    fn select_agenda_row(&mut self, row: &Row, window: &mut Window, cx: &mut Context<Self>) {
        self.agenda.select(row);
        self.maintain_note_detail(window, cx);
        cx.notify();
    }

    /// The Agenda page's header: New note, then the selected row's Note actions. Without
    /// calendar access the list shows no rows, so New note is left out.
    pub(super) fn render_agenda_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let granted = self.calendar_sync.read(cx).access() == Access::Granted;
        h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .when(granted, |toolbar| {
                toolbar.child(
                    button("agenda-new-note")
                        .ghost()
                        .small()
                        .icon(app_icon("note-add").size_4())
                        .tooltip("New note")
                        .on_click(
                            cx.listener(|view, _, window, cx| view.new_note(true, window, cx)),
                        ),
                )
            })
            // Empty header space still drags and zooms the window.
            .child(div().flex_1())
            .children(
                self.agenda
                    .selected
                    .is_some()
                    .then(|| self.render_note_actions(cx))
                    .flatten(),
            )
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
                        let color = self.store.account_color(&calendar.account_id);
                        (calendar.id.clone(), tint(color, cx))
                    })
                    .collect(),
            );
            let rows = self.agenda.rows.clone();
            let selected = rows.iter().position(|row| self.agenda.is_selected(row));
            let view = cx.entity().downgrade();
            let today = today();
            column
                .child(
                    list(self.agenda.list.clone(), move |index, _, cx| {
                        let Some(row) = rows.get(index) else {
                            return div().into_any_element();
                        };
                        let item = RowItem {
                            first: index == 0,
                            alone: rows.len() == 1,
                            selected: selected == Some(index),
                            today,
                        };
                        let view = view.clone();
                        let select =
                            Rc::new(move |row: &Row, window: &mut Window, cx: &mut App| {
                                let _ = view
                                    .update(cx, |view, cx| view.select_agenda_row(row, window, cx));
                            });
                        render_row(row, item, &colors, select, cx)
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
        let content = self.agenda.selected.is_some().then(|| {
            div()
                .id("agenda-content")
                .flex_1()
                .min_w_0()
                .h_full()
                .overflow_y_scroll()
                .px(px(CONTENT_PADDING))
                .pb_6()
                .child(self.render_note_detail(cx))
        });
        h_flex()
            .size_full()
            .child(column)
            .children(content)
            .into_any_element()
    }
}

/// The Quick notes made from `from` through `today`, newest first; none when they cannot be
/// read, which is logged.
fn read_quick_notes(notes: &mut Notes, from: NaiveDate, today: NaiveDate) -> Vec<Note> {
    let start = |day: NaiveDate| local_midnight(day).map(|start| start.with_timezone(&Utc));
    let (Some(from), Some(to)) = (start(from), today.succ_opt().and_then(start)) else {
        return Vec::new();
    };
    notes.quick_notes(from..to).unwrap_or_else(|error| {
        eprintln!("Could not read the Quick notes: {error}");
        Vec::new()
    })
}

/// A Quick note's title, or `Quick note` when it has none.
fn quick_note_label(note: &Note) -> String {
    note.title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| "Quick note".to_owned())
}

/// The Agenda column's opaque surface: half the sidebar over the page, so this second-level
/// column sits between them.
fn surface(cx: &App) -> Hsla {
    let theme = cx.theme();
    theme.background.blend(theme.sidebar.opacity(0.5))
}

/// Where a row sits in the Agenda list: `first` opens it, `alone` says it holds nothing else.
struct RowItem {
    first: bool,
    alone: bool,
    selected: bool,
    today: NaiveDate,
}

type SelectRow = Rc<dyn Fn(&Row, &mut Window, &mut App)>;

/// One item of the Agenda list; Events and Quick notes select on click, and on Enter or Space
/// once Tab reaches them.
fn render_row(
    row: &Row,
    item: RowItem,
    colors: &HashMap<String, Hsla>,
    select: SelectRow,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let RowItem {
        first,
        alone,
        selected,
        today,
    } = item;
    let selectable = |id: String, color: Hsla, title: String, time: String| {
        let row = row.clone();
        let hover = hover_fill(cx);
        div()
            .px(px(CONTENT_PADDING - ROW_PADDING))
            .child(
                h_flex()
                    .id(ElementId::Name(id.into()))
                    .test_support()
                    .tab_index(0)
                    .aria_label(title.clone())
                    .cursor_pointer()
                    .px(px(ROW_PADDING))
                    .py_1p5()
                    .gap_3()
                    .items_stretch()
                    .rounded(px(RADIUS))
                    .when(selected, |row| row.bg(theme.sidebar_accent))
                    .when(!selected, |row| row.hover(move |style| style.bg(hover)))
                    .on_click(move |_, window, cx| select(&row, window, cx))
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
                                    .child(time),
                            ),
                    ),
            )
            .into_any_element()
    };
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
            selectable(
                format!("agenda-event-{}", event.id),
                color,
                title,
                time_label(&event.when),
            )
        }
        // No Account color: a Quick note belongs to none.
        Row::QuickNote(note) => selectable(
            format!("agenda-note-{}", note.id),
            theme.muted_foreground.opacity(0.5),
            note.label.clone(),
            note.at.format("%H:%M").to_string(),
        ),
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

#[cfg(test)]
mod tests {
    use super::{RowItem, render_row};
    use chrono::Local;
    use crumble_calendar::agenda::{QuickNote, Row};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Bounds, Context, IntoElement, ParentElement, Render, Styled, TestAppContext,
        Window, WindowBounds, WindowOptions, div, px, size,
    };
    use std::collections::HashMap;
    use std::rc::Rc;

    /// Stands in for the Agenda list, counting the rows it is asked to select.
    #[derive(Default)]
    struct List {
        selected: Vec<Row>,
    }

    impl Render for List {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let row = Row::QuickNote(Rc::new(QuickNote {
                id: "kickoff".into(),
                at: Local::now(),
                label: "Kickoff ideas".into(),
            }));
            let item = RowItem {
                first: true,
                alone: true,
                selected: false,
                today: Local::now().date_naive(),
            };
            let list = cx.entity().downgrade();
            let select = Rc::new(move |row: &Row, _: &mut Window, cx: &mut gpui_kit::App| {
                let _ = list.update(cx, |list, _| list.selected.push(row.clone()));
            });
            div()
                .size_full()
                .child(render_row(&row, item, &HashMap::new(), select, cx))
        }
    }

    #[gpui_kit::test]
    fn a_quick_note_row_selects_on_click_enter_and_space(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (handle, list) = cx
            .update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            Default::default(),
                            size(px(320.), px(80.)),
                        ))),
                        ..Default::default()
                    },
                    cx,
                    |_, cx| cx.new(|_| List::default()),
                )
            })
            .unwrap_or_else(|error| panic!("open test window: {error}"));

        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            // The click also focuses the row, which the keyboard then selects.
            window.click("agenda-note-kickoff", cx);
            window.press("enter", cx);
            window.press("space", cx);
        })
        .unwrap_or_else(|error| panic!("update test window: {error}"));

        cx.update(|cx| {
            let selected = &list.read(cx).selected;
            assert_eq!(selected.len(), 3);
            assert!(matches!(&selected[0], Row::QuickNote(note) if note.id == "kickoff"));
        });
    }
}
