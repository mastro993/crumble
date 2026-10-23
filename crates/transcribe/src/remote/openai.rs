//! OpenAI's audio transcriptions, which OpenRouter and Whisper servers also speak:
//! `POST {base}/audio/transcriptions` with the piece and form fields, answered with timed
//! segments or with text alone, depending on the Model.

use std::time::Duration;

use anyhow::Context;
use crumble_remote::Provider;
use serde_json::Value;

use super::{Request, Timed, language, seconds};

/// The most bytes uploaded at once. OpenAI takes files up to 25 MB; M4A holds the same AAC
/// frames as ADTS in a little less room, so this leaves a margin.
const MAX_PIECE: usize = 20_000_000;
/// The longest piece a `diarize` Model is sent: OpenAI's takes at most 1,400 s of audio.
const MAX_SPEAKERS_LENGTH: Duration = Duration::from_secs(20 * 60);
/// The longest piece a Model that answers with text alone is sent: OpenAI's write at most
/// 2,000 tokens, which runs out after about 8 minutes of speech, and its text is one segment
/// spanning the piece.
const MAX_TEXT_LENGTH: Duration = Duration::from_secs(5 * 60);

/// What a Model is asked to answer with, as its id tells.
#[derive(Debug, PartialEq, Eq)]
enum Answer {
    /// `verbose_json` with timed segments, as Whisper Models give.
    Segments,
    /// `diarized_json`: timed segments with speakers, as OpenAI's `diarize` Models give.
    Speakers,
    /// `json`: the text alone, which is all OpenAI's other Models give.
    Text,
}

/// What `provider`'s `model` is asked to answer with. A server the user runs gets Whisper's
/// request, which Whisper servers expect, unless the id names one of OpenAI's other Models.
fn answer(provider: Provider, model: &str) -> Answer {
    let model = model.to_ascii_lowercase();
    if model.contains("diarize") {
        Answer::Speakers
    } else if model.contains("whisper")
        || (provider == Provider::OpenAiCompatible && !model.contains("transcribe"))
    {
        Answer::Segments
    } else {
        Answer::Text
    }
}

/// The most bytes, and the longest piece, `provider`'s `model` is sent at once. Whisper Models
/// take any length.
pub(super) fn max_piece(provider: Provider, model: &str) -> (usize, Duration) {
    let length = match answer(provider, model) {
        Answer::Segments => Duration::MAX,
        Answer::Speakers => MAX_SPEAKERS_LENGTH,
        Answer::Text => MAX_TEXT_LENGTH,
    };
    (MAX_PIECE, length)
}

/// The form fields that go before the piece.
fn fields<'a>(provider: Provider, model: &'a str, language: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut fields = vec![("model", model)];
    match answer(provider, model) {
        Answer::Segments => fields.extend([
            ("response_format", "verbose_json"),
            ("timestamp_granularities[]", "segment"),
        ]),
        Answer::Speakers => fields.extend([
            ("response_format", "diarized_json"),
            // Needed for audio over 30 s.
            ("chunking_strategy", "auto"),
        ]),
        Answer::Text => fields.push(("response_format", "json")),
    }
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
    let url = format!("{}/audio/transcriptions", request.base);
    let fields = fields(request.provider, request.model, language(request.locale));
    let answer = request.post_form(&url, &fields, m4a, timeout)?;
    parse(&answer, length).with_context(|| request.unreadable())
}

/// The speech in an answer for a piece `length` long, timed within it. `verbose_json` and
/// `diarized_json` give timed segments; an answer with only `text`, as `json` gives and as some
/// servers give whatever was asked, is one segment spanning the piece. `None` when it isn't a
/// transcription, or a time is past any Recording.
fn parse(answer: &str, length: Duration) -> Option<Vec<Timed>> {
    let answer: Value = serde_json::from_str(answer).ok()?;
    if let Some(segments) = answer["segments"].as_array() {
        return segments
            .iter()
            .map(|segment| {
                Some(Timed {
                    start: seconds(&segment["start"])?,
                    end: seconds(&segment["end"])?,
                    text: segment["text"].as_str().unwrap_or_default().to_owned(),
                })
            })
            .collect();
    }
    Some(vec![Timed {
        start: Duration::ZERO,
        end: length,
        text: answer["text"].as_str()?.to_owned(),
    }])
}

