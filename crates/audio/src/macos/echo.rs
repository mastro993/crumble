//! Removing the Mac's own sound from the microphone. On speakers the microphone hears the call
//! too, so the mic track would hold the other side a second time, a little later. The system
//! track is what the Mac played, so it is the reference that WebRTC's echo canceller (AEC3,
//! through `sonora`) subtracts from the microphone, 10 ms at a time (ADR 0005).
//!
//! The writer thread feeds both tracks here as their blocks arrive. Each block carries the host
//! time of its first sample, so a microphone frame is paired with the reference from the same
//! moment, whichever device started first, and across a gap or two clocks drifting apart. The
//! microphone's samples all come out, in order, one for one; only the reference is trimmed or
//! padded with silence to line up.

use std::collections::VecDeque;

use anyhow::anyhow;
use sonora::config::{EchoCanceller, MaxProcessingRate, Pipeline};
use sonora::{AudioProcessing, Config, StreamConfig};

/// How far the reference may sit from the microphone frame before it is trimmed or padded to
/// line up. Within it, AEC3's own delay estimate absorbs the difference.
const TOLERANCE: f64 = 0.002;
/// How long the microphone waits for its reference before it is cleaned against silence, so a
/// stalled tap never holds the microphone back for long.
const MAX_WAIT: f64 = 0.5;
/// How much reference is kept: well past [`MAX_WAIT`], and a bound for when the microphone
/// stops delivering while the tap goes on.
const KEPT: f64 = 2.0;

/// The echo canceller and the samples of each track still waiting to be paired.
pub(super) struct Echo {
    processing: AudioProcessing,
    mic: Stream,
    reference: Stream,
    /// Scratch frames of 10 ms, at each track's rate, allocated once.
    mic_in: Vec<f32>,
    mic_out: Vec<f32>,
    reference_in: Vec<f32>,
    reference_out: Vec<f32>,
    /// Set after the first processing error, which is logged once; the microphone then passes
    /// through as it was recorded.
    failed: bool,
    /// Set once a block came without a host time, which is logged once: such blocks are paired
    /// by count.
    untimed: bool,
}

impl Echo {
    /// An echo canceller for a microphone at `mic_rate` and a reference at `reference_rate`.
    /// Only echo cancellation runs: no noise suppression and no gain control, so the voice
    /// stays as recorded.
    pub(super) fn new(mic_rate: f64, reference_rate: f64) -> anyhow::Result<Self> {
        let (mic, reference) = (hertz(mic_rate)?, hertz(reference_rate)?);
        let config = Config {
            pipeline: Pipeline {
                // Keeps the band above 16 kHz instead of the 32 kHz default.
                maximum_internal_processing_rate: MaxProcessingRate::Rate48kHz,
                ..Pipeline::default()
            },
            echo_canceller: Some(EchoCanceller::default()),
            ..Config::default()
        };
        let (mic_config, reference_config) =
            (StreamConfig::new(mic, 1), StreamConfig::new(reference, 1));
        let (mic_frame, reference_frame) = (mic_config.num_frames(), reference_config.num_frames());
        Ok(Self {
            processing: AudioProcessing::builder()
                .config(config)
                .capture_config(mic_config)
                .render_config(reference_config)
                .build(),
            mic: Stream::new(mic_rate),
            reference: Stream::new(reference_rate),
            mic_in: vec![0.0; mic_frame],
            mic_out: vec![0.0; mic_frame],
            reference_in: vec![0.0; reference_frame],
            reference_out: vec![0.0; reference_frame],
            failed: false,
            untimed: false,
        })
    }

