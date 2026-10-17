//! Notes kept in Crumble's database: each attached to one occurrence of an Event, or standing
//! alone as a Quick note. A Note finds its Event again by the key and snapshot of ADR 0004.
//! A Note's Recording is kept as files in its own folder, which this crate names and deletes;
//! `crumble-audio` writes and reads the files in it (ADR 0005).

use std::fmt;
use std::io::ErrorKind;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail};
use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use crumble_db::schema::notes;
use crumble_eventkit::{Event, Occurrence, When};
use diesel::dsl::sql;
use diesel::expression::SqlLiteral;
use diesel::prelude::*;
use diesel::sql_types::Text;
use uuid::Uuid;

/// A Note as stored. See CONTEXT.md.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    /// Stable across edits, restarts, and later across devices.
    pub id: NoteId,
    /// The title the user gave, if any.
    pub title: Option<String>,
    /// What the user typed, possibly empty.
    pub memo: String,
    /// The Event occurrence the Note is attached to; `None` for a Quick note.
    pub event: Option<EventLink>,
    /// When the Note was made.
    pub created_at: DateTime<Utc>,
    /// When the Note last changed.
    pub updated_at: DateTime<Utc>,
    /// The Note's Recording, if it has one.
    pub recording: Option<RecordingInfo>,
}

impl Note {
    /// When the Agenda lists the Note as a Quick note: its Recording's start, else when it was
    /// made.
    pub fn listed_at(&self) -> DateTime<Utc> {
        self.recording
            .as_ref()
            .map_or(self.created_at, |recording| recording.started_at)
    }
}

/// What the database keeps about a Note's Recording; the audio is in [`Notes::recording_dir`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordingInfo {
    /// When the Recording started, to the millisecond.
    pub started_at: DateTime<Utc>,
    /// How long it is once finished; `None` while it runs, and after a crash until
    /// [`Notes::set_recording_finished`] stores what the files hold.
    pub duration: Option<Duration>,
}

/// What a Note keeps about the Event occurrence it is attached to (ADR 0004).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventLink {
    /// The Event's [`Event::series_uid`] when the Note was made; `None` if it had not synced.
    pub series_uid: Option<String>,
    /// The Event's [`Event::occurrence`]; `None` for an Event that does not repeat.
    pub occurrence: Option<Occurrence>,
    /// The Event's title when the Note was made.
    pub title: String,
    /// The Event's start when the Note was made: its first day, or its start time.
    pub start: Occurrence,
}

/// Identifies a Note: a UUID v7, so ids sort by creation time.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NoteId(String);

impl fmt::Display for NoteId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The Notes in the app's database. Every change is written as it happens; reads skip deleted
/// Notes. Methods take `now` so callers decide the clock; the app passes `Utc::now()`.
pub struct Notes {
    /// `None` only when not even an in-memory database opens; every call then errors.
    connection: Option<SqliteConnection>,
    /// The folder holding one folder of audio per Recording.
    recordings: PathBuf,
    /// Whether `recordings` is a temporary folder, deleted with `Notes`.
    temporary: bool,
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = notes, check_for_backend(diesel::sqlite::Sqlite))]
struct NoteRow {
    id: String,
    title: Option<String>,
    memo: String,
    event_series_uid: Option<String>,
    event_occurrence: Option<String>,
    event_title: Option<String>,
    event_start: Option<String>,
    created_at: String,
    updated_at: String,
    recording_started_at: Option<String>,
    recording_duration_ms: Option<i32>,
}

impl Notes {
    /// The app's database file; when it cannot be opened, an in-memory one, logged like
    /// `Store::open`, so Notes made in that run are not kept.
    /// Recordings go in `recordings/` beside the file.
    pub fn open() -> Self {
        let opened = crumble_db::open_file().and_then(|connection| {
            Ok(Self {
                connection: Some(connection),
                recordings: crumble_db::data_dir()?.join("recordings"),
                temporary: false,
            })
        });
        opened.unwrap_or_else(|error| {
            eprintln!("Could not open the database, Notes are not kept: {error}");
            Self::in_memory()
        })
    }

