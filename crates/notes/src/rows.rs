//! How a Note is stored: its row, the queries every part shares, and the text encodings.

use std::time::Duration;

use anyhow::{anyhow, bail};
use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use crumble_db::schema::{notes, transcript_segments};
use crumble_eventkit::Occurrence;
use diesel::prelude::*;

use crate::{EventLink, Note, NoteId, RecordingInfo, SummaryInfo, TranscriptInfo};

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = notes, check_for_backend(diesel::sqlite::Sqlite))]
pub(crate) struct NoteRow {
    pub(crate) id: String,
    pub(crate) title: Option<String>,
    pub(crate) memo: String,
    pub(crate) event_series_uid: Option<String>,
    pub(crate) event_occurrence: Option<String>,
    pub(crate) event_title: Option<String>,
    pub(crate) event_start: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) recording_started_at: Option<String>,
    pub(crate) recording_duration_ms: Option<i32>,
    pub(crate) transcript_model: Option<String>,
    pub(crate) transcript_locale: Option<String>,
    pub(crate) transcribed_at: Option<String>,
    pub(crate) summary: Option<String>,
    pub(crate) summary_provider: Option<String>,
    pub(crate) summary_model: Option<String>,
    pub(crate) summarized_at: Option<String>,
    pub(crate) transcript_provider: Option<String>,
}

/// The Note `id`, unless it is deleted: writes never bring a deleted Note back.
pub(crate) fn live(
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
pub(crate) fn no_summary() -> (
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
pub(crate) fn segments_of(
    id: &NoteId,
) -> diesel::dsl::Filter<
    transcript_segments::table,
    diesel::dsl::Eq<transcript_segments::note_id, &str>,
> {
    transcript_segments::table.filter(transcript_segments::note_id.eq(id.0.as_str()))
}

/// Milliseconds as stored, at most `i32::MAX` (24 days).
pub(crate) fn millis(duration: Duration) -> i32 {
    i32::try_from(duration.as_millis()).unwrap_or(i32::MAX)
}

pub(crate) fn duration(ms: i32) -> Duration {
    Duration::from_millis(u64::try_from(ms).unwrap_or(0))
}

/// Errors when a write to `id` changed no row: the Note is deleted or unknown.
pub(crate) fn written(updated: usize, id: &NoteId) -> anyhow::Result<()> {
    if updated == 0 {
        bail!("no Note {id}");
    }
    Ok(())
}

/// `2026-10-13` for a day, `2026-10-13T07:00:00Z` for an instant, to the second.
pub(crate) fn encode_occurrence(occurrence: Occurrence) -> String {
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
pub(crate) fn encode_time(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub(crate) fn decode_time(time: &str) -> anyhow::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(time)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|error| anyhow!("bad stored time {time:?}: {error}"))
}

pub(crate) fn decode_note(row: NoteRow) -> anyhow::Result<Note> {
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
            row.transcript_provider,
            row.transcript_model,
            row.transcript_locale,
            row.transcribed_at,
        ) {
            (Some(provider), Some(model), Some(locale), Some(at)) => Some(TranscriptInfo {
                provider,
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
mod tests {
    use chrono::Duration;
    use crumble_eventkit::Occurrence;

    use super::{decode_occurrence, decode_time, encode_occurrence, encode_time};
    use crate::fixtures::{at, day, ok};

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
}
