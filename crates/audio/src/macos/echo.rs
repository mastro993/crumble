//! Removing the Mac's own sound from the microphone. On speakers the microphone hears the call
//! too, so the mic track would hold the other side a second time, a little later. The system
//! track is what the Mac played, so it is the reference whose echo WebRTC's echo canceller (AEC3,
//! from `sonora`) subtracts from the microphone, 10 ms at a time (ADR 0005).
//!
//! The writer thread feeds both tracks here as their blocks arrive. Each block carries the host
//! time of its first sample, so a microphone frame is paired with the reference from the same
//! moment, [`LEAD`] on, whichever device started first, and across a gap or two clocks drifting
//! apart. The microphone's samples all come out, in order, one for one; only the reference is
//! trimmed or padded with silence to line up.

mod linear;
#[cfg(test)]
mod tests;

use std::collections::VecDeque;

use anyhow::anyhow;
use sonora_common_audio::audio_util::MAX_SAMPLE_RATE_HZ;

use linear::Linear;

/// How much later than the microphone frame the reference it is paired with starts. The echo
/// filter only finds echo that comes after its reference, so this keeps it there even if the
/// tap stamps its blocks a little after the Mac plays them.
const LEAD: f64 = 0.05;
/// How far the reference may drift from where [`LEAD`] puts it before it is trimmed or padded to
/// line up: well above any jitter in the host times. Within it, AEC3's own delay estimate
/// follows.
const TOLERANCE: f64 = 0.01;
/// How long the microphone waits for its reference before it is cleaned against silence, so a
/// stalled tap never holds the microphone back for long.
const MAX_WAIT: f64 = 0.5;

/// The echo canceller and the samples of each track still waiting to be paired.
pub(super) struct Echo {
    linear: Linear,
    mic: Stream,
    reference: Stream,
    /// Scratch frames of 10 ms, at each track's rate, allocated once.
    mic_in: Vec<f32>,
    mic_out: Vec<f32>,
    reference_in: Vec<f32>,
    /// The most reference samples kept: what the writer may drain at once, plus [`MAX_WAIT`].
    kept: usize,
    /// Set once a block came without a host time, which is logged once: such blocks are paired
    /// by count.
    untimed: bool,
}

impl Echo {
    /// An echo canceller for a microphone at `mic_rate` and a reference at `reference_rate`,
    /// of which the writer may hand over up to `queued` samples at once, as after a stall. Only
    /// AEC3 runs, and below 8 kHz only its echo filter ([`linear`]): no noise suppression and no
    /// gain control, so the voice stays as recorded.
    pub(super) fn new(mic_rate: f64, reference_rate: f64, queued: usize) -> anyhow::Result<Self> {
        let (mic, reference) = (hertz(mic_rate)?, hertz(reference_rate)?);
        Ok(Self {
            linear: Linear::new(mic, reference),
            mic: Stream::new(mic_rate),
            reference: Stream::new(reference_rate),
            mic_in: vec![0.0; mic / 100],
            mic_out: vec![0.0; mic / 100],
            reference_in: vec![0.0; reference / 100],
            untimed: false,
            // Enough that a full queue, drained before the microphone, loses none the microphone
            // still needs; and a bound for when the microphone stops while the tap goes on.
            kept: queued + (MAX_WAIT * reference_rate) as usize,
        })
    }

    /// Adds what the Mac played, starting at host time `at` in seconds when known. Only the
    /// newest `kept` samples stay.
    pub(super) fn reference(&mut self, at: Option<f64>, samples: &[f32]) {
        self.note_untimed(at);
        self.reference.push(at, samples);
        let excess = self.reference.samples.len().saturating_sub(self.kept);
        self.reference.drop_front(excess);
    }

    /// Adds microphone samples starting at host time `at` in seconds when known, and appends to
    /// `out` the cleaned samples that are ready. Some wait for their reference, at most
    /// [`MAX_WAIT`].
    pub(super) fn mic(&mut self, at: Option<f64>, samples: &[f32], out: &mut Vec<f32>) {
        self.note_untimed(at);
        self.mic.push(at, samples);
        self.drain(false, out);
    }