    /// An in-memory database, with Recordings in a fresh temporary folder deleted on drop, as
    /// neither is kept.
    fn in_memory() -> Self {
        let connection = crumble_db::open(":memory:")
            .inspect_err(|error| eprintln!("Could not open an in-memory database: {error}"))
            .ok();
        let recordings =
            std::env::temp_dir().join(format!("crumble-recordings-{}", Uuid::now_v7()));
        Self {
            connection,
            recordings,
            temporary: true,
        }
    }

    fn connection(&mut self) -> anyhow::Result<&mut SqliteConnection> {
        self.connection
            .as_mut()
            .ok_or_else(|| anyhow!("no database"))
    }

    /// A new Quick note when `event` is None. Otherwise the Note of `event`'s occurrence, made
    /// only when [`Notes::for_event`] finds none, as an occurrence has at most one Note. A new
    /// Note's times come back as stored: `now` to the millisecond, the Event's to the second.
    pub fn create(&mut self, event: Option<&Event>, now: DateTime<Utc>) -> anyhow::Result<Note> {
        if let Some(event) = event
            && let Some(note) = self.for_event(event)?
        {
            return Ok(note);
        }
        let row = NoteRow {
            id: Uuid::now_v7().to_string(),
            title: None,
            memo: String::new(),
            event_series_uid: event.and_then(|event| event.series_uid.clone()),
            event_occurrence: event.and_then(|event| event.occurrence.map(encode_occurrence)),
            event_title: event.map(|event| event.title.clone()),
            event_start: event.map(|event| encode_occurrence(start_of(&event.when))),
            created_at: encode_time(now),
            updated_at: encode_time(now),
            recording_started_at: None,
            recording_duration_ms: None,
        };
        diesel::insert_into(notes::table)
            .values(&row)
            .execute(self.connection()?)?;
        decode_note(row)
    }

    /// The Note of this occurrence: the one whose key matches the Event's, else the oldest whose
    /// snapshot matches its title and start: a Note made before the Event synced, or one of its
    /// series whose occurrence key changed. A Note keyed to another series never matches, so two
    /// Events that share a title and start keep their own Notes. A read only; stored keys are
    /// never rewritten here.
    pub fn for_event(&mut self, event: &Event) -> anyhow::Result<Option<Note>> {
        let occurrence = event.occurrence.map(encode_occurrence);
        // With no UID, `event_series_uid = NULL` is never true, so only the snapshot matches.
        let key = notes::event_series_uid
            .eq(event.series_uid.as_deref())
            .and(notes::event_occurrence.is(occurrence.as_deref()));
        let snapshot = notes::event_title
            .eq(&event.title)
            .and(notes::event_start.eq(encode_occurrence(start_of(&event.when))))
            .and(
                notes::event_series_uid
                    .is_null()
                    .or(notes::event_series_uid.eq(event.series_uid.as_deref())),
            );
        let row = notes::table
            .filter(notes::deleted_at.is_null())
            .filter(key.or(snapshot))
            // The key's match first: SQLite sorts its true before false and NULL.
            .order((key.desc(), notes::created_at, notes::id))
            .select(NoteRow::as_select())
            .first(self.connection()?)
            .optional()?;
        row.map(decode_note).transpose()
    }

    /// Quick notes listed in `range` by [`Note::listed_at`], newest first, for the Agenda page.
    pub fn quick_notes(&mut self, range: Range<DateTime<Utc>>) -> anyhow::Result<Vec<Note>> {
        let rows = notes::table
            .filter(notes::event_start.is_null())
            .filter(notes::deleted_at.is_null())
            .filter(listed_at().ge(encode_time(range.start)))
            .filter(listed_at().lt(encode_time(range.end)))
            .order((listed_at().desc(), notes::id.desc()))
            .select(NoteRow::as_select())
            .load(self.connection()?)?;
        rows.into_iter().map(decode_note).collect()
    }

    /// Gives the Note `id` the title `title`, or removes it when `None`.
    pub fn set_title(
        &mut self,
        id: &NoteId,
        title: Option<&str>,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let updated = diesel::update(live(id))
            .set((
                notes::title.eq(title),
                notes::updated_at.eq(encode_time(now)),
            ))
            .execute(self.connection()?)?;
        written(updated, id)
    }

