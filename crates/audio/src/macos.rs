//! The Core Audio, AudioToolbox and AVFAudio bridge. Every call is `unsafe`; each block says why
//! it holds.
#![allow(unsafe_code)]

mod capture;
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

/// How much audio the Recording in `dir` holds: its longest readable track. Reads what a crash
/// left behind as well as a finished Recording.
pub fn recorded_length(dir: &Path) -> anyhow::Result<Duration> {
    Track::BOTH
        .into_iter()
        .filter_map(|track| {
            file::length(&track.path(dir))
                .inspect_err(|error| eprintln!("Recording: {track:?} track: {error}"))
                .ok()
        })
        .max()
        .ok_or_else(|| anyhow!("no readable track in {dir:?}"))
}
