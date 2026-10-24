//! What the Agenda list shows and in what order: Today, always, with its upcoming and In progress
//! Events, the timed Events of the next day that has any, every other Event that has a Note, and
//! every Quick note, newest first, under the day they started or are listed at, with Today's
//! upcoming Events above the Now line and its In progress ones just below it. Also which days the
//! Agenda reads next and how a read is split for EventKit.

use crate::events::when_text;
use chrono::{DateTime, Datelike, Days, Local, Months, NaiveDate};
use crumble_eventkit::{Event, When};
use std::cmp::Reverse;
use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;

/// Days each read further into the past covers.
const CHUNK_DAYS: u64 = 30;
/// Days after Today the Agenda reads, to find the next day with timed Events.
const AHEAD_DAYS: u64 = 30;
/// How far back the Agenda reads.
const HORIZON_MONTHS: u32 = 5 * 12;
/// EventKit searches at most four years at a time, so longer reads are split.
const READ_DAYS: u64 = 365;

/// The days and Calendars an Agenda read covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    /// The first day read.
    pub from: NaiveDate,
    /// The day after the last.
    pub to: NaiveDate,
    /// The shown Calendars read.
    pub calendar_ids: Vec<String>,
}

/// The oldest day the Agenda reads.
pub fn horizon(today: NaiveDate) -> NaiveDate {
    today
        .checked_sub_months(Months::new(HORIZON_MONTHS))
        .unwrap_or(today)
}

/// The read the Agenda wants next, and whether it is older, adding to the Events already read
/// rather than replacing them; `None` when the Events read still answer. `span` is what the
/// Events read cover; `stale` says EventKit reported a change; `reading` says a read is under
/// way; `wants_more` says the list is short or scrolled near its end.
///
/// The first read covers the last 30 days up to Today, and the 30 days after it. After a new day, a change to the
/// shown Calendars, or an EventKit change, the covered days are read again. Otherwise, while no
/// read is under way, the list reads further back when it wants more, down to the horizon.
pub fn next_read(
    span: Option<&Span>,
    stale: bool,
    reading: bool,
    wants_more: bool,
    today: NaiveDate,
    calendar_ids: Vec<String>,
) -> Option<(Span, bool)> {
    let to = today
        .checked_add_days(Days::new(AHEAD_DAYS + 1))
        .unwrap_or(today);
    let horizon = horizon(today);
    match span {
        None => Some((
            Span {
                from: today
                    .checked_sub_days(Days::new(CHUNK_DAYS - 1))
                    .unwrap_or(today),
                to,
                calendar_ids,
            },
            false,
        )),
        Some(span) if stale || span.to != to || span.calendar_ids != calendar_ids => Some((
            Span {
                // The horizon moves with the day, so the oldest days fall out.
                from: span.from.max(horizon),
                to,
                calendar_ids,
            },
            false,
        )),
        Some(span) if !reading && span.from > horizon && wants_more => {
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
    }
}

/// The days from `from` up to the day before `to`, in pieces EventKit accepts.
pub fn read_pieces(from: NaiveDate, to: NaiveDate) -> impl Iterator<Item = Range<NaiveDate>> {
    let end = move |first: NaiveDate| {
        first
            .checked_add_days(Days::new(READ_DAYS))
            .map_or(to, |end| end.min(to))
    };
    std::iter::successors(Some(from), move |&first| Some(end(first)))
        .take_while(move |&first| first < to)
        .map(move |first| first..end(first))
}

/// The Events of each piece's read, as `now` sees them. Each piece keeps the Events starting
/// inside it, and the piece holding today also the timed ones still In progress, however long
/// ago they started. Each Event is kept once.
pub fn span_events(
    pieces: impl IntoIterator<Item = (Range<NaiveDate>, Vec<Event>)>,
    now: DateTime<Local>,
) -> Vec<Event> {
    let mut seen = HashSet::new();
    let mut events = Vec::new();
    for (piece, read) in pieces {
        events.extend(
            read.into_iter()
                .filter(|event| {
                    piece.contains(&start_day(event))
                        || (piece.contains(&now.date_naive())
                            && matches!(event.when, When::Timed { end, .. } if end > now))
                })
                .filter(|event| seen.insert(event.id.clone())),
        );
    }
    events
}

/// One item of the Agenda list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    /// The header of a day's Events.
    Day(NaiveDate),
    /// The Now line, between Today's upcoming Events and the rest.
    Now,
    /// Says Today has no upcoming or In progress Event, above the Now line where they would be.
    NothingLeft,
    /// An Event, under the day it is listed in.
    Event(Rc<Event>),
    /// The last In progress Event, below the Now line, marking now: it shows its `progress` and
    /// `time_left`.
    Current(Rc<Event>),
    /// A Quick note, under the day it is listed at.
    QuickNote(Rc<QuickNote>),
    /// The footer once nothing older is left to read.
    End,
}

