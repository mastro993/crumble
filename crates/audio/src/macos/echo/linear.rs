//! AEC3's linear echo filter, without its suppressor below 8 kHz. The filter subtracts the echo
//! it predicts from what the Mac played, so a voice passes as it was recorded even while the
//! call talks over it. The suppressor, which `sonora`'s `AudioProcessing` always runs after the
//! filter, mutes whatever it takes for echo: over loud speakers it muted the voice too (ADR 0005).
//!
//! AEC3 runs at 48 kHz in three bands of 16 kHz. Its filter works on the lowest, below 8 kHz,
//! where speech and its echo are, and the filter's output is what this keeps there. Above 8 kHz
//! there is no filter, so the suppressor's output is kept: little of a voice is up there. A
//! track at another rate is resampled to 48 kHz and, for the microphone, back.

use sonora::high_pass_filter::HighPassFilter;
use sonora::three_band_filter_bank::{
    FULL_BAND_SIZE, NUM_BANDS, SPLIT_BAND_SIZE, ThreeBandFilterBank,
};
use sonora_aec3::block::Block;
use sonora_aec3::block_framer::BlockFramer;
use sonora_aec3::block_processor::BlockProcessor;
use sonora_aec3::common::SUB_FRAME_LENGTH;
use sonora_aec3::config::EchoCanceller3Config;
use sonora_aec3::frame_blocker::FrameBlocker;
use sonora_common_audio::push_resampler::PushResampler;

/// The rate AEC3 runs at here.
const RATE: usize = 48_000;
/// The rate of each band.
const BAND_RATE: i32 = 16_000;
/// AEC3 takes samples scaled to the 16-bit range.
const SCALE: f32 = 32_768.0;

/// The filter, and the blocks AEC3 takes 10 ms in.
pub(super) struct Linear {
    processor: BlockProcessor,
    mic: Bands,
    reference: Bands,
    /// The high-pass filter AEC3 needs on the microphone's lowest band.
    high_pass: HighPassFilter,
    low: Vec<Vec<f32>>,
    mic_blocker: FrameBlocker,
    reference_blocker: FrameBlocker,
    framer: BlockFramer,
    block: Block,
    filtered: Block,
    /// Half of a cleaned frame: band, channel, sample.
    cleaned: Vec<Vec<Vec<f32>>>,
}

impl Linear {
    /// A filter for a microphone at `mic_rate` and a reference at `reference_rate`, each a
    /// multiple of 100 Hz from 8 kHz to 384 kHz.
    pub(super) fn new(mic_rate: usize, reference_rate: usize) -> Self {
        Self {
            processor: BlockProcessor::new(&EchoCanceller3Config::default(), RATE, 1, 1),
            mic: Bands::new(mic_rate, true),
            reference: Bands::new(reference_rate, false),
            high_pass: HighPassFilter::new(BAND_RATE, 1),
            low: vec![vec![0.0; SPLIT_BAND_SIZE]],
            mic_blocker: FrameBlocker::new(NUM_BANDS, 1),
            reference_blocker: FrameBlocker::new(NUM_BANDS, 1),
            framer: BlockFramer::new(NUM_BANDS, 1),
            block: Block::new(NUM_BANDS, 1),
            filtered: Block::new(1, 1),
            cleaned: vec![vec![vec![0.0; SUB_FRAME_LENGTH]]; NUM_BANDS],
        }
    }

    /// Cleans 10 ms of microphone against the 10 ms the Mac played with it, into `out`.
    pub(super) fn process(&mut self, reference: &[f32], mic: &[f32], out: &mut [f32]) {
        self.reference.split(reference);
        for half in 0..2 {
            self.reference_blocker
                .insert_sub_frame_and_extract_block(&self.reference.half(half), &mut self.block);
            self.processor.buffer_render(&self.block);
        }
        if self.reference_blocker.is_block_available() {
            self.reference_blocker.extract_block(&mut self.block);
            self.processor.buffer_render(&self.block);
        }

        self.mic.split(mic);
        self.low[0].copy_from_slice(&self.mic.split[0]);
        self.high_pass.process_channels(&mut self.low);
        self.mic.split[0].copy_from_slice(&self.low[0]);
        for half in 0..2 {
            self.mic_blocker
                .insert_sub_frame_and_extract_block(&self.mic.half(half), &mut self.block);
            self.cancel();
            self.framer
                .insert_block_and_extract_sub_frame(&self.block, &mut self.cleaned);
            for (band, cleaned) in self.mic.split.iter_mut().zip(&self.cleaned) {
                band[half * SUB_FRAME_LENGTH..][..SUB_FRAME_LENGTH].copy_from_slice(&cleaned[0]);
            }
        }
        if self.mic_blocker.is_block_available() {
            self.mic_blocker.extract_block(&mut self.block);
            self.cancel();
            self.framer.insert_block(&self.block);
        }
        self.mic.merge(out);
    }

    /// Runs AEC3 over the microphone's `block`, then puts the filter's output in the lowest band.
    fn cancel(&mut self) {
        self.processor
            .process_capture(false, false, Some(&mut self.filtered), &mut self.block);
        self.block
            .view_mut(0, 0)
            .copy_from_slice(self.filtered.view(0, 0));
    }
}

/// One track's 10 ms in the three bands AEC3 takes, and the way there from the track's rate.
struct Bands {
    filter_bank: ThreeBandFilterBank,
    /// To 48 kHz and back, when the track runs at another rate.
    up: Option<PushResampler<f32>>,
    down: Option<PushResampler<f32>>,
    full: [f32; FULL_BAND_SIZE],
    split: [[f32; SPLIT_BAND_SIZE]; NUM_BANDS],
}

impl Bands {
    /// Bands for a track at `rate`, joined back at that rate when `back`.
    fn new(rate: usize, back: bool) -> Self {
        let (frame, resampled) = (rate / 100, rate != RATE);
        Self {
            filter_bank: ThreeBandFilterBank::new(),
            up: resampled.then(|| PushResampler::new(frame, FULL_BAND_SIZE, 1)),
            down: (resampled && back).then(|| PushResampler::new(FULL_BAND_SIZE, frame, 1)),
            full: [0.0; FULL_BAND_SIZE],
            split: [[0.0; SPLIT_BAND_SIZE]; NUM_BANDS],
        }
    }

    fn split(&mut self, samples: &[f32]) {
        match &mut self.up {
            Some(up) => up.resample(samples, &mut self.full),
            None => self.full.copy_from_slice(samples),
        }
        self.full.iter_mut().for_each(|sample| *sample *= SCALE);
        self.filter_bank.analysis(&self.full, &mut self.split);
    }

    fn merge(&mut self, out: &mut [f32]) {
        self.filter_bank.synthesis(&self.split, &mut self.full);
        self.full.iter_mut().for_each(|sample| *sample /= SCALE);
        match &mut self.down {
            Some(down) => down.resample(&self.full, out),
            None => out.copy_from_slice(&self.full),
        }
    }

    /// The first or second half of each band, as AEC3 takes 10 ms in two.
    fn half(&self, half: usize) -> Vec<Vec<&[f32]>> {
        self.split
            .iter()
            .map(|band| vec![&band[half * SUB_FRAME_LENGTH..][..SUB_FRAME_LENGTH]])
            .collect()
    }
}
