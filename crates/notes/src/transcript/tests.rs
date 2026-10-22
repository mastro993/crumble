use crate::fixtures::{at, line, ok, recorded};
use crate::{Note, Notes};

#[test]
fn a_transcript_is_kept_in_order_and_replaced_whole() {
    use crate::Track::{Mic, System};
    let mut notes = Notes::in_memory();
    let note = recorded(&mut notes);
    let first = [
        line(Mic, 0, "Hi."),
        line(System, 0, "Hello."),
        line(Mic, 4, "Bye."),
    ];
    let info = ok(notes.set_transcript(&note.id, "apple", "Model", "en_US", &first, at(8)));
    assert_eq!(info.transcribed_at, at(8));
    assert_eq!(ok(notes.transcript(&note.id)), first);
    let found = ok(notes.quick_notes(at(0)..at(23))).remove(0);
    assert_eq!(found.transcript, Some(info));
    assert_eq!(found.updated_at, at(8));

    let again = [line(System, 2, "Only this.")];
    ok(notes.set_transcript(&note.id, "openai", "whisper-1", "it_IT", &again, at(9)));
    assert_eq!(ok(notes.transcript(&note.id)), again);
    let found = ok(notes.quick_notes(at(0)..at(23))).remove(0);
    assert_eq!(
        found
            .transcript
            .map(|info| (info.provider, info.model, info.locale)),
        Some(("openai".into(), "whisper-1".into(), "it_IT".into()))
    );

    // A Local Model's Transcript (ADR 0010), which the provider's CHECK once refused.
    ok(notes.set_transcript(&note.id, "local", "Whisper Medium", "it", &again, at(10)));
    let found = ok(notes.quick_notes(at(0)..at(23))).remove(0);
    assert_eq!(
        found.transcript.map(|info| (info.provider, info.model)),
        Some(("local".into(), "Whisper Medium".into()))
    );
}

#[test]
fn a_transcript_needs_a_finished_recording() {
    let mut notes = Notes::in_memory();
    let plain = ok(notes.create(None, at(5)));
    let running = ok(notes.create(None, at(5)));
    ok(notes.set_recording_started(&running.id, at(6)));
    for note in [&plain, &running] {
        assert!(
            notes
                .set_transcript(&note.id, "apple", "Model", "en_US", &[], at(8))
                .is_err()
        );
        assert_eq!(ok(notes.transcript(&note.id)), []);
    }
}

#[test]
fn only_a_finished_recording_without_a_transcript_needs_one() {
    let mut notes = Notes::in_memory();
    ok(notes.create(None, at(5)));
    let running = ok(notes.create(None, at(5)));
    ok(notes.set_recording_started(&running.id, at(6)));
    let waiting = recorded(&mut notes);
    let done = recorded(&mut notes);
    ok(notes.set_transcript(&done.id, "apple", "Model", "en_US", &[], at(8)));
    let deleted = recorded(&mut notes);
    ok(notes.delete(&deleted.id, at(8)));
    assert_eq!(ok(notes.untranscribed()), [waiting.id]);
}

#[test]
fn deleting_the_recording_deletes_its_transcript() {
    let mut notes = Notes::in_memory();
    let note = recorded(&mut notes);
    ok(notes.set_transcript(
        &note.id,
        "apple",
        "Model",
        "en_US",
        &[line(crate::Track::Mic, 0, "Hi.")],
        at(8),
    ));
    ok(notes.delete_recording(&note.id, at(9)));
    assert_eq!(ok(notes.transcript(&note.id)), []);
    assert_eq!(ok(notes.quick_notes(at(0)..at(23)))[0].transcript, None);
}

/// A Quick note with a finished Recording and a one-line Transcript.
fn transcribed(notes: &mut Notes) -> Note {
    let note = recorded(notes);
    ok(notes.set_transcript(
        &note.id,
        "apple",
        "Model",
        "en_US",
        &[line(crate::Track::Mic, 0, "Hi.")],
        at(8),
    ));
    note
}

fn summary(notes: &mut Notes) -> Option<crate::SummaryInfo> {
    ok(notes.quick_notes(at(0)..at(23))).remove(0).summary
}

#[test]
fn a_summary_is_kept_and_replaced() {
    let mut notes = Notes::in_memory();
    let note = transcribed(&mut notes);
    let info = ok(notes.set_summary(&note.id, "apple", "Foundation Models", "# Hi", at(9)));
    assert_eq!(info.summarized_at, at(9));
    assert_eq!(summary(&mut notes), Some(info));
    ok(notes.set_summary(&note.id, "claude", "m1", "# Hello", at(10)));
    let found = ok(notes.quick_notes(at(0)..at(23))).remove(0);
    assert_eq!(
        found
            .summary
            .map(|info| (info.text, info.provider, info.model)),
        Some(("# Hello".into(), "claude".into(), "m1".into()))
    );
    assert_eq!(found.updated_at, at(10));
}

#[test]
fn a_summary_needs_a_transcript() {
    let mut notes = Notes::in_memory();
    let note = recorded(&mut notes);
    assert!(
        notes
            .set_summary(&note.id, "apple", "m", "# Hi", at(9))
            .is_err()
    );
    assert_eq!(summary(&mut notes), None);
}

#[test]
fn a_new_transcript_or_none_clears_the_summary() {
    let mut notes = Notes::in_memory();
    let note = transcribed(&mut notes);
    ok(notes.set_summary(&note.id, "apple", "m", "# Hi", at(9)));
    ok(notes.set_transcript(&note.id, "apple", "Model", "en_US", &[], at(10)));
    assert_eq!(summary(&mut notes), None);

    ok(notes.set_transcript(
        &note.id,
        "apple",
        "Model",
        "en_US",
        &[line(crate::Track::Mic, 0, "Hi.")],
        at(10),
    ));
    ok(notes.set_summary(&note.id, "apple", "m", "# Hi", at(11)));
    ok(notes.delete_recording(&note.id, at(12)));
    assert_eq!(summary(&mut notes), None);
}

#[test]
fn only_a_spoken_transcript_without_a_summary_needs_one() {
    let mut notes = Notes::in_memory();
    recorded(&mut notes);
    let silent = recorded(&mut notes);
    ok(notes.set_transcript(&silent.id, "apple", "Model", "en_US", &[], at(8)));
    let empty = recorded(&mut notes);
    let nothing = [line(crate::Track::Mic, 0, "")];
    ok(notes.set_transcript(&empty.id, "apple", "Model", "en_US", &nothing, at(8)));
    let waiting = transcribed(&mut notes);
    let done = transcribed(&mut notes);
    ok(notes.set_summary(&done.id, "apple", "m", "# Hi", at(9)));
    let deleted = transcribed(&mut notes);
    ok(notes.delete(&deleted.id, at(9)));
    assert_eq!(ok(notes.unsummarized()), [waiting.id]);
}
