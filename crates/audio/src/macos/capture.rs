//! Taking a Recording: an IO proc on the default input device for the microphone, and one on a
//! private aggregate device around a process tap for the Mac's own sound. The IO procs copy
//! each cycle into fixed-size blocks and hand them to the writer thread without blocking or
//! allocating; a full queue drops the block and counts it. With both tracks, the writer
//! removes the Mac's sound from the microphone before encoding it (ADR 0005).

mod io_proc;
mod source;
mod writer;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::sync_channel;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail};
use objc2_core_audio::AudioConvertHostTimeToNanos;

use super::file::AacFile;
use crate::{RecorderStatus, Track};
use io_proc::Capture;
use source::Source;
use writer::{Written, write};

/// Frames per block handed to the writer.
const BLOCK: usize = 1024;
/// Blocks the queue holds per track: about 10 s at 48 kHz, allocated once at start.
const QUEUE: usize = 512;
/// Microphone blocks the writer takes before it turns to the system queue, so a busy
/// microphone cannot starve it.
const BATCH: usize = 16;
/// How long the writer sleeps once both queues are empty. It polls, so an IO proc never has to
/// wake it, which could take a lock.
const WRITER_NAP: Duration = Duration::from_millis(20);

/// Up to [`BLOCK`] mono frames, copied by value through the queue.
struct Block {
    len: usize,
    /// The host time of the IO cycle's first frame, or 0 when Core Audio gave none.
    host_time: u64,
    /// Where the block starts in its cycle, in frames.
    offset: usize,
    samples: [f32; BLOCK],
}

impl Block {
    /// The host time of the block's first frame, in seconds.
    fn time(&self, sample_rate: f64) -> Option<f64> {
        if self.host_time == 0 {
            return None;
        }
        // SAFETY: a plain conversion with no preconditions.
        let nanos = unsafe { AudioConvertHostTimeToNanos(self.host_time) };
        Some(nanos as f64 / 1e9 + self.offset as f64 / sample_rate)
    }
}

/// A running Recording. Dropping it stops and finalizes it, as [`Recorder::stop`] does.
pub struct Recorder {
    /// Stopped before the writer is waited for, so the queues close.
    captures: Vec<Capture>,
    writer: Option<JoinHandle<Vec<Written>>>,
    /// Set once the captures are stopped: the writer then closes each file when its queue is
    /// empty, even if an IO proc could not be unregistered and keeps its queue open.
    done: Arc<AtomicBool>,
    /// Whether each track, in [`Track::BOTH`] order, is being recorded.
    recording: [bool; 2],
    /// Whether each track, in [`Track::BOTH`] order, has heard more than silence.
    heard: [Arc<AtomicBool>; 2],
    started: Instant,
}

impl Recorder {
    /// Starts capturing into `dir` (created if missing): the microphone and the system tap, one
    /// file each. A track that cannot open or start is logged and left out, with no file; it
    /// errors only when neither starts. The first tap asks macOS for system audio access, and
    /// the first microphone read for microphone access, unless asked before.
    pub fn start(dir: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let heard = [Arc::default(), Arc::default()];
        let mut tracks = Vec::new();
        for (track, heard) in Track::BOTH.into_iter().zip(&heard) {
            match prepare(track, dir, heard) {
                Ok(prepared) => tracks.push(prepared),
                Err(error) => eprintln!("Recording: no {track:?} track: {error:#}"),
            }
        }
        // Started back to back, so the tracks begin within a few milliseconds of each other.
        tracks.retain_mut(|(_, capture)| {
            capture
                .start()
                .inspect_err(|error| {
                    eprintln!("Recording: no {:?} track: {error:#}", capture.track)
                })
                .is_ok()
        });
        let recording =
            Track::BOTH.map(|track| tracks.iter().any(|(written, _)| written.track == track));
        // A track left out keeps no file.
        for (track, recording) in Track::BOTH.into_iter().zip(recording) {
            if !recording {
                let _ = std::fs::remove_file(track.path(dir));
            }
        }
        if tracks.is_empty() {
            bail!("neither the microphone nor the Mac's sound can be recorded");
        }
        let (written, captures): (Vec<_>, Vec<_>) = tracks.into_iter().unzip();
        let done = Arc::new(AtomicBool::new(false));
        let writer = thread::Builder::new()
            .name("crumble-recording".into())
            .spawn({
                let done = Arc::clone(&done);
                move || write(written, &done)
            })?;
        Ok(Self {
            captures,
            writer: Some(writer),
            done,
            recording,
            heard,
            started: Instant::now(),
        })
    }

