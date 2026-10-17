use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone, Utc};
use crumble_eventkit::{Event, Occurrence, When};
use diesel::RunQueryDsl;
use diesel::sql_types::{Nullable, Text};

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
    assert_eq!(ok(notes.for_event(&moved)), Some(note));
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
    assert_eq!(ok(notes.for_event(&standup)), Some(note));
}

#[test]
fn a_moved_single_event_keeps_its_notes_by_key() {
    let mut notes = Notes::in_memory();
    let note = ok(notes.create(Some(&event(Some("uid"), None, "1:1", timed(at(7)))), at(6)));
    let moved = event(Some("uid"), None, "1:1 moved", timed(at(10)));
    assert_eq!(ok(notes.for_event(&moved)), Some(note));
}

#[test]
fn a_note_made_before_its_event_synced_is_found_by_snapshot() {
    let mut notes = Notes::in_memory();
    let note = ok(notes.create(Some(&event(None, None, "Lunch", timed(at(12)))), at(6)));
    let synced = event(Some("uid"), None, "Lunch", timed(at(12)));
    assert_eq!(ok(notes.for_event(&synced)), Some(note.clone()));
    // Making one again, before or after the sync, finds the same Note.
    let unsynced = event(None, None, "Lunch", timed(at(12)));
    assert_eq!(ok(notes.create(Some(&unsynced), at(7))), note);
    assert_eq!(ok(notes.create(Some(&synced), at(8))), note);
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
    assert_eq!(ok(notes.for_event(&tuesday)), None);
    // Nor does the series' non-repeating twin with the same UID.
    let single = event(Some("uid"), None, "Standup", timed(at(8)));
    assert_eq!(ok(notes.for_event(&single)), None);
}

#[test]
fn an_occurrence_has_one_note_in_every_calendar() {
    let mut notes = Notes::in_memory();
    let mine = event(
        Some("uid"),
        Some(Occurrence::At(at(7))),
        "Standup",
        timed(at(7)),
    );
    let note = ok(notes.create(Some(&mine), at(6)));
    let invited = Event {
        id: "other".into(),
        calendar_id: "work".into(),
        ..mine
    };
    assert_eq!(ok(notes.create(Some(&invited), at(8))), note);
    assert_eq!(ok(notes.for_event(&invited)), Some(note));
}

#[test]
fn the_key_wins_over_an_older_snapshot() {
    let mut notes = Notes::in_memory();
    let unsynced = ok(notes.create(Some(&event(None, None, "Lunch", timed(at(12)))), at(5)));
    let renamed = event(Some("uid"), None, "Lunch, moved", timed(at(12)));
    let keyed = ok(notes.create(Some(&renamed), at(6)));
    assert_ne!(keyed.id, unsynced.id);
    let back = event(Some("uid"), None, "Lunch", timed(at(12)));
    assert_eq!(ok(notes.for_event(&back)), Some(keyed));
}

#[test]
fn events_of_other_series_with_the_same_title_and_start_keep_their_own_notes() {
    let mut notes = Notes::in_memory();
    let mine = ok(notes.create(Some(&event(Some("a"), None, "Lunch", timed(at(12)))), at(6)));
    let other = event(Some("b"), None, "Lunch", timed(at(12)));
    assert_eq!(ok(notes.for_event(&other)), None);
    assert_ne!(ok(notes.create(Some(&other), at(7))).id, mine.id);
    let unsynced = event(None, None, "Lunch", timed(at(12)));
    assert_eq!(ok(notes.for_event(&unsynced)), None);
}

/// Inserts an attached Note straight into the table, past `create`'s lookup.
fn insert(
    notes: &mut Notes,
    id: &str,
    uid: Option<&str>,
    occurrence: Option<&str>,
) -> diesel::QueryResult<usize> {
    diesel::sql_query(
        "INSERT INTO notes (id, event_series_uid, event_occurrence, event_title, event_start, \
         created_at, updated_at) VALUES (?, ?, ?, 'Standup', '2026-10-13T07:00:00Z', \
         '2026-10-13T06:00:00.000Z', '2026-10-13T06:00:00.000Z')",
    )
    .bind::<Text, _>(id)
    .bind::<Nullable<Text>, _>(uid)
    .bind::<Nullable<Text>, _>(occurrence)
    .execute(ok(notes.connection()))
}

