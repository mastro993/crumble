//! The Core Audio, AudioToolbox and AVFAudio bridge. Every call is `unsafe`; each block says why
//! it holds.
#![allow(unsafe_code)]

mod capture;
mod echo;
mod file;
mod player;

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::anyhow;
use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_avf_audio::{AVAudioApplication, AVAudioApplicationRecordPermission};

pub use capture::Recorder;
pub use player::Player;

use crate::{Access, Track};

/// Whether Crumble may record the microphone.
pub fn mic_access() -> Access {
    // SAFETY: getters on the shared application object.
    let permission = unsafe { AVAudioApplication::sharedInstance().recordPermission() };
    match permission {
        AVAudioApplicationRecordPermission::Granted => Access::Granted,
        AVAudioApplicationRecordPermission::Denied => Access::Denied,
        _ => Access::NotDetermined,
    }
}

/// Asks macOS for microphone access, showing its prompt the first time, then calls `done` on
/// any thread with whether it was granted.
pub fn request_mic_access(done: impl FnOnce(bool) + Send + 'static) {
    let done = Mutex::new(Some(done));
    let block = RcBlock::new(move |granted: Bool| {
        if let Some(done) = done.lock().ok().and_then(|mut done| done.take()) {
            done(granted.as_bool());
        }
    });
    // SAFETY: the block is valid for this call, and AVFAudio copies it before returning.
    unsafe { AVAudioApplication::requestRecordPermissionWithCompletionHandler(&block) };
}

/// The track `file` of a Recording, such as `mic.aac`, as mono `f32` samples at `sample_rate`:
/// what a Local Model transcribes. An hour at 16 kHz is about 230 MB.
pub fn read_samples(file: &Path, sample_rate: f64) -> anyhow::Result<Vec<f32>> {
    file::read(file, sample_rate)
}

/// How much audio the Recording in `dir` holds: its longest track. Reads what a crash left
/// behind as well as a finished Recording. Zero when the tracks are missing or empty, so the
/// Recording holds nothing; an error when a track exists but cannot be read and no other holds
/// audio, so the files may still hold some and must not be deleted.
pub fn recorded_length(dir: &Path) -> anyhow::Result<Duration> {
    let mut longest = Duration::ZERO;
    let mut unreadable = None;
    for track in Track::BOTH {
        let path = track.path(dir);
        // A file cut before its first packet is empty, and ExtAudioFile cannot open it.
        match std::fs::metadata(&path) {
            Ok(metadata) if metadata.len() > 0 => {}
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                unreadable.get_or_insert(anyhow!("{path:?}: {error}"));
                continue;
            }
            _ => continue,
        }
        match file::length(&path) {
            Ok(length) => longest = longest.max(length),
            Err(error) => {
                eprintln!("Recording: {track:?} track: {error:#}");
                unreadable.get_or_insert(error);
            }
        }
    }
    match unreadable {
        Some(error) if longest.is_zero() => Err(error),
        _ => Ok(longest),
    }
}

#[cfg(test)]
mod tests {
    use super::{Track, file, recorded_length};

    pub(super) fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("crumble-audio-{}-{name}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn a_recording_with_no_audio_is_zero_long() {
        let dir = folder("empty");
        let created =
            file::AacFile::create(&Track::Mic.path(&dir), 48_000.0).and_then(|aac| aac.close());
        let length = recorded_length(&dir);
        let missing = recorded_length(&dir.join("missing"));
        let _ = std::fs::remove_dir_all(&dir);
        if let Err(error) = created {
            panic!("{error:#}");
        }
        assert_eq!(length.ok(), Some(std::time::Duration::ZERO));
        assert_eq!(missing.ok(), Some(std::time::Duration::ZERO));
    }

    #[test]
    fn a_track_that_cannot_be_read_is_an_error_rather_than_empty() {
        let dir = folder("unreadable");
        let written = std::fs::write(Track::System.path(&dir), b"not audio");
        let length = recorded_length(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(written.is_ok());
        assert!(length.is_err(), "{length:?}");
    }
}
