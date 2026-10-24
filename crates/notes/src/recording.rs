//! A Note's Recording: its folder of audio, its start and length, and recovery after a crash.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail};
use chrono::{DateTime, Utc};
use crumble_db::schema::notes;
use diesel::prelude::*;

use crate::rows::{decode_time, encode_time, live, millis, no_summary, segments_of, written};
use crate::{FolderEntry, NoteId, Notes, RecordingInfo};

impl Notes {
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
    pub(crate) fn set_recording_finished(
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
                    notes::transcript_provider.eq(None::<String>),
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

    /// What shows the Note `id`'s folder in Finder: the first of its audio files by name, for
    /// Finder to select, or else the folder itself, made if it is missing.
    pub fn folder_entry(&self, id: &NoteId) -> anyhow::Result<FolderEntry> {
        let dir = self.recording_dir(id);
        let mut files = match std::fs::read_dir(&dir) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.is_file())
                .collect(),
            Err(error) if error.kind() == ErrorKind::NotFound => Vec::new(),
            Err(error) => bail!("could not read the folder of Note {id}: {error}"),
        };
        files.sort();
        if let Some(file) = files.into_iter().next() {
            return Ok(FolderEntry::File(file));
        }
        std::fs::create_dir_all(&dir)
            .map_err(|error| anyhow!("could not make the folder of Note {id}: {error}"))?;
        Ok(FolderEntry::Folder(dir))
    }

    /// Deletes the Note `id`'s Recording folder, if there is one.
    pub(crate) fn remove_audio(&self, id: &NoteId) -> anyhow::Result<()> {
        match std::fs::remove_dir_all(self.recording_dir(id)) {
            Err(error) if error.kind() != ErrorKind::NotFound => Err(anyhow!(
                "could not delete the Recording of Note {id}: {error}"
            )),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests;
