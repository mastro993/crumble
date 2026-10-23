//! A remote Provider's Model, through `crumble-remote` (ADRs 0008 and 0011): OpenAI's audio
//! transcriptions, which OpenRouter and Whisper servers also speak, ElevenLabs' and xAI's speech
//! to text, and Gemini's transcribe Model, one module each in `remote/`. Each track is uploaded
//! on its own, in pieces under the API's limits on size and on length, each repackaged from ADTS
//! into M4A, which all of them accept.

mod elevenlabs;
mod gemini;
mod openai;
mod words;
mod xai;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, bail};
use crumble_remote::{
    Provider, Speech, TIMEOUT, base_url, failure, no_response, post, without_userinfo,
};
use serde_json::Value;
use uuid::Uuid;

use crate::adts::{Chunk, chunks};
use crate::{DETECT, Segment, Track, Transcriber, merge, tracks};
use words::Timed;

/// A remote Provider's Model that transcribes, such as OpenAI's `whisper-1`, ElevenLabs'
/// `scribe_v2`, or a Whisper server's.
pub struct Remote {
    /// Who runs the Model: a remote Provider whose API transcribes.
    pub provider: Provider,
    /// The address the user gave the Provider in Settings, or "" for its usual one.
    pub base_url: String,
    /// The Provider's key, from the Keychain; a server the user runs may want none.
    pub api_key: Option<String>,
    /// The Model's id, such as `whisper-1`.
    pub model: String,
}

impl Transcriber for Remote {
    fn provider(&self) -> Provider {
        self.provider
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
        let (request, speech) = self.request(locale)?;
        let ((max_bytes, max_length), transcribe): ((usize, Duration), Api) = match speech {
            Speech::OpenAiTranscriptions => (
                openai::max_piece(request.provider, request.model),
                openai::transcribe,
            ),
            Speech::ElevenLabsSpeechToText => (
                (elevenlabs::MAX_PIECE, Duration::MAX),
                elevenlabs::transcribe,
            ),
            Speech::XaiSpeechToText => ((xai::MAX_PIECE, Duration::MAX), xai::transcribe),
            Speech::GeminiInteractions => {
                ((gemini::MAX_PIECE, gemini::MAX_LENGTH), gemini::transcribe)
            }
        };
        let mut uploads = Vec::new();
        for (track, path) in tracks(dir).context("This Recording has no audio to transcribe.")? {
            let audio = std::fs::read(&path)
                .with_context(|| format!("could not read {}", path.display()))
                .context("Couldn't read the Recording's audio.")?;
            let pieces = chunks(&audio, max_bytes, max_length)
                .with_context(|| format!("could not read {}", path.display()))
                .context("Couldn't read the Recording's audio.")?;
            uploads.push((track, audio, pieces));
        }
        let total: usize = uploads.iter().map(|(_, _, pieces)| pieces.len()).sum();
        let scratch = Scratch::new()?;
        let mut done = 0;
        let mut segments = Vec::new();
        for (track, audio, pieces) in &uploads {
            let mut heard = Vec::new();
            for piece in pieces {
                let m4a = scratch.m4a(&audio[piece.bytes.clone()])?;
                // A local server may transcribe slower than the audio plays.
                let timeout = TIMEOUT.max(piece.length * 2);
                let timed = transcribe(&request, &m4a, piece.length, timeout)?;
                heard.extend(place(timed, *track, piece).with_context(|| request.unreadable())?);
                done += 1;
                #[allow(clippy::cast_precision_loss, reason = "a few pieces")]
                progress(done as f32 / total as f32);
            }
            segments.push(heard);
        }
        Ok(merge(segments))
    }
}

impl Remote {
    /// What every request for this Model shares, and the API it goes through; or, for the
    /// user, what Settings is missing.
    fn request<'a>(&'a self, locale: &'a str) -> anyhow::Result<(Request<'a>, Speech)> {
        let name = self.provider.name();
        let remote = self.provider.remote();
        let Some((remote, speech)) = remote.and_then(|remote| Some((remote, remote.speech()?)))
        else {
            bail!("{name} doesn't make Transcripts.");
        };
        let base = base_url(remote.base_url(&self.base_url))?;
        let key = (self.api_key.as_deref())
            .map(str::trim)
            .filter(|key| !key.is_empty());
        if key.is_none() && remote.api_key().is_some_and(|format| format.required()) {
            bail!("Add an API key for {name} in Settings.");
        }
        let model = self.model.trim();
        if model.is_empty() {
            match self.provider {
                Provider::OpenAiCompatible => {
                    bail!("Enter the server's Transcription Model in Settings.")
                }
                _ => bail!("Choose a Transcription Model for {name} in Settings."),
            }
        }
        let request = Request {
            provider: self.provider,
            base,
            headers: remote.headers(key),
            key,
            model,
            locale,
        };
        Ok((request, speech))
    }
}

/// How an API transcribes a piece, M4A that plays for the given length, within the given
/// timeout: the speech in it, timed within it.
type Api = fn(&Request, &[u8], Duration, Duration) -> anyhow::Result<Vec<Timed>>;

