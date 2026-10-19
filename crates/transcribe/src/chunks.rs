//! Where long audio is cut for a Local Model that cannot take it whole: in the middle of pauses
//! Silero VAD finds, into chunks of 5 to 40 s near 30 s (ADR 0010). Every length is in samples
//! of 16 kHz audio.
//!
//! Ported from Handy's `audio_toolkit/chunking.rs` (MIT, Copyright (c) 2025 CJ Pais; see
//! `crates/local-models/handy/LICENSE`): the same cut plan, offline only, keeping each chunk's
//! place so its segments keep their times.

use std::ops::Range;

/// Samples per second of the audio Local Models read.
pub(crate) const RATE: usize = 16_000;

const TARGET_SECONDS: f64 = 30.0;
/// Audio up to this long is one chunk; longer audio is cut.
const MAX_CHUNK_SAMPLES: usize = 40 * RATE;
const MIN_CHUNK_SAMPLES: usize = 5 * RATE;

/// A place a chunk may end: the middle of a pause.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CutCandidate {
    cut: usize,
    gap_seconds: f64,
}

/// The chunks `samples` of audio are cut into, in order and covering all of it, given its
/// `speech`, in order.
pub(crate) fn chunks(speech: &[Range<usize>], samples: usize) -> Vec<Range<usize>> {
    let cuts = plan_cuts(&cut_candidates(speech, samples), samples, 0);
    let mut chunks = Vec::with_capacity(cuts.len() + 1);
    let mut start = 0;
    for cut in cuts.into_iter().chain(std::iter::once(samples)) {
        chunks.push(start..cut);
        start = cut;
    }
    chunks
}

/// One candidate in the middle of each pause, lead-in and tail included.
fn cut_candidates(segments: &[Range<usize>], samples: usize) -> Vec<CutCandidate> {
    let mut candidates = Vec::new();
    let mut silence_start = 0;
    for segment in segments
        .iter()
        .cloned()
        .chain(std::iter::once(samples..samples))
    {
        if segment.start > silence_start {
            candidates.push(CutCandidate {
                cut: (silence_start + segment.start) / 2,
                gap_seconds: (segment.start - silence_start) as f64 / RATE as f64,
            });
        }
        silence_start = silence_start.max(segment.end);
    }
    candidates
}

/// Greedy, left to right from `from`: each cut is the best-scoring pause 5-40 s after the
/// previous one, preferring long pauses near 30 s and ones that leave at least 5 s after them.
/// With no pause, it is a hard cut at 40 s, or halfway when that would leave under 5 s. Audio of
/// 40 s or less is never cut. `candidates` must be in order.
fn plan_cuts(candidates: &[CutCandidate], samples: usize, from: usize) -> Vec<usize> {
    let mut cuts = Vec::new();
    let mut start = from;
    // Each window starts after the last, so earlier candidates never return.
    let mut first = 0;
    while samples - start > MAX_CHUNK_SAMPLES {
        while candidates
            .get(first)
            .is_some_and(|candidate| candidate.cut < start + MIN_CHUNK_SAMPLES)
        {
            first += 1;
        }
        // A pause that leaves under 5 s is only used when no other pause will do.
        let mut best: Option<(usize, f64)> = None;
        let mut fallback: Option<(usize, f64)> = None;
        for candidate in candidates
            .get(first..)
            .unwrap_or_default()
            .iter()
            .take_while(|candidate| candidate.cut <= start + MAX_CHUNK_SAMPLES)
        {
            let position = (candidate.cut - start) as f64 / RATE as f64;
            let off_target = if position > TARGET_SECONDS {
                0.1 * (position - TARGET_SECONDS)
            } else {
                0.05 * (TARGET_SECONDS - position)
            };
            let score = candidate.gap_seconds.min(1.0) - off_target;
            if samples - candidate.cut >= MIN_CHUNK_SAMPLES
                && best.is_none_or(|(_, best)| score > best)
            {
                best = Some((candidate.cut, score));
            }
            if fallback.is_none_or(|(_, fallback)| score > fallback) {
                fallback = Some((candidate.cut, score));
            }
        }
        let hard_cut = if samples - start < MAX_CHUNK_SAMPLES + MIN_CHUNK_SAMPLES {
            start + (samples - start) / 2
        } else {
            start + MAX_CHUNK_SAMPLES
        };
        start = best.or(fallback).map_or(hard_cut, |(cut, _)| cut);
        cuts.push(start);
    }
    cuts
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init, reason = "a chunk is a range")]
mod tests {
    use super::{RATE, chunks};

    fn seconds(range: &std::ops::Range<usize>) -> (f64, f64) {
        (
            range.start as f64 / RATE as f64,
            range.end as f64 / RATE as f64,
        )
    }

    #[test]
    fn short_audio_is_one_chunk() {
        assert_eq!(chunks(&[], 40 * RATE), [0..40 * RATE]);
        assert_eq!(chunks(&[RATE..39 * RATE], 40 * RATE), [0..40 * RATE]);
    }

    #[test]
    fn long_audio_is_cut_in_pauses_near_30_seconds() {
        // Speech in 9 s stretches with 1 s pauses, for 100 s.
        let speech: Vec<_> = (0..10)
            .map(|i| (i * 10 + 1) * RATE..(i * 10 + 10) * RATE)
            .collect();
        let chunks = chunks(&speech, 100 * RATE);
        let times: Vec<_> = chunks.iter().map(seconds).collect();
        assert_eq!(times, [(0.0, 30.5), (30.5, 60.5), (60.5, 100.0)]);
    }

    #[test]
    fn speech_without_pauses_is_cut_hard() {
        let times: Vec<_> = chunks(&[0..90 * RATE], 90 * RATE)
            .iter()
            .map(seconds)
            .collect();
        assert_eq!(times, [(0.0, 40.0), (40.0, 80.0), (80.0, 90.0)]);
        // Under 45 s left, the cut goes halfway rather than leave under 5 s.
        let times: Vec<_> = chunks(&[0..42 * RATE], 42 * RATE)
            .iter()
            .map(seconds)
            .collect();
        assert_eq!(times, [(0.0, 21.0), (21.0, 42.0)]);
    }
}
