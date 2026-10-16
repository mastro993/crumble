//! Playing a Recording: one `AVAudioEngine` with a player node per track, both scheduled from
//! the same position and started at the same host time, so they stay in step (ADR 0005).
//! `AVAudioFile` counts ADTS frames exactly; `AVAudioPlayer` guesses an ADTS file's length from
//! its bit rate, which made a 10-minute file 13 minutes long.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, bail};
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_avf_audio::{AVAudioEngine, AVAudioFile, AVAudioPlayerNode, AVAudioTime};
use objc2_core_audio::AudioGetCurrentHostTime;
use objc2_foundation::NSURL;

use crate::Track;

/// How far ahead both tracks are scheduled, so the second starts on the same host time.
const START_AHEAD: f64 = 0.05;

/// One track: its node and its file.
struct Playing {
    node: Retained<AVAudioPlayerNode>,
    file: Retained<AVAudioFile>,
    sample_rate: f64,
}

impl Playing {
    #[allow(
        clippy::cast_precision_loss,
        reason = "frames of hours of audio fit in f64"
    )]
    fn length(&self) -> f64 {
        // SAFETY: a getter on an open file.
        let frames = unsafe { self.file.length() };
        frames as f64 / self.sample_rate
    }
}

/// Plays a Recording's tracks together.
pub struct Player {
    engine: Retained<AVAudioEngine>,
    tracks: Vec<Playing>,
    /// Where playback last started or was moved to, in seconds.
    from: f64,
    playing: bool,
}

impl Player {
    /// Opens the Recording in `dir`: whichever of its track files are there and readable.
    pub fn open(dir: &Path) -> anyhow::Result<Self> {
        // SAFETY: a new engine; nodes are attached before they are connected.
        let engine = unsafe { AVAudioEngine::new() };
        let mut tracks = Vec::new();
        for track in Track::BOTH {
            let path = track.path(dir);
            if !path.exists() {
                continue;
            }
            let url = NSURL::from_file_path(&path).ok_or_else(|| anyhow!("bad path {path:?}"))?;
            // SAFETY: a file URL; failure comes back as an error.
            let file =
                match unsafe { AVAudioFile::initForReading_error(AVAudioFile::alloc(), &url) } {
                    Ok(file) => file,
                    Err(error) => {
                        eprintln!("Recording: cannot play {path:?}: {error}");
                        continue;
                    }
                };
            // SAFETY: getters on the open file, and a node attached to this engine.
            unsafe {
                let format = file.processingFormat();
                let node = AVAudioPlayerNode::new();
                engine.attachNode(&node);
                engine.connect_to_format(&node, &engine.mainMixerNode(), Some(&format));
                tracks.push(Playing {
                    node,
                    file,
                    sample_rate: format.sampleRate(),
                });
            }
        }
        if tracks.is_empty() {
            bail!("no playable track in {dir:?}");
        }
        // SAFETY: a configured engine.
        unsafe { engine.prepare() };
        Ok(Self {
            engine,
            tracks,
            from: 0.0,
            playing: false,
        })
    }

    /// Plays from the current position, from the start once it reached the end.
    pub fn play(&mut self) {
        let from = self.seconds();
        self.halt();
        self.from = if from >= self.length() { 0.0 } else { from };
        // SAFETY: the engine and its attached nodes are live; segments stay within each file.
        unsafe {
            if let Err(error) = self.engine.startAndReturnError() {
                eprintln!("Recording: cannot play: {error}");
                return;
            }
            let at = AVAudioTime::timeWithHostTime(
                AudioGetCurrentHostTime() + AVAudioTime::hostTimeForSeconds(START_AHEAD),
            );
            for track in &self.tracks {
                #[allow(clippy::cast_possible_truncation, reason = "a frame index")]
                let start = (self.from * track.sample_rate) as i64;
                let left = u32::try_from(track.file.length() - start).unwrap_or(0);
                if left > 0 {
                    track
                        .node
                        .scheduleSegment_startingFrame_frameCount_atTime_completionHandler(
                            &track.file,
                            start,
                            left,
                            None,
                            std::ptr::null_mut(),
                        );
                }
                track.node.playAtTime(Some(&at));
            }
        }
        self.playing = true;
    }

    /// Pauses at the current position.
    pub fn pause(&mut self) {
        self.from = self.seconds();
        self.halt();
    }

    /// Moves both tracks to `to`, keeping on playing if they were.
    pub fn seek(&mut self, to: Duration) {
        let playing = self.is_playing();
        self.halt();
        self.from = to.as_secs_f64().min(self.length());
        if playing {
            self.play();
        }
    }

    /// How far playback is.
    pub fn position(&self) -> Duration {
        Duration::from_secs_f64(self.seconds())
    }

    /// The longest track's length.
    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.length())
    }

    /// Whether the tracks are playing; false again once they reach the end.
    pub fn is_playing(&self) -> bool {
        self.playing && self.seconds() < self.length()
    }

    fn length(&self) -> f64 {
        self.tracks.iter().map(Playing::length).fold(0.0, f64::max)
    }

    /// Where playback is: where it started plus how far the first node has played since.
    fn seconds(&self) -> f64 {
        if !self.playing {
            return self.from;
        }
        let node = &self.tracks[0].node;
        // SAFETY: getters on a live node; both are nil until the node has rendered.
        let played = unsafe {
            node.lastRenderTime()
                .and_then(|time| node.playerTimeForNodeTime(&time))
                .map_or(0.0, |time| {
                    #[allow(clippy::cast_precision_loss, reason = "a frame count")]
                    let frames = time.sampleTime() as f64;
                    frames / time.sampleRate()
                })
        };
        (self.from + played.max(0.0)).min(self.length())
    }

    /// Stops every node and the engine, so nothing keeps running while paused.
    fn halt(&mut self) {
        // SAFETY: live nodes and engine; stopping also drops what was scheduled.
        unsafe {
            for track in &self.tracks {
                track.node.stop();
            }
            self.engine.pause();
        }
        self.playing = false;
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.halt();
        // SAFETY: a live engine, not used again.
        unsafe { self.engine.stop() };
    }
}
