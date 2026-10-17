//! A server that speaks OpenAI's audio transcriptions, through `crumble-remote`: OpenAI itself,
//! or a Whisper server. Each track is uploaded on its own, in pieces under the upload limit,
//! each repackaged from ADTS into M4A, which the API accepts (ADR 0008).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, bail};
use crumble_remote::{Provider, base_url, failure, no_response, post, without_userinfo};
use serde_json::Value;
use uuid::Uuid;

use crate::adts::{Chunk, chunks};
use crate::{Segment, Track, Transcriber, merge, tracks};

/// The most audio uploaded at once. OpenAI takes files up to 25 MB; M4A holds the same AAC
/// frames as ADTS in a little less room, so this leaves a margin. About 40 minutes of a
/// 65 kbit/s track.
const MAX_UPLOAD: usize = 20_000_000;

/// A server that speaks OpenAI's audio transcriptions, such as OpenAI's or a Whisper server.
pub struct OpenAiCompatible {
    /// Its address up to `/audio/transcriptions`, such as `http://localhost:8000/v1`.
    pub base_url: String,
    /// The key, from the Keychain, if the server wants one.
    pub api_key: Option<String>,
    /// The Model's name on that server, such as `whisper-1`.
    pub model: String,
}

impl Transcriber for OpenAiCompatible {
    fn provider(&self) -> Provider {
        Provider::OpenAiCompatible
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn transcribe(
        &self,
        dir: &Path,
        locale: &str,
        progress: &mut (dyn FnMut(f32) + Send),
    ) -> anyhow::Result<Vec<Segment>> {
        let base = base_url(&self.base_url)?;
        if self.model.trim().is_empty() {
            bail!("Enter the server's Transcription Model in Settings.");
        }
        let mut uploads = Vec::new();
        for (track, path) in tracks(dir).context("This Recording has no audio to transcribe.")? {
            let audio = std::fs::read(&path)
                .with_context(|| format!("could not read {}", path.display()))
                .context("Couldn't read the Recording's audio.")?;
            let pieces = chunks(&audio, MAX_UPLOAD)
                .with_context(|| format!("could not read {}", path.display()))
                .context("Couldn't read the Recording's audio.")?;
            uploads.push((track, audio, pieces));
        }
        let total: usize = uploads.iter().map(|(_, _, pieces)| pieces.len()).sum();
        let scratch = Scratch::new()?;
        let url = format!("{base}/audio/transcriptions");
        let mut done = 0;
        let mut segments = Vec::new();
        for (track, audio, pieces) in &uploads {
            let mut heard = Vec::new();
            for piece in pieces {
                let m4a = scratch.m4a(&audio[piece.bytes.clone()])?;
                let response = self.upload(&url, base, &m4a, language(locale))?;
                heard.extend(parse(&response, *track, piece)?);
                done += 1;
                #[allow(clippy::cast_precision_loss, reason = "a few pieces")]
                progress(done as f32 / total as f32);
            }
            segments.push(heard);
        }
        Ok(merge(segments))
    }
}

impl OpenAiCompatible {
    /// Sends one piece of audio and returns the server's answer.
    fn upload(&self, url: &str, base: &str, m4a: &[u8], language: &str) -> anyhow::Result<String> {
        let boundary = format!("crumble-{}", Uuid::now_v7().simple());
        let mut fields = vec![
            ("model", self.model.trim()),
            ("response_format", "verbose_json"),
            ("timestamp_granularities[]", "segment"),
        ];
        if !language.is_empty() {
            fields.push(("language", language));
        }
        let body = multipart(&boundary, &fields, ("audio.m4a", "audio/mp4", m4a));
        let authorization = self.api_key.as_ref().map(|key| format!("Bearer {key}"));
        let headers: Vec<(&str, &str)> = authorization
            .iter()
            .map(|value| ("authorization", value.as_str()))
            .collect();
        let content_type = format!("multipart/form-data; boundary={boundary}");
        let (status, text) = post(url, &headers, &content_type, &body).map_err(|error| {
            // The address may hold a user name and password.
            let base = without_userinfo(base);
            no_response(&base, "Check that the server is running.", &error)
        })?;
        if status != 200 {
            return Err(failure(
                Provider::OpenAiCompatible,
                &self.model,
                status,
                &text,
            ));
        }
        Ok(text)
    }
}

/// The ISO 639-1 language of `locale`, as the API takes it: `en` for `en_US`.
fn language(locale: &str) -> &str {
    locale.split(['_', '-']).next().unwrap_or_default()
}

/// A `multipart/form-data` body: each of `fields` as text, then `file` as its name, content type
/// and bytes. `boundary` must not occur in any of them.
fn multipart(boundary: &str, fields: &[(&str, &str)], file: (&str, &str, &[u8])) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    }
    let (file_name, content_type, bytes) = file;
    body.extend(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\nContent-Type: {content_type}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend(bytes);
    body.extend(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

/// The segments of `track` in a server's answer for `piece`, at their place in the Recording.
/// `verbose_json` gives timed segments; an answer with only `text`, as some Models give, is one
/// segment spanning the piece.
fn parse(response: &str, track: Track, piece: &Chunk) -> anyhow::Result<Vec<Segment>> {
    let response: Value =
        serde_json::from_str(response).context("The server's answer wasn't a transcription.")?;
    let segment = |start: Duration, end: Duration, text: &str| {
        let text = text.trim();
        (!text.is_empty()).then(|| Segment {
            track,
            start: piece.start + start,
            end: piece.start + end.max(start),
            text: text.to_owned(),
        })
    };
    if let Some(segments) = response["segments"].as_array() {
        let seconds =
            |value: &Value| Duration::from_secs_f64(value.as_f64().unwrap_or(0.0).max(0.0));
        return Ok(segments
            .iter()
            .filter_map(|item| {
                segment(
                    seconds(&item["start"]),
                    seconds(&item["end"]),
                    item["text"].as_str().unwrap_or_default(),
                )
            })
            .collect());
    }
    let Some(text) = response["text"].as_str() else {
        bail!("The server's answer wasn't a transcription.");
    };
    Ok(segment(Duration::ZERO, piece.length, text)
        .into_iter()
        .collect())
}

/// A folder of its own in the temp folder for the pieces being uploaded; it goes, with them,
/// when this is dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> anyhow::Result<Self> {
        let dir = std::env::temp_dir().join(format!("crumble-upload-{}", Uuid::now_v7().simple()));
        std::fs::create_dir_all(&dir).context("Couldn't prepare the audio to upload.")?;
        Ok(Self(dir))
    }

    /// `adts` repackaged into M4A by `afconvert`, which comes with macOS. The AAC frames are
    /// copied as they are, not encoded again.
    fn m4a(&self, adts: &[u8]) -> anyhow::Result<Vec<u8>> {
        let (source, converted) = (self.0.join("piece.aac"), self.0.join("piece.m4a"));
        std::fs::write(&source, adts).context("Couldn't prepare the audio to upload.")?;
        let status = Command::new("/usr/bin/afconvert")
            .args(["-f", "m4af", "-d", "0"])
            .arg(&source)
            .arg(&converted)
            .status()
            .context("Couldn't prepare the audio to upload.")?;
        if !status.success() {
            eprintln!("afconvert could not repackage the audio ({status})");
            bail!("Couldn't prepare the audio to upload.");
        }
        let m4a = std::fs::read(&converted).context("Couldn't prepare the audio to upload.")?;
        std::fs::remove_file(&source).ok();
        std::fs::remove_file(&converted).ok();
        Ok(m4a)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::{Chunk, OpenAiCompatible, language, multipart, parse};
    use crate::{Segment, Track, Transcriber};
    use std::path::Path;
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
    fn verbose_json_segments_land_at_the_piece_s_place() {
        let response = r#"{"task":"transcribe","language":"english","duration":9.5,
            "text":"Hello there. Can you hear me?","segments":[
            {"id":0,"seek":0,"start":0.0,"end":2.4,"text":" Hello there.","tokens":[1]},
            {"id":1,"seek":0,"start":3.0,"end":5.25,"text":" Can you hear me?","tokens":[2]},
            {"id":2,"seek":0,"start":6.0,"end":5.0,"text":"   ","tokens":[]}]}"#;
        assert_eq!(
            parse(response, Track::System, &piece(60_000, 9_500)).ok(),
            Some(vec![
                segment(Track::System, 60_000, 62_400, "Hello there."),
                segment(Track::System, 63_000, 65_250, "Can you hear me?"),
            ])
        );
    }

    #[test]
    fn a_text_only_answer_spans_the_piece() {
        assert_eq!(
            parse(r#"{"text":" Hi. "}"#, Track::Mic, &piece(1_000, 4_000)).ok(),
            Some(vec![segment(Track::Mic, 1_000, 5_000, "Hi.")])
        );
        assert_eq!(
            parse(r#"{"text":""}"#, Track::Mic, &piece(0, 1)).ok(),
            Some(Vec::new())
        );
        assert!(parse("<html>", Track::Mic, &piece(0, 1)).is_err());
        assert!(parse(r#"{"error":"no"}"#, Track::Mic, &piece(0, 1)).is_err());
    }

    #[test]
    fn remote_segments_merge_across_tracks() {
        let mic = parse(
            r#"{"segments":[{"start":0.5,"end":1.5,"text":"Hi."}]}"#,
            Track::Mic,
            &piece(0, 10_000),
        )
        .unwrap_or_default();
        let system = parse(
            r#"{"segments":[{"start":0.0,"end":1.0,"text":"Hello?"}]}"#,
            Track::System,
            &piece(0, 10_000),
        )
        .unwrap_or_default();
        let merged = crate::merge(vec![mic, system]);
        let texts: Vec<_> = merged.iter().map(|segment| segment.text.as_str()).collect();
        assert_eq!(texts, ["Hello?", "Hi."]);
    }

    #[test]
    fn a_locale_gives_its_language() {
        assert_eq!(language("en_US"), "en");
        assert_eq!(language("zh-Hans-CN"), "zh");
        assert_eq!(language("it"), "it");
        assert_eq!(language(""), "");
    }

    #[test]
    fn missing_settings_fail_before_any_upload() {
        let error = |base_url: &str| {
            let server = OpenAiCompatible {
                base_url: base_url.into(),
                api_key: Some("sk-test-secret".into()),
                model: "whisper-1".into(),
            };
            server
                .transcribe(Path::new("/nonexistent"), "en_US", &mut |_| {})
                .map_err(|error| error.to_string())
                .err()
        };
        assert_eq!(
            error(" ").as_deref(),
            Some("Enter the server's address in Settings.")
        );
        assert_eq!(
            error("whisper.lan:8000/v1").as_deref(),
            Some("Start the server's address with http:// or https://.")
        );
    }
}
