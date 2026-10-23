//! Summarizes an invented Transcript with real Models. Run them by hand:
//! `cargo test -p crumble-summarize --locked -- --ignored --nocapture`. Apple's needs Apple
//! Intelligence on; the server's needs Ollama on this Mac with `OLLAMA_MODEL` pulled (default
//! `llama3.2`), and skips otherwise.

use crumble_remote::{Provider, ping};
use crumble_summarize::{
    Apple, Availability, Remote, Speaker, Summarizer, apple_availability, transcript_text,
};

/// An invented meeting; nothing real.
fn transcript(repeat: usize) -> String {
    let lines = [
        (Speaker::You, "Morning. Let's plan the Crumble beta launch."),
        (
            Speaker::Others,
            "Sure. The build is ready, but the onboarding copy isn't.",
        ),
        (
            Speaker::You,
            "Then let's ship on Thursday instead of Tuesday.",
        ),
        (
            Speaker::Others,
            "Agreed. I'll finish the onboarding copy by Wednesday.",
        ),
        (
            Speaker::You,
            "And I'll email the beta testers on Thursday morning.",
        ),
    ];
    transcript_text(lines.iter().cycle().take(lines.len() * repeat).copied())
}

fn check(summarizer: &dyn Summarizer, text: &str) {
    let mut last = 0.0;
    let summary = summarizer
        .summarize(text, &mut |fraction| {
            assert!(fraction >= last, "progress went back");
            last = fraction;
        })
        .unwrap_or_else(|error| panic!("could not summarize: {error:#}"));
    eprintln!("{summary}");
    assert!((last - 1.0).abs() < f32::EPSILON);
    assert!(summary.to_lowercase().contains("thursday"), "{summary}");
}

#[test]
#[ignore = "runs Apple's on-device model"]
fn apple_summarizes_an_invented_meeting() {
    assert_eq!(apple_availability(), Availability::Available);
    check(&Apple, &transcript(1));
}

#[test]
#[ignore = "runs Apple's on-device model on a Transcript longer than its context"]
fn apple_summarizes_a_long_meeting_in_parts() {
    assert_eq!(apple_availability(), Availability::Available);
    check(&Apple, &transcript(120));
}

#[test]
#[ignore = "needs a local Ollama"]
fn a_local_server_summarizes_an_invented_meeting() {
    let server = Remote {
        provider: Provider::OpenAiCompatible,
        base_url: "http://localhost:11434/v1".into(),
        api_key: None,
        model: std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2".into()),
    };
    // Ollama lists a Model with its tag, such as `llama3.2:latest`.
    let tagged = format!("{}:", server.model);
    let pulled = ping(Provider::OpenAiCompatible, &server.base_url, None).map(|listed| {
        (listed.chat.iter()).any(|id| *id == server.model || id.starts_with(&tagged))
    });
    match pulled {
        Ok(true) => check(&server, &transcript(1)),
        Ok(false) => eprintln!("Skipped: Ollama here hasn't pulled {}", server.model),
        Err(error) => eprintln!("Skipped: no Ollama here ({error})"),
    }
}