/// A Quick note as the Agenda lists it; the caller keeps the Note itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickNote {
    /// The Note's id, as text.
    pub id: String,
    /// When the Note is listed: its Recording's start, else when it was made.
    pub at: DateTime<Local>,
    /// The Note's title, or `Quick note` when it has none.
    pub label: String,
}

/// The day an Event is listed under: the day it starts.
fn start_day(event: &Event) -> NaiveDate {
    match event.when {
        When::AllDay { first, .. } => first,
        When::Timed { start, .. } => start.date_naive(),
    }
}

fn in_progress(event: &Event, now: DateTime<Local>) -> bool {
    matches!(event.when, When::Timed { start, end } if start <= now && now < end)
}

/// How much of a timed Event has passed at `now`, from 0 at its start to 1 at its end; 0 for an
/// all-day Event.
pub fn progress(event: &Event, now: DateTime<Local>) -> f32 {
    let When::Timed { start, end } = event.when else {
        return 0.0;
    };
    let length = (end - start).num_seconds();
    if length <= 0 {
        return 0.0;
    }
    ((now - start).num_seconds() as f32 / length as f32).clamp(0.0, 1.0)
}

/// What is left of a timed Event at `now`, as its row says it: `48m left`, `1h 5m left`, `2h
/// left`, counting a started minute as whole; `None` once it has ended, and for an all-day Event.
pub fn time_left(event: &Event, now: DateTime<Local>) -> Option<String> {
    let When::Timed { end, .. } = event.when else {
        return None;
    };
    let seconds = (end - now).num_seconds();
    if seconds <= 0 {
        return None;
    }
    let minutes = (seconds + 59) / 60;
    Some(match (minutes / 60, minutes % 60) {
        (0, minutes) => format!("{minutes}m left"),
        (hours, 0) => format!("{hours}h left"),
        (hours, minutes) => format!("{hours}h {minutes}m left"),
    })
}

/// Ends Today's upcoming and In progress Events: the Now line goes above the In progress ones,
/// so it never crosses one, and the last of them marks now. With neither, Today's header is
/// last, and a row says so before the Now line.
fn close_now(rows: &mut Vec<Row>, now: DateTime<Local>) {
    let started = rows
        .iter()
        .rev()
        .take_while(|row| matches!(row, Row::Event(event) if in_progress(event, now)))
        .count();
    if started > 0
        && let Some(Row::Event(event)) = rows.pop()
    {
        rows.push(Row::Current(event));
    }
    if started == 0 && matches!(rows.last(), Some(Row::Day(_))) {
        rows.push(Row::NothingLeft);
    }
    rows.insert(rows.len() - started, Row::Now);
}

