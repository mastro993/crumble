use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone, Utc};
use crumble_eventkit::{Event, Occurrence, When};

use super::{
    EventLink, Note, Notes, decode_occurrence, decode_time, encode_occurrence, encode_time,
    start_of,
};

fn at(hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 13, hour, 0, 0)
        .single()
        .unwrap_or_else(|| panic!("bad time"))
}

fn day(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, day).unwrap_or_else(|| panic!("bad day"))
}

fn timed(start: DateTime<Utc>) -> When {
    When::Timed {
        start: start.with_timezone(&Local),
        end: (start + Duration::hours(1)).with_timezone(&Local),
    }
}

fn event(
    series_uid: Option<&str>,
    occurrence: Option<Occurrence>,
    title: &str,
    when: When,
) -> Event {
    Event {
        id: "unstable".into(),
        series_uid: series_uid.map(str::to_owned),
        occurrence,
        calendar_id: "calendar".into(),
        title: title.into(),
        when,
        location: None,
        description: None,
        join: None,
        participants: Vec::new(),
    }
}

fn ok<T>(result: anyhow::Result<T>) -> T {
    result.unwrap_or_else(|error| panic!("{error}"))
}

fn ids(notes: &[Note]) -> Vec<&str> {
    notes.iter().map(|note| note.id.0.as_str()).collect()
}

#[test]
fn encodings_round_trip() {
    assert_eq!(encode_occurrence(Occurrence::Day(day(13))), "2026-10-13");
    assert_eq!(
        encode_occurrence(Occurrence::At(at(7) + Duration::milliseconds(250))),
        "2026-10-13T07:00:00Z"
    );
    for occurrence in [Occurrence::Day(day(13)), Occurrence::At(at(7))] {
        assert_eq!(
            ok(decode_occurrence(&encode_occurrence(occurrence))),
            occurrence
        );
    }
    let time = at(7) + Duration::milliseconds(250);
    assert_eq!(encode_time(time), "2026-10-13T07:00:00.250Z");
    assert_eq!(ok(decode_time(&encode_time(time))), time);
    assert!(decode_occurrence("yesterday").is_err());
}

#[test]
fn an_event_start_is_its_first_day_or_its_start_in_utc() {
    let all_day = When::AllDay {
        first: day(13),
        last: day(14),
    };
    assert_eq!(start_of(&all_day), Occurrence::Day(day(13)));
    assert_eq!(start_of(&timed(at(7))), Occurrence::At(at(7)));
}

#[test]
fn a_moved_repeating_occurrence_keeps_its_notes_by_key() {
    let mut notes = Notes::in_memory();
    let original = event(
        Some("uid"),
        Some(Occurrence::At(at(7))),
        "Standup",
        timed(at(7)),
    );
    let note = ok(notes.create(Some(&original), at(6)));
    assert_eq!(
        note.event,
        Some(EventLink {
            series_uid: Some("uid".into()),
            occurrence: Some(Occurrence::At(at(7))),
            title: "Standup".into(),
            start: Occurrence::At(at(7)),
        })
    );
    let moved = event(
        Some("uid"),
        Some(Occurrence::At(at(7))),
        "Standup, later",
        timed(at(9)),
    );
    assert_eq!(ok(notes.for_event(&moved)), [note]);
}

#[test]
fn a_created_note_reads_back_as_it_was_returned() {
    let mut notes = Notes::in_memory();
    let start = at(7) + Duration::nanoseconds(123_456_789);
    let standup = event(
        Some("uid"),
        Some(Occurrence::At(start)),
        "Standup",
        timed(start),
    );
    let note = ok(notes.create(Some(&standup), at(6) + Duration::nanoseconds(987_654_321)));
    assert_eq!(note.created_at, at(6) + Duration::milliseconds(987));
    assert_eq!(ok(notes.for_event(&standup)), [note]);
}

#[test]
fn a_moved_single_event_keeps_its_notes_by_key() {
    let mut notes = Notes::in_memory();
    let note = ok(notes.create(Some(&event(Some("uid"), None, "1:1", timed(at(7)))), at(6)));
    let moved = event(Some("uid"), None, "1:1 moved", timed(at(10)));
    assert_eq!(ok(notes.for_event(&moved)), [note]);
}

#[test]
fn a_note_made_before_its_event_synced_is_found_by_snapshot() {
    let mut notes = Notes::in_memory();
    let note = ok(notes.create(Some(&event(None, None, "Lunch", timed(at(12)))), at(6)));
    let synced = event(Some("uid"), None, "Lunch", timed(at(12)));
    assert_eq!(ok(notes.for_event(&synced)), [note]);
}

