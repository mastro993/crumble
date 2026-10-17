//! Cutting a track, AAC in ADTS as `crumble-audio` writes it, into pieces small enough to
//! upload. Every ADTS frame starts with a header that gives its length, sample rate and sample
//! count, so a cut between frames needs no decoding (ADR 0008).

use std::ops::Range;
use std::time::Duration;

use anyhow::bail;

/// The bytes of an ADTS header.
const HEADER: usize = 7;
/// The samples in one AAC raw data block.
const SAMPLES_PER_BLOCK: u64 = 1024;
/// The sample rates an ADTS header can index.
const SAMPLE_RATES: [u64; 13] = [
    96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025, 8_000,
    7_350,
];

/// A run of whole frames of a track.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Chunk {
    /// Its bytes in the track.
    pub(crate) bytes: Range<usize>,
    /// Where it starts in the track.
    pub(crate) start: Duration,
    /// How long it plays.
    pub(crate) length: Duration,
}

/// Cuts `adts` into runs of whole frames, each at most `max_bytes` long unless one frame is
/// longer, in order. A frame cut short at the end, as a crash leaves one, is dropped. Errors
/// when `adts` is not ADTS.
// ponytail: cuts fall wherever the size runs out, so a word can be split across two uploads;
// cut at a quiet frame if Transcripts show it.
pub(crate) fn chunks(adts: &[u8], max_bytes: usize) -> anyhow::Result<Vec<Chunk>> {
    let mut chunks = Vec::new();
    let (mut start, mut at) = (0, 0);
    let (mut start_samples, mut samples) = (0_u64, 0_u64);
    let mut rate = 0;
    while adts.len() - at >= HEADER {
        let header = &adts[at..at + HEADER];
        if header[0] != 0xFF || header[1] & 0xF0 != 0xF0 {
            bail!("the track is not AAC in ADTS at byte {at}");
        }
        let Some(&frame_rate) = SAMPLE_RATES.get(usize::from((header[2] >> 2) & 0x0F)) else {
            bail!("the track has a bad sample rate at byte {at}");
        };
        let length = usize::from(header[3] & 0x03) << 11
            | usize::from(header[4]) << 3
            | usize::from(header[5] >> 5);
        if length < HEADER {
            bail!("the track has a bad frame length at byte {at}");
        }
        if at + length > adts.len() {
            break;
        }
        if at > start && at + length - start > max_bytes {
            chunks.push(chunk(start..at, start_samples, samples, rate));
            (start, start_samples) = (at, samples);
        }
        rate = frame_rate;
        samples += SAMPLES_PER_BLOCK * (u64::from(header[6] & 0x03) + 1);
        at += length;
    }
    if at > start {
        chunks.push(chunk(start..at, start_samples, samples, rate));
    }
    Ok(chunks)
}

fn chunk(bytes: Range<usize>, start: u64, end: u64, rate: u64) -> Chunk {
    let time = |samples: u64| Duration::from_micros(samples * 1_000_000 / rate);
    Chunk {
        bytes,
        start: time(start),
        length: time(end) - time(start),
    }
}

#[cfg(test)]
mod tests {
    use super::{Chunk, chunks};
    use std::time::Duration;

    /// An ADTS frame of `length` bytes at 48 kHz, holding one block of 1024 samples.
    fn frame(length: usize) -> Vec<u8> {
        let mut frame = vec![0; length];
        frame[..7].copy_from_slice(&[
            0xFF,
            0xF1,
            (1 << 6) | (3 << 2),
            0x80 | ((length >> 11) & 0x03) as u8,
            ((length >> 3) & 0xFF) as u8,
            (((length & 0x07) << 5) as u8) | 0x1F,
            0xFC,
        ]);
        frame
    }

    fn track(lengths: &[usize]) -> Vec<u8> {
        lengths.iter().flat_map(|length| frame(*length)).collect()
    }

    /// 1024 samples at 48 kHz.
    fn frames(count: u64) -> Duration {
        Duration::from_micros(count * 1024 * 1_000_000 / 48_000)
    }

    #[test]
    fn a_short_track_is_one_chunk() {
        let audio = track(&[100, 120, 90]);
        assert_eq!(
            chunks(&audio, 1000).ok(),
            Some(vec![Chunk {
                bytes: 0..310,
                start: Duration::ZERO,
                length: frames(3),
            }])
        );
        assert_eq!(chunks(&[], 1000).ok(), Some(Vec::new()));
    }

    #[test]
    fn a_long_track_is_cut_between_frames_with_each_cut_s_start() {
        let audio = track(&[100, 100, 100, 100, 100]);
        let cut = chunks(&audio, 250).unwrap_or_default();
        assert_eq!(
            cut,
            [
                Chunk {
                    bytes: 0..200,
                    start: Duration::ZERO,
                    length: frames(2),
                },
                Chunk {
                    bytes: 200..400,
                    start: frames(2),
                    length: frames(4) - frames(2),
                },
                Chunk {
                    bytes: 400..500,
                    start: frames(4),
                    length: frames(5) - frames(4),
                },
            ]
        );
        // A frame longer than the limit still goes, alone.
        let big = chunks(&track(&[50, 400, 50]), 100).unwrap_or_default();
        let sizes: Vec<_> = big.iter().map(|chunk| chunk.bytes.len()).collect();
        assert_eq!(sizes, [50, 400, 50]);
    }

    #[test]
    fn a_frame_cut_short_is_dropped_and_garbage_is_an_error() {
        let mut audio = track(&[100, 100]);
        audio.truncate(150);
        let cut = chunks(&audio, 1000).unwrap_or_default();
        assert_eq!(cut.len(), 1);
        assert_eq!(cut[0].bytes, 0..100);
        assert!(chunks(b"RIFF....WAVEfmt ", 1000).is_err());
    }
}