#[test]
fn the_database_keeps_one_live_note_per_key() {
    let mut notes = Notes::in_memory();
    assert!(insert(&mut notes, "single", Some("uid"), None).is_ok());
    assert!(insert(&mut notes, "single again", Some("uid"), None).is_err());
    assert!(insert(&mut notes, "monday", Some("uid"), Some("2026-10-13")).is_ok());
    assert!(insert(&mut notes, "monday again", Some("uid"), Some("2026-10-13")).is_err());
    // Notes with no UID yet are left out; `create` keeps them single instead.
    assert!(insert(&mut notes, "unsynced", None, None).is_ok());
    assert!(insert(&mut notes, "unsynced again", None, None).is_ok());
    // So are deleted Notes.
    ok(notes.delete(&super::NoteId("single".into()), at(7)));
    assert!(insert(&mut notes, "single after delete", Some("uid"), None).is_ok());
}

#[test]
fn quick_notes_are_never_an_events() {
    let mut notes = Notes::in_memory();
    let quick = ok(notes.create(None, at(7)));
    assert_eq!(quick.event, None);
    // An Event with no UID and no title still finds nothing: a Quick note has no snapshot.
    let untitled = event(None, None, "", timed(at(7)));
    assert_eq!(ok(notes.for_event(&untitled)), None);
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
    let found = ok(notes.for_event(&standup)).unwrap_or_else(|| panic!("no Note"));
    assert_eq!(found.title.as_deref(), Some("Plan"));
    assert_eq!(found.memo, "Ship it");
    assert_eq!(found.created_at, at(6));
    assert_eq!(found.updated_at, at(8));
    ok(notes.set_title(&note.id, None, at(9)));
    let found = ok(notes.for_event(&standup)).unwrap_or_else(|| panic!("no Note"));
    assert_eq!(found.title, None);
    assert_eq!(found.updated_at, at(9));
}

