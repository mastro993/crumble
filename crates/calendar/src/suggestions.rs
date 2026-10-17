//! Which Events Crumble suggests recording as they start: meetings, by their join link or the
//! people invited, each once per run and only near its start.

use chrono::{DateTime, Local, TimeDelta};
use crumble_eventkit::{Event, Occurrence, When};
use std::collections::HashSet;

/// How long after its start an Event is still suggested, as after the Mac wakes from sleep.
pub const LATE_BY: TimeDelta = TimeDelta::minutes(5);

/// What tells a Suggestion's Event apart across reads: its series UID and occurrence once it
/// has a UID, as [`Event::id`] changes when the Event moves (ADR 0004), else its id.
pub fn key(event: &Event) -> String {
    let Some(uid) = &event.series_uid else {
        return format!("id:{}", event.id);
    };
    let occurrence = match event.occurrence {
        None => String::new(),
        Some(Occurrence::Day(day)) => day.to_string(),
        Some(Occurrence::At(at)) => at.timestamp().to_string(),
    };
    format!("uid:{uid}#{occurrence}")
}

/// The Event in `events` whose [`key`] is `key`.
pub fn find<'a>(events: &'a [Event], key: &str) -> Option<&'a Event> {
    events.iter().find(|event| self::key(event) == key)
}

/// The `events` to suggest at `now`: timed, in one of the `shown` Calendars, with a join link
/// or someone invited besides the user, started no more than [`LATE_BY`] ago, not the one
/// `recording` names by its [`key`], and not already `suggested`.
pub fn due<'a>(
    events: &'a [Event],
    shown: &[String],
    recording: Option<&str>,
    suggested: &HashSet<String>,
    now: DateTime<Local>,
) -> Vec<&'a Event> {
    events
        .iter()
        .filter(|event| {
            let When::Timed { start, .. } = event.when else {
                return false;
            };
            let key = key(event);
            start <= now
                && now - start <= LATE_BY
                && shown.contains(&event.calendar_id)
                && (event.join.is_some() || event.participants.iter().any(|person| !person.you))
                && recording != Some(key.as_str())
                && !suggested.contains(&key)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{due, find, key};
    use chrono::{DateTime, Local, NaiveDate, TimeDelta, TimeZone, Utc};
    use crumble_eventkit::{Event, Join, Occurrence, Participant, Response, When};
    use std::collections::HashSet;

    fn at(hour: u32, minute: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 10, 17, hour, minute, 0)
            .earliest()
            .unwrap_or_default()
    }

    fn person(you: bool) -> Participant {
        Participant {
            name: "Someone".into(),
            response: Response::Accepted,
            organizer: false,
            you,
        }
    }

    /// A call at 10:00 with a join link, in the shown Calendar.
    fn call(id: &str) -> Event {
        Event {
            id: id.into(),
            series_uid: None,
            occurrence: None,
            calendar_id: "shown".into(),
            title: id.into(),
            when: When::Timed {
                start: at(10, 0),
                end: at(10, 30),
            },
            location: None,
            description: None,
            join: Some(Join {
                url: "https://meet.example.com/abc".into(),
                service: "Google Meet",
            }),
            participants: Vec::new(),
        }
    }

    fn due_at(events: &[Event], now: DateTime<Local>) -> Vec<String> {
        due(events, &["shown".into()], None, &HashSet::new(), now)
            .into_iter()
            .map(|event| event.id.clone())
            .collect()
    }

    #[test]
    fn a_call_is_due_from_its_start_until_five_minutes_later() {
        let events = [call("Call")];
        assert!(due_at(&events, at(9, 59)).is_empty());
        assert_eq!(due_at(&events, at(10, 0)), ["Call"]);
        assert_eq!(due_at(&events, at(10, 5)), ["Call"]);
        assert!(due_at(&events, at(10, 5) + TimeDelta::seconds(1)).is_empty());
    }

    #[test]
    fn an_all_day_event_is_never_due() {
        let events = [Event {
            when: When::AllDay {
                first: NaiveDate::from_ymd_opt(2026, 10, 17).unwrap_or_default(),
                last: NaiveDate::from_ymd_opt(2026, 10, 17).unwrap_or_default(),
            },
            ..call("Offsite")
        }];
        assert!(due_at(&events, at(10, 0)).is_empty());
    }

    #[test]
    fn only_shown_calendars_are_due() {
        let events = [Event {
            calendar_id: "hidden".into(),
            ..call("Call")
        }];
        assert!(due_at(&events, at(10, 0)).is_empty());
    }

    #[test]
    fn a_meeting_needs_a_join_link_or_someone_besides_the_user() {
        let alone = Event {
            join: None,
            participants: vec![person(true)],
            ..call("Focus")
        };
        let invited = Event {
            participants: vec![person(true), person(false)],
            ..alone.clone()
        };
        let invited = Event {
            id: "Review".into(),
            ..invited
        };
        assert_eq!(due_at(&[alone, invited], at(10, 0)), ["Review"]);
    }

    #[test]
    fn the_event_being_recorded_and_those_suggested_are_not_due() {
        let events = [call("Recorded"), call("Suggested"), call("New")];
        let suggested = HashSet::from([key(&events[1])]);
        let recording = key(&events[0]);
        let due = due(
            &events,
            &["shown".into()],
            Some(&recording),
            &suggested,
            at(10, 1),
        );
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, "New");
    }

    #[test]
    fn the_key_follows_an_occurrence_that_moved() {
        let first = Event {
            series_uid: Some("series".into()),
            occurrence: Some(Occurrence::At(Utc::now())),
            ..call("before")
        };
        let moved = Event {
            id: "after".into(),
            when: When::Timed {
                start: at(11, 0),
                end: at(11, 30),
            },
            ..first.clone()
        };
        assert_eq!(key(&first), key(&moved));
        let next = Event {
            occurrence: Some(Occurrence::At(Utc::now() + TimeDelta::days(7))),
            ..first.clone()
        };
        assert_ne!(key(&first), key(&next));
        // Without a UID, the id tells Events apart.
        assert_ne!(key(&call("a")), key(&call("b")));
        let events = [call("a"), moved];
        assert_eq!(
            find(&events, &key(&first)).map(|event| event.id.as_str()),
            Some("after")
        );
        assert!(find(&events, "id:gone").is_none());
    }
}