    /// Adds what the Mac played, starting at host time `at` in seconds when known. Only the
    /// last [`KEPT`] seconds are kept.
    pub(super) fn reference(&mut self, at: Option<f64>, samples: &[f32]) {
        self.note_untimed(at);
        self.reference.push(at, samples);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a few seconds of samples"
        )]
        let kept = (KEPT * self.reference.rate) as usize;
        let excess = self.reference.samples.len().saturating_sub(kept);
        self.reference.drop_front(excess);
    }

    /// Adds microphone samples starting at host time `at` in seconds when known, and appends to
    /// `out` the cleaned samples that are ready. Some wait for their reference, at most
    /// [`MAX_WAIT`].
    pub(super) fn mic(&mut self, at: Option<f64>, samples: &[f32], out: &mut Vec<f32>) {
        self.note_untimed(at);
        self.mic.push(at, samples);
        while self.mic.samples.len() >= self.mic_in.len() {
            let waited = self.mic.samples.len() as f64 / self.mic.rate;
            if !self.take_reference(waited >= MAX_WAIT) {
                break;
            }
            self.clean_frame(out);
        }
    }

    /// Appends to `out` everything still waiting, as the microphone stopped: whole frames
    /// cleaned against whatever reference there is, and the last partial frame as it was.
    pub(super) fn finish(&mut self, out: &mut Vec<f32>) {
        while self.mic.samples.len() >= self.mic_in.len() {
            self.take_reference(true);
            self.clean_frame(out);
        }
        out.extend(self.mic.samples.drain(..));
    }

    fn note_untimed(&mut self, at: Option<f64>) {
        if at.is_none() && !self.untimed {
            eprintln!(
                "Recording: a block came without a host time; the tracks are paired by count"
            );
            self.untimed = true;
        }
    }

    /// Fills `reference_in` with the reference for the microphone's next frame. Returns false
    /// to wait for more, unless `now`, which pads what is missing with silence.
    fn take_reference(&mut self, now: bool) -> bool {
        let (rate, frame) = (self.reference.rate, self.reference_in.len());
        let ahead = match (self.reference.front_time(), self.mic.front_time()) {
            (Some(reference), Some(mic)) => reference - mic,
            _ => 0.0,
        };
        if ahead < -TOLERANCE {
            // The reference starts before this frame: what came earlier has no echo left in it.
            let late = (-ahead * rate).round() as usize;
            self.reference.drop_front(late);
            if self.reference.samples.len() < frame && !now {
                return false;
            }
        }
        // The reference starts after this frame, as when the tap started later or had a gap:
        // nothing played then, so the frame begins with silence.
        let silence = if ahead > TOLERANCE {
            ((ahead * rate).round() as usize).min(frame)
        } else {
            0
        };
        let wanted = frame - silence;
        if self.reference.samples.len() < wanted && !now {
            return false;
        }
        self.reference_in[..silence].fill(0.0);
        let taken = self
            .reference
            .take(wanted, &mut self.reference_in[silence..]);
        self.reference_in[silence + taken..].fill(0.0);
        true
    }

    /// Feeds `reference_in`, then cleans the microphone's next frame and appends it to `out`.
    fn clean_frame(&mut self, out: &mut Vec<f32>) {
        self.mic.take(self.mic_in.len(), &mut self.mic_in);
        if !self.failed {
            let processed = self
                .processing
                .process_render_f32(&[&self.reference_in], &mut [&mut self.reference_out])
                .and_then(|()| {
                    self.processing
                        .process_capture_f32(&[&self.mic_in], &mut [&mut self.mic_out])
                });
            match processed {
                Ok(()) => {
                    out.extend_from_slice(&self.mic_out);
                    return;
                }
                Err(error) => {
                    eprintln!("Recording: echo cancellation stopped: {error}");
                    self.failed = true;
                }
            }
        }
        out.extend_from_slice(&self.mic_in);
    }
}

/// One track's samples waiting to be paired, with the host time of each block that brought
/// them, so a frame's time stays known across a gap or a drifting clock.
struct Stream {
    rate: f64,
    samples: VecDeque<f32>,
    /// Each block's first sample, counted from the start, and its host time in seconds.
    anchors: VecDeque<(u64, f64)>,
    /// Samples taken from the front so far.
    taken: u64,
}

impl Stream {
    fn new(rate: f64) -> Self {
        Self {
            rate,
            samples: VecDeque::new(),
            anchors: VecDeque::new(),
            taken: 0,
        }
    }

    fn push(&mut self, at: Option<f64>, samples: &[f32]) {
        if let Some(at) = at {
            self.anchors
                .push_back((self.taken + self.samples.len() as u64, at));
        }
        self.samples.extend(samples);
    }

