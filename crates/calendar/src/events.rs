//! Where Events fall on the Calendar page: which days they touch, the part of a day a timed
//! Event covers, side-by-side columns for overlapping ones, and the text that says when.

use chrono::{Days, NaiveDate, NaiveDateTime, NaiveTime};
use crumble_eventkit::{Event, When};

const DAY_MINUTES: f32 = 24.0 * 60.0;

fn midnight(date: NaiveDate) -> NaiveDateTime {
    date.and_time(NaiveTime::MIN)
}

/// Whether `event` shows on `date`. A timed Event that ends at midnight stays on the day before.
pub fn on_day(event: &Event, date: NaiveDate) -> bool {
    match event.when {
        When::AllDay { first, last } => first <= date && date <= last,
        When::Timed { .. } => minutes_on(event, date).is_some(),
    }
}

/// The minutes after midnight a timed Event covers on `date`, clipped to that day.
pub fn minutes_on(event: &Event, date: NaiveDate) -> Option<(f32, f32)> {
    let When::Timed { start, end } = event.when else {
        return None;
    };
    let (start, end) = (start.naive_local(), end.naive_local());
    let day_start = midnight(date);
    let day_end = midnight(date + Days::new(1));
    // A zero-length Event still shows at its start.
    let touches = start < day_end && (end > day_start || (start == end && start >= day_start));
    if !touches {
        return None;
    }
    let minutes = |time: NaiveDateTime| (time - day_start).num_minutes() as f32;
    Some((
        minutes(start.max(day_start)),
        minutes(end.min(day_end)).min(DAY_MINUTES),
    ))
}

/// For each span, its column and the number of columns its group of overlapping spans needs,
/// so overlapping Events sit side by side. Spans are `(start, end)` minutes.
pub fn columns(spans: &[(f32, f32)]) -> Vec<(usize, usize)> {
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_by(|&a, &b| {
        spans[a]
            .0
            .total_cmp(&spans[b].0)
            .then(spans[b].1.total_cmp(&spans[a].1))
    });
    // A zero-length span still takes a minute, so it does not share a column with a neighbour.
    let end = |span: usize| spans[span].1.max(spans[span].0 + 1.0);
    let mut column = vec![0; spans.len()];
    let mut group = vec![0; spans.len()];
    // Per group of transitively overlapping spans: how many columns it needs.
    let mut widths: Vec<usize> = Vec::new();
    // When each column of the current group frees up, and when the group ends.
    let mut column_ends: Vec<f32> = Vec::new();
    let mut group_end = f32::NEG_INFINITY;
    for span in order {
        let start = spans[span].0;
        if start >= group_end {
            widths.push(0);
            column_ends.clear();
        }
        let free = column_ends.iter().position(|&free| free <= start);
        let slot = free.unwrap_or(column_ends.len());
        if free.is_none() {
            column_ends.push(0.0);
        }
        column_ends[slot] = end(span);
        group_end = if start >= group_end {
            end(span)
        } else {
            group_end.max(end(span))
        };
        column[span] = slot;
        group[span] = widths.len() - 1;
        let width = widths.len() - 1;
        widths[width] = column_ends.len();
    }
    (0..spans.len())
        .map(|span| (column[span], widths[group[span]]))
        .collect()
}

/// The Events on `date` in reading order: all-day ones first, then by start time, then by title.
pub fn day_order<'a>(
    events: impl IntoIterator<Item = &'a Event>,
    date: NaiveDate,
) -> Vec<&'a Event> {
    let mut day: Vec<&Event> = events
        .into_iter()
        .filter(|event| on_day(event, date))
        .collect();
    day.sort_by_key(|event| {
        let start = match event.when {
            When::AllDay { .. } => None,
            When::Timed { start, .. } => Some(start),
        };
        (start.is_some(), start, event.title.to_lowercase())
    });
    day
}

