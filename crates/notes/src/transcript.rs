//! A Note's Transcript, kept as segments, and the Summary made from it.

use anyhow::{anyhow, bail};
use chrono::{DateTime, Utc};
use crumble_db::schema::{notes, transcript_segments};
use diesel::prelude::*;
use uuid::Uuid;

use crate::rows::{decode_time, duration, encode_time, live, millis, no_summary, segments_of};
use crate::{NoteId, Notes, SummaryInfo, Track, TranscriptInfo, TranscriptSegment};

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
    /// Stores `segments`, in order, as the Transcript of the Note `id`, made by `model` of the
    /// Provider `provider` in `locale`, and returns how it was made as stored. It replaces any Transcript the Note had,
    /// and clears its Summary, in one transaction. Errors unless the Note has a finished
    /// Recording.
    pub fn set_transcript(
        &mut self,
        id: &NoteId,
        provider: &str,
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
                        notes::transcript_provider.eq(provider),
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
            provider: provider.to_owned(),
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
        // Speech recognition trims each segment, so an empty one said nothing.
        let spoken = diesel::dsl::exists(
            transcript_segments::table
                .filter(transcript_segments::note_id.eq(notes::id))
                .filter(transcript_segments::text.ne("")),
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

#[cfg(test)]
mod tests;
