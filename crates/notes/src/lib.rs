//! Notes kept in Crumble's database: each attached to one occurrence of an Event, or standing
//! alone as a Quick note. A Note finds its Event again by the key and snapshot of ADR 0004.
//! A Note's Recording is kept as files in its own folder, which this crate names and deletes;
//! `crumble-audio` writes and reads the files in it (ADR 0005). Its Transcript is kept here as
//! segments, replaced wholesale each time it is made (ADR 0006), and its Summary beside it,
//! cleared whenever the Transcript changes (ADR 0007).

use std::fmt;
use std::io::ErrorKind;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail};
use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use crumble_db::schema::{notes, transcript_segments};
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
    /// How the Note's Transcript was made, if it has one; [`Notes::transcript`] reads its text.
    pub transcript: Option<TranscriptInfo>,
    /// The Summary of the Note's Transcript, if it has one.
    pub summary: Option<SummaryInfo>,
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

/// How a Note's Transcript was made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptInfo {
    /// The Model that made it, such as "Apple SpeechAnalyzer".
    pub model: String,
    /// The locale it was made in, such as `en_US`.
    pub locale: String,
    /// When it was made, to the millisecond.
    pub transcribed_at: DateTime<Utc>,
}

/// A Note's Summary and how it was made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SummaryInfo {
    /// The Summary, as Markdown.
    pub text: String,
    /// The id of the Provider whose Model made it, such as `claude`.
    pub provider: String,
    /// The Model that made it, such as `claude-sonnet-5-5`.
    pub model: String,
    /// When it was made, to the millisecond.
    pub summarized_at: DateTime<Utc>,
}

/// One stretch of speech in a Transcript.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptSegment {
    /// The track it was heard on, which says who spoke.
    pub track: Track,
    /// Where it starts in the Recording, to the millisecond.
    pub start: Duration,
    /// Where it ends in the Recording, to the millisecond.
    pub end: Duration,
    /// What was said.
    pub text: String,
}

/// One of a Recording's two tracks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Track {
    /// The microphone: the user's side.
    Mic,
    /// The Mac's sound: the others on the call.
    System,
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
    transcript_model: Option<String>,
    transcript_locale: Option<String>,
    transcribed_at: Option<String>,
    summary: Option<String>,
    summary_provider: Option<String>,
    summary_model: Option<String>,
    summarized_at: Option<String>,
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = transcript_segments, check_for_backend(diesel::sqlite::Sqlite))]
struct SegmentRow {
    id: String,
    note_id: String,
    track: String,
    start_ms: i32,
    end_ms: i32,
    text: String,
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
            transcript_model: None,
            transcript_locale: None,
            transcribed_at: None,
            summary: None,
            summary_provider: None,
            summary_model: None,
            summarized_at: None,
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

