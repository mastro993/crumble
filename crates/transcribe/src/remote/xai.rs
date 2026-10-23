//! xAI's speech to text: `POST {base}/stt` with form fields and then the piece, which xAI
//! requires last, answered with the text and its timed words, which become segments here.

use std::time::Duration;

use anyhow::Context;
use serde_json::Value;

use super::words::segments_or_text;
use super::{Request, Timed, language, word};

/// The most audio uploaded at once. xAI takes files up to 500 MB; this keeps each request as
/// long as OpenAI's, about 40 minutes of a 65 kbit/s track.
pub(super) const MAX_PIECE: usize = 20_000_000;

/// The form fields that go before the piece.
fn fields<'a>(model: &'a str, language: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut fields = vec![("model", model)];
    if !language.is_empty() {
        fields.push(("language", language));
    }
    fields
}

/// The speech in `m4a`, a piece `length` long, timed within it.
pub(super) fn transcribe(
    request: &Request,
    m4a: &[u8],
    length: Duration,
    timeout: Duration,
) -> anyhow::Result<Vec<Timed>> {
    let url = format!("{}/stt", request.base);
    let fields = fields(request.model, language(request.locale));
    let answer = request.post_form(&url, &fields, m4a, timeout)?;
    parse(&answer, length).with_context(|| request.unreadable())
}

/// The speech in an answer for a piece `length` long, timed within it: its words as segments,
/// or, without words, its text as one segment spanning the piece. `None` when it isn't a
/// transcription, or a time is past any Recording.
fn parse(answer: &str, length: Duration) -> Option<Vec<Timed>> {
    let answer: Value = serde_json::from_str(answer).ok()?;
    let words: Vec<Timed> = (answer["words"].as_array().into_iter().flatten())
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
    fn the_fields_name_the_model_and_the_language_when_set() {
        assert_eq!(
            fields("grok-voice-transcribe-2.0", "en"),
            [("model", "grok-voice-transcribe-2.0"), ("language", "en")]
        );
        assert_eq!(
            fields("grok-voice-transcribe-2.0", ""),
            [("model", "grok-voice-transcribe-2.0")]
        );
    }

    #[test]
    fn the_words_become_segments_and_text_alone_spans_the_piece() {
        // xAI's example answer.
        let example = r#"{"text":"The balance is $167,983.15.","language":"en","duration":3.45,
            "words":[{"text":"The","start":0.24,"end":0.48},
            {"text":"balance","start":0.48,"end":0.96},{"text":"is","start":0.96,"end":1.12},
            {"text":"$167,983.15.","start":1.12,"end":3.20}]}"#;
        let length = Duration::from_millis(3_450);
        assert_eq!(
            parse(example, length),
            Some(vec![timed(240, 3_200, "The balance is $167,983.15.")])
        );
        assert_eq!(
            parse(
                r#"{"text":"Hello.","language":"en","duration":1.5}"#,
                length
            ),
            Some(vec![timed(0, 3_450, "Hello.")])
        );
        assert_eq!(
            parse(r#"{"text":"","words":[]}"#, length),
            Some(vec![timed(0, 3_450, "")])
        );
        assert_eq!(parse(r#"{"error":"bad audio"}"#, length), None);
        let huge = r#"{"text":"Hi","words":[{"text":"Hi","start":1e300,"end":1e300}]}"#;
        assert_eq!(parse(huge, length), None);
    }
}