    /// Appends to `out` everything still waiting, as the microphone stopped, cleaned against
    /// whatever reference there is. The last partial frame is padded with silence to be cleaned
    /// whole, so the end stays in step with what came before, and the padding is left out.
    pub(super) fn finish(&mut self, out: &mut Vec<f32>) {
        let waiting = self.mic.samples.len();
        let padding = waiting.next_multiple_of(self.mic_in.len()) - waiting;
        self.mic.samples.extend(std::iter::repeat_n(0.0, padding));
        self.drain(true, out);
        out.truncate(out.len() - padding);
    }

    /// Cleans each whole microphone frame whose reference is in, or has waited [`MAX_WAIT`];
    /// every whole frame when `stopping`.
    fn drain(&mut self, stopping: bool, out: &mut Vec<f32>) {
        while self.mic.samples.len() >= self.mic_in.len() {
            let waited = self.mic.samples.len() as f64 / self.mic.rate;
            if !self.take_reference(stopping || waited >= MAX_WAIT) {
                return;
            }
            self.clean_frame(out);
        }
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
            (Some(reference), Some(mic)) => reference - mic - LEAD,
            _ => 0.0,
        };
        if ahead < -TOLERANCE {
            // The reference starts earlier than this frame needs, as when the tap started first:
            // what came before is dropped.
            let late = (-ahead * rate).round() as usize;
            self.reference.drop_front(late);
        }
        // The reference starts later than this frame needs, as when the tap started later or had
        // a gap: nothing played then, so the frame begins with silence.
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
            .take(&mut self.reference_in[silence..silence + wanted]);
        self.reference_in[silence + taken..].fill(0.0);
        true
    }

    /// Cleans the microphone's next frame against `reference_in` and appends it to `out`.
    fn clean_frame(&mut self, out: &mut Vec<f32>) {
        self.mic.take(&mut self.mic_in);
        self.linear
            .process(&self.reference_in, &self.mic_in, &mut self.mic_out);
        out.extend_from_slice(&self.mic_out);
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
            self.forget_anchors();
        }
        self.samples.extend(samples);
    }

    /// The host time of the front sample, from the last block that started at or before it.
    fn front_time(&self) -> Option<f64> {
        let &(index, at) = self.anchors.front()?;
        Some(at + (self.taken as f64 - index as f64) / self.rate)
    }

    /// Moves samples from the front into `into`, as many as fit or are there; returns how many.
    fn take(&mut self, into: &mut [f32]) -> usize {
        let count = into.len().min(self.samples.len());
        for (slot, sample) in into.iter_mut().zip(self.samples.drain(..count)) {
            *slot = sample;
        }
        self.advance(count);
        count
    }

    fn drop_front(&mut self, count: usize) {
        let count = count.min(self.samples.len());
        self.samples.drain(..count);
        self.advance(count);
    }

    fn advance(&mut self, count: usize) {
        self.taken += count as u64;
        self.forget_anchors();
    }

    /// Forgets the anchors no sample left needs: only the last one at or before the front stays.
    fn forget_anchors(&mut self) {
        while self.anchors.len() > 1 && self.anchors[1].0 <= self.taken {
            self.anchors.pop_front();
        }
    }
}

/// A device's nominal rate as the whole hertz the canceller takes: a multiple of 100, so its
/// 10 ms frames hold whole samples and keep time with the other track's, and from 8 kHz, the
/// lowest rate a microphone uses, to [`MAX_SAMPLE_RATE_HZ`], the highest `sonora`'s resampler
/// takes; it panics above that and below about 4.9 kHz.
fn hertz(rate: f64) -> anyhow::Result<usize> {
    if rate % 100.0 != 0.0 || !(8_000.0..=f64::from(MAX_SAMPLE_RATE_HZ)).contains(&rate) {
        return Err(anyhow!("no echo cancellation at {rate} Hz"));
    }
    Ok(rate as usize)
}
