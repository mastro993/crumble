//! Agenda page: every Event up to the end of Today and every Quick note, newest first, in a list
//! that reads further into the past as it scrolls toward its end. Selecting a row, or clicking an
//! Event in the Calendar page, shows its Note beside the list.

mod now_hover;
mod row;
mod view;

use super::NativeView;
use chrono::{DateTime, Local};
use crumble_calendar::agenda::Row;
use crumble_calendar::events;
use crumble_eventkit::Event;
use crumble_notes::{Note, NoteId};
use crumble_sync::{AgendaEvents, OlderEventsRead};
use gpui_kit::{
    App, AppContext, Context, Entity, ListAlignment, ListOffset, ListState, Pixels, Window,
    ease_in_out, px,
};
use now_hover::NowHover;
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Scrolling within this many rows of the end reads further back.
const PREFETCH_ROWS: usize = 20;
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
    /// Whether the list still has to scroll the selected row into view, after an Event was
    /// opened from outside the list.
    reveal: bool,
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
    /// When the first read finished and the list began to show.
    revealed: Option<Instant>,
    /// Whether the pointer is on the Now line, which then shows the time above it.
    now_hover: Rc<Cell<NowHover>>,
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
            reveal: false,
            rows: Rc::default(),
            built: None,
            list,
            near_end,
            scrolling_up,
            ticking: false,
            revealed: None,
            now_hover: Rc::default(),
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

    /// Keeps the listed copy of a Quick note current, and its row when its title or its time
    /// changed.
    pub(super) fn note_changed(&mut self, note: &Note) {
        if let Some(listed) = self
            .quick_notes
            .iter_mut()
            .find(|listed| listed.id == note.id)
        {
            // The row shows the title at the time the Note is listed at.
            if listed.title != note.title || listed.listed_at() != note.listed_at() {
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
            Row::Event(event) | Row::Current(event) => Some(Selected::Event(event.clone())),
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
            (Some(Selected::Event(selected)), Row::Event(event) | Row::Current(event)) => {
                selected.id == event.id
            }
            (Some(Selected::QuickNote(id)), Row::QuickNote(note)) => id.to_string() == note.id,
            _ => false,
        }
    }

    /// Keeps the selection on the same row after the rows are built again: an Event is found
    /// again even after it moved, and one with no row, as one opened from the Calendar page
    /// outside the days read, stays as it was opened; a Quick note that is gone clears it.
    fn reselect(&mut self) {
        self.selected = match self.selected.take() {
            Some(Selected::Event(selected)) => {
                let events: Vec<Rc<Event>> = self
                    .rows
                    .iter()
                    .filter_map(|row| match row {
                        Row::Event(event) | Row::Current(event) => Some(event.clone()),
                        _ => None,
                    })
                    .collect();
                let id = events::latest(&events, &selected).id.clone();
                let event = events.into_iter().find(|event| event.id == id);
                Some(Selected::Event(event.unwrap_or(selected)))
            }
            Some(Selected::QuickNote(id)) => self
                .quick_note(&id)
                .is_some()
                .then_some(Selected::QuickNote(id)),
            None => None,
        };
    }

    /// Scrolls the selected row into view once, when an Event was opened from outside the
    /// list; an Event with no row gives up.
    fn reveal_selected(&mut self) {
        if !std::mem::take(&mut self.reveal) {
            return;
        }
        let Some(ix) = self.rows.iter().position(|row| self.is_selected(row)) else {
            return;
        };
        if self.list.bounds_for_item(ix).is_some() {
            self.list.scroll_to_reveal_item(ix);
        } else {
            // Revealing needs the rows above measured; a row never drawn goes near the top,
            // under the row before it, as its day header, clear of the list's top fade.
            self.list.scroll_to(ListOffset {
                item_ix: ix.saturating_sub(1),
                offset_in_item: px(0.0),
            });
        }
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
