use chrono::Duration;

use crate::fixtures::{at, ok, record_audio};
use crate::{Note, Notes};

#[test]
fn a_recording_starts_once_then_finishes() {
    let mut notes = Notes::in_memory();
    let note = ok(notes.create(None, at(6)));
    let started =
        ok(notes.set_recording_started(&note.id, at(7) + Duration::nanoseconds(1_234_567)));
    assert_eq!(started.started_at, at(7) + Duration::milliseconds(1));
    assert_eq!(started.duration, None);
    // A Note has at most one Recording.
    assert!(notes.set_recording_started(&note.id, at(8)).is_err());
    assert_eq!(
        ok(notes.unfinished_recordings()),
        std::slice::from_ref(&note.id)
    );

    let finished =
        ok(notes.finish_recording(&note.id, std::time::Duration::from_millis(90_500), at(8)));
    let [found] = <[Note; 1]>::try_from(ok(notes.quick_notes(at(0)..at(23))))
        .unwrap_or_else(|found| panic!("{found:?}"));
    assert_eq!(
        found.recording,
        Some(crate::RecordingInfo {
            started_at: started.started_at,
            duration: Some(std::time::Duration::from_millis(90_500)),
        })
    );
    assert_eq!(finished, found.recording);
    assert_eq!(found.updated_at, at(8));
    assert_eq!(ok(notes.unfinished_recordings()), []);
}

#[test]
fn finishing_needs_a_started_recording() {
    let mut notes = Notes::in_memory();
    let note = ok(notes.create(None, at(6)));
    assert!(
        notes
            .finish_recording(&note.id, std::time::Duration::from_secs(1), at(7))
            .is_err()
    );
}

#[test]
fn a_recording_finished_with_no_audio_is_deleted() {
    let mut notes = Notes::in_memory();
    let note = ok(notes.create(None, at(6)));
    ok(notes.set_recording_started(&note.id, at(7)));
    let dir = record_audio(&notes, &note);
    assert_eq!(
        ok(notes.finish_recording(&note.id, std::time::Duration::ZERO, at(8))),
        None
    );
    assert!(!dir.exists());
    assert_eq!(ok(notes.quick_notes(at(0)..at(23)))[0].recording, None);
}

#[test]
fn a_deleted_note_has_no_unfinished_recording() {
    let mut notes = Notes::in_memory();
    let note = ok(notes.create(None, at(6)));
    ok(notes.set_recording_started(&note.id, at(7)));
    ok(notes.delete(&note.id, at(8)));
    assert_eq!(ok(notes.unfinished_recordings()), []);
}

#[test]
fn deleting_the_recording_forgets_it_and_its_audio() {
    let mut notes = Notes::in_memory();
    let note = ok(notes.create(None, at(6)));
    ok(notes.set_recording_started(&note.id, at(7)));
    let dir = record_audio(&notes, &note);
    ok(notes.delete_recording(&note.id, at(8)));
    assert!(!dir.exists());
    assert_eq!(ok(notes.quick_notes(at(0)..at(23)))[0].recording, None);
    // The Note may record again, and deleting with no audio left is fine.
    ok(notes.set_recording_started(&note.id, at(9)));
    ok(notes.delete_recording(&note.id, at(10)));
}

#[test]
fn deleting_a_note_deletes_its_audio() {
    let mut notes = Notes::in_memory();
    let note = ok(notes.create(None, at(6)));
    ok(notes.set_recording_started(&note.id, at(7)));
    let dir = record_audio(&notes, &note);
    ok(notes.delete(&note.id, at(8)));
    assert!(!dir.exists());
}

