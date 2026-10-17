//! Recordings: the microphone and the Mac's own sound, taken as two tracks through Core Audio
//! and written as AAC files that stay playable after a crash, then played back together.
//!
//! The caller picks the folder; this crate names the files in it. Core Audio callbacks never
//! block: they hand fixed-size blocks to a writer thread, which encodes them (ADR 0005).
//! Other platforms record nothing.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod unsupported;

use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(target_os = "macos")]
pub use macos::{Player, Recorder, mic_access, recorded_length, request_mic_access};
#[cfg(not(target_os = "macos"))]
pub use unsupported::{Player, Recorder, mic_access, recorded_length, request_mic_access};

/// Whether Crumble may record the microphone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// The user has not been asked yet.
    NotDetermined,
    /// The user refused; only System Settings can change it.
    Denied,
    /// Crumble may record the microphone.
    Granted,
}

/// How a running Recording is going.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecorderStatus {
    /// Time since the Recording started.
    pub elapsed: Duration,
    /// Whether the microphone track is recorded and has had only digital silence so far.
    pub mic_silent: bool,
    /// Whether the system track is recorded and has had only digital silence so far. macOS
    /// feeds a process tap silence while Crumble may not record system audio, so this stays
    /// true then.
    pub system_silent: bool,
}

/// One of a Recording's two tracks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Track {
    Mic,
    System,
}

impl Track {
    pub(crate) const BOTH: [Track; 2] = [Track::Mic, Track::System];

    /// The track's file in a Recording's folder: `mic.aac` or `system.aac`, AAC in ADTS.
    pub(crate) fn path(self, dir: &Path) -> PathBuf {
        dir.join(match self {
            Track::Mic => "mic.aac",
            Track::System => "system.aac",
        })
    }
}

/// Peaks under this count as digital silence: a muted or denied source delivers exact zeros,
/// and a live microphone never stays this quiet.
const SILENCE: f32 = 1e-6;

/// Whether `samples` are digital silence: every one within [`SILENCE`] of zero.
pub(crate) fn is_silent(samples: &[f32]) -> bool {
    samples.iter().all(|sample| sample.abs() < SILENCE)
}

/// Averages each frame of `samples`, `channels` interleaved values, into one value of `mono`,
/// and returns how many frames it wrote: as many as fit in both.
pub(crate) fn downmix(samples: &[f32], channels: usize, mono: &mut [f32]) -> usize {
    if channels == 0 {
        return 0;
    }
    let frames = (samples.len() / channels).min(mono.len());
    #[allow(
        clippy::cast_precision_loss,
        reason = "a device has a handful of channels"
    )]
    let scale = 1.0 / channels as f32;
    for (out, frame) in mono.iter_mut().zip(samples.chunks_exact(channels)) {
        *out = frame.iter().sum::<f32>() * scale;
    }
    frames
}

#[cfg(test)]
mod tests {
    use super::{Track, downmix, is_silent};
    use std::path::Path;

    #[test]
    fn only_near_zero_samples_are_silent() {
        assert!(is_silent(&[]));
        assert!(is_silent(&[0.0, -0.0, 0.0]));
        assert!(is_silent(&[5e-7, -9e-7]));
        assert!(!is_silent(&[0.0, 1e-6, 0.0]));
        assert!(!is_silent(&[0.0, -0.25]));
    }

    #[test]
    fn downmix_averages_each_frame() {
        let mut mono = [9.0; 4];
        assert_eq!(downmix(&[0.5, 0.25, -1.0], 1, &mut mono), 3);
        assert_eq!(mono, [0.5, 0.25, -1.0, 9.0]);
        assert_eq!(downmix(&[1.0, 0.0, 0.5, 0.5, 2.0], 2, &mut mono), 2);
        assert_eq!(mono[..2], [0.5, 0.5]);
        // Never more frames than `mono` holds, and no channels writes nothing.
        let mut short = [0.0; 1];
        assert_eq!(downmix(&[1.0, 1.0, 1.0, 1.0], 2, &mut short), 1);
        assert_eq!(downmix(&[1.0], 0, &mut short), 0);
    }

    #[test]
    fn each_track_has_its_file() {
        let dir = Path::new("/recordings/note");
        assert_eq!(Track::Mic.path(dir), dir.join("mic.aac"));
        assert_eq!(Track::System.path(dir), dir.join("system.aac"));
    }
}