    /// The host time of the front sample, from the last block that started at or before it.
    fn front_time(&mut self) -> Option<f64> {
        while self.anchors.len() > 1 && self.anchors[1].0 <= self.taken {
            self.anchors.pop_front();
        }
        let &(index, at) = self.anchors.front()?;
        Some(at + (self.taken as f64 - index as f64) / self.rate)
    }

    /// Moves up to `into.len()` samples from the front into `into`; returns how many.
    fn take(&mut self, count: usize, into: &mut [f32]) -> usize {
        let count = count.min(into.len()).min(self.samples.len());
        for (slot, sample) in into.iter_mut().zip(self.samples.drain(..count)) {
            *slot = sample;
        }
        self.taken += count as u64;
        count
    }

    fn drop_front(&mut self, count: usize) {
        let count = count.min(self.samples.len());
        self.samples.drain(..count);
        self.taken += count as u64;
    }
}

/// A device's nominal rate as the whole hertz the canceller takes: a multiple of 100, so its
/// 10 ms frames hold whole samples and keep time with the other track's.
fn hertz(rate: f64) -> anyhow::Result<u32> {
    if rate % 100.0 != 0.0 || !(8_000.0..=384_000.0).contains(&rate) {
        return Err(anyhow!("no echo cancellation at {rate} Hz"));
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a whole number in range"
    )]
    Ok(rate as u32)
}

#[cfg(test)]
mod tests {
    use super::{Echo, KEPT, MAX_WAIT};

    const RATE: f64 = 48_000.0;
    /// Samples per block, as an IO proc delivers them.
    const BLOCK: usize = 512;
    /// 40 ms from the speakers to the microphone.
    const DELAY: usize = 1_920;

