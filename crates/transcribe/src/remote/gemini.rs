//! Google Gemini's transcribe Model. Each piece goes up through the Files API
//! (https://ai.google.dev/gemini-api/docs/files), is transcribed with word times by
//! `POST {base}/v1beta/interactions` (https://ai.google.dev/gemini-api/docs/transcribe), and is
//! deleted afterwards, whatever happened. The words become segments here.

use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use crumble_remote::{delete, get, no_response, post_reading_header};
use serde_json::{Value, json};

use super::words::segments_or_text;
use super::{Request, Timed, language_tag};

/// The most bytes uploaded at once.
pub(super) const MAX_PIECE: usize = 10_000_000;
/// The longest piece sent at once: the transcribe Model times words in at most 30 minutes of
/// audio, and a track's bytes say little of its length, as a quiet or low-rate one is small.
pub(super) const MAX_LENGTH: Duration = Duration::from_secs(25 * 60);

/// The type pieces are uploaded and sent as: M4A, as the Interactions API names it.
const MIME_TYPE: &str = "audio/m4a";
/// How often an upload Google is still processing is checked.
const POLL: Duration = Duration::from_millis(500);
/// How long Google may process an upload before Crumble gives up on it.
const PROCESSING: Duration = Duration::from_secs(180);
/// How long checking or deleting an upload may take.
const QUICK: Duration = Duration::from_secs(20);

/// An uploaded piece, as the Files API describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct File {
    /// Its resource name, `files/` and an id, which addresses it.
    name: String,
    /// What an interaction refers to it by.
    uri: String,
    /// `PROCESSING`, `ACTIVE` or `FAILED`; "" when not given.
    state: String,
}

/// The speech in `m4a`, a piece `length` long, timed within it.
pub(super) fn transcribe(
    request: &Request,
    m4a: &[u8],
    length: Duration,
    timeout: Duration,
) -> anyhow::Result<Vec<Timed>> {
    let file = upload(request, m4a, timeout)?;
    let heard =
        ready(request, file.clone()).and_then(|file| interact(request, &file, length, timeout));
    remove(request, &file.name);
    heard
}

/// Uploads `m4a` with the Files API's resumable protocol: a start that says what is coming
/// and is answered with where to send it, then the bytes.
fn upload(request: &Request, m4a: &[u8], timeout: Duration) -> anyhow::Result<File> {
    let size = m4a.len().to_string();
    let start = [
        ("x-goog-upload-protocol", "resumable"),
        ("x-goog-upload-command", "start"),
        ("x-goog-upload-header-content-length", size.as_str()),
        ("x-goog-upload-header-content-type", MIME_TYPE),
    ];
    let metadata = json!({"file": {"display_name": "Crumble Recording"}}).to_string();
    let (status, text, address) = post_reading_header(
        &format!("{}/upload/v1beta/files", request.base),
        &request.headers(&start),
        ("application/json", metadata.as_bytes()),
        timeout,
        "x-goog-upload-url",
    )
    .map_err(|error| no_response(&request.address(), request.hint(), timeout, &error))?;
    request.accepted(status, text)?;
    let address = (address.as_deref())
        .and_then(|address| on_base(address, request.base))
        .with_context(|| request.unreadable())?;
    let bytes = [
        ("x-goog-upload-offset", "0"),
        ("x-goog-upload-command", "upload, finalize"),
    ];
    let answer = request.post(&address, &bytes, (MIME_TYPE, m4a), timeout)?;
    file(&answer).with_context(|| request.unreadable())
}

/// `url`, where Google said to send an upload, moved onto `base`, its path included, so the key
/// goes only where the user said requests go: the upload's path after `base`'s, unless a proxy
/// at `base` already put it there.
fn on_base(url: &str, base: &str) -> Option<String> {
    // Where the path starts: at the first `/` after the host.
    let path_at = |url: &str| {
        let host = url.find("://")? + 3;
        Some(url[host..].find('/').map_or(url.len(), |at| host + at))
    };
    let (origin, prefix) = base.split_at(path_at(base)?);
    let path = &url[path_at(url)?..];
    let under_base = prefix.is_empty()
        || (path.strip_prefix(prefix)).is_some_and(|rest| rest.starts_with(['/', '?']));
    Some(if under_base {
        format!("{origin}{path}")
    } else {
        format!("{base}{path}")
    })
}