/// The rows for `events` and `quick_notes` at `now`, listing only Today's upcoming and In
/// progress Events, the timed Events of the next day after Today that has any, and the others
/// `noted` says have a Note. Today always shows, with its Now line even when it lists nothing
/// else: its upcoming Events, then the Now line, then the In progress ones, whatever day they
/// started, the last marking now, then all-day Events, then the ended Events and the Quick
/// notes; other days list their all-day Events first, then the rest newest first. Only days from
/// `from` on are listed, but an In progress Event shows however long ago it started. `end` adds
/// the footer.
pub fn rows(
    events: &[Rc<Event>],
    quick_notes: &[Rc<QuickNote>],
    noted: impl Fn(&Event) -> bool,
    now: DateTime<Local>,
    from: NaiveDate,
    end: bool,
) -> Vec<Row> {
    let today = now.date_naive();
    let next_day = events
        .iter()
        .filter_map(|event| match event.when {
            When::Timed { start, .. } if start.date_naive() > today => Some(start.date_naive()),
            _ => None,
        })
        .min();
    // Each row with its day, its rank in the day, its start, and its title.
    let events = events.iter().filter_map(|event| {
        let (day, rank) = match event.when {
            _ if in_progress(event, now) => (today, 1),
            When::Timed { start, .. } if start > now && start.date_naive() == today => (today, 0),
            When::Timed { start, .. } if Some(start.date_naive()) == next_day => {
                (start.date_naive(), 3)
            }
            _ if start_day(event) < from || !noted(event) => return None,
            When::AllDay { first, .. } => (first, 2),
            When::Timed { start, .. } => (start.date_naive(), 3),
        };
        let start = match event.when {
            When::AllDay { .. } => None,
            When::Timed { start, .. } => Some(start),
        };
        Some((day, rank, start, &event.title, Row::Event(event.clone())))
    });
    let notes = quick_notes
        .iter()
        .filter(|note| note.at.date_naive() >= from)
        .map(|note| {
            let row = Row::QuickNote(note.clone());
            (note.at.date_naive(), 3, Some(note.at), &note.label, row)
        });
    let mut listed: Vec<_> = events.chain(notes).collect();
    listed.sort_by_cached_key(|(day, rank, start, title, _)| {
        (Reverse(*day), *rank, Reverse(*start), title.to_lowercase())
    });
    let mut rows = Vec::with_capacity(listed.len() + 8);
    let mut current = None;
    let mut now_shown = false;
    for (day, rank, _, _, row) in listed {
        // The Now line goes after Today's upcoming and In progress Events, before the rest of
        // Today and any earlier day, with Today's header even when nothing else is listed in it.
        if !now_shown && (day < today || (day == today && rank > 1)) {
            if current != Some(today) {
                rows.push(Row::Day(today));
                current = Some(today);
            }
            close_now(&mut rows, now);
            now_shown = true;
        }
        if current != Some(day) {
            rows.push(Row::Day(day));
            current = Some(day);
        }
        rows.push(row);
    }
    if !now_shown {
        if current != Some(today) {
            rows.push(Row::Day(today));
        }
        close_now(&mut rows, now);
    }
    if end {
        rows.push(Row::End);
    }
    rows
}

/// A day header: `Tomorrow`, `Today`, `Yesterday`, `Fri 22 Oct`, and the year outside the
/// current one.
pub fn day_label(day: NaiveDate, today: NaiveDate) -> String {
    if day == today {
        "Today".into()
    } else if today.succ_opt() == Some(day) {
        "Tomorrow".into()
    } else if day.checked_add_days(Days::new(1)) == Some(today) {
        "Yesterday".into()
    } else if day.year() == today.year() {
        day.format("%a %-d %b").to_string()
    } else {
        day.format("%a %-d %b %Y").to_string()
    }
}

/// When an Event happens, short when it fits in its day: `09:00 – 10:30`, `All day`; an Event
/// across days says both, as its details do.
pub fn time_label(when: &When) -> String {
    match *when {
        When::AllDay { first, last } if first == last => "All day".into(),
        When::Timed { start, end } if start.date_naive() == end.date_naive() => {
            format!("{} – {}", start.format("%H:%M"), end.format("%H:%M"))
        }
        _ => when_text(when),
    }
}

#[cfg(test)]
mod tests;
