//! Notes kept in Crumble's database: each attached to one occurrence of an Event, or standing
//! alone as a Quick note. A Note finds its Event again by the key and snapshot of ADR 0004.
//! A Note's Recording is kept as files in its own folder, which this crate names and deletes;
//! `crumble-audio` writes and reads the files in it (ADR 0005). Its Transcript is kept here as
//! segments, replaced wholesale each time it is made (ADR 0006), and its Summary beside it,
//! cleared whenever the Transcript changes (ADR 0007).

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::anyhow;
use chrono::{DateTime, Utc};
use crumble_eventkit::Occurrence;
use diesel::prelude::*;
use uuid::Uuid;

#[cfg(test)]
mod fixtures;
mod noted;
mod queries;
mod recording;
mod rows;
mod transcript;

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
    /// The id of the Provider whose Model made it, such as `apple` or `openai`.
    pub provider: String,
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

/// What shows a Note's folder in Finder, from [`Notes::folder_entry`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FolderEntry {
    /// An audio file of its Recording, to select in its folder.
    File(PathBuf),
    /// The folder, empty, to open.
    Folder(PathBuf),
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
}

impl Drop for Notes {
    fn drop(&mut self) {
        if self.temporary {
            let _ = std::fs::remove_dir_all(&self.recordings);
        }
    }
}