/// `opened` as last read in `events`: the same occurrence, else the one Event with its
/// [`Event::series_uid`] and [`Event::occurrence`], as after the Event moved and its start
/// changed; else `opened` as it was.
pub fn latest<'a>(events: &'a [Event], opened: &'a Event) -> &'a Event {
    if let Some(same) = events.iter().find(|event| event.id == opened.id) {
        return same;
    }
    if opened.series_uid.is_none() {
        return opened;
    }
    let mut moved = events.iter().filter(|event| {
        event.series_uid == opened.series_uid && event.occurrence == opened.occurrence
    });
    match (moved.next(), moved.next()) {
        (Some(only), None) => only,
        _ => opened,
    }
}

/// When an Event happens, as the details show it: `Mon 13 Oct · 09:00 – 10:00`.
pub fn when_text(when: &When) -> String {
    const DAY: &str = "%a %-d %b";
    match *when {
        When::AllDay { first, last } if first == last => {
            format!("{} · All day", first.format(DAY))
        }
        When::AllDay { first, last } => {
            format!("{} – {} · All day", first.format(DAY), last.format(DAY))
        }
        When::Timed { start, end } if start.date_naive() == end.date_naive() => format!(
            "{} · {} – {}",
            start.format(DAY),
            start.format("%H:%M"),
            end.format("%H:%M")
        ),
        When::Timed { start, end } => format!(
            "{} {} – {} {}",
            start.format(DAY),
            start.format("%H:%M"),
            end.format(DAY),
            end.format("%H:%M")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{columns, day_order, latest, minutes_on, on_day, when_text};
    use chrono::{Local, NaiveDate, TimeZone, Utc};
    use crumble_eventkit::{Event, Occurrence, When};

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap_or_default()
    }

    fn timed(
        title: &str,
        (day, hour, minute): (u32, u32, u32),
        (end_day, end_hour, end_minute): (u32, u32, u32),
    ) -> Event {
        let at = |day, hour, minute| {
            Local
                .with_ymd_and_hms(2026, 10, day, hour, minute, 0)
                .earliest()
                .unwrap_or_default()
        };
        Event {
            id: title.into(),
            series_uid: None,
            occurrence: None,
            calendar_id: "calendar".into(),
            title: title.into(),
            when: When::Timed {
                start: at(day, hour, minute),
                end: at(end_day, end_hour, end_minute),
            },
            location: None,
            description: None,
            join: None,
            participants: Vec::new(),
        }
    }

    fn all_day(title: &str, first: u32, last: u32) -> Event {
        Event {
            when: When::AllDay {
                first: date(first),
                last: date(last),
            },
            ..timed(title, (1, 0, 0), (1, 0, 0))
        }
    }

    #[test]
    fn timed_events_are_clipped_to_each_day() {
        let standup = timed("Standup", (13, 9, 30), (13, 10, 0));
        assert_eq!(minutes_on(&standup, date(13)), Some((570.0, 600.0)));
        assert_eq!(minutes_on(&standup, date(14)), None);

        let overnight = timed("Flight", (13, 23, 0), (14, 1, 30));
        assert_eq!(minutes_on(&overnight, date(13)), Some((1380.0, 1440.0)));
        assert_eq!(minutes_on(&overnight, date(14)), Some((0.0, 90.0)));

        // Ending at midnight keeps the Event off the next day.
        let late = timed("Late", (13, 22, 0), (14, 0, 0));
        assert!(on_day(&late, date(13)));
        assert!(!on_day(&late, date(14)));

        let reminder = timed("Reminder", (13, 12, 0), (13, 12, 0));
        assert_eq!(minutes_on(&reminder, date(13)), Some((720.0, 720.0)));
    }

    #[test]
    fn all_day_events_cover_every_day_from_first_to_last() {
        let trip = all_day("Trip", 13, 15);
        assert!(!on_day(&trip, date(12)));
        assert!(on_day(&trip, date(13)) && on_day(&trip, date(15)));
        assert!(!on_day(&trip, date(16)));
        assert_eq!(minutes_on(&trip, date(13)), None);
    }

    #[test]
    fn overlapping_spans_sit_side_by_side() {
        // a overlaps b and c; b and c do not overlap, so they share the second column.
        // d starts after the group ends and gets the whole width again.
        let spans = [
            (540.0, 660.0),
            (540.0, 600.0),
            (600.0, 630.0),
            (700.0, 720.0),
        ];
        assert_eq!(columns(&spans), [(0, 2), (1, 2), (1, 2), (0, 1)]);
        assert_eq!(
            columns(&[(0.0, 60.0), (30.0, 90.0), (45.0, 50.0)]),
            [(0, 3), (1, 3), (2, 3)]
        );
        assert_eq!(columns(&[]), []);
    }

    #[test]
    fn a_day_lists_all_day_events_first_then_by_start() {
        let events = [
            timed("Lunch", (13, 12, 0), (13, 13, 0)),
            all_day("Holiday", 13, 13),
            timed("Standup", (13, 9, 0), (13, 9, 15)),
            timed("Tomorrow", (14, 9, 0), (14, 10, 0)),
        ];
        let titles: Vec<&str> = day_order(&events, date(13))
            .iter()
            .map(|event| event.title.as_str())
            .collect();
        assert_eq!(titles, ["Holiday", "Standup", "Lunch"]);
    }

    fn in_series(id: &str, uid: &str, occurrence: Option<Occurrence>, event: Event) -> Event {
        Event {
            id: id.into(),
            series_uid: Some(uid.into()),
            occurrence,
            ..event
        }
    }

    #[test]
    fn an_opened_event_follows_its_series_uid_when_it_moves() {
        let opened = in_series(
            "a@1",
            "uid",
            None,
            timed("Standup", (13, 9, 0), (13, 9, 15)),
        );
        let moved = in_series(
            "a@2",
            "uid",
            None,
            timed("Standup", (13, 10, 0), (13, 10, 15)),
        );
        let other = in_series(
            "b@1",
            "other",
            None,
            timed("Lunch", (13, 12, 0), (13, 13, 0)),
        );
        assert_eq!(latest(&[other.clone(), moved.clone()], &opened), &moved);
        // The same UID twice, as when one Event shows in two Calendars, is ambiguous.
        let copy = Event {
            id: "c@2".into(),
            ..moved.clone()
        };
        assert_eq!(latest(&[moved, copy], &opened), &opened);
        assert_eq!(latest(&[other], &opened), &opened);
        // An Event not yet synced has no UID to follow.
        let unsynced = Event {
            series_uid: None,
            ..opened.clone()
        };
        let unsynced_moved = Event {
            id: "a@2".into(),
            ..unsynced.clone()
        };
        assert_eq!(latest(&[unsynced_moved], &unsynced), &unsynced);
    }

    #[test]
    fn an_opened_occurrence_follows_its_original_date_when_it_moves() {
        let original = |day| {
            Local
                .with_ymd_and_hms(2026, 10, day, 9, 0, 0)
                .earliest()
                .map(|start| Occurrence::At(start.with_timezone(&Utc)))
        };
        let opened = in_series(
            "a@1",
            "uid",
            original(13),
            timed("Standup", (13, 9, 0), (13, 9, 15)),
        );
        let moved = in_series(
            "a@2",
            "uid",
            original(13),
            timed("Standup", (13, 11, 0), (13, 11, 15)),
        );
        let next_week = in_series(
            "a@3",
            "uid",
            original(20),
            timed("Standup", (20, 9, 0), (20, 9, 15)),
        );
        assert_eq!(latest(&[next_week.clone(), moved.clone()], &opened), &moved);
        // Other occurrences of the series never stand in for the opened one.
        assert_eq!(latest(&[next_week], &opened), &opened);
    }

    #[test]
    fn details_say_when() {
        assert_eq!(
            when_text(&all_day("x", 13, 13).when),
            "Tue 13 Oct · All day"
        );
        assert_eq!(
            when_text(&all_day("x", 13, 15).when),
            "Tue 13 Oct – Thu 15 Oct · All day"
        );
        assert_eq!(
            when_text(&timed("x", (13, 9, 0), (13, 10, 30)).when),
            "Tue 13 Oct · 09:00 – 10:30"
        );
        assert_eq!(
            when_text(&timed("x", (13, 23, 0), (14, 1, 0)).when),
            "Tue 13 Oct 23:00 – Wed 14 Oct 01:00"
        );
    }
}