#[test]
fn a_deleted_note_is_gone_everywhere_and_stays_deleted() {
    let mut notes = Notes::in_memory();
    let standup = event(Some("uid"), None, "Standup", timed(at(7)));
    let attached = ok(notes.create(Some(&standup), at(6)));
    let quick = ok(notes.create(None, at(6)));
    ok(notes.delete(&attached.id, at(7)));
    ok(notes.delete(&quick.id, at(7)));
    assert_eq!(ok(notes.for_event(&standup)), None);
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

/// Puts a stand-in audio file in the Note's Recording folder.
fn record_audio(notes: &Notes, note: &Note) -> std::path::PathBuf {
    let dir = notes.recording_dir(&note.id);
    ok(std::fs::create_dir_all(&dir).map_err(anyhow::Error::from));
    ok(std::fs::write(dir.join("mic.aac"), b"audio").map_err(anyhow::Error::from));
    dir
}

#[test]
fn a_recording_starts_once_then_finishes() {
    let mut notes = Notes::in_memory();
    let note = ok(notes.create(None, at(6)));
    let started =
        ok(notes.set_recording_started(&note.id, at(7) + Duration::nanoseconds(1_234_567)));
    assert_eq!(started.started_at, at(7) + Duration::milliseconds(1));
    assert_eq!(started.duration, None);
    // A Note has at most one Recording.
    assert!(notes.set_recording_started(&note.id, at(8)).is_err());
    assert_eq!(
        ok(notes.unfinished_recordings()),
        std::slice::from_ref(&note.id)
    );

    ok(notes.set_recording_finished(&note.id, std::time::Duration::from_millis(90_500), at(8)));
    let [found] = <[Note; 1]>::try_from(ok(notes.quick_notes(at(0)..at(23))))
        .unwrap_or_else(|found| panic!("{found:?}"));
    assert_eq!(
        found.recording,
        Some(super::RecordingInfo {
            started_at: started.started_at,
            duration: Some(std::time::Duration::from_millis(90_500)),
        })
    );
    assert_eq!(found.updated_at, at(8));
    assert_eq!(ok(notes.unfinished_recordings()), []);
}

#[test]
fn finishing_needs_a_started_recording() {
    let mut notes = Notes::in_memory();
    let note = ok(notes.create(None, at(6)));
    assert!(
        notes
            .set_recording_finished(&note.id, std::time::Duration::from_secs(1), at(7))
            .is_err()
    );
}

#[test]
fn a_deleted_note_has_no_unfinished_recording() {
    let mut notes = Notes::in_memory();
    let note = ok(notes.create(None, at(6)));
    ok(notes.set_recording_started(&note.id, at(7)));
    ok(notes.delete(&note.id, at(8)));
    assert_eq!(ok(notes.unfinished_recordings()), []);
}

#[test]
fn deleting_the_recording_forgets_it_and_its_audio() {
    let mut notes = Notes::in_memory();
    let note = ok(notes.create(None, at(6)));
    ok(notes.set_recording_started(&note.id, at(7)));
    let dir = record_audio(&notes, &note);
    ok(notes.delete_recording(&note.id, at(8)));
    assert!(!dir.exists());
    assert_eq!(ok(notes.quick_notes(at(0)..at(23)))[0].recording, None);
    // The Note may record again, and deleting with no audio left is fine.
    ok(notes.set_recording_started(&note.id, at(9)));
    ok(notes.delete_recording(&note.id, at(10)));
}

#[test]
fn deleting_a_note_deletes_its_audio() {
    let mut notes = Notes::in_memory();
    let note = ok(notes.create(None, at(6)));
    ok(notes.set_recording_started(&note.id, at(7)));
    let dir = record_audio(&notes, &note);
    ok(notes.delete(&note.id, at(8)));
    assert!(!dir.exists());
}

#[test]
fn audio_that_cannot_be_deleted_keeps_the_note_for_another_try() {
    let mut notes = Notes::in_memory();
    let note = ok(notes.create(None, at(6)));
    ok(notes.set_recording_started(&note.id, at(7)));
    // A file where the folder should be, so deleting the folder fails.
    let dir = notes.recording_dir(&note.id);
    ok(std::fs::create_dir_all(&notes.recordings).map_err(anyhow::Error::from));
    ok(std::fs::write(&dir, b"audio").map_err(anyhow::Error::from));
    assert!(notes.delete_recording(&note.id, at(8)).is_err());
    assert!(notes.delete(&note.id, at(8)).is_err());
    let listed = ok(notes.quick_notes(at(0)..at(23)));
    assert!(listed[0].recording.is_some());
    ok(std::fs::remove_file(&dir).map_err(anyhow::Error::from));
    ok(notes.delete(&note.id, at(9)));
}

#[test]
fn an_in_memory_notes_deletes_its_recordings_when_dropped() {
    let notes = Notes::in_memory();
    let note_dir = notes.recordings.join("note");
    ok(std::fs::create_dir_all(&note_dir).map_err(anyhow::Error::from));
    let recordings = notes.recordings.clone();
    drop(notes);
    assert!(!recordings.exists());
}

#[test]
fn a_quick_note_is_listed_at_its_recordings_start() {
    let mut notes = Notes::in_memory();
    // Made before the range, recorded inside it.
    let recorded = ok(notes.create(None, at(5)));
    let started = ok(notes.set_recording_started(&recorded.id, at(8)));
    let plain = ok(notes.create(None, at(7)));
    // Made inside the range, recorded after it.
    let late = ok(notes.create(None, at(6)));
    ok(notes.set_recording_started(&late.id, at(10)));
    let found = ok(notes.quick_notes(at(6)..at(9)));
    assert_eq!(ids(&found), [recorded.id.0.as_str(), plain.id.0.as_str()]);
    assert_eq!(found[0].listed_at(), started.started_at);
    assert_eq!(found[1].listed_at(), at(7));
}

#[test]
fn a_crash_left_recording_keeps_what_its_files_hold() {
    let mut notes = Notes::in_memory();
    let kept = ok(notes.create(None, at(5)));
    let empty = ok(notes.create(None, at(5)));
    let unreadable = ok(notes.create(None, at(5)));
    let finished = ok(notes.create(None, at(5)));
    for note in [&kept, &empty, &unreadable, &finished] {
        ok(notes.set_recording_started(&note.id, at(6)));
    }
    ok(notes.set_recording_finished(&finished.id, std::time::Duration::from_secs(9), at(7)));
    let kept_dir = notes.recording_dir(&kept.id);
    let empty_dir = notes.recording_dir(&empty.id);
    let unreadable_dir = notes.recording_dir(&unreadable.id);
    ok(notes.recover_recordings(
        |dir| {
            if dir == kept_dir {
                Ok(std::time::Duration::from_secs(42))
            } else if dir == empty_dir {
                Ok(std::time::Duration::ZERO)
            } else {
                assert_eq!(dir, unreadable_dir);
                Err(anyhow::anyhow!("cannot read"))
            }
        },
        at(8),
    ));
    let recordings: Vec<_> = ok(notes.quick_notes(at(0)..at(23)))
        .into_iter()
        .map(|note| (note.id, note.recording.map(|recording| recording.duration)))
        .collect();
    assert!(recordings.contains(&(kept.id, Some(Some(std::time::Duration::from_secs(42))))));
    // Nothing in it: deleted.
    assert!(recordings.contains(&(empty.id, None)));
    // Its files may still hold audio: left unfinished for the next launch.
    assert!(recordings.contains(&(unreadable.id.clone(), Some(None))));
    assert!(recordings.contains(&(finished.id, Some(Some(std::time::Duration::from_secs(9))))));
    assert_eq!(ok(notes.unfinished_recordings()), [unreadable.id]);
}