    /// Soft delete: sets `deleted_at`, and every read skips the Note. Its row stays only as a
    /// tombstone for sync: what the user wrote or said goes for good, the Recording's audio
    /// first, then its title, Memo, Event link, Recording, Transcript and Summary in one
    /// transaction.
    pub fn delete(&mut self, id: &NoteId, now: DateTime<Utc>) -> anyhow::Result<()> {
        // The audio goes first: once the Note is deleted, nothing can reach its folder again.
        self.remove_audio(id)?;
        let now = encode_time(now);
        self.connection()?.transaction(|connection| {
            let updated = diesel::update(live(id))
                .set((
                    notes::deleted_at.eq(&now),
                    notes::updated_at.eq(&now),
                    notes::title.eq(None::<String>),
                    notes::memo.eq(""),
                    notes::event_series_uid.eq(None::<String>),
                    notes::event_occurrence.eq(None::<String>),
                    notes::event_title.eq(None::<String>),
                    notes::event_start.eq(None::<String>),
                    notes::recording_started_at.eq(None::<String>),
                    notes::recording_duration_ms.eq(None::<i32>),
                    notes::transcript_model.eq(None::<String>),
                    notes::transcript_locale.eq(None::<String>),
                    notes::transcribed_at.eq(None::<String>),
                    no_summary(),
                ))
                .execute(connection)?;
            written(updated, id)?;
            diesel::delete(segments_of(id)).execute(connection)?;
            anyhow::Ok(())
        })
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

    /// Keeps the Note `id`'s stopped or recovered Recording at `duration` and returns it as
    /// stored. One zero long holds no audio, so it is deleted instead and `None` comes back.
    pub fn finish_recording(
        &mut self,
        id: &NoteId,
        duration: Duration,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Option<RecordingInfo>> {
        if duration.is_zero() {
            self.delete_recording(id, now)?;
            return Ok(None);
        }
        self.set_recording_finished(id, duration, now)?;
        let started_at = notes::table
            .find(id.0.as_str())
            .select(notes::recording_started_at)
            .first::<Option<String>>(self.connection()?)?
            .ok_or_else(|| anyhow!("Note {id} has no Recording"))?;
        Ok(Some(RecordingInfo {
            started_at: decode_time(&started_at)?,
            duration: Some(duration),
        }))
    }

    /// Stores how long the Note `id`'s Recording is.
    fn set_recording_finished(
        &mut self,
        id: &NoteId,
        duration: Duration,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let duration_ms = millis(duration);
        let updated = diesel::update(live(id).filter(notes::recording_started_at.is_not_null()))
            .set((
                notes::recording_duration_ms.eq(duration_ms),
                notes::updated_at.eq(encode_time(now)),
            ))
            .execute(self.connection()?)?;
        written(updated, id)
    }

    /// Removes the Note `id`'s Recording, and the Transcript and Summary made from it, which
    /// nothing can make again: deletes the audio's folder, then forgets them.
    pub fn delete_recording(&mut self, id: &NoteId, now: DateTime<Utc>) -> anyhow::Result<()> {
        // The audio goes first, so a failure leaves the Recording listed for another try.
        self.remove_audio(id)?;
        self.connection()?.transaction(|connection| {
            let updated = diesel::update(live(id))
                .set((
                    notes::recording_started_at.eq(None::<String>),
                    notes::recording_duration_ms.eq(None::<i32>),
                    notes::transcript_model.eq(None::<String>),
                    notes::transcript_locale.eq(None::<String>),
                    notes::transcribed_at.eq(None::<String>),
                    no_summary(),
                    notes::updated_at.eq(encode_time(now)),
                ))
                .execute(connection)?;
            written(updated, id)?;
            diesel::delete(segments_of(id)).execute(connection)?;
            anyhow::Ok(())
        })
    }

    /// Stores `segments`, in order, as the Transcript of the Note `id`, made by `model` in
    /// `locale`, and returns how it was made as stored. It replaces any Transcript the Note had,
    /// and clears its Summary, in one transaction. Errors unless the Note has a finished
    /// Recording.
    pub fn set_transcript(
        &mut self,
        id: &NoteId,
        model: &str,
        locale: &str,
        segments: &[TranscriptSegment],
        now: DateTime<Utc>,
    ) -> anyhow::Result<TranscriptInfo> {
        let transcribed_at = encode_time(now);
        let rows: Vec<SegmentRow> = segments
            .iter()
            .map(|segment| SegmentRow {
                id: Uuid::now_v7().to_string(),
                note_id: id.0.clone(),
                track: encode_track(segment.track).to_owned(),
                start_ms: millis(segment.start),
                end_ms: millis(segment.end),
                text: segment.text.clone(),
            })
            .collect();
        self.connection()?.transaction(|connection| {
            let updated =
                diesel::update(live(id).filter(notes::recording_duration_ms.is_not_null()))
                    .set((
                        notes::transcript_model.eq(model),
                        notes::transcript_locale.eq(locale),
                        notes::transcribed_at.eq(&transcribed_at),
                        no_summary(),
                        notes::updated_at.eq(&transcribed_at),
                    ))
                    .execute(connection)?;
            if updated == 0 {
                bail!("no Note {id} with a finished Recording");
            }
            diesel::delete(segments_of(id)).execute(connection)?;
            diesel::insert_into(transcript_segments::table)
                .values(&rows)
                .execute(connection)?;
            anyhow::Ok(())
        })?;
        Ok(TranscriptInfo {
            model: model.to_owned(),
            locale: locale.to_owned(),
            transcribed_at: decode_time(&transcribed_at)?,
        })
    }

    /// Stores `text` as the Summary of the Note `id`, made by `model` of the Provider `provider`,
    /// and returns it as stored. It replaces any Summary the Note had. Errors unless the Note
    /// has a Transcript.
    pub fn set_summary(
        &mut self,
        id: &NoteId,
        provider: &str,
        model: &str,
        text: &str,
        now: DateTime<Utc>,
    ) -> anyhow::Result<SummaryInfo> {
        let summarized_at = encode_time(now);
        let updated = diesel::update(live(id).filter(notes::transcript_model.is_not_null()))
            .set((
                notes::summary.eq(text),
                notes::summary_provider.eq(provider),
                notes::summary_model.eq(model),
                notes::summarized_at.eq(&summarized_at),
                notes::updated_at.eq(&summarized_at),
            ))
            .execute(self.connection()?)?;
        if updated == 0 {
            bail!("no Note {id} with a Transcript");
        }
        Ok(SummaryInfo {
            text: text.to_owned(),
            provider: provider.to_owned(),
            model: model.to_owned(),
            summarized_at: decode_time(&summarized_at)?,
        })
    }

    /// The Notes whose Transcript holds speech but has no Summary, oldest first, as when the app
    /// quit while summarizing. Run at launch, to summarize them.
    pub fn unsummarized(&mut self) -> anyhow::Result<Vec<NoteId>> {
        let spoken = diesel::dsl::exists(
            transcript_segments::table.filter(transcript_segments::note_id.eq(notes::id)),
        );
        let ids = notes::table
            .filter(notes::deleted_at.is_null())
            .filter(notes::transcript_model.is_not_null())
            .filter(notes::summary.is_null())
            .filter(spoken)
            .order(notes::id)
            .select(notes::id)
            .load::<String>(self.connection()?)?;
        Ok(ids.into_iter().map(NoteId).collect())
    }

    /// The Transcript of the Note `id` in time order; empty when it has none, or is deleted.
    pub fn transcript(&mut self, id: &NoteId) -> anyhow::Result<Vec<TranscriptSegment>> {
        let rows = transcript_segments::table
            .inner_join(notes::table)
            .filter(transcript_segments::note_id.eq(&id.0))
            .filter(notes::deleted_at.is_null())
            .order((transcript_segments::start_ms, transcript_segments::id))
            .select(SegmentRow::as_select())
            .load(self.connection()?)?;
        rows.into_iter()
            .map(|row| {
                Ok(TranscriptSegment {
                    track: decode_track(&row.track)?,
                    start: duration(row.start_ms),
                    end: duration(row.end_ms),
                    text: row.text,
                })
            })
            .collect()
    }

    /// The Notes with a finished Recording but no Transcript, oldest first, as when the app quit
    /// while transcribing. Run at launch, to transcribe them.
    pub fn untranscribed(&mut self) -> anyhow::Result<Vec<NoteId>> {
        let ids = notes::table
            .filter(notes::deleted_at.is_null())
            .filter(notes::recording_duration_ms.is_not_null())
            .filter(notes::transcript_model.is_null())
            .order(notes::id)
            .select(notes::id)
            .load::<String>(self.connection()?)?;
        Ok(ids.into_iter().map(NoteId).collect())
    }

    /// Finishes the Recordings a crash left running, as [`Notes::finish_recording`] does with
    /// the length `length` reads from each folder. One `length` cannot read stays unfinished, as
    /// its files may still hold audio, and is tried again at the next launch. A Recording that
    /// fails does not stop the others; the first error comes back once all were tried. Run at
    /// launch, before anything records; `crumble_audio::recorded_length` reads the files.
    pub fn recover_recordings(
        &mut self,
        length: impl Fn(&Path) -> anyhow::Result<Duration>,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let mut failed = None;
        for id in self.unfinished_recordings()? {
            let recovered = length(&self.recording_dir(&id))
                .inspect_err(|error| {
                    eprintln!("Could not read the Recording of Note {id}, so it is left for later: {error:#}");
                })
                .map_or(Ok(()), |duration| {
                    self.finish_recording(&id, duration, now).map(drop)
                });
            if let Err(error) = recovered {
                eprintln!("Could not recover the Recording of Note {id}: {error:#}");
                failed.get_or_insert(error);
            }
        }
        failed.map_or(Ok(()), Err)
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

/// The columns that clear a Note's Summary.
#[allow(clippy::type_complexity, reason = "Diesel's assignment types")]
fn no_summary() -> (
    diesel::dsl::Eq<notes::summary, Option<String>>,
    diesel::dsl::Eq<notes::summary_provider, Option<String>>,
    diesel::dsl::Eq<notes::summary_model, Option<String>>,
    diesel::dsl::Eq<notes::summarized_at, Option<String>>,
) {
    (
        notes::summary.eq(None),
        notes::summary_provider.eq(None),
        notes::summary_model.eq(None),
        notes::summarized_at.eq(None),
    )
}

/// The Transcript segments of the Note `id`.
fn segments_of(
    id: &NoteId,
) -> diesel::dsl::Filter<
    transcript_segments::table,
    diesel::dsl::Eq<transcript_segments::note_id, &str>,
> {
    transcript_segments::table.filter(transcript_segments::note_id.eq(id.0.as_str()))
}

/// Milliseconds as stored, at most `i32::MAX` (24 days).
fn millis(duration: Duration) -> i32 {
    i32::try_from(duration.as_millis()).unwrap_or(i32::MAX)
}

fn duration(ms: i32) -> Duration {
    Duration::from_millis(u64::try_from(ms).unwrap_or(0))
}

fn encode_track(track: Track) -> &'static str {
    match track {
        Track::Mic => "mic",
        Track::System => "system",
    }
}

fn decode_track(track: &str) -> anyhow::Result<Track> {
    match track {
        "mic" => Ok(Track::Mic),
        "system" => Ok(Track::System),
        _ => Err(anyhow!("bad stored track {track:?}")),
    }
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
                    duration: row.recording_duration_ms.map(duration),
                })
            })
            .transpose()?,
        transcript: match (
            row.transcript_model,
            row.transcript_locale,
            row.transcribed_at,
        ) {
            (Some(model), Some(locale), Some(at)) => Some(TranscriptInfo {
                model,
                locale,
                transcribed_at: decode_time(&at)?,
            }),
            _ => None,
        },
        summary: match (
            row.summary,
            row.summary_provider,
            row.summary_model,
            row.summarized_at,
        ) {
            (Some(text), Some(provider), Some(model), Some(at)) => Some(SummaryInfo {
                text,
                provider,
                model,
                summarized_at: decode_time(&at)?,
            }),
            _ => None,
        },
    })
}

#[cfg(test)]
mod tests;
