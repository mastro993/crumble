//! Platforms without Core Audio taps: nothing records or plays.

use std::path::Path;
use std::time::Duration;

use anyhow::bail;

use crate::{Access, RecorderStatus};

/// Never starts on this platform.
pub struct Recorder;

impl Recorder {
    /// Always errors.
    pub fn start(_dir: &Path) -> anyhow::Result<Self> {
        bail!("recording needs macOS")
    }

    /// Never reached, as nothing starts.
    pub fn status(&self) -> RecorderStatus {
        RecorderStatus {
            elapsed: Duration::ZERO,
            mic_silent: false,
            system_silent: false,
        }
    }

    /// Never reached, as nothing starts.
    pub fn stop(self) -> anyhow::Result<Duration> {
        Ok(Duration::ZERO)
    }
}

/// Never opens on this platform.
pub struct Player;

impl Player {
    /// Always errors.
    pub fn open(_dir: &Path) -> anyhow::Result<Self> {
        bail!("playback needs macOS")
    }
    /// Does nothing.
    pub fn play(&mut self) {}
    /// Does nothing.
    pub fn pause(&mut self) {}
    /// Does nothing.
    pub fn seek(&mut self, _to: Duration) {}
    /// Always zero.
    pub fn position(&self) -> Duration {
        Duration::ZERO
    }
    /// Always zero.
    pub fn duration(&self) -> Duration {
        Duration::ZERO
    }
    /// Always false.
    pub fn is_playing(&self) -> bool {
        false
    }
}

/// Always [`Access::Denied`].
pub fn mic_access() -> Access {
    Access::Denied
}

/// Calls `done(false)` right away.
pub fn request_mic_access(done: impl FnOnce(bool) + Send + 'static) {
    done(false);
}

/// Always errors.
pub fn read_samples(_file: &Path, _sample_rate: f64) -> anyhow::Result<Vec<f32>> {
    bail!("reading audio needs macOS")
}

/// Always errors.
pub fn recorded_length(_dir: &Path) -> anyhow::Result<Duration> {
    bail!("recordings need macOS")
}
