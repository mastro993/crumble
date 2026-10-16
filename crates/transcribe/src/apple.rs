//! Apple's SpeechAnalyzer, through the Swift bridge: everything stays on the Mac (ADR 0006).

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, anyhow};
use crumble_remote::Provider;

use crate::{Segment, Track, Transcriber, ffi, merge, tracks};

/// The Model Apple's [`Transcriber`] records on a Transcript.
pub const MODEL: &str = "Apple SpeechAnalyzer";

/// Apple's on-device SpeechAnalyzer. The locale's model must be installed first:
/// [`model_status`](crate::model_status) says whether it is, and
/// [`install_model`](crate::install_model) gets it.
pub struct Apple;

impl Transcriber for Apple {
    fn provider(&self) -> Provider {
        Provider::Apple
    }

    fn model(&self) -> &str {
        MODEL
    }

    fn transcribe(
        &self,
        dir: &Path,
        locale: &str,
        progress: &mut (dyn FnMut(f32) + Send),
    ) -> anyhow::Result<Vec<Segment>> {
        let tracks = tracks(dir)?;
        #[allow(clippy::cast_precision_loss, reason = "at most two tracks")]
        let count = tracks.len() as f32;
        let mut segments = Vec::new();
        for (done, (track, path)) in tracks.iter().enumerate() {
            #[allow(clippy::cast_precision_loss, reason = "at most two tracks")]
            let done = done as f32;
            // Both tracks run the whole Recording, so each is an equal share of the work.
            let lines = ffi::transcribe_file(path, locale, &mut |fraction| {
                progress((done + fraction) / count);
            })
            .with_context(|| format!("could not transcribe {}", path.display()))?;
            segments.push(parse(&lines, *track)?);
        }
        Ok(merge(segments))
    }
}

/// Reads the bridge's output: one `start_ms\tend_ms\ttext` line per segment of `track`.
pub(crate) fn parse(lines: &str, track: Track) -> anyhow::Result<Vec<Segment>> {
    lines
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.splitn(3, '\t');
            let mut time = || -> anyhow::Result<Duration> {
                let ms = fields
                    .next()
                    .ok_or_else(|| anyhow!("a short line {line:?}"))?;
                Ok(Duration::from_millis(
                    ms.parse()
                        .with_context(|| format!("a bad time in {line:?}"))?,
                ))
            };
            let (start, end) = (time()?, time()?);
            let text = fields
                .next()
                .ok_or_else(|| anyhow!("no text in {line:?}"))?;
            Ok(Segment {
                track,
                start,
                end,
                text: text.trim().to_owned(),
            })
        })
        .collect()
}
