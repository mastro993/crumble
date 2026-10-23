//! ElevenLabs' Speech to Text: `POST {base}/v1/speech-to-text` with the piece and form fields,
//! answered with timed words, which become segments here.

use std::time::Duration;

use anyhow::Context;
use serde_json::Value;

use super::words::segments_or_text;
use super::{Request, Timed, language, word};

/// The most audio uploaded at once. ElevenLabs takes files up to gigabytes; this keeps each
/// request as long as OpenAI's, about 40 minutes of a 65 kbit/s track.
pub(super) const MAX_PIECE: usize = 20_000_000;

/// The form fields that go before the piece: word times, and no tags for laughter and other
/// sounds, which are no speech.
fn fields<'a>(model: &'a str, language: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut fields = vec![("model_id", model)];
    if !language.is_empty() {
        fields.push(("language_code", language));
    }
    fields.extend([
        ("timestamps_granularity", "word"),
        ("tag_audio_events", "false"),
    ]);
    fields
}

/// The speech in `m4a`, a piece `length` long, timed within it.
pub(super) fn transcribe(
    request: &Request,
    m4a: &[u8],
    length: Duration,
    timeout: Duration,
) -> anyhow::Result<Vec<Timed>> {
    let url = format!("{}/v1/speech-to-text", request.base);
    let fields = fields(request.model, language(request.locale));
    let answer = request.post_form(&url, &fields, m4a, timeout)?;
    parse(&answer, length).with_context(|| request.unreadable())
}

/// The speech in an answer for a piece `length` long, timed within it: its words, without the
/// spaces and sounds between them, as segments, or, without words, its text as one segment
/// spanning the piece. `None` when it isn't a transcription, or a time is past any Recording.
fn parse(answer: &str, length: Duration) -> Option<Vec<Timed>> {
    let answer: Value = serde_json::from_str(answer).ok()?;
    let words: Vec<Timed> = (answer["words"].as_array()?.iter())
        .filter(|item| item["type"] == "word")
        .map(word)
        .collect::<Option<_>>()?;
    Some(segments_or_text(words, answer["text"].as_str()?, length))
}

#[cfg(test)]
mod tests {
    use super::{Timed, fields, parse};
    use std::time::Duration;

    fn timed(start_ms: u64, end_ms: u64, text: &str) -> Timed {
        Timed {
            start: Duration::from_millis(start_ms),
            end: Duration::from_millis(end_ms),
            text: text.to_owned(),
        }
    }

    #[test]
    fn the_fields_ask_for_word_times_and_the_language_when_set() {
        assert_eq!(
            fields("scribe_v2", "it"),
            [
                ("model_id", "scribe_v2"),
                ("language_code", "it"),
                ("timestamps_granularity", "word"),
                ("tag_audio_events", "false"),
            ]
        );
        assert_eq!(
            fields("scribe_v2", ""),
            [
                ("model_id", "scribe_v2"),
                ("timestamps_granularity", "word"),
                ("tag_audio_events", "false"),
            ]
        );
    }

    #[test]
    fn the_words_become_segments_without_spaces_or_sounds() {
        let length = Duration::from_secs(4);
        // ElevenLabs' example answer.
        let example = r#"{"language_code":"en","language_probability":0.98,"text":"Hello world!",
            "words":[{"end":0.5,"logprob":-0.124,"speaker_id":"speaker_1","start":0,
            "text":"Hello","type":"word"}]}"#;
        assert_eq!(parse(example, length), Some(vec![timed(0, 500, "Hello")]));
        let answer = r#"{"language_code":"eng","language_probability":0.99,
            "text":"Hello, world! (laughs) Bye.","words":[
            {"text":"Hello,","type":"word","start":0.1,"end":0.5,"logprob":-0.1},
            {"text":" ","type":"spacing","start":0.5,"end":0.6,"logprob":0},
            {"text":"world!","type":"word","start":0.6,"end":1.0,"logprob":-0.1},
            {"text":"(laughs)","type":"audio_event","start":1.0,"end":2.0,"logprob":0},
            {"text":"Bye.","type":"word","start":3.0,"end":3.4,"logprob":-0.2}]}"#;
        assert_eq!(
            parse(answer, length),
            Some(vec![
                timed(100, 1_000, "Hello, world!"),
                timed(3_000, 3_400, "Bye.")
            ])
        );
        // Silence: nothing to place.
        assert_eq!(
            parse(r#"{"text":"","words":[]}"#, length),
            Some(vec![timed(0, 4_000, "")])
        );
        assert_eq!(parse(r#"{"detail":{"message":"bad audio"}}"#, length), None);
        let huge =
            r#"{"text":"Hi","words":[{"text":"Hi","type":"word","start":1e300,"end":1e300}]}"#;
        assert_eq!(parse(huge, length), None);
    }
}
