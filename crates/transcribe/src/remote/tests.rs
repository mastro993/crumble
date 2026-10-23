use super::{Remote, Timed, language, language_tag, multipart, place};
use crate::adts::Chunk;
use crate::{Segment, Track, Transcriber};
use crumble_remote::Provider;
use std::path::Path;
use std::time::Duration;

/// Why `remote` fails before reading any audio.
fn refusal(provider: Provider, base_url: &str, api_key: Option<&str>, model: &str) -> String {
    let remote = Remote {
        provider,
        base_url: base_url.into(),
        api_key: api_key.map(str::to_owned),
        model: model.into(),
    };
    match remote.transcribe(Path::new("/nonexistent"), "en_US", &mut |_| {}) {
        Ok(segments) => panic!("transcribed {segments:?}"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn missing_settings_fail_before_any_upload() {
    let key = Some("sk-test-secret");
    assert_eq!(
        refusal(Provider::OpenAiCompatible, " ", key, "whisper-1"),
        "Enter the server's address in Settings."
    );
    assert_eq!(
        refusal(
            Provider::OpenAiCompatible,
            "whisper.lan:8000/v1",
            key,
            "whisper-1"
        ),
        "Start the server's address with http:// or https://."
    );
    assert_eq!(
        refusal(
            Provider::OpenAiCompatible,
            "http://whisper.lan/v1",
            None,
            " "
        ),
        "Enter the server's Transcription Model in Settings."
    );
    assert_eq!(
        refusal(Provider::ElevenLabs, "", key, ""),
        "Choose a Transcription Model for ElevenLabs in Settings."
    );
    for key in [None, Some(" ")] {
        assert_eq!(
            refusal(Provider::OpenAi, "", key, "whisper-1"),
            "Add an API key for OpenAI in Settings."
        );
    }
    // A server needs no key, so it goes on to read the Recording.
    assert_eq!(
        refusal(
            Provider::OpenAiCompatible,
            "http://whisper.lan/v1",
            None,
            "m1"
        ),
        "This Recording has no audio to transcribe."
    );
}

#[test]
fn only_providers_whose_api_transcribes_make_transcripts() {
    for provider in [Provider::Anthropic, Provider::DeepSeek, Provider::Apple] {
        assert_eq!(
            refusal(provider, "", Some("k1"), "m1"),
            format!("{} doesn't make Transcripts.", provider.name())
        );
    }
}

#[test]
fn a_locale_gives_its_language_or_none_to_detect_it() {
    assert_eq!(language("en_US"), "en");
    assert_eq!(language("zh-Hans-CN"), "zh");
    assert_eq!(language("en_GB@rg=uszzzz"), "en");
    assert_eq!(language("it"), "it");
    assert_eq!(language(""), "");
    assert_eq!(language(crate::DETECT), "");
    assert_eq!(language_tag("en_US"), "en-US");
    assert_eq!(language_tag("zh_Hans_CN"), "zh-Hans-CN");
    assert_eq!(language_tag("en_GB@rg=uszzzz"), "en-GB");
    assert_eq!(language_tag("it"), "it");
    assert_eq!(language_tag(""), "");
    assert_eq!(language_tag(crate::DETECT), "");
}

#[test]
fn the_body_holds_each_field_then_the_file() {
    let body = multipart(
        "b0",
        &[("model", "whisper-1"), ("language", "en")],
        ("audio.m4a", "audio/mp4", b"\x00\xFFaudio"),
    );
    let mut expected =
        b"--b0\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nwhisper-1\r\n\
--b0\r\nContent-Disposition: form-data; name=\"language\"\r\n\r\nen\r\n\
--b0\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.m4a\"\r\n\
Content-Type: audio/mp4\r\n\r\n"
            .to_vec();
    expected.extend(b"\x00\xFFaudio\r\n--b0--\r\n");
    assert_eq!(body, expected);
}

#[test]
fn heard_speech_lands_at_the_piece_s_place_without_empty_segments() {
    let piece = Chunk {
        bytes: 0..0,
        start: Duration::from_secs(60),
        length: Duration::from_secs(10),
    };
    let timed = |start_ms, end_ms, text: &str| Timed {
        start: Duration::from_millis(start_ms),
        end: Duration::from_millis(end_ms),
        text: text.to_owned(),
    };
    assert_eq!(
        place(
            vec![
                timed(500, 1_500, " Hi. "),
                timed(2_000, 1_000, "Back?"),
                timed(3_000, 4_000, "  "),
            ],
            Track::Mic,
            &piece
        ),
        Some(vec![
            Segment {
                track: Track::Mic,
                start: Duration::from_millis(60_500),
                end: Duration::from_millis(61_500),
                text: "Hi.".to_owned(),
            },
            Segment {
                track: Track::Mic,
                start: Duration::from_millis(62_000),
                end: Duration::from_millis(62_000),
                text: "Back?".to_owned(),
            },
        ])
    );
    let past_any_recording = Timed {
        start: Duration::MAX,
        end: Duration::MAX,
        text: "Hi.".to_owned(),
    };
    assert_eq!(place(vec![past_any_recording], Track::Mic, &piece), None);
}
