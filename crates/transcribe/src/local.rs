//! A Local Model: one Crumble downloaded, run in a worker process (`worker`) so a crash in it
//! cannot end the app. A worker killed by a signal, as a GPU fault ends one, runs once more on
//! the CPU (ADR 0010).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, bail};
use crumble_remote::Provider;

use crate::{Segment, Track, Transcriber, merge, tracks};

/// A downloaded Local Model, with the VAD file that cuts long audio for it.
pub struct Local {
    /// The Model's file.
    pub model: PathBuf,
    /// The VAD file, `crumble_local_models::Library::vad`.
    pub vad: PathBuf,
    /// The Model's name, as the Transcript records it.
    pub name: String,
}

impl Transcriber for Local {
    fn provider(&self) -> Provider {
        Provider::Local
    }

    fn model(&self) -> &str {
        &self.name
    }

    fn transcribe(
        &self,
        dir: &Path,
        locale: &str,
        progress: &mut (dyn FnMut(f32) + Send),
    ) -> anyhow::Result<Vec<Segment>> {
        let tracks = tracks(dir)?;
        let language = language(locale);
        let segments = match self.run(&tracks, &language, false, progress)? {
            Some(segments) => segments,
            None => {
                eprintln!("The transcription worker crashed; trying again on the CPU.");
                self.run(&tracks, &language, true, progress)?
                    .context("Couldn't transcribe this recording.")?
            }
        };
        let mut by_track = vec![Vec::new(); tracks.len()];
        for (index, segment) in segments {
            if let Some(track) = by_track.get_mut(index) {
                track.push(segment);
            }
        }
        Ok(merge(by_track))
    }
}

impl Local {
    /// Runs one worker on `tracks`, `None` when a signal killed it.
    fn run(
        &self,
        tracks: &[(Track, PathBuf)],
        language: &str,
        cpu: bool,
        progress: &mut (dyn FnMut(f32) + Send),
    ) -> anyhow::Result<Option<Vec<(usize, Segment)>>> {
        let exe = std::env::current_exe().context("Couldn't start transcribing.")?;
        let mut child = Command::new(exe)
            .arg(crate::WORKER_FLAG)
            .arg(&self.model)
            .arg(&self.vad)
            .arg(language)
            .arg(if cpu { "cpu" } else { "gpu" })
            .args(tracks.iter().map(|(_, path)| path))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .context("Couldn't start transcribing.")?;
        let mut segments = Vec::new();
        let mut said = None;
        if let Some(stdout) = child.stdout.take() {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                match Line::parse(&line, tracks) {
                    Some(Line::Progress(fraction)) => progress(fraction),
                    Some(Line::Segment(index, segment)) => segments.push((index, segment)),
                    Some(Line::Error(sentence)) => said = Some(sentence),
                    None => eprintln!("Transcription worker said: {line}"),
                }
            }
        }
        let status = child
            .wait()
            .context("Couldn't transcribe this recording.")?;
        if status.success() {
            return Ok(Some(segments));
        }
        #[cfg(unix)]
        if std::os::unix::process::ExitStatusExt::signal(&status).is_some() {
            return Ok(None);
        }
        bail!(said.unwrap_or_else(|| "Couldn't transcribe this recording.".to_owned()))
    }
}

/// The language a Model is asked for, as the worker takes it: the ISO 639 code of `locale`
/// (`en` for `en_US`), or `-` to detect it for `auto`.
fn language(locale: &str) -> String {
    if locale == crate::DETECT {
        return "-".to_owned();
    }
    let code = locale.split(['_', '-']).next().unwrap_or_default();
    code.to_ascii_lowercase()
}

/// One line the worker prints.
#[derive(Debug, PartialEq)]
enum Line {
    /// `p\t<fraction>`: how much of the job is done, 0 to 1.
    Progress(f32),
    /// `s\t<track>\t<start_ms>\t<end_ms>\t<text>`: a segment of the track at that index.
    Segment(usize, Segment),
    /// `e\t<sentence>`: why it failed, for the user.
    Error(String),
}

impl Line {
    fn parse(line: &str, tracks: &[(Track, PathBuf)]) -> Option<Line> {
        let mut fields = line.splitn(5, '\t');
        match fields.next()? {
            "p" => Some(Line::Progress(fields.next()?.parse().ok()?)),
            "e" => Some(Line::Error(fields.next()?.to_owned())),
            "s" => {
                let index: usize = fields.next()?.parse().ok()?;
                let start: u64 = fields.next()?.parse().ok()?;
                let end: u64 = fields.next()?.parse().ok()?;
                let (track, _) = tracks.get(index)?;
                Some(Line::Segment(
                    index,
                    Segment {
                        track: *track,
                        start: Duration::from_millis(start),
                        end: Duration::from_millis(end.max(start)),
                        text: fields.next()?.to_owned(),
                    },
                ))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Line, language};
    use crate::{Segment, Track};
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn locales_become_the_language_a_model_is_asked_for() {
        assert_eq!(language("en_US"), "en");
        assert_eq!(language("it"), "it");
        assert_eq!(language("zh-Hans"), "zh");
        assert_eq!(language("auto"), "-");
    }

    #[test]
    fn worker_lines_read_back() {
        let tracks = [
            (Track::Mic, PathBuf::from("mic.aac")),
            (Track::System, PathBuf::from("system.aac")),
        ];
        assert_eq!(Line::parse("p\t0.5000", &tracks), Some(Line::Progress(0.5)));
        assert_eq!(
            Line::parse("s\t1\t1500\t2250\thello there", &tracks),
            Some(Line::Segment(
                1,
                Segment {
                    track: Track::System,
                    start: Duration::from_millis(1500),
                    end: Duration::from_millis(2250),
                    text: "hello there".to_owned(),
                }
            ))
        );
        assert_eq!(
            Line::parse("e\tThis Model doesn't transcribe Italian.", &tracks),
            Some(Line::Error(
                "This Model doesn't transcribe Italian.".to_owned()
            ))
        );
        assert_eq!(Line::parse("s\t2\t0\t1\tno such track", &tracks), None);
        assert_eq!(Line::parse("ggml_metal_init: loaded", &tracks), None);
    }
}
