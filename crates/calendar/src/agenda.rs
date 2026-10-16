//! What the Agenda list shows and in what order: every Event that has started, newest first,
//! under the day it started, with Today's In progress Events pinned above the Now line. Also
//! which days the Agenda reads next and how a read is split for EventKit.

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
/// The first read covers the last 30 days up to Today. After a new day, a change to the
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
    let to = today.checked_add_days(Days::new(1)).unwrap_or(today);
    let horizon = horizon(today);
    match span {
        None => Some((
            Span {
                from: to.checked_sub_days(Days::new(CHUNK_DAYS)).unwrap_or(to),
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

/// A read that is the same as the last failed one is not tried again until EventKit reports a
/// change or a different read is wanted, so a persistent error cannot loop.
pub fn skip_failed_read(
    wanted: Option<(Span, bool)>,
    failed: Option<&(Span, bool)>,
) -> Option<(Span, bool)> {
    wanted.filter(|wanted| failed != Some(wanted))
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
    /// The line between Today's In progress Events and the rest.
    Now,
    /// An Event, under the day it is listed in.
    Event(Rc<Event>),
    /// The footer once nothing older is left to read.
    End,
}

/// The day an Event is listed under: the day it starts.
fn start_day(event: &Event) -> NaiveDate {
    match event.when {
        When::AllDay { first, .. } => first,
        When::Timed { start, .. } => start.date_naive(),
    }
}

fn started(event: &Event, now: DateTime<Local>) -> bool {
    match event.when {
        When::AllDay { first, .. } => first <= now.date_naive(),
        When::Timed { start, .. } => start <= now,
    }
}

fn in_progress(event: &Event, now: DateTime<Local>) -> bool {
    matches!(event.when, When::Timed { start, end } if start <= now && now < end)
}

/// The rows for `events` at `now`. In progress Events sit under Today whatever day they
/// started, then the Now line, then all-day Events, then the ended ones; other days list their
/// all-day Events first, then the rest newest first. Only days from `from` on are listed, but
/// an In progress Event shows however long ago it started. `end` adds the footer.
pub fn rows(events: &[Rc<Event>], now: DateTime<Local>, from: NaiveDate, end: bool) -> Vec<Row> {
    let today = now.date_naive();
    let mut listed: Vec<(NaiveDate, u8, &Rc<Event>)> = events
        .iter()
        .filter(|event| {
            started(event, now) && (start_day(event) >= from || in_progress(event, now))
        })
        .map(|event| match event.when {
            _ if in_progress(event, now) => (today, 0, event),
            When::AllDay { first, .. } => (first, 1, event),
            When::Timed { start, .. } => (start.date_naive(), 2, event),
        })
        .collect();
    listed.sort_by_cached_key(|&(day, rank, event)| {
        let start = match event.when {
            When::AllDay { .. } => None,
            When::Timed { start, .. } => Some(start),
        };
        (
            Reverse(day),
            rank,
            Reverse(start),
            event.title.to_lowercase(),
        )
    });
    let mut rows = Vec::with_capacity(listed.len() + 8);
    let mut current = None;
    let mut now_shown = false;
    for (day, rank, event) in listed {
        if current != Some(day) {
            // Today is the newest day, so leaving it means only In progress Events were in it.
            if current == Some(today) && !now_shown {
                rows.push(Row::Now);
                now_shown = true;
            }
            rows.push(Row::Day(day));
            current = Some(day);
        }
        if day == today && rank > 0 && !now_shown {
            rows.push(Row::Now);
            now_shown = true;
        }
        rows.push(Row::Event(event.clone()));
    }
    if current == Some(today) && !now_shown {
        rows.push(Row::Now);
    }
    if end {
        rows.push(Row::End);
    }
    rows
}

/// A day header: `Today`, `Yesterday`, `Fri 22 Oct`, and the year outside the current one.
pub fn day_label(day: NaiveDate, today: NaiveDate) -> String {
    if day == today {
        "Today".into()
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
mod tests {
    use super::{
        Row, Span, day_label, horizon, next_read, read_pieces, rows, skip_failed_read, span_events,
        time_label,
    };
    use chrono::{DateTime, Days, Local, NaiveDate, TimeZone};
    use crumble_eventkit::{Event, When};
    use std::rc::Rc;

    fn at(day: u32, hour: u32, minute: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 10, day, hour, minute, 0)
            .earliest()
            .unwrap_or_default()
    }

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap_or_default()
    }

    fn timed(title: &str, start: DateTime<Local>, end: DateTime<Local>) -> Rc<Event> {
        Rc::new(Event {
            id: title.into(),
            calendar_id: "calendar".into(),
            title: title.into(),
            when: When::Timed { start, end },
            location: None,
            description: None,
            join: None,
            participants: Vec::new(),
        })
    }

    fn all_day(title: &str, first: u32, last: u32) -> Rc<Event> {
        let event = timed(title, at(1, 0, 0), at(1, 0, 0));
        Rc::new(Event {
            when: When::AllDay {
                first: date(first),
                last: date(last),
            },
            ..(*event).clone()
        })
    }

    /// Rows as text: a day as its date, the Now line as `--`, an Event as its title.
    fn labels(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                Row::Day(day) => day.format("%d").to_string(),
                Row::Now => "--".into(),
                Row::Event(event) => event.title.clone(),
                Row::End => "end".into(),
            })
            .collect()
    }

    #[test]
    fn only_started_events_are_listed_newest_first() {
        let now = at(15, 12, 0);
        let events = [
            timed("Standup", at(15, 9, 0), at(15, 9, 15)),
            timed("Review", at(15, 11, 0), at(15, 11, 30)),
            timed("Later today", at(15, 16, 0), at(15, 17, 0)),
            all_day("Holiday", 15, 15),
            all_day("Tomorrow", 16, 16),
            timed("Yesterday", at(14, 10, 0), at(14, 11, 0)),
            all_day("Trip", 13, 14),
        ];
        assert_eq!(
            labels(&rows(&events, now, date(1), true)),
            [
                "15",
                "--",
                "Holiday",
                "Review",
                "Standup",
                "14",
                "Yesterday",
                "13",
                "Trip",
                "end"
            ]
        );
    }

    #[test]
    fn in_progress_events_are_pinned_above_the_now_line() {
        let now = at(15, 10, 0);
        let events = [
            timed("Workshop", at(15, 9, 0), at(15, 12, 0)),
            timed("Call", at(15, 9, 30), at(15, 9, 45)),
            // Started yesterday and still going: it shows under Today.
            timed("Flight", at(14, 22, 0), at(15, 11, 0)),
            timed("Dinner", at(14, 19, 0), at(14, 20, 0)),
        ];
        assert_eq!(
            labels(&rows(&events, now, date(1), false)),
            ["15", "Workshop", "Flight", "--", "Call", "14", "Dinner"]
        );

        // Only In progress Events today: the line closes Today's group.
        let only = [timed("Workshop", at(15, 9, 0), at(15, 12, 0))];
        assert_eq!(
            labels(&rows(&only, now, date(1), false)),
            ["15", "Workshop", "--"]
        );

        // An Event ends at its end time: it is no longer In progress then.
        let ended = [timed("Workshop", at(15, 9, 0), at(15, 10, 0))];
        assert_eq!(
            labels(&rows(&ended, now, date(1), false)),
            ["15", "--", "Workshop"]
        );
    }

    #[test]
    fn days_before_the_read_ones_show_only_while_in_progress() {
        let flight = [timed("Flight", at(9, 22, 0), at(15, 11, 0))];
        assert_eq!(
            labels(&rows(&flight, at(15, 10, 0), date(10), false)),
            ["15", "Flight", "--"]
        );
        // Once it ends, its start day is not among those read, so it leaves the list.
        assert!(rows(&flight, at(15, 12, 0), date(10), false).is_empty());
        assert_eq!(
            labels(&rows(&flight, at(15, 12, 0), date(9), false)),
            ["09", "Flight"]
        );
    }

    #[test]
    fn days_without_today_have_no_now_line() {
        let events = [timed("Dinner", at(14, 19, 0), at(14, 20, 0))];
        assert_eq!(
            labels(&rows(&events, at(15, 10, 0), date(1), false)),
            ["14", "Dinner"]
        );
        assert_eq!(labels(&rows(&[], at(15, 10, 0), date(1), true)), ["end"]);
    }

    #[test]
    fn day_headers_name_recent_days_and_add_the_year_outside_this_one() {
        let today = date(15);
        assert_eq!(day_label(today, today), "Today");
        assert_eq!(day_label(date(14), today), "Yesterday");
        assert_eq!(day_label(date(13), today), "Tue 13 Oct");
        let last_year = NaiveDate::from_ymd_opt(2025, 12, 31).unwrap_or_default();
        assert_eq!(day_label(last_year, today), "Wed 31 Dec 2025");
    }

    #[test]
    fn times_are_short_within_a_day() {
        assert_eq!(time_label(&all_day("x", 13, 13).when), "All day");
        assert_eq!(
            time_label(&timed("x", at(13, 9, 0), at(13, 10, 30)).when),
            "09:00 – 10:30"
        );
        assert_eq!(
            time_label(&all_day("x", 13, 15).when),
            "Tue 13 Oct – Thu 15 Oct · All day"
        );
        assert_eq!(
            time_label(&timed("x", at(13, 23, 0), at(14, 1, 0)).when),
            "Tue 13 Oct 23:00 – Wed 14 Oct 01:00"
        );
    }

    fn span(from: NaiveDate, to: NaiveDate) -> Span {
        Span {
            from,
            to,
            calendar_ids: vec!["work".into()],
        }
    }

    fn days_before(day: NaiveDate, days: u64) -> NaiveDate {
        day.checked_sub_days(Days::new(days)).unwrap_or(day)
    }

    #[test]
    fn the_first_read_covers_the_last_thirty_days_up_to_today() {
        let ids = vec!["work".to_owned()];
        assert_eq!(
            next_read(None, false, false, false, date(15), ids),
            Some((span(date(16) - Days::new(30), date(16)), false))
        );
    }

    #[test]
    fn a_change_reads_the_covered_days_again_back_to_the_horizon() {
        let today = date(15);
        let horizon = horizon(today);
        let ids = || vec!["work".to_owned()];
        let current = span(horizon, date(16));
        let again = Some((span(horizon, date(16)), false));
        // EventKit changed.
        assert_eq!(
            next_read(Some(&current), true, true, false, today, ids()),
            again
        );
        // A new day: the horizon moved, so the oldest day falls out.
        let yesterdays = span(days_before(horizon, 1), today);
        assert_eq!(
            next_read(Some(&yesterdays), false, false, false, today, ids()),
            again
        );
        // The shown Calendars changed.
        assert_eq!(
            next_read(Some(&current), false, false, false, today, vec![]),
            Some((
                Span {
                    calendar_ids: vec![],
                    ..span(horizon, date(16))
                },
                false
            ))
        );
    }

    #[test]
    fn older_reads_stop_at_the_horizon_and_wait_for_the_one_under_way() {
        let today = date(15);
        let horizon = horizon(today);
        let ids = || vec!["work".to_owned()];
        let recent = span(date(1), date(16));
        assert_eq!(
            next_read(Some(&recent), false, false, true, today, ids()),
            Some((span(days_before(date(1), 30), date(16)), true))
        );
        // The last chunk ends at the horizon, and nothing is read past it.
        let near = span(horizon + Days::new(10), date(16));
        assert_eq!(
            next_read(Some(&near), false, false, true, today, ids()),
            Some((span(horizon, date(16)), true))
        );
        let done = span(horizon, date(16));
        assert_eq!(
            next_read(Some(&done), false, false, true, today, ids()),
            None
        );
        // A read under way, or a list that wants nothing more, reads nothing.
        assert_eq!(
            next_read(Some(&recent), false, true, true, today, ids()),
            None
        );
        assert_eq!(
            next_read(Some(&recent), false, false, false, today, ids()),
            None
        );
    }

    #[test]
    fn a_failed_read_is_not_retried_until_a_different_one_is_wanted() {
        let wanted = Some((span(date(1), date(16)), true));
        assert_eq!(skip_failed_read(wanted.clone(), None), wanted);
        assert_eq!(skip_failed_read(wanted.clone(), wanted.as_ref()), None);
        let other = span(date(1), date(15));
        assert_eq!(
            skip_failed_read(wanted.clone(), Some(&(other, true))),
            wanted
        );
        assert_eq!(
            skip_failed_read(wanted.clone(), Some(&(span(date(1), date(16)), false))),
            wanted
        );
    }

    #[test]
    fn long_reads_are_split_into_pieces_eventkit_accepts() {
        let from = date(1);
        let to = from + Days::new(800);
        let pieces: Vec<_> = read_pieces(from, to).collect();
        assert_eq!(
            pieces,
            [
                from..from + Days::new(365),
                from + Days::new(365)..from + Days::new(730),
                from + Days::new(730)..to,
            ]
        );
        assert_eq!(read_pieces(to, to).count(), 0);
    }

    #[test]
    fn each_piece_keeps_its_own_events_and_today_s_in_progress_ones_once() {
        let now = at(15, 10, 0);
        let event = |title: &str, start, end| (*timed(title, start, end)).clone();
        let flight = event("Flight", at(9, 22, 0), at(15, 11, 0));
        let dinner = event("Dinner", at(9, 19, 0), at(9, 20, 0));
        let standup = event("Standup", at(15, 9, 0), at(15, 9, 15));
        // EventKit returns every Event overlapping a piece, so the Flight is in both.
        let pieces = [
            (date(1)..date(10), vec![dinner.clone(), flight.clone()]),
            (
                date(10)..date(16),
                vec![flight.clone(), dinner, standup.clone()],
            ),
        ];
        let titles: Vec<String> = span_events(pieces, now)
            .into_iter()
            .map(|event| event.title)
            .collect();
        assert_eq!(titles, ["Dinner", "Flight", "Standup"]);
        // Without today in it, a piece keeps only the Events starting inside it.
        let earlier = [(date(10)..date(15), vec![flight, standup])];
        assert!(span_events(earlier, now).is_empty());
    }
}