    /// Elapsed time and whether each recorded track has heard only digital silence so far.
    pub fn status(&self) -> RecorderStatus {
        let silent =
            |track: usize| self.recording[track] && !self.heard[track].load(Ordering::Relaxed);
        RecorderStatus {
            elapsed: self.started.elapsed(),
            mic_silent: silent(0),
            system_silent: silent(1),
        }
    }

    /// Stops, finalizes the files and returns the Recording's length: its longest track, zero
    /// when no audio arrived. Errors when a track could not be written or closed in full, its
    /// file keeping what was written before.
    pub fn stop(mut self) -> anyhow::Result<Duration> {
        self.finish()
    }

    fn finish(&mut self) -> anyhow::Result<Duration> {
        for capture in self.captures.drain(..) {
            let dropped = capture.feed.dropped.load(Ordering::Relaxed);
            if dropped > 0 {
                eprintln!(
                    "Recording: the {:?} track dropped {dropped} frames",
                    capture.track
                );
            }
        }
        self.done.store(true, Ordering::Release);
        let Some(writer) = self.writer.take() else {
            return Ok(Duration::ZERO);
        };
        let written = writer
            .join()
            .map_err(|_| anyhow!("the Recording's writer panicked"))?;
        if let Some((track, error)) = written
            .iter()
            .find_map(|track| Some((track.track, track.failed.as_ref()?)))
        {
            bail!("the {track:?} track stopped being written: {error:#}");
        }
        Ok(written
            .iter()
            .map(Written::length)
            .max()
            .unwrap_or_default())
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            eprintln!("Recording: {error}");
        }
    }
}

/// A track's file and its IO proc, registered but not started.
fn prepare(
    track: Track,
    dir: &Path,
    heard: &Arc<AtomicBool>,
) -> anyhow::Result<(Written, Capture)> {
    let source = Source::open(track)?;
    let sample_rate = source.sample_rate;
    let file = AacFile::create(&track.path(dir), sample_rate)?;
    let (sender, blocks) = sync_channel(QUEUE);
    let written = Written {
        track,
        blocks,
        file: Some(file),
        frames: 0,
        sample_rate,
        heard: Arc::clone(heard),
        failed: None,
    };
    Ok((written, Capture::new(track, source, sender)?))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc::sync_channel;

    use super::{AacFile, BLOCK, Block, QUEUE, Track, Written, write};
    use crate::macos::tests::folder;

    /// A track whose queue holds `blocks` blocks without host times, then closes, as at stop.
    fn queued(track: Track, dir: &Path, blocks: usize) -> anyhow::Result<Written> {
        let (sender, receiver) = sync_channel(QUEUE);
        for _ in 0..blocks {
            sender.send(Block {
                len: BLOCK,
                host_time: 0,
                offset: 0,
                samples: [0.1; BLOCK],
            })?;
        }
        Ok(Written {
            track,
            blocks: receiver,
            file: Some(AacFile::create(&track.path(dir), 48_000.0)?),
            frames: 0,
            sample_rate: 48_000.0,
            heard: Default::default(),
            failed: None,
        })
    }

    #[test]
    fn stopping_writes_every_microphone_sample_even_past_its_reference() {
        let dir = folder("writer");
        // The microphone runs past its reference but within the canceller's wait, so its tail
        // comes out only when the writer finishes it.
        let written = queued(Track::Mic, &dir, 20)
            .and_then(|mic| Ok(vec![mic, queued(Track::System, &dir, 4)?]))
            .map(|written| write(written, &AtomicBool::new(true)));
        let _ = std::fs::remove_dir_all(&dir);
        let written = written.unwrap_or_else(|error| panic!("{error:#}"));
        let frames = |of: Track| {
            written
                .iter()
                .find(|track| track.track == of)
                .map(|track| (track.frames, track.file.is_none(), track.failed.is_none()))
        };
        assert_eq!(frames(Track::Mic), Some((20 * BLOCK as u64, true, true)));
        assert_eq!(frames(Track::System), Some((4 * BLOCK as u64, true, true)));
    }
}
