//! Transcripts of Recordings, made on the Mac by Apple's SpeechAnalyzer through a small Swift
//! bridge (ADR 0006). Each track is transcribed on its own, so the track labels the speech: the
//! microphone is the user, the Mac's sound is the others on the call. No GPUI.

#[cfg(target_os = "macos")]
mod ffi;
#[cfg(not(target_os = "macos"))]
#[path = "unsupported.rs"]
mod ffi;

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, anyhow, bail};

pub use ffi::{current_locale, install_model, language_name, model_status};

/// The Model this crate transcribes with, as a Transcript stores it.
pub const MODEL: &str = "Apple SpeechAnalyzer";

/// One stretch of speech in a Recording.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    /// The track it was heard on, which says who spoke.
    pub track: Track,
    /// Where it starts in the Recording.
    pub start: Duration,
    /// Where it ends in the Recording.
    pub end: Duration,
    /// What was said.
    pub text: String,
}

/// One of a Recording's two tracks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Track {
    /// The microphone: the user's side.
    Mic,
    /// The Mac's sound: the call's side.
    System,
}

impl Track {
    /// The track's file in a Recording's folder, as `crumble-audio` names it.
    fn path(self, dir: &Path) -> PathBuf {
        dir.join(match self {
            Track::Mic => "mic.aac",
            Track::System => "system.aac",
        })
    }
}

/// Whether a locale's speech model is on this Mac.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelStatus {
    /// Installed: transcribing can start.
    Installed,
    /// Apple's model for it is downloading.
    Downloading,
    /// Apple has a model for it, not yet downloaded; [`install_model`] gets it.
    Downloadable,
    /// This Mac cannot transcribe it.
    Unsupported,
}

/// Transcribes the Recording in `dir`: each of its tracks that exists, merged in time order.
/// The locale's model must be installed. Blocks, so call it off the main thread; `progress`
/// gets 0.0..=1.0, from another thread while this one waits.
pub fn transcribe(
    dir: &Path,
    locale: &str,
    mut progress: impl FnMut(f32) + Send,
) -> anyhow::Result<Vec<Segment>> {
    let tracks: Vec<(Track, PathBuf)> = [Track::Mic, Track::System]
        .into_iter()
        .map(|track| (track, track.path(dir)))
        .filter(|(_, path)| path.exists())
        .collect();
    if tracks.is_empty() {
        bail!("the Recording in {} has no audio", dir.display());
    }
    #[allow(clippy::cast_precision_loss, reason = "at most two tracks")]
    let count = tracks.len() as f32;
    let mut segments = Vec::new();
    for (done, (track, path)) in tracks.iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "at most two tracks")]
        let done = done as f32;
        // Both tracks run the whole Recording, so each is an equal share of the work.
        let lines = ffi::transcribe_file(path, locale, &mut |fraction| {
            progress((done + fraction) / count);
        })
        .with_context(|| format!("could not transcribe {}", path.display()))?;
        segments.push(parse(&lines, *track)?);
    }
    Ok(merge(segments))
}

/// Reads the bridge's output: one `start_ms\tend_ms\ttext` line per segment of `track`.
fn parse(lines: &str, track: Track) -> anyhow::Result<Vec<Segment>> {
    lines
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.splitn(3, '\t');
            let mut time = || -> anyhow::Result<Duration> {
                let ms = fields
                    .next()
                    .ok_or_else(|| anyhow!("a short line {line:?}"))?;
                Ok(Duration::from_millis(
                    ms.parse()
                        .with_context(|| format!("a bad time in {line:?}"))?,
                ))
            };
            let (start, end) = (time()?, time()?);
            let text = fields
                .next()
                .ok_or_else(|| anyhow!("no text in {line:?}"))?;
            Ok(Segment {
                track,
                start,
                end,
                text: text.trim().to_owned(),
            })
        })
        .collect()
}

/// Every track's segments in one list ordered by start, then end, then the microphone first;
/// overlapping speech stays as two segments.
fn merge(tracks: Vec<Vec<Segment>>) -> Vec<Segment> {
    let mut segments: Vec<Segment> = tracks.into_iter().flatten().collect();
    segments.sort_by_key(|segment| (segment.start, segment.end, segment.track));
    segments
}

#[cfg(test)]
mod tests;
