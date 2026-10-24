//! Which of many Events have a Note, in one read, for the Agenda page.

use std::collections::{HashMap, HashSet};

use crumble_db::schema::notes;
use crumble_eventkit::Event;
use diesel::prelude::*;

use crate::Notes;
use crate::queries::start_of;
use crate::rows::encode_occurrence;

/// An Event link as stored: series UID, occurrence, title and start.
type Link = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

impl Notes {
    /// The [`Event::id`]s of those of `events` whose occurrence has a Note, matched as
    /// [`Notes::for_event`] matches one: by key, else by snapshot.
    pub fn noted<'a>(
        &mut self,
        events: impl IntoIterator<Item = &'a Event>,
    ) -> anyhow::Result<HashSet<String>> {
        let links: Vec<Link> = notes::table
            .filter(notes::deleted_at.is_null())
            .filter(notes::event_start.is_not_null())
            .select((
                notes::event_series_uid,
                notes::event_occurrence,
                notes::event_title,
                notes::event_start,
            ))
            .load(self.connection()?)?;
        // Each key's series UID with its occurrences, and each snapshot's title and start with
        // the series UIDs of the Notes taken from it.
        let mut keys: HashMap<String, HashSet<Option<String>>> = HashMap::new();
        let mut snapshots: HashMap<(String, String), Vec<Option<String>>> = HashMap::new();
        for (series_uid, occurrence, title, start) in links {
            if let Some(series_uid) = &series_uid {
                keys.entry(series_uid.clone())
                    .or_default()
                    .insert(occurrence);
            }
            if let (Some(title), Some(start)) = (title, start) {
                snapshots
                    .entry((title, start))
                    .or_default()
                    .push(series_uid);
            }
        }
        let has_note = |event: &Event| {
            let key = event.series_uid.as_ref().is_some_and(|series_uid| {
                keys.get(series_uid).is_some_and(|occurrences| {
                    occurrences.contains(&event.occurrence.map(encode_occurrence))
                })
            });
            // A Note keyed to another series never matches by snapshot.
            let snapshot = || {
                let start = encode_occurrence(start_of(&event.when));
                snapshots
                    .get(&(event.title.clone(), start))
                    .is_some_and(|series_uids| {
                        series_uids.iter().any(|series_uid| {
                            series_uid.is_none() || *series_uid == event.series_uid
                        })
                    })
            };
            key || snapshot()
        };
        Ok(events
            .into_iter()
            .filter(|event| has_note(event))
            .map(|event| event.id.clone())
            .collect())
    }
}

#[cfg(test)]
mod tests;
