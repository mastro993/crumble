//! Making, finding, listing, editing and deleting Notes, and how a Note finds its Event.

use std::ops::Range;

use chrono::{DateTime, Utc};
use crumble_db::schema::notes;
use crumble_eventkit::{Event, Occurrence, When};
use diesel::dsl::sql;
use diesel::expression::SqlLiteral;
use diesel::prelude::*;
use diesel::sql_types::Text;
use uuid::Uuid;

use crate::rows::{
    NoteRow, decode_note, encode_occurrence, encode_time, live, no_summary, segments_of, written,
};
use crate::{Note, NoteId, Notes};

impl Notes {
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
            transcript_provider: None,
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
                    notes::transcript_provider.eq(None::<String>),
                    notes::transcribed_at.eq(None::<String>),
                    no_summary(),
                ))
                .execute(connection)?;
            written(updated, id)?;
            diesel::delete(segments_of(id)).execute(connection)?;
            anyhow::Ok(())
        })
    }
}

/// Where the Agenda lists a Quick note, as stored: its Recording's start, else its creation.
/// The same expression as the `notes_quick` index, so SQLite uses it.
fn listed_at() -> SqlLiteral<Text> {
    sql::<Text>("COALESCE(recording_started_at, created_at)")
}

/// The start a Note snapshots: an all-day Event's first day, else its start as an instant.
fn start_of(when: &When) -> Occurrence {
    match when {
        When::AllDay { first, .. } => Occurrence::Day(*first),
        When::Timed { start, .. } => Occurrence::At(start.with_timezone(&Utc)),
    }
}

#[cfg(test)]
mod tests;