#[cfg(test)]
mod tests {
    use super::{Answer, answer, fields, max_piece, parse};
    use crate::adts::Chunk;
    use crate::remote::place;
    use crate::{Segment, Track, merge};
    use crumble_remote::Provider;
    use std::time::Duration;

    fn piece(start_ms: u64, length_ms: u64) -> Chunk {
        Chunk {
            bytes: 0..0,
            start: Duration::from_millis(start_ms),
            length: Duration::from_millis(length_ms),
        }
    }

    fn segment(track: Track, start_ms: u64, end_ms: u64, text: &str) -> Segment {
        Segment {
            track,
            start: Duration::from_millis(start_ms),
            end: Duration::from_millis(end_ms),
            text: text.to_owned(),
        }
    }

    /// The segments of `track` in `response` for `piece`, at their place in the Recording.
    fn heard(response: &str, track: Track, piece: &Chunk) -> Option<Vec<Segment>> {
        place(parse(response, piece.length)?, track, piece)
    }

    #[test]
    fn each_model_is_asked_for_the_answer_it_gives() {
        let asked = |provider, model| answer(provider, model);
        assert_eq!(asked(Provider::OpenAi, "whisper-1"), Answer::Segments);
        assert_eq!(asked(Provider::OpenAi, "gpt-transcribe"), Answer::Text);
        assert_eq!(
            asked(Provider::OpenAi, "gpt-4o-mini-transcribe"),
            Answer::Text
        );
        assert_eq!(
            asked(Provider::OpenAi, "gpt-4o-transcribe-diarize"),
            Answer::Speakers
        );
        assert_eq!(
            asked(Provider::OpenRouter, "openai/whisper-large-v3"),
            Answer::Segments
        );
        assert_eq!(
            asked(Provider::OpenRouter, "microsoft/mai-transcribe-1.5"),
            Answer::Text
        );
        // A server gets Whisper's request unless the id names another of OpenAI's Models.
        for whisper in [
            "whisper-1",
            "Systran/faster-whisper-small",
            "parakeet-tdt-0.6b",
        ] {
            assert_eq!(
                asked(Provider::OpenAiCompatible, whisper),
                Answer::Segments,
                "{whisper}"
            );
        }
        assert_eq!(
            asked(Provider::OpenAiCompatible, "gpt-4o-transcribe"),
            Answer::Text
        );
    }

    #[test]
    fn a_piece_is_as_long_as_the_model_takes() {
        let length = |provider, model| max_piece(provider, model).1;
        assert_eq!(length(Provider::OpenAi, "whisper-1"), Duration::MAX);
        assert_eq!(
            length(Provider::OpenAiCompatible, "Systran/faster-whisper-small"),
            Duration::MAX
        );
        assert_eq!(
            length(Provider::OpenAi, "gpt-4o-transcribe-diarize"),
            Duration::from_secs(20 * 60)
        );
        assert_eq!(
            length(Provider::OpenAi, "gpt-transcribe"),
            Duration::from_secs(5 * 60)
        );
        assert_eq!(
            length(Provider::OpenRouter, "microsoft/mai-transcribe-1.5"),
            Duration::from_secs(5 * 60)
        );
    }

    #[test]
    fn the_fields_ask_for_the_model_s_answer_and_the_language() {
        assert_eq!(
            fields(Provider::OpenAi, "whisper-1", "en"),
            [
                ("model", "whisper-1"),
                ("response_format", "verbose_json"),
                ("timestamp_granularities[]", "segment"),
                ("language", "en"),
            ]
        );
        assert_eq!(
            fields(Provider::OpenAi, "gpt-transcribe", ""),
            [("model", "gpt-transcribe"), ("response_format", "json")]
        );
        assert_eq!(
            fields(Provider::OpenAi, "gpt-4o-transcribe-diarize", "it"),
            [
                ("model", "gpt-4o-transcribe-diarize"),
                ("response_format", "diarized_json"),
                ("chunking_strategy", "auto"),
                ("language", "it"),
            ]
        );
    }

