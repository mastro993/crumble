//! What the Agenda list shows and in what order: every Event up to the end of Today and every
//! Quick note, newest first, under the day they started or are listed at, with Today's upcoming
//! and In progress Events above the Now line, and Tomorrow's above them once Today has none
//! left. Also which days the Agenda reads next and how a read is split for EventKit.

use crate::events::when_text;
use chrono::{DateTime, Datelike, Days, Local, Months, NaiveDate};
use crumble_eventkit::{Event, When};
use std::cmp::Reverse;
use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;

/// Days each read further into the past covers.
const CHUNK_DAYS: u64 = 30;
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
/// The first read covers the last 30 days up to Today, and Tomorrow. After a new day, a change to the
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
    let to = today.checked_add_days(Days::new(2)).unwrap_or(today);
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
    /// The Now line, between Today's upcoming and In progress Events and the rest.
    Now,
    /// An Event, under the day it is listed in.
    Event(Rc<Event>),
    /// The In progress Event just above where the Now line would fall, marking now in place of
    /// the line by showing its `progress` and `time_left`.
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

/// Ends Today's upcoming and In progress Events: the last of them marks now when it is In
/// progress, and the Now line follows it otherwise.
fn close_now(rows: &mut Vec<Row>, now: DateTime<Local>) {
    if let Some(Row::Event(event)) = rows.last()
        && in_progress(event, now)
    {
        let event = event.clone();
        rows.pop();
        rows.push(Row::Current(event));
    } else {
        rows.push(Row::Now);
    }
}

/// The rows for `events` and `quick_notes` at `now`. Today lists its upcoming Events, then the
/// In progress ones, whatever day they started, then the Now line, or the last In progress one
/// marking now, then all-day Events, then the ended Events and the Quick notes; other days list
/// their all-day Events first, then the rest newest first. Tomorrow is listed above Today while
/// no Event is upcoming or In progress. Only days from `from` on are listed, but an In progress
/// Event shows however long ago it started. `end` adds the footer.
pub fn rows(
    events: &[Rc<Event>],
    quick_notes: &[Rc<QuickNote>],
    now: DateTime<Local>,
    from: NaiveDate,
    end: bool,
) -> Vec<Row> {
    let today = now.date_naive();
    let today_left = events.iter().any(|event| {
        start_day(event) <= today && matches!(event.when, When::Timed { end, .. } if end > now)
    });
    let last_day = if today_left {
        Some(today)
    } else {
        today.succ_opt()
    };
    // Each row with its day, its rank in the day, its start, and its title.
    let events = events
        .iter()
        .filter(|event| {
            Some(start_day(event)) <= last_day
                && (start_day(event) >= from || in_progress(event, now))
        })
        .map(|event| {
            let (day, rank) = match event.when {
                _ if in_progress(event, now) => (today, 1),
                When::AllDay { first, .. } => (first, 2),
                When::Timed { start, .. } if start > now && start.date_naive() == today => {
                    (today, 0)
                }
                When::Timed { start, .. } => (start.date_naive(), 3),
            };
            let start = match event.when {
                When::AllDay { .. } => None,
                When::Timed { start, .. } => Some(start),
            };
            (day, rank, start, &event.title, Row::Event(event.clone()))
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
        if current != Some(day) {
            // Today is the newest day, so leaving it means only upcoming and In progress Events
            // were in it.
            if current == Some(today) && !now_shown {
                close_now(&mut rows, now);
                now_shown = true;
            }
            rows.push(Row::Day(day));
            current = Some(day);
        }
        if day == today && rank > 1 && !now_shown {
            close_now(&mut rows, now);
            now_shown = true;
        }
        rows.push(row);
    }
    if current == Some(today) && !now_shown {
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
