//! The worker process a Local Model runs in, so a crash in ggml or Metal ends it rather than
//! Crumble and a Recording in progress (ADR 0010). It is the `crumble` binary started again
//! with [`FLAG`]; `apps/desktop` hands over to [`run_if_asked`] before anything else starts.
//!
//! Arguments: the Model's file, the VAD file, the language (`-` to detect it), `cpu` or `gpu`,
//! then each track. It prints one line per update on stdout, as `local::Line` reads them.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, anyhow, bail};
use transcribe_cpp::{
    Backend, Model, ModelOptions, RunOptions, Session, TimestampKind, VadOptions, VadSession,
};

use crate::chunks::{RATE, chunks};

/// The first argument that starts the worker.
pub(crate) const FLAG: &str = "--transcription-worker";

/// Runs the worker and returns its exit code when this process was started as one; `None`
/// otherwise, for the app to start as usual.
pub fn run_if_asked() -> Option<ExitCode> {
    let mut args = std::env::args_os().skip(1);
    if args.next()? != FLAG {
        return None;
    }
    // Crumble closes stdin when it cancels the job, quits or crashes: stop rather than work for
    // no one.
    std::thread::spawn(|| {
        let _ = std::io::copy(&mut std::io::stdin().lock(), &mut std::io::sink());
        quit_now();
    });
    Some(match work(args.collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Transcription worker: {error:#}");
            if let Some(Said(sentence)) = error.downcast_ref::<Said>() {
                println!("e\t{}", one_line(sentence));
            }
            ExitCode::FAILURE
        }
    })
}

/// An error whose sentence is for the user.
#[derive(Debug)]
struct Said(String);

impl std::fmt::Display for Said {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Said {}

fn work(args: Vec<OsString>) -> anyhow::Result<()> {
    let [model, vad_file, language, backend, tracks @ ..] = args.as_slice() else {
        bail!("expected a Model, the VAD, a language, a backend and tracks");
    };
    let language = language.to_str().filter(|language| *language != "-");
    let backend = if backend == "cpu" {
        Backend::Cpu
    } else {
        Backend::Auto
    };
    let model = Model::load_with(
        PathBuf::from(model),
        &ModelOptions {
            backend,
            ..ModelOptions::default()
        },
    )
    .context("load the Model")?;
    let capabilities = model
        .capabilities()
        .context("read the Model's capabilities")?;
    if let Some(language) = language
        && !capabilities.languages.is_empty()
        && !capabilities.languages.iter().any(|code| code == language)
    {
        return Err(anyhow!(Said(format!(
            "This Model doesn't transcribe {}. Pick another Language or Model in Settings.",
            crate::language_name(language)
        ))));
    }
    // Whisper and the Parakeet family take an hour at once with their own times; the rest
    // degrade or refuse past their window, and those with no times need chunks for segments.
    let whole = matches!(model.arch().as_str(), "whisper" | "parakeet" | "gigaam")
        && capabilities.max_audio_ms == 0
        && capabilities.max_timestamp_kind != TimestampKind::None;
    let mut session = model.session().context("start a session")?;
    let mut vad: Option<VadSession> = None;
    let options = RunOptions {
        language: language.map(str::to_owned),
        ..RunOptions::default()
    };
    for (index, track) in tracks.iter().enumerate() {
        let samples = crumble_audio::read_samples(&PathBuf::from(track), RATE as f64)
            .context("decode the track")?;
        let pieces = if whole || samples.len() <= 40 * RATE {
            std::iter::once(0..samples.len()).collect()
        } else {
            if vad.is_none() {
                let session = Model::load(PathBuf::from(vad_file))
                    .and_then(|vad| vad.vad_session())
                    .context("load the VAD")?;
                vad = Some(session);
            }
            let speech = vad
                .as_mut()
                .context("no VAD")?
                .run(&samples, &VadOptions::default())
                .context("find the pauses")?
                .segments
                .iter()
                .map(|segment| sample(segment.start_sample)..sample(segment.end_sample))
                .collect::<Vec<_>>();
            chunks(&speech, samples.len())
        };
        for piece in pieces {
            let audio = samples.get(piece.clone()).unwrap_or_default();
            transcribe(&mut session, &options, audio, index, &piece)?;
            #[allow(
                clippy::cast_precision_loss,
                reason = "progress needs no exact sample counts"
            )]
            let done = (index as f64 + piece.end as f64 / samples.len().max(1) as f64)
                / tracks.len() as f64;
            println!("p\t{done:.4}");
        }
    }
    Ok(())
}

/// Transcribes `audio`, the `piece` of track `track`, printing its segments with times in the
/// track. A Model without times gives the piece one segment.
fn transcribe(
    session: &mut Session,
    options: &RunOptions,
    audio: &[f32],
    track: usize,
    piece: &std::ops::Range<usize>,
) -> anyhow::Result<()> {
    if audio.is_empty() {
        return Ok(());
    }
    let result = session.run(audio, options).context("transcribe")?;
    let offset = milliseconds(piece.start);
    if result.timestamp_kind == TimestampKind::None || result.segments.is_empty() {
        let text = result.text.trim();
        if !text.is_empty() {
            let end = milliseconds(piece.end);
            println!("s\t{track}\t{offset}\t{end}\t{}", one_line(text));
        }
        return Ok(());
    }
    for segment in &result.segments {
        let text = segment.text.trim();
        if !text.is_empty() {
            let start = offset + segment.t0_ms.max(0);
            let end = offset + segment.t1_ms.max(segment.t0_ms).max(0);
            println!("s\t{track}\t{start}\t{end}\t{}", one_line(text));
        }
    }
    Ok(())
}

fn sample(at: i64) -> usize {
    usize::try_from(at).unwrap_or_default()
}

fn milliseconds(samples: usize) -> i64 {
    i64::try_from(samples * 1000 / RATE).unwrap_or(i64::MAX)
}

/// `text` with tabs and line breaks as spaces, so it fits one line of output.
fn one_line(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

/// Ends the process with code 1 at once. `std::process::exit` runs ggml's C++ destructors while
/// its threads still compute, which kills the worker with SIGILL, read as a GPU fault.
#[allow(unsafe_code)]
fn quit_now() -> ! {
    unsafe extern "C" {
        fn _exit(code: i32) -> !;
    }
    // SAFETY: `_exit` takes any code and never returns.
    unsafe { _exit(1) }
}
