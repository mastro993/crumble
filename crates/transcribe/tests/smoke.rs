//! Transcribes speech that `say` makes, through Apple's real model and a Whisper server on this
//! Mac. Run them by hand: `cargo test -p crumble-transcribe --locked -- --ignored --nocapture`.
//! Apple's downloads the English model the first time; neither needs a microphone.

use std::path::Path;
use std::process::Command;

use crumble_transcribe::{Apple, ModelStatus, Track, Transcriber, install_model, model_status};

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
    let dir = recording(
        "transcribe",
        "Hello from Crumble. This is a test of the transcript.",
    );

    assert_ne!(model_status(LOCALE), ModelStatus::Unsupported);
    if model_status(LOCALE) != ModelStatus::Installed {
        install_model(LOCALE, |fraction| {
            eprintln!("model {:.0}%", fraction * 100.0)
        })
        .unwrap_or_else(|error| panic!("could not install the model: {error:#}"));
    }
    let mut last = 0.0;
    let segments = Apple
        .transcribe(Path::new(&dir), LOCALE, &mut |fraction| {
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

/// Makes a Recording in a new folder whose microphone track says `speech`.
fn recording(name: &str, speech: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("crumble-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok();
    let aiff = dir.join("speech.aiff");
    run(Command::new("say").arg("-o").arg(&aiff).arg(speech));
    run(Command::new("afconvert")
        .args(["-f", "adts", "-d", "aac"])
        .arg(&aiff)
        .arg(dir.join("mic.aac")));
    dir
}

/// Uploads speech that `say` makes to a Whisper server on this Mac, never anywhere else:
/// `CRUMBLE_WHISPER_URL` (default `http://127.0.0.1:8000/v1`, as speaches serves) with
/// `CRUMBLE_WHISPER_MODEL` (default `whisper-1`). Skips when nothing listens there.
#[test]
#[ignore = "needs a Whisper server on this Mac"]
fn transcribes_speech_from_say_on_a_local_server() {
    let base_url = std::env::var("CRUMBLE_WHISPER_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8000/v1".to_owned());
    assert!(
        crumble_remote::runs_locally(crumble_remote::Provider::OpenAiCompatible, &base_url),
        "{base_url} is not on this Mac; this test uploads only to loopback servers"
    );
    // The server's `host:port`, which a plain TCP connection can try.
    let address = base_url
        .split_once("://")
        .and_then(|(_, rest)| rest.split('/').next())
        .unwrap_or_default()
        .to_owned();
    if std::net::TcpStream::connect(&address).is_err() {
        eprintln!("Skipped: no Whisper server listens on {address}.");
        return;
    }
    let server = crumble_transcribe::Remote {
        provider: crumble_remote::Provider::OpenAiCompatible,
        base_url,
        api_key: None,
        model: std::env::var("CRUMBLE_WHISPER_MODEL").unwrap_or_else(|_| "whisper-1".to_owned()),
    };
    let dir = recording(
        "whisper",
        "Hello from Crumble. This is a test of the transcript.",
    );
    let segments = server.transcribe(&dir, LOCALE, &mut |_| {});
    std::fs::remove_dir_all(&dir).ok();
    let segments = segments.unwrap_or_else(|error| panic!("could not transcribe: {error:#}"));
    eprintln!("{segments:#?}");
    assert!(segments.iter().all(|segment| segment.track == Track::Mic));
    let heard: String = segments
        .iter()
        .map(|segment| segment.text.to_lowercase())
        .collect();
    assert!(heard.contains("test of the transcript"), "heard {heard:?}");
}