    /// Noise in [-0.5, 0.5) from a fixed seed, standing in for a voice.
    fn noise(len: usize, seed: u32) -> Vec<f32> {
        let mut state = seed;
        (0..len)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 8) as f32 / (1 << 24) as f32 - 0.5
            })
            .collect()
    }

    fn energy(samples: &[f32]) -> f32 {
        samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32
    }

    /// How much quieter `out` is than `mic` over the last 5 s, in dB.
    fn reduction(mic: &[f32], out: &[f32]) -> f32 {
        let tail = mic.len() - 48_000 * 5..;
        10.0 * (energy(&mic[tail.clone()]) / energy(&out[tail])).log10()
    }

    fn echo() -> Echo {
        Echo::new(RATE, RATE).unwrap_or_else(|error| panic!("{error:#}"))
    }

    /// How a test hands blocks over: when the reference starts, which blocks get lost, and
    /// whether they carry host times.
    #[derive(Clone, Copy)]
    struct Delivery {
        /// The reference starts this many seconds after the microphone.
        reference_start: f64,
        /// Whether block `index` of the microphone (`true`) or of the reference arrives.
        arrives: fn(bool, usize) -> bool,
        timed: bool,
    }

    const ALL: Delivery = Delivery {
        reference_start: 0.0,
        arrives: |_, _| true,
        timed: true,
    };

    /// The blocks of `samples` that arrive, with their host times.
    fn blocks(
        samples: &[f32],
        is_mic: bool,
        start: f64,
        delivery: Delivery,
    ) -> Vec<(f64, bool, &[f32])> {
        samples
            .chunks(BLOCK)
            .enumerate()
            .filter(|(index, _)| (delivery.arrives)(is_mic, *index))
            .map(|(index, block)| (start + (index * BLOCK) as f64 / RATE, is_mic, block))
            .collect()
    }

    /// Feeds both tracks block by block in the order their host times put them, and returns the
    /// cleaned microphone and the microphone as it arrived.
    fn run(echo: &mut Echo, mic: &[f32], reference: &[f32], delivery: Delivery) -> [Vec<f32>; 2] {
        let mut all = blocks(reference, false, delivery.reference_start, delivery);
        all.extend(blocks(mic, true, 0.0, delivery));
        all.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let (mut out, mut arrived) = (Vec::new(), Vec::new());
        for (at, is_mic, block) in all {
            let at = delivery.timed.then_some(at);
            if is_mic {
                arrived.extend_from_slice(block);
                echo.mic(at, block, &mut out);
            } else {
                echo.reference(at, block);
            }
        }
        echo.finish(&mut out);
        [out, arrived]
    }

    /// The microphone hears the far end [`DELAY`] after the Mac plays it, at half the level.
    /// The tap's first block comes `offset` samples after the microphone's, or before it when
    /// negative. Returns the cleaned microphone's reduction in dB over the last 5 s.
    fn cancelled(offset: isize, delivery: Delivery) -> f32 {
        // One second of the call before the microphone starts.
        let lead = 48_000;
        let far = noise(lead + 48_000 * 10, 3);
        let mic: Vec<f32> = (lead..far.len())
            .map(|index| far[index - DELAY] * 0.5)
            .collect();
        let first = lead.checked_add_signed(offset).unwrap_or(0);
        let delivery = Delivery {
            reference_start: offset as f64 / RATE,
            ..delivery
        };
        let [out, arrived] = run(&mut echo(), &mic, &far[first..], delivery);
        // Every microphone sample comes out, one for one.
        assert_eq!(out.len(), arrived.len());
        reduction(&arrived, &out)
    }

    fn assert_cancelled(reduction: f32) {
        assert!(reduction > 20.0, "only {reduction:.1} dB quieter");
    }

    #[test]
    fn the_macs_sound_is_removed_from_the_microphone() {
        assert_cancelled(cancelled(0, ALL));
    }

    #[test]
    fn a_tap_that_started_first_is_lined_up() {
        // Paired by count instead of by time, this reference would trail the echo by 260 ms.
        assert_cancelled(cancelled(-14_400, ALL));
    }

    #[test]
    fn a_tap_that_started_later_is_lined_up() {
        assert_cancelled(cancelled(14_400, ALL));
    }

    #[test]
    fn a_gap_in_the_tap_keeps_the_rest_lined_up() {
        // About 200 ms of the tap never arrive, 2 s in.
        let gap = Delivery {
            arrives: |is_mic, index| is_mic || !(190..210).contains(&index),
            ..ALL
        };
        assert_cancelled(cancelled(0, gap));
    }

    #[test]
    fn a_gap_in_the_microphone_keeps_the_rest_lined_up() {
        // About 200 ms of the microphone are lost, as when its queue was full.
        let gap = Delivery {
            arrives: |is_mic, index| !is_mic || !(190..210).contains(&index),
            ..ALL
        };
        assert_cancelled(cancelled(0, gap));
    }

    #[test]
    fn blocks_without_host_times_are_paired_by_count() {
        let untimed = Delivery {
            timed: false,
            ..ALL
        };
        assert_cancelled(cancelled(0, untimed));
    }

    #[test]
    fn a_voice_with_no_echo_stays() {
        let near = noise(48_000 * 6, 11);
        let silence = vec![0.0; near.len()];
        let [out, _] = run(&mut echo(), &near, &silence, ALL);
        let change = reduction(&near, &out);
        assert!(change.abs() < 1.0, "{change:.1} dB");
    }

    #[test]
    fn the_microphone_waits_for_its_reference_only_so_long() {
        let mut echo = echo();
        let mic = noise(48_000, 5);
        let mut out = Vec::new();
        for (index, block) in mic.chunks(BLOCK).enumerate() {
            echo.mic(Some((index * BLOCK) as f64 / RATE), block, &mut out);
        }
        // No reference ever came: all but about the last half second is out already.
        let held = mic.len() - out.len();
        assert!(
            held as f64 <= MAX_WAIT * RATE + BLOCK as f64,
            "{held} samples held"
        );
        echo.finish(&mut out);
        assert_eq!(out.len(), mic.len());
    }

    #[test]
    fn the_reference_is_bounded_when_the_microphone_stops() {
        let mut echo = echo();
        for (index, block) in noise(48_000 * 10, 9).chunks(BLOCK).enumerate() {
            echo.reference(Some((index * BLOCK) as f64 / RATE), block);
        }
        assert!(echo.reference.samples.len() as f64 <= KEPT * RATE);
    }

    #[test]
    fn unsupported_rates_are_refused() {
        assert!(Echo::new(44_100.5, RATE).is_err());
        assert!(Echo::new(RATE, 1_000.0).is_err());
        // 10 ms of 22 050 Hz is not a whole number of samples.
        assert!(Echo::new(22_050.0, RATE).is_err());
        assert!(Echo::new(44_100.0, RATE).is_ok());
    }
}