/// What every request for one Recording shares, and how its failures name the Provider.
struct Request<'a> {
    /// Who the requests go to.
    provider: Provider,
    /// The address paths follow, without a trailing `/`.
    base: &'a str,
    /// The key in its header, then those every request to the Provider carries.
    headers: Vec<(&'static str, String)>,
    /// The API key, so a failure can take it out of what the server said.
    key: Option<&'a str>,
    /// The Model's id.
    model: &'a str,
    /// The Recording's locale, such as `en_US`, or [`DETECT`].
    locale: &'a str,
}

impl Request<'_> {
    /// The headers a request carries: the Provider's, then `extra`.
    fn headers<'h>(&'h self, extra: &[(&'h str, &'h str)]) -> Vec<(&'h str, &'h str)> {
        (self.headers.iter())
            .map(|(name, value)| (*name, value.as_str()))
            .chain(extra.iter().copied())
            .collect()
    }

    /// POSTs `body`, a content type and its bytes, to `url` with `extra` headers, and returns
    /// the answer; or, for the user, why there is none.
    fn post(
        &self,
        url: &str,
        extra: &[(&str, &str)],
        body: (&str, &[u8]),
        timeout: Duration,
    ) -> anyhow::Result<String> {
        let (status, text) = post(url, &self.headers(extra), body, timeout)
            .map_err(|error| no_response(&self.address(), self.hint(), timeout, &error))?;
        self.accepted(status, text)
    }

    /// POSTs `fields`, then `m4a` as the file, as `multipart/form-data` to `url`.
    fn post_form(
        &self,
        url: &str,
        fields: &[(&str, &str)],
        m4a: &[u8],
        timeout: Duration,
    ) -> anyhow::Result<String> {
        let boundary = format!("crumble-{}", Uuid::now_v7().simple());
        let body = multipart(&boundary, fields, ("audio.m4a", "audio/mp4", m4a));
        let content_type = format!("multipart/form-data; boundary={boundary}");
        self.post(url, &[], (&content_type, &body), timeout)
    }

    /// `text`, when `status` says the Provider did what was asked; else what to tell the user.
    fn accepted(&self, status: u16, text: String) -> anyhow::Result<String> {
        if (200..300).contains(&status) {
            Ok(text)
        } else {
            Err(failure(self.provider, self.model, status, &text, self.key))
        }
    }

    /// Who a request that got no answer names: a company's name, or a server's address
    /// without the user name and password it may hold.
    fn address(&self) -> String {
        match self.provider {
            Provider::OpenAiCompatible => without_userinfo(self.base),
            provider => provider.name().to_owned(),
        }
    }

    /// What to check when a request got no answer.
    fn hint(&self) -> &'static str {
        match self.provider {
            Provider::OpenAiCompatible => "Check that the server is running.",
            _ => "Check the internet connection.",
        }
    }

    /// What to tell the user when an answer isn't a transcription.
    fn unreadable(&self) -> String {
        match self.provider {
            Provider::OpenAiCompatible => "The server's answer wasn't a transcription.".to_owned(),
            provider => format!("{}'s answer wasn't a transcription.", provider.name()),
        }
    }
}

/// The ISO 639-1 language of `locale`, as most APIs take it: `en` for `en_US`; "" for
/// [`DETECT`], which leaves the language to the Model.
fn language(locale: &str) -> &str {
    if locale == DETECT {
        return "";
    }
    locale.split(['_', '-', '@']).next().unwrap_or_default()
}

/// The BCP-47 tag of `locale`, as Gemini takes it: `en-US` for `en_US`; "" for [`DETECT`].
fn language_tag(locale: &str) -> String {
    if locale == DETECT {
        return String::new();
    }
    locale
        .split('@')
        .next()
        .unwrap_or_default()
        .replace('_', "-")
}

/// A `multipart/form-data` body: each of `fields` as text, then `file` as its name, content type
/// and bytes, last, as xAI requires. `boundary` must not occur in any of them.
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

/// A time in seconds in an answer, 0 when it has none; `None` when no Recording is that long.
fn seconds(value: &Value) -> Option<Duration> {
    Duration::try_from_secs_f64(value.as_f64().unwrap_or(0.0).max(0.0)).ok()
}

/// A timed word in an answer: `{text, start, end}`, its times in seconds.
fn word(value: &Value) -> Option<Timed> {
    Some(Timed {
        start: seconds(&value["start"])?,
        end: seconds(&value["end"])?,
        text: value["text"].as_str().unwrap_or_default().to_owned(),
    })
}

/// `heard`, timed within `piece`, as segments of `track` at their place in the Recording,
/// without the empty ones; `None` when a time is past any Recording, as a broken server may
/// send.
fn place(heard: Vec<Timed>, track: Track, piece: &Chunk) -> Option<Vec<Segment>> {
    let mut placed = Vec::new();
    for Timed { start, end, text } in heard {
        let start = piece.start.checked_add(start)?;
        let end = start.max(piece.start.checked_add(end)?);
        let text = text.trim();
        if !text.is_empty() {
            placed.push(Segment {
                track,
                start,
                end,
                text: text.to_owned(),
            });
        }
    }
    Some(placed)
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
mod tests;