#[test]
fn audio_that_cannot_be_deleted_keeps_the_note_for_another_try() {
    let mut notes = Notes::in_memory();
    let note = ok(notes.create(None, at(6)));
    ok(notes.set_recording_started(&note.id, at(7)));
    // A file where the folder should be, so deleting the folder fails.
    let dir = notes.recording_dir(&note.id);
    ok(std::fs::create_dir_all(&notes.recordings).map_err(anyhow::Error::from));
    ok(std::fs::write(&dir, b"audio").map_err(anyhow::Error::from));
    assert!(notes.delete_recording(&note.id, at(8)).is_err());
    assert!(notes.delete(&note.id, at(8)).is_err());
    let listed = ok(notes.quick_notes(at(0)..at(23)));
    assert!(listed[0].recording.is_some());
    ok(std::fs::remove_file(&dir).map_err(anyhow::Error::from));
    ok(notes.delete(&note.id, at(9)));
}

#[test]
fn an_in_memory_notes_deletes_its_recordings_when_dropped() {
    let notes = Notes::in_memory();
    let note_dir = notes.recordings.join("note");
    ok(std::fs::create_dir_all(&note_dir).map_err(anyhow::Error::from));
    let recordings = notes.recordings.clone();
    drop(notes);
    assert!(!recordings.exists());
}

#[test]
fn recovery_goes_on_past_a_recording_it_cannot_finish() {
    let mut notes = Notes::in_memory();
    let stuck = ok(notes.create(None, at(5)));
    let kept = ok(notes.create(None, at(5)));
    for note in [&stuck, &kept] {
        ok(notes.set_recording_started(&note.id, at(6)));
    }
    // A file where the folder should be, so deleting the empty Recording fails.
    let stuck_dir = notes.recording_dir(&stuck.id);
    ok(std::fs::create_dir_all(&notes.recordings).map_err(anyhow::Error::from));
    ok(std::fs::write(&stuck_dir, b"").map_err(anyhow::Error::from));
    let recovered = notes.recover_recordings(
        |dir| {
            Ok(if dir == stuck_dir {
                std::time::Duration::ZERO
            } else {
                std::time::Duration::from_secs(42)
            })
        },
        at(8),
    );
    assert!(recovered.is_err());
    assert_eq!(ok(notes.unfinished_recordings()), [stuck.id]);
}

#[test]
fn a_crash_left_recording_keeps_what_its_files_hold() {
    let mut notes = Notes::in_memory();
    let kept = ok(notes.create(None, at(5)));
    let empty = ok(notes.create(None, at(5)));
    let unreadable = ok(notes.create(None, at(5)));
    let finished = ok(notes.create(None, at(5)));
    for note in [&kept, &empty, &unreadable, &finished] {
        ok(notes.set_recording_started(&note.id, at(6)));
    }
    ok(notes.set_recording_finished(&finished.id, std::time::Duration::from_secs(9), at(7)));
    let kept_dir = notes.recording_dir(&kept.id);
    let empty_dir = notes.recording_dir(&empty.id);
    let unreadable_dir = notes.recording_dir(&unreadable.id);
    ok(notes.recover_recordings(
        |dir| {
            if dir == kept_dir {
                Ok(std::time::Duration::from_secs(42))
            } else if dir == empty_dir {
                Ok(std::time::Duration::ZERO)
            } else {
                assert_eq!(dir, unreadable_dir);
                Err(anyhow::anyhow!("cannot read"))
            }
        },
        at(8),
    ));
    let recordings: Vec<_> = ok(notes.quick_notes(at(0)..at(23)))
        .into_iter()
        .map(|note| (note.id, note.recording.map(|recording| recording.duration)))
        .collect();
    assert!(recordings.contains(&(kept.id, Some(Some(std::time::Duration::from_secs(42))))));
    // Nothing in it: deleted.
    assert!(recordings.contains(&(empty.id, None)));
    // Its files may still hold audio: left unfinished for the next launch.
    assert!(recordings.contains(&(unreadable.id.clone(), Some(None))));
    assert!(recordings.contains(&(finished.id, Some(Some(std::time::Duration::from_secs(9))))));
    assert_eq!(ok(notes.unfinished_recordings()), [unreadable.id]);
}
