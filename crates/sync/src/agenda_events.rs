//! The Events the Agenda page lists, read further into the past as the list wants more.

use chrono::{Local, NaiveDate};
use crumble_calendar::agenda::{self, Span};
use crumble_calendar::local_midnight;
use crumble_eventkit::{self as eventkit, Event};
use gpui::{AppContext, Context, EventEmitter, Task};
use std::collections::HashSet;
use std::rc::Rc;

/// The Events read so far for the Agenda and the days they cover; kept in memory while the app
/// runs, so the Agenda reopens where it was left.
pub struct AgendaEvents {
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
}

/// Emitted when a read further back adds Events to those already read.
pub struct OlderEventsRead;

impl EventEmitter<OlderEventsRead> for AgendaEvents {}

impl Default for AgendaEvents {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            span: None,
            stale: false,
            reading: false,
            read: Task::ready(()),
            failed: None,
            reads: 0,
        }
    }
}

impl AgendaEvents {
    /// The Events read so far, newest first.
    pub fn events(&self) -> &[Rc<Event>] {
        &self.events
    }

    /// The days the Events cover, or will once the read under way finishes; `None` before the
    /// first read.
    pub fn span(&self) -> Option<&Span> {
        self.span.as_ref()
    }

    /// Whether a read is under way.
    pub fn reading(&self) -> bool {
        self.reading
    }

    /// How many reads have finished, so callers can tell when the Events changed.
    pub fn reads(&self) -> u64 {
        self.reads
    }

    /// Reads the covered days again the next time a read is asked for.
    pub fn mark_stale(&mut self) {
        self.stale = true;
        self.failed = None;
    }

    /// Starts the read [`agenda::next_read`] asks for on `today` in `calendar_ids`, unless it is
    /// the one that last failed; `wants_more` says the list is short or scrolled near its end.
    pub fn read_next(
        &mut self,
        today: NaiveDate,
        calendar_ids: Vec<String>,
        wants_more: bool,
        cx: &mut Context<Self>,
    ) {
        let wanted = agenda::next_read(
            self.span.as_ref(),
            self.stale,
            self.reading,
            wants_more,
            today,
            calendar_ids,
        );
        if let Some((span, older)) = agenda::skip_failed_read(wanted, self.failed.as_ref()) {
            self.stale = false;
            self.read(span, older, cx);
        }
    }

    /// Starts reading `span`; `older` adds its Events to those already read instead of
    /// replacing them. A failed read is logged, and the last Events and the days they cover
    /// stay.
    fn read(&mut self, span: Span, older: bool, cx: &mut Context<Self>) {
        let previous = self.span.replace(span.clone());
        self.reading = true;
        // An older read covers only the days before those already read.
        let to = match &previous {
            Some(previous) if older => previous.from,
            _ => span.to,
        };
        let requested = span.clone();
        // Apple advises reading off the main thread.
        let read = cx.background_spawn(async move { read_span(span.from, to, &span.calendar_ids) });
        self.read = cx.spawn(async move |this, cx| {
            let result = read.await;
            let _ = this.update(cx, |agenda, cx| {
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
                            if !events.is_empty() {
                                cx.emit(OlderEventsRead);
                            }
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
