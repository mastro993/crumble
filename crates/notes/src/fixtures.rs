//! What the tests of every part share: times, Events, and Notes with a Recording.

use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone, Utc};
use crumble_eventkit::{Event, Occurrence, When};

use crate::{Note, Notes};

pub(crate) fn at(hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 13, hour, 0, 0)
        .single()
        .unwrap_or_else(|| panic!("bad time"))
}

pub(crate) fn day(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, day).unwrap_or_else(|| panic!("bad day"))
}

pub(crate) fn timed(start: DateTime<Utc>) -> When {
    When::Timed {
        start: start.with_timezone(&Local),
        end: (start + Duration::hours(1)).with_timezone(&Local),
    }
}

pub(crate) fn event(
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

pub(crate) fn ok<T>(result: anyhow::Result<T>) -> T {
    result.unwrap_or_else(|error| panic!("{error}"))
}

/// Puts a stand-in audio file in the Note's Recording folder.
pub(crate) fn record_audio(notes: &Notes, note: &Note) -> std::path::PathBuf {
    let dir = notes.recording_dir(&note.id);
    ok(std::fs::create_dir_all(&dir).map_err(anyhow::Error::from));
    ok(std::fs::write(dir.join("mic.aac"), b"audio").map_err(anyhow::Error::from));
    dir
}

pub(crate) fn line(track: crate::Track, start_s: u64, text: &str) -> crate::TranscriptSegment {
    crate::TranscriptSegment {
        track,
        start: std::time::Duration::from_secs(start_s),
        end: std::time::Duration::from_secs(start_s + 1),
        text: text.into(),
    }
}

/// A Quick note with a finished Recording.
pub(crate) fn recorded(notes: &mut Notes) -> Note {
    let note = ok(notes.create(None, at(5)));
    ok(notes.set_recording_started(&note.id, at(6)));
    ok(notes.set_recording_finished(&note.id, std::time::Duration::from_secs(60), at(7)));
    note
}