/// `file` once Google has processed it, checked every [`POLL`] for up to [`PROCESSING`].
fn ready(request: &Request, mut file: File) -> anyhow::Result<File> {
    let name = request.provider.name();
    let started = Instant::now();
    while file.state == "PROCESSING" {
        if started.elapsed() > PROCESSING {
            bail!("{name} took too long to prepare the audio. Try again later.");
        }
        std::thread::sleep(POLL);
        let url = format!("{}/v1beta/{}", request.base, file.name);
        let (status, text) = get(&url, &request.headers(&[]), QUICK)
            .map_err(|error| no_response(&request.address(), request.hint(), QUICK, &error))?;
        file =
            self::file(&request.accepted(status, text)?).with_context(|| request.unreadable())?;
    }
    if file.state == "FAILED" {
        bail!("{name} couldn't read the audio.");
    }
    Ok(file)
}

/// The uploaded file in a Files API answer: an upload's `{file: {...}}`, or a file itself.
/// `None` without a name that addresses a file, or without a URI.
fn file(answer: &str) -> Option<File> {
    let answer: Value = serde_json::from_str(answer).ok()?;
    let file = if answer["file"].is_object() {
        &answer["file"]
    } else {
        &answer
    };
    let name = file["name"].as_str()?;
    let id = name.strip_prefix("files/")?;
    let addressable =
        !id.is_empty() && (id.chars()).all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let uri = file["uri"].as_str().filter(|uri| !uri.is_empty())?;
    addressable.then(|| File {
        name: name.to_owned(),
        uri: uri.to_owned(),
        state: file["state"].as_str().unwrap_or_default().to_owned(),
    })
}

/// The speech in `file`, a piece `length` long, as the transcribe Model hears it.
fn interact(
    request: &Request,
    file: &File,
    length: Duration,
    timeout: Duration,
) -> anyhow::Result<Vec<Timed>> {
    let url = format!("{}/v1beta/interactions", request.base);
    let body = interaction(request.model, &file.uri, &language_tag(request.locale)).to_string();
    let answer = request.post(&url, &[], ("application/json", body.as_bytes()), timeout)?;
    let answer: Value = serde_json::from_str(&answer).with_context(|| request.unreadable())?;
    if matches!(answer["status"].as_str(), Some("failed" | "cancelled")) {
        bail!(
            "{} couldn't transcribe the audio. Try again later.",
            request.provider.name()
        );
    }
    parse(&answer, length).with_context(|| request.unreadable())
}

/// The interaction that transcribes the file at `uri` word for word, with word times, in
/// `language`, a BCP-47 tag, or "" to let the Model tell. `store: false` keeps Google from
/// keeping it.
fn interaction(model: &str, uri: &str, language: &str) -> Value {
    let mut transcription = json!({
        "mode": {"type": "verbatim", "timestamp_granularities": ["word"]},
    });
    if !language.is_empty() {
        transcription["language_codes"] = json!([language]);
    }
    json!({
        "model": model,
        "input": [{"type": "audio", "uri": uri, "mime_type": MIME_TYPE}],
        "generation_config": {"transcription_config": transcription},
        "store": false,
    })
}

/// The speech in an interaction for a piece `length` long, timed within it: the words its
/// text is annotated with, as segments, or, without them, its text as one segment spanning
/// the piece. `None` when it isn't a transcription, or a time is past any Recording.
fn parse(answer: &Value, length: Duration) -> Option<Vec<Timed>> {
    let texts = (answer["steps"].as_array()?.iter())
        .filter(|step| step["type"] == "model_output")
        .flat_map(|step| step["content"].as_array().into_iter().flatten())
        .filter(|content| content["type"] == "text");
    let (mut text, mut words) = (String::new(), Vec::new());
    for content in texts {
        text.push_str(content["text"].as_str().unwrap_or_default());
        let annotations = content["annotations"].as_array().into_iter().flatten();
        for word in annotations.filter(|annotation| annotation["type"] == "word_info") {
            words.push(Timed {
                start: offset(&word["start_offset"])?,
                end: offset(&word["end_offset"])?,
                text: word["text"].as_str().unwrap_or_default().to_owned(),
            });
        }
    }
    Some(segments_or_text(words, &text, length))
}

/// A time as the API writes it, seconds and an `s`: `0.100s`. `None` when it isn't one, or no
/// Recording is that long.
fn offset(value: &Value) -> Option<Duration> {
    let seconds: f64 = value.as_str()?.strip_suffix('s')?.parse().ok()?;
    Duration::try_from_secs_f64(seconds.max(0.0)).ok()
}

/// Deletes the uploaded file, which Google would otherwise keep for 48 hours. A failure is
/// logged: the file goes on its own then.
fn remove(request: &Request, name: &str) {
    let url = format!("{}/v1beta/{name}", request.base);
    match delete(&url, &request.headers(&[]), QUICK) {
        Ok((200..=299, _)) => {}
        Ok((status, _)) => eprintln!("Could not delete the uploaded audio {name} ({status})"),
        Err(error) => eprintln!("Could not delete the uploaded audio {name}: {error}"),
    }
}

#[cfg(test)]
mod tests;
