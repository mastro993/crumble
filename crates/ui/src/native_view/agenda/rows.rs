//! What the Agenda list shows and in what order: every Event that has started, newest first,
//! under the day it started, with Today's In progress Events pinned above the Now line.

use crate::native_view::calendar::when_text;
use chrono::{DateTime, Datelike, Days, Local, NaiveDate};
use crumble_eventkit::{Event, When};
use std::cmp::Reverse;
use std::rc::Rc;

/// One item of the Agenda list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Row {
    /// The header of a day's Events.
    Day(NaiveDate),
    /// The line between Today's In progress Events and the rest.
    Now,
    Event(Rc<Event>),
    /// The footer once nothing older is left to read.
    End,
}

/// The day an Event is listed under: the day it starts.
pub(super) fn start_day(event: &Event) -> NaiveDate {
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
/// all-day Events first, then the rest newest first. `end` adds the footer.
pub(super) fn rows(events: &[Rc<Event>], now: DateTime<Local>, end: bool) -> Vec<Row> {
    let today = now.date_naive();
    let mut listed: Vec<(NaiveDate, u8, &Rc<Event>)> = events
        .iter()
        .filter(|event| started(event, now))
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
pub(super) fn day_label(day: NaiveDate, today: NaiveDate) -> String {
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
pub(super) fn time_label(when: &When) -> String {
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
    use super::{Row, day_label, rows, time_label};
    use chrono::{DateTime, Local, NaiveDate, TimeZone};
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
            labels(&rows(&events, now, true)),
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
            labels(&rows(&events, now, false)),
            ["15", "Workshop", "Flight", "--", "Call", "14", "Dinner"]
        );

        // Only In progress Events today: the line closes Today's group.
        let only = [timed("Workshop", at(15, 9, 0), at(15, 12, 0))];
        assert_eq!(labels(&rows(&only, now, false)), ["15", "Workshop", "--"]);

        // An Event ends at its end time: it is no longer In progress then.
        let ended = [timed("Workshop", at(15, 9, 0), at(15, 10, 0))];
        assert_eq!(labels(&rows(&ended, now, false)), ["15", "--", "Workshop"]);
    }

    #[test]
    fn days_without_today_have_no_now_line() {
        let events = [timed("Dinner", at(14, 19, 0), at(14, 20, 0))];
        assert_eq!(
            labels(&rows(&events, at(15, 10, 0), false)),
            ["14", "Dinner"]
        );
        assert_eq!(labels(&rows(&[], at(15, 10, 0), true)), ["end"]);
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
}
