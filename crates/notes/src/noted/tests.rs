use crumble_eventkit::{Event, Occurrence};

use crate::Notes;
use crate::fixtures::{at, event, ok, timed};

fn with_id(id: &str, event: Event) -> Event {
    Event {
        id: id.into(),
        ..event
    }
}

#[test]
fn events_with_a_note_are_found_as_for_event_finds_them() {
    let mut notes = Notes::in_memory();
    let standup = |hour| {
        event(
            Some("uid"),
            Some(Occurrence::At(at(hour))),
            "Standup",
            timed(at(hour)),
        )
    };
    let keyed = standup(7);
    ok(notes.create(Some(&keyed), at(6)));
    let unsynced = event(None, None, "Review", timed(at(9)));
    ok(notes.create(Some(&unsynced), at(6)));
    let deleted = event(Some("gone"), None, "Retro", timed(at(11)));
    let note = ok(notes.create(Some(&deleted), at(6)));
    ok(notes.delete(&note.id, at(6)));
    ok(notes.create(None, at(6)));

    let events = [
        // Its key, though it moved and was renamed.
        with_id(
            "moved",
            event(
                Some("uid"),
                Some(Occurrence::At(at(7))),
                "Standup, later",
                timed(at(8)),
            ),
        ),
        // Another occurrence of the series.
        with_id("other", standup(8)),
        // The snapshot of a Note made before the Event synced.
        with_id("synced", event(Some("new"), None, "Review", timed(at(9)))),
        // The same title and start in another series than the keyed Note's.
        with_id(
            "lookalike",
            event(Some("else"), None, "Standup", timed(at(7))),
        ),
        with_id("deleted", deleted),
        with_id("none", event(None, None, "Lunch", timed(at(12)))),
    ];
    let noted = ok(notes.noted(&events));
    for event in &events {
        assert_eq!(
            noted.contains(&event.id),
            ok(notes.for_event(event)).is_some(),
            "{}",
            event.id
        );
    }
    let mut noted: Vec<_> = noted.into_iter().collect();
    noted.sort();
    assert_eq!(noted, ["moved", "synced"]);
}
