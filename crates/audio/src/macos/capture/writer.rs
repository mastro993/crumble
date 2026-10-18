//! The writer thread: drains each track's queue into its file, cleaning the microphone
//! against the Mac's sound when both tracks record.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::thread;
use std::time::Duration;

use super::{BATCH, BLOCK, Block, QUEUE, WRITER_NAP};
use crate::macos::echo::Echo;
use crate::macos::file::AacFile;
use crate::{Track, is_silent};

/// A track's file as the writer fills it.
pub(super) struct Written {
    pub(super) track: Track,
    pub(super) blocks: Receiver<Block>,
    /// `None` once stop was signalled, its queue emptied, and the file finalized.
    pub(super) file: Option<AacFile>,
    pub(super) frames: u64,
    pub(super) sample_rate: f64,
    pub(super) heard: Arc<AtomicBool>,
    /// The first write that failed. The file then takes no more, so it holds the audio up to
    /// the failure rather than skipping a gap.
    pub(super) failed: Option<anyhow::Error>,
}

impl Written {
    /// Encodes `samples` into the file, unless an earlier write failed.
    fn write(&mut self, samples: &[f32]) {
        let Some(file) = &mut self.file else {
            return;
        };
        if self.failed.is_some() || samples.is_empty() {
            return;
        }
        match file.write(samples) {
            Ok(()) => self.frames += samples.len() as u64,
            Err(error) => {
                eprintln!("Recording: {:?}: {error:#}", self.track);
                self.failed = Some(error);
            }
        }
    }

    /// Finalizes the file; a failure to close is kept like a failed write.
    fn close(&mut self) {
        if let Some(Err(error)) = self.file.take().map(AacFile::close) {
            eprintln!("Recording: {:?}: {error:#}", self.track);
            self.failed.get_or_insert(error);
        }
    }

    pub(super) fn length(&self) -> Duration {
        Duration::from_secs_f64(self.frames as f64 / self.sample_rate)
    }
}

/// The writer thread: drains each queue into its file until each is empty or closed once
/// `done` is set, then returns. The system queue goes first and is drained in full, as it is
/// the reference the microphone is cleaned against, so the microphone never gives up on a
/// reference still queued; then up to [`BATCH`] microphone blocks.
pub(super) fn write(mut written: Vec<Written>, done: &AtomicBool) -> Vec<Written> {
    written.sort_by_key(|track| track.track == Track::Mic);
    let mut echo = canceller(&written);
    let mut cleaned = Vec::new();
    while written.iter().any(|track| track.file.is_some()) {
        // Read once per pass, so the system track is drained in full before the microphone
        // finishes against it.
        let stopping = done.load(Ordering::Acquire);
        let mut idle = true;
        for track in &mut written {
            if track.file.is_none() {
                continue;
            }
            let batch = match track.track {
                Track::System => QUEUE,
                Track::Mic => BATCH,
            };
            for _ in 0..batch {
                match track.blocks.try_recv() {
                    Ok(block) => {
                        idle = false;
                        let samples = &block.samples[..block.len];
                        if !is_silent(samples) {
                            track.heard.store(true, Ordering::Relaxed);
                        }
                        let at = block.time(track.sample_rate);
                        let samples = match (track.track, &mut echo) {
                            (Track::System, Some(echo)) => {
                                echo.reference(at, samples);
                                samples
                            }
                            // A file that failed takes nothing more, so nothing is cleaned for it.
                            (Track::Mic, Some(echo)) if track.failed.is_none() => {
                                cleaned.clear();
                                echo.mic(at, samples, &mut cleaned);
                                &cleaned
                            }
                            _ => samples,
                        };
                        track.write(samples);
                    }
                    // The captures close their queues before stop is signalled, so until it
                    // shows, a closed queue only ends this pass like an empty one. Once it does,
                    // the system queue, first and taken whole, is drained before the microphone
                    // finishes against it.
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) if !stopping => break,
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
                        if let (Track::Mic, Some(echo)) = (track.track, &mut echo) {
                            cleaned.clear();
                            echo.finish(&mut cleaned);
                            track.write(&cleaned);
                        }
                        track.close();
                        break;
                    }
                }
            }
        }
        if idle {
            thread::sleep(WRITER_NAP);
        }
    }
    written
}

/// The echo canceller for the microphone, when both tracks record.
fn canceller(written: &[Written]) -> Option<Echo> {
    let rate = |of: Track| {
        written
            .iter()
            .find(|track| track.track == of)
            .map(|track| track.sample_rate)
    };
    Echo::new(rate(Track::Mic)?, rate(Track::System)?, QUEUE * BLOCK)
        .inspect_err(|error| {
            eprintln!("Recording: the microphone keeps the Mac's sound: {error:#}")
        })
        .ok()
}