    /// Replaces the Memo of the Note `id`.
    pub fn set_memo(&mut self, id: &NoteId, memo: &str, now: DateTime<Utc>) -> anyhow::Result<()> {
        let updated = diesel::update(live(id))
            .set((notes::memo.eq(memo), notes::updated_at.eq(encode_time(now))))
            .execute(self.connection()?)?;
        written(updated, id)
    }

    /// Soft delete: sets `deleted_at`; every read skips it. The Recording's audio is deleted
    /// for good, as the user deleted the Note.
    pub fn delete(&mut self, id: &NoteId, now: DateTime<Utc>) -> anyhow::Result<()> {
        // The audio goes first: once the Note is deleted, nothing can reach its folder again.
        self.remove_audio(id)?;
        let now = encode_time(now);
        let updated = diesel::update(live(id))
            .set((notes::deleted_at.eq(&now), notes::updated_at.eq(&now)))
            .execute(self.connection()?)?;
        written(updated, id)
    }

    /// The folder of the Note `id`'s Recording: `recordings/<id>/`. It may not exist.
    pub fn recording_dir(&self, id: &NoteId) -> PathBuf {
        self.recordings.join(&id.0)
    }

    /// Marks the Note `id` as recording since `now`, and returns that as stored. Errors when it
    /// already has a Recording: a Note has at most one.
    pub fn set_recording_started(
        &mut self,
        id: &NoteId,
        now: DateTime<Utc>,
    ) -> anyhow::Result<RecordingInfo> {
        let started_at = encode_time(now);
        let updated = diesel::update(live(id).filter(notes::recording_started_at.is_null()))
            .set((
                notes::recording_started_at.eq(&started_at),
                notes::updated_at.eq(&started_at),
            ))
            .execute(self.connection()?)?;
        if updated == 0 {
            bail!("no Note {id} without a Recording");
        }
        Ok(RecordingInfo {
            started_at: decode_time(&started_at)?,
            duration: None,
        })
    }

    /// Stores how long the Note `id`'s Recording is, once it stopped or was recovered.
    pub fn set_recording_finished(
        &mut self,
        id: &NoteId,
        duration: Duration,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let duration_ms = i32::try_from(duration.as_millis()).unwrap_or(i32::MAX);
        let updated = diesel::update(live(id).filter(notes::recording_started_at.is_not_null()))
            .set((
                notes::recording_duration_ms.eq(duration_ms),
                notes::updated_at.eq(encode_time(now)),
            ))
            .execute(self.connection()?)?;
        written(updated, id)
    }

    /// Removes the Note `id`'s Recording: deletes its folder, then forgets it.
    pub fn delete_recording(&mut self, id: &NoteId, now: DateTime<Utc>) -> anyhow::Result<()> {
        // The audio goes first, so a failure leaves the Recording listed for another try.
        self.remove_audio(id)?;
        let updated = diesel::update(live(id))
            .set((
                notes::recording_started_at.eq(None::<String>),
                notes::recording_duration_ms.eq(None::<i32>),
                notes::updated_at.eq(encode_time(now)),
            ))
            .execute(self.connection()?)?;
        written(updated, id)
    }

    /// Finishes the Recordings a crash left running: each keeps the length `length` reads from
    /// its folder, and one with nothing readable is deleted. Run at launch, before anything
    /// records; `crumble_audio::recorded_length` reads the files.
    pub fn recover_recordings(
        &mut self,
        length: impl Fn(&Path) -> anyhow::Result<Duration>,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        for id in self.unfinished_recordings()? {
            match length(&self.recording_dir(&id)) {
                Ok(duration) => self.set_recording_finished(&id, duration, now)?,
                Err(error) => {
                    eprintln!(
                        "Deleting the Recording of Note {id}, as none of it is readable: {error}"
                    );
                    self.delete_recording(&id, now)?;
                }
            }
        }
        Ok(())
    }

