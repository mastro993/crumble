use super::{
    QuickNote, Row, Span, day_label, horizon, next_read, read_pieces, rows, skip_failed_read,
    span_events, time_label,
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
        series_uid: None,
        occurrence: None,
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
            Row::QuickNote(note) => note.label.clone(),
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
        labels(&rows(&events, &[], now, date(1), true)),
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
        labels(&rows(&events, &[], now, date(1), false)),
        ["15", "Workshop", "Flight", "--", "Call", "14", "Dinner"]
    );

    // Only In progress Events today: the line closes Today's group.
    let only = [timed("Workshop", at(15, 9, 0), at(15, 12, 0))];
    assert_eq!(
        labels(&rows(&only, &[], now, date(1), false)),
        ["15", "Workshop", "--"]
    );

    // An Event ends at its end time: it is no longer In progress then.
    let ended = [timed("Workshop", at(15, 9, 0), at(15, 10, 0))];
    assert_eq!(
        labels(&rows(&ended, &[], now, date(1), false)),
        ["15", "--", "Workshop"]
    );
}

#[test]
fn days_before_the_read_ones_show_only_while_in_progress() {
    let flight = [timed("Flight", at(9, 22, 0), at(15, 11, 0))];
    assert_eq!(
        labels(&rows(&flight, &[], at(15, 10, 0), date(10), false)),
        ["15", "Flight", "--"]
    );
    // Once it ends, its start day is not among those read, so it leaves the list.
    assert!(rows(&flight, &[], at(15, 12, 0), date(10), false).is_empty());
    assert_eq!(
        labels(&rows(&flight, &[], at(15, 12, 0), date(9), false)),
        ["09", "Flight"]
    );
}

#[test]
fn days_without_today_have_no_now_line() {
    let events = [timed("Dinner", at(14, 19, 0), at(14, 20, 0))];
    assert_eq!(
        labels(&rows(&events, &[], at(15, 10, 0), date(1), false)),
        ["14", "Dinner"]
    );
    assert_eq!(
        labels(&rows(&[], &[], at(15, 10, 0), date(1), true)),
        ["end"]
    );
}

fn quick_note(label: &str, at: DateTime<Local>) -> Rc<QuickNote> {
    Rc::new(QuickNote {
        id: label.into(),
        at,
        label: label.into(),
    })
}

#[test]
fn quick_notes_join_the_events_at_their_time() {
    let now = at(15, 12, 0);
    let events = [
        timed("Workshop", at(15, 11, 0), at(15, 13, 0)),
        timed("Review", at(15, 11, 0), at(15, 11, 30)),
        timed("Standup", at(15, 9, 0), at(15, 9, 15)),
        all_day("Holiday", 15, 15),
        timed("Dinner", at(13, 19, 0), at(13, 20, 0)),
    ];
    let notes = [
        quick_note("Ideas", at(15, 10, 0)),
        quick_note("Groceries", at(14, 8, 0)),
        quick_note("Too old", at(9, 8, 0)),
    ];
    assert_eq!(
        labels(&rows(&events, &notes, now, date(10), false)),
        [
            "15",
            "Workshop",
            "--",
            "Holiday",
            "Review",
            "Ideas",
            "Standup",
            "14",
            "Groceries",
            "13",
            "Dinner"
        ]
    );
    // Quick notes alone still get their day header and the Now line.
    assert_eq!(
        labels(&rows(&[], &notes[..1], now, date(10), false)),
        ["15", "--", "Ideas"]
    );
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
