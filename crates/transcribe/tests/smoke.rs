//! Transcribes speech that `say` makes, through Apple's real model. Run it by hand:
//! `cargo test -p crumble-transcribe --locked -- --ignored`. It downloads the English model
//! the first time, and needs no microphone.

use std::path::Path;
use std::process::Command;

use crumble_transcribe::{ModelStatus, Track, install_model, model_status, transcribe};

const LOCALE: &str = "en_US";

fn run(command: &mut Command) {
    let status = command.status();
    assert!(
        status.as_ref().is_ok_and(|status| status.success()),
        "{command:?} failed: {status:?}"
    );
}

#[test]
#[ignore = "runs Apple's speech model, and may download it"]
fn transcribes_speech_from_say() {
    let dir = std::env::temp_dir().join(format!("crumble-transcribe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok();
    let speech = dir.join("speech.aiff");
    run(Command::new("say")
        .arg("-o")
        .arg(&speech)
        .arg("Hello from Crumble. This is a test of the transcript."));
    // A Recording's track: AAC in ADTS, as `crumble-audio` writes it.
    run(Command::new("afconvert")
        .args(["-f", "adts", "-d", "aac"])
        .arg(&speech)
        .arg(dir.join("mic.aac")));

    assert_ne!(model_status(LOCALE), ModelStatus::Unsupported);
    if model_status(LOCALE) != ModelStatus::Installed {
        install_model(LOCALE, |fraction| {
            eprintln!("model {:.0}%", fraction * 100.0)
        })
        .unwrap_or_else(|error| panic!("could not install the model: {error:#}"));
    }
    let mut last = 0.0;
    let segments = transcribe(Path::new(&dir), LOCALE, |fraction| {
        assert!(fraction >= last, "progress went back");
        last = fraction;
    })
    .unwrap_or_else(|error| panic!("could not transcribe: {error:#}"));
    std::fs::remove_dir_all(&dir).ok();

    eprintln!("{segments:#?}");
    assert!((last - 1.0).abs() < f32::EPSILON);
    assert!(segments.iter().all(|segment| segment.track == Track::Mic));
    let text: String = segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect();
    // The model has never heard of Crumble, and may spell it "Krumble".
    let heard = text.to_lowercase();
    assert!(heard.contains("rumble"), "heard {text:?}");
    assert!(heard.contains("test of the transcript"), "heard {text:?}");
}
