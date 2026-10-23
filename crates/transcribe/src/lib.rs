//! Transcripts of Recordings, made by a Model the user picks: Apple's SpeechAnalyzer on the Mac
//! through a small Swift bridge (ADR 0006), a Local Model Crumble downloaded, run through
//! transcribe.cpp in a worker process (ADR 0010), or a remote Provider's Model, through the API
//! its `crumble_remote::Speech` names (ADRs 0008 and 0011). Each is a [`Transcriber`]. Each
//! track is transcribed on its own, so the track labels the speech: the microphone is the user,
//! the Mac's sound is the others on the call. No GPUI.

mod adts;
mod apple;
mod chunks;
#[cfg(target_os = "macos")]
mod ffi;
#[cfg(not(target_os = "macos"))]
#[path = "unsupported.rs"]
mod ffi;
mod local;
mod remote;
#[cfg(target_os = "macos")]
mod worker;

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::bail;
use crumble_remote::Provider;

pub use apple::Apple;
pub use ffi::{current_locale, install_model, language_name, model_status, supported_locales};
pub use local::Local;
pub use remote::Remote;
#[cfg(target_os = "macos")]
pub use worker::run_if_asked;

/// The locale that asks a Model to tell the language itself, for "Detect automatically".
pub const DETECT: &str = "auto";

#[cfg(target_os = "macos")]
use worker::FLAG as WORKER_FLAG;
#[cfg(not(target_os = "macos"))]
const WORKER_FLAG: &str = "--transcription-worker";

/// Never a worker on this platform, as Local Models run only on macOS.
#[cfg(not(target_os = "macos"))]
pub fn run_if_asked() -> Option<std::process::ExitCode> {
    None
}

/// A Model that turns a Recording into a Transcript.
pub trait Transcriber {
    /// Who runs the Model.
    fn provider(&self) -> Provider;
    /// The Model, as the Transcript records it: its id, or "Apple SpeechAnalyzer" for Apple's.
    fn model(&self) -> &str;
    /// The Transcript of the Recording in `dir`, in `locale` such as `en_US`: each of its tracks
    /// that exists, merged in time order. Blocks, so call it off the main thread; `progress`
    /// gets 0.0..=1.0, possibly from another thread while this one waits. Its error is a
    /// sentence to show the user, and never holds an API key.
    fn transcribe(
        &self,
        dir: &Path,
        locale: &str,
        progress: &mut (dyn FnMut(f32) + Send),
    ) -> anyhow::Result<Vec<Segment>>;
}

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

/// The tracks of the Recording in `dir` that exist, the microphone first.
fn tracks(dir: &Path) -> anyhow::Result<Vec<(Track, PathBuf)>> {
    let tracks: Vec<(Track, PathBuf)> = [Track::Mic, Track::System]
        .into_iter()
        .map(|track| (track, track.path(dir)))
        .filter(|(_, path)| path.exists())
        .collect();
    if tracks.is_empty() {
        bail!("the Recording in {} has no audio", dir.display());
    }
    Ok(tracks)
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
