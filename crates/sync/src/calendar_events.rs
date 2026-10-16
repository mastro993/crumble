//! The Events of the dates the Calendar page shows.

use chrono::NaiveDate;
use crumble_calendar::local_midnight;
use crumble_eventkit::{self as eventkit, Event};
use gpui::{AppContext, Context, Task};

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
pub struct CalendarEvents {
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

impl CalendarEvents {
    /// The Events last read.
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Reads the Events from the first date of `range` up to the day before its end in
    /// `calendar_ids`, unless they are already read; `None` wants no Events. `force` reads
    /// again anyway, as after EventKit reports a change. A newer read replaces one in flight.
    /// A failed read is logged and the last Events stay.
    pub fn refresh(
        &mut self,
        range: Option<(NaiveDate, NaiveDate)>,
        calendar_ids: Vec<String>,
        force: bool,
        cx: &mut Context<Self>,
    ) {
        let wanted = range.map(|(first, end)| EventsRead {
            first,
            end,
            calendar_ids,
        });
        if !force && wanted == self.wanted {
            return;
        }
        self.wanted = wanted.clone();
        let Some(range) = wanted.and_then(|read| {
            Some((
                local_midnight(read.first)?,
                local_midnight(read.end)?,
                read.calendar_ids,
            ))
        }) else {
            self.events.clear();
            self.read = Task::ready(());
            cx.notify();
            return;
        };
        // Apple advises reading off the main thread.
        let read = cx.background_spawn(async move {
            let (start, end, calendar_ids) = range;
            eventkit::events(start, end, &calendar_ids)
        });
        self.read = cx.spawn(async move |this, cx| {
            let result = read.await;
            let _ = this.update(cx, |loaded, cx| match result {
                Ok(events) => {
                    loaded.events = events;
                    cx.notify();
                }
                Err(error) => eprintln!("Could not read events: {error}"),
            });
        });
    }
}