    /// The Notes whose Recording started but never finished, as when the app died while
    /// recording.
    fn unfinished_recordings(&mut self) -> anyhow::Result<Vec<NoteId>> {
        let ids = notes::table
            .filter(notes::deleted_at.is_null())
            .filter(notes::recording_started_at.is_not_null())
            .filter(notes::recording_duration_ms.is_null())
            .select(notes::id)
            .load::<String>(self.connection()?)?;
        Ok(ids.into_iter().map(NoteId).collect())
    }

    /// Deletes the Note `id`'s Recording folder, if there is one.
    fn remove_audio(&self, id: &NoteId) -> anyhow::Result<()> {
        match std::fs::remove_dir_all(self.recording_dir(id)) {
            Err(error) if error.kind() != ErrorKind::NotFound => Err(anyhow!(
                "could not delete the Recording of Note {id}: {error}"
            )),
            _ => Ok(()),
        }
    }
}

impl Drop for Notes {
    fn drop(&mut self) {
        if self.temporary {
            let _ = std::fs::remove_dir_all(&self.recordings);
        }
    }
}

/// Where the Agenda lists a Quick note, as stored: its Recording's start, else its creation.
/// The same expression as the `notes_quick` index, so SQLite uses it.
fn listed_at() -> SqlLiteral<Text> {
    sql::<Text>("COALESCE(recording_started_at, created_at)")
}

/// The Note `id`, unless it is deleted: writes never bring a deleted Note back.
fn live(
    id: &NoteId,
) -> diesel::dsl::Filter<
    diesel::dsl::Find<notes::table, &str>,
    diesel::dsl::IsNull<notes::deleted_at>,
> {
    notes::table
        .find(id.0.as_str())
        .filter(notes::deleted_at.is_null())
}

/// Errors when a write to `id` changed no row: the Note is deleted or unknown.
fn written(updated: usize, id: &NoteId) -> anyhow::Result<()> {
    if updated == 0 {
        bail!("no Note {id}");
    }
    Ok(())
}

/// The start a Note snapshots: an all-day Event's first day, else its start as an instant.
fn start_of(when: &When) -> Occurrence {
    match when {
        When::AllDay { first, .. } => Occurrence::Day(*first),
        When::Timed { start, .. } => Occurrence::At(start.with_timezone(&Utc)),
    }
}

/// `2026-10-13` for a day, `2026-10-13T07:00:00Z` for an instant, to the second.
fn encode_occurrence(occurrence: Occurrence) -> String {
    match occurrence {
        Occurrence::Day(day) => day.format("%Y-%m-%d").to_string(),
        Occurrence::At(at) => at.to_rfc3339_opts(SecondsFormat::Secs, true),
    }
}

fn decode_occurrence(occurrence: &str) -> anyhow::Result<Occurrence> {
    match NaiveDate::parse_from_str(occurrence, "%Y-%m-%d") {
        Ok(day) => Ok(Occurrence::Day(day)),
        Err(_) => Ok(Occurrence::At(decode_time(occurrence)?)),
    }
}

/// RFC 3339 in UTC, always to the millisecond, so stored times sort as text.
fn encode_time(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn decode_time(time: &str) -> anyhow::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(time)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|error| anyhow!("bad stored time {time:?}: {error}"))
}

fn decode_note(row: NoteRow) -> anyhow::Result<Note> {
    let event = match (row.event_title, row.event_start) {
        (Some(title), Some(start)) => Some(EventLink {
            series_uid: row.event_series_uid,
            occurrence: row
                .event_occurrence
                .as_deref()
                .map(decode_occurrence)
                .transpose()?,
            title,
            start: decode_occurrence(&start)?,
        }),
        _ => None,
    };
    Ok(Note {
        id: NoteId(row.id),
        title: row.title,
        memo: row.memo,
        event,
        created_at: decode_time(&row.created_at)?,
        updated_at: decode_time(&row.updated_at)?,
        recording: row
            .recording_started_at
            .as_deref()
            .map(|started_at| {
                anyhow::Ok(RecordingInfo {
                    started_at: decode_time(started_at)?,
                    duration: row
                        .recording_duration_ms
                        .map(|ms| Duration::from_millis(u64::try_from(ms).unwrap_or(0))),
                })
            })
            .transpose()?,
    })
}

#[cfg(test)]
mod tests;
