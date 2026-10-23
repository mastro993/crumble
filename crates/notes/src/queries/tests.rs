use chrono::Duration;
use crumble_eventkit::{Event, Occurrence, When};
use diesel::RunQueryDsl;
use diesel::sql_types::{Nullable, Text};

use super::start_of;
use crate::fixtures::{at, day, event, line, ok, record_audio, timed};
use crate::rows::encode_time;
use crate::{EventLink, Note, Notes};

fn ids(notes: &[Note]) -> Vec<&str> {
    notes.iter().map(|note| note.id.0.as_str()).collect()
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
    ok(notes.delete(&crate::NoteId("single".into()), at(7)));
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
    let unknown = crate::NoteId("missing".into());
    assert!(notes.set_memo(&unknown, "memo", at(8)).is_err());
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
fn a_deleted_note_keeps_only_its_id_and_timestamps() {
    use crumble_db::schema::{notes as table, transcript_segments};
    use diesel::prelude::*;
    let mut notes = Notes::in_memory();
    let standup = event(
        Some("uid"),
        Some(Occurrence::At(at(7))),
        "Standup",
        timed(at(7)),
    );
    let note = ok(notes.create(Some(&standup), at(5)));
    ok(notes.set_title(&note.id, Some("Plans"), at(6)));
    ok(notes.set_memo(&note.id, "Secret plans", at(6)));
    ok(notes.set_recording_started(&note.id, at(6)));
    ok(notes.set_recording_finished(&note.id, std::time::Duration::from_secs(60), at(7)));
    let dir = record_audio(&notes, &note);
    ok(notes.set_transcript(
        &note.id,
        "apple",
        "Model",
        "en_US",
        &[line(crate::Track::Mic, 0, "Hi.")],
        at(8),
    ));
    ok(notes.set_summary(&note.id, "anthropic", "m1", "Secret summary", at(8)));
    ok(notes.delete(&note.id, at(9)));

    let connection = ok(notes.connection());
    let row = ok(table::table
        .find(&note.id.0)
        .select(crate::rows::NoteRow::as_select())
        .first(connection)
        .map_err(anyhow::Error::from));
    assert_eq!(row.title, None);
    assert_eq!(row.memo, "");
    assert_eq!(
        (
            row.event_series_uid,
            row.event_occurrence,
            row.event_title,
            row.event_start
        ),
        (None, None, None, None)
    );
    assert_eq!(
        (
            row.transcript_model,
            row.transcript_locale,
            row.transcribed_at
        ),
        (None, None, None)
    );
    assert_eq!(
        (row.recording_started_at, row.recording_duration_ms),
        (None, None)
    );
    assert_eq!(
        (
            row.summary,
            row.summary_provider,
            row.summary_model,
            row.summarized_at
        ),
        (None, None, None, None)
    );
    assert_eq!(row.updated_at, encode_time(at(9)));
    let deleted_at: Option<String> = ok(table::table
        .find(&note.id.0)
        .select(table::deleted_at)
        .first(connection)
        .map_err(anyhow::Error::from));
    assert_eq!(deleted_at, Some(encode_time(at(9))));
    let segments: i64 = ok(transcript_segments::table
        .count()
        .get_result(connection)
        .map_err(anyhow::Error::from));
    assert_eq!(segments, 0);
    assert!(!dir.exists());

    assert_eq!(ok(notes.for_event(&standup)), None);
    assert_eq!(ok(notes.quick_notes(at(0)..at(23))), []);
    assert_eq!(ok(notes.transcript(&note.id)), []);
    assert_eq!(ok(notes.untranscribed()), []);
    assert_eq!(ok(notes.unsummarized()), []);
}