#[test]
fn another_occurrence_of_the_series_does_not_match() {
    let mut notes = Notes::in_memory();
    let monday = event(
        Some("uid"),
        Some(Occurrence::At(at(7))),
        "Standup",
        timed(at(7)),
    );
    ok(notes.create(Some(&monday), at(6)));
    let tuesday = event(
        Some("uid"),
        Some(Occurrence::At(at(7) + Duration::days(1))),
        "Standup",
        timed(at(7) + Duration::days(1)),
    );
    assert_eq!(ok(notes.for_event(&tuesday)), []);
    // Nor does the series' non-repeating twin with the same UID.
    let single = event(Some("uid"), None, "Standup", timed(at(8)));
    assert_eq!(ok(notes.for_event(&single)), []);
}

#[test]
fn notes_of_an_occurrence_come_oldest_first() {
    let mut notes = Notes::in_memory();
    let standup = event(Some("uid"), None, "Standup", timed(at(7)));
    let second = ok(notes.create(Some(&standup), at(8)));
    let first = ok(notes.create(Some(&standup), at(6)));
    let found = ok(notes.for_event(&standup));
    assert_eq!(ids(&found), [first.id.0.as_str(), second.id.0.as_str()]);
}

#[test]
fn quick_notes_are_never_an_events() {
    let mut notes = Notes::in_memory();
    let quick = ok(notes.create(None, at(7)));
    assert_eq!(quick.event, None);
    // An Event with no UID and no title still finds nothing: a Quick note has no snapshot.
    let untitled = event(None, None, "", timed(at(7)));
    assert_eq!(ok(notes.for_event(&untitled)), []);
}

#[test]
fn quick_notes_in_a_range_come_newest_first() {
    let mut notes = Notes::in_memory();
    let before = ok(notes.create(None, at(5)));
    let first = ok(notes.create(None, at(6)));
    let second = ok(notes.create(None, at(8)));
    let end = ok(notes.create(None, at(9)));
    ok(notes.create(Some(&event(None, None, "Lunch", timed(at(7)))), at(7)));
    let found = ok(notes.quick_notes(at(6)..at(9)));
    assert_eq!(ids(&found), [second.id.0.as_str(), first.id.0.as_str()]);
    assert!(!ids(&found).contains(&before.id.0.as_str()));
    assert!(!ids(&found).contains(&end.id.0.as_str()));
}

#[test]
fn edits_are_kept_and_bump_updated_at() {
    let mut notes = Notes::in_memory();
    let standup = event(Some("uid"), None, "Standup", timed(at(7)));
    let note = ok(notes.create(Some(&standup), at(6)));
    ok(notes.set_title(&note.id, Some("Plan"), at(7)));
    ok(notes.set_memo(&note.id, "Ship it", at(8)));
    let [found] = <[Note; 1]>::try_from(ok(notes.for_event(&standup)))
        .unwrap_or_else(|found| panic!("{found:?}"));
    assert_eq!(found.title.as_deref(), Some("Plan"));
    assert_eq!(found.memo, "Ship it");
    assert_eq!(found.created_at, at(6));
    assert_eq!(found.updated_at, at(8));
    ok(notes.set_title(&note.id, None, at(9)));
    let found = ok(notes.for_event(&standup));
    assert_eq!(found[0].title, None);
    assert_eq!(found[0].updated_at, at(9));
}

#[test]
fn a_deleted_note_is_gone_everywhere_and_stays_deleted() {
    let mut notes = Notes::in_memory();
    let standup = event(Some("uid"), None, "Standup", timed(at(7)));
    let attached = ok(notes.create(Some(&standup), at(6)));
    let quick = ok(notes.create(None, at(6)));
    ok(notes.delete(&attached.id, at(7)));
    ok(notes.delete(&quick.id, at(7)));
    assert_eq!(ok(notes.for_event(&standup)), []);
    assert_eq!(ok(notes.quick_notes(at(0)..at(23))), []);
    assert!(notes.set_memo(&attached.id, "back?", at(8)).is_err());
    assert!(notes.set_title(&quick.id, Some("back?"), at(8)).is_err());
    assert!(notes.delete(&quick.id, at(8)).is_err());
    assert_eq!(ok(notes.quick_notes(at(0)..at(23))), []);
}

#[test]
fn writing_an_unknown_note_errors() {
    let mut notes = Notes::in_memory();
    let unknown = super::NoteId("missing".into());
    assert!(notes.set_memo(&unknown, "memo", at(8)).is_err());
}