    #[test]
    fn verbose_json_segments_land_at_the_piece_s_place() {
        let response = r#"{"task":"transcribe","language":"english","duration":9.5,
            "text":"Hello there. Can you hear me?","segments":[
            {"id":0,"seek":0,"start":0.0,"end":2.4,"text":" Hello there.","tokens":[1]},
            {"id":1,"seek":0,"start":3.0,"end":5.25,"text":" Can you hear me?","tokens":[2]},
            {"id":2,"seek":0,"start":6.0,"end":5.0,"text":"   ","tokens":[]}]}"#;
        assert_eq!(
            heard(response, Track::System, &piece(60_000, 9_500)),
            Some(vec![
                segment(Track::System, 60_000, 62_400, "Hello there."),
                segment(Track::System, 63_000, 65_250, "Can you hear me?"),
            ])
        );
    }

    #[test]
    fn diarized_json_segments_are_read_like_verbose_ones() {
        // Shaped as openai-python's `TranscriptionDiarized` describes it.
        let response = r#"{"task":"transcribe","duration":3.0,"text":"Hello. Hi there.","segments":[
            {"type":"transcript.text.segment","id":"seg_001","start":0.0,"end":1.2,
             "text":"Hello.","speaker":"A"},
            {"type":"transcript.text.segment","id":"seg_002","start":1.4,"end":2.9,
             "text":"Hi there.","speaker":"B"}],
            "usage":{"type":"duration","seconds":3}}"#;
        assert_eq!(
            heard(response, Track::System, &piece(0, 3_000)),
            Some(vec![
                segment(Track::System, 0, 1_200, "Hello."),
                segment(Track::System, 1_400, 2_900, "Hi there."),
            ])
        );
    }

    #[test]
    fn a_text_only_answer_spans_the_piece() {
        assert_eq!(
            heard(r#"{"text":" Hi. "}"#, Track::Mic, &piece(1_000, 4_000)),
            Some(vec![segment(Track::Mic, 1_000, 5_000, "Hi.")])
        );
        // `gpt-transcribe` adds the languages it heard.
        assert_eq!(
            heard(
                r#"{"text":"Ciao.","languages":[{"code":"it"}]}"#,
                Track::Mic,
                &piece(0, 2_000)
            ),
            Some(vec![segment(Track::Mic, 0, 2_000, "Ciao.")])
        );
        assert_eq!(
            heard(r#"{"text":""}"#, Track::Mic, &piece(0, 1)),
            Some(Vec::new())
        );
        assert_eq!(heard("<html>", Track::Mic, &piece(0, 1)), None);
        assert_eq!(heard(r#"{"error":"no"}"#, Track::Mic, &piece(0, 1)), None);
        let huge = r#"{"segments":[{"start":1e300,"end":1e300,"text":"Hi."}]}"#;
        assert_eq!(heard(huge, Track::Mic, &piece(0, 1)), None);
    }

    #[test]
    fn remote_segments_merge_across_tracks() {
        let mic = heard(
            r#"{"segments":[{"start":0.5,"end":1.5,"text":"Hi."}]}"#,
            Track::Mic,
            &piece(0, 10_000),
        )
        .unwrap_or_default();
        let system = heard(
            r#"{"segments":[{"start":0.0,"end":1.0,"text":"Hello?"}]}"#,
            Track::System,
            &piece(0, 10_000),
        )
        .unwrap_or_default();
        let merged = merge(vec![mic, system]);
        let texts: Vec<_> = merged.iter().map(|segment| segment.text.as_str()).collect();
        assert_eq!(texts, ["Hello?", "Hi."]);
    }
}
