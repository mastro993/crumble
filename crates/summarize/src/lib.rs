//! Summaries of Transcripts, made by a Model the user picks: Apple's on-device Foundation
//! Models through a small Swift bridge, the Claude API, or any OpenAI-compatible server such as
//! Ollama or LM Studio (ADR 0007). Each is a [`Summarizer`]. No GPUI.

mod apple;
mod chunk;
#[cfg(target_os = "macos")]
mod ffi;
#[cfg(not(target_os = "macos"))]
#[path = "unsupported.rs"]
mod ffi;
mod prompt;
mod remote;

pub use apple::Apple;
pub use ffi::apple_availability;
pub use remote::{Claude, OpenAiCompatible};

/// A Model that turns a Transcript into a Summary.
pub trait Summarizer {
    /// Who runs the Model.
    fn provider(&self) -> Provider;
    /// The Model, as the Summary records it: its id, or its name for Apple's.
    fn model(&self) -> &str;
    /// The Summary of `transcript`, the text [`transcript_text`] makes, as Markdown in the
    /// Transcript's language. Blocks, so call it off the main thread; `progress` gets
    /// 0.0..=1.0. Its error is a sentence to show the user, and never holds an API key.
    fn summarize(&self, transcript: &str, progress: &mut dyn FnMut(f32)) -> anyhow::Result<String>;
}

/// Where a Summary Model runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Provider {
    /// Apple's on-device Foundation Models.
    Apple,
    /// Anthropic's Claude API.
    Claude,
    /// A server that speaks OpenAI's chat completions.
    OpenAiCompatible,
}

impl Provider {
    /// Every Provider, in the order Settings lists them.
    pub const ALL: [Provider; 3] = [
        Provider::Apple,
        Provider::Claude,
        Provider::OpenAiCompatible,
    ];

    /// Its stable id, as stored and as its Keychain item's account.
    pub fn id(self) -> &'static str {
        match self {
            Provider::Apple => "apple",
            Provider::Claude => "claude",
            Provider::OpenAiCompatible => "openai",
        }
    }

    /// The Provider whose [`Provider::id`] is `id`.
    pub fn from_id(id: &str) -> Option<Provider> {
        Provider::ALL
            .into_iter()
            .find(|provider| provider.id() == id)
    }

    /// Its name for the user, as Settings lists it and "Summarized by" shows it.
    pub fn name(self) -> &'static str {
        match self {
            Provider::Apple => "Apple, on this Mac",
            Provider::Claude => "Claude",
            Provider::OpenAiCompatible => "OpenAI-compatible server",
        }
    }
}

/// Whether `provider` keeps the Transcript on this Mac: Apple's Model, or an OpenAI-compatible
/// server at `openai_base_url` whose host is `localhost`, `127.0.0.1` or `::1`. Only such a
/// Provider summarizes older Transcripts without being asked (ADR 0007).
pub fn runs_locally(provider: Provider, openai_base_url: &str) -> bool {
    match provider {
        Provider::Apple => true,
        Provider::Claude => false,
        Provider::OpenAiCompatible => is_loopback(openai_base_url),
    }
}

/// Whether `url`'s host is `localhost`, `127.0.0.1` or `::1`. An address with no scheme is
/// never sent, so it is not local either.
fn is_loopback(url: &str) -> bool {
    let [scheme, _, authority, _] = url_parts(url.trim());
    if scheme.is_empty() {
        return false;
    }
    let host = match authority.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next().unwrap_or_default(),
        None => authority.split(':').next().unwrap_or_default(),
    };
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
}

/// `url` in four parts: its scheme with `://` (empty without one), any `user:password@`, its host
/// and port, and the rest.
pub(crate) fn url_parts(url: &str) -> [&str; 4] {
    let start = url.find("://").map_or(0, |at| at + 3);
    let end = url[start..]
        .find(['/', '?', '#'])
        .map_or(url.len(), |at| start + at);
    // Credentials before an `@` are not the host.
    let host = url[start..end]
        .rfind('@')
        .map_or(start, |at| start + at + 1);
    [
        &url[..start],
        &url[start..host],
        &url[host..end],
        &url[end..],
    ]
}

/// Whether Apple's on-device model can run on this Mac now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    /// It can.
    Available,
    /// Apple Intelligence is off in System Settings.
    NotEnabled,
    /// Apple's model is still downloading.
    NotReady,
    /// This Mac can't run it.
    Unsupported,
    /// It can't, for a reason this version of Crumble doesn't know.
    Unavailable,
}

impl Availability {
    /// Why Apple's model can't be used, for Settings; `None` when it can.
    pub fn reason(self) -> Option<&'static str> {
        match self {
            Availability::Available => None,
            Availability::NotEnabled => Some("Turn on Apple Intelligence in System Settings."),
            Availability::NotReady => Some("Apple Intelligence is still getting ready."),
            Availability::Unsupported => Some("This Mac can't run Apple Intelligence."),
            Availability::Unavailable => Some("Apple Intelligence isn't available right now."),
        }
    }
}

/// Who spoke a line of a Transcript.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speaker {
    /// The user, heard on the microphone.
    You,
    /// The others on the call, heard in the Mac's sound.
    Others,
}

/// A Transcript as a Model reads it: one `You: …` or `Others: …` line per segment, in order.
pub fn transcript_text<'a>(segments: impl IntoIterator<Item = (Speaker, &'a str)>) -> String {
    let lines: Vec<String> = segments
        .into_iter()
        .filter(|(_, text)| !text.trim().is_empty())
        .map(|(speaker, text)| {
            let speaker = match speaker {
                Speaker::You => "You",
                Speaker::Others => "Others",
            };
            // A segment is one line, whatever speech recognition wrote.
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            format!("{speaker}: {text}")
        })
        .collect();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{Provider, Speaker, runs_locally, transcript_text};

    #[test]
    fn a_transcript_reads_one_labelled_line_per_segment() {
        let text = transcript_text([
            (Speaker::You, "Shall we start?"),
            (Speaker::Others, " Yes.\nGo  ahead. "),
            (Speaker::Others, "  "),
            (Speaker::You, "Great."),
        ]);
        assert_eq!(
            text,
            "You: Shall we start?\nOthers: Yes. Go ahead.\nYou: Great."
        );
        assert_eq!(transcript_text([]), "");
    }

    #[test]
    fn provider_ids_round_trip() {
        for provider in Provider::ALL {
            assert_eq!(Provider::from_id(provider.id()), Some(provider));
        }
        assert_eq!(Provider::from_id("gemini"), None);
    }

    #[test]
    fn only_apple_and_a_loopback_server_run_locally() {
        use Provider::{Apple, Claude, OpenAiCompatible};
        assert!(runs_locally(Apple, ""));
        assert!(!runs_locally(Claude, "http://localhost:11434/v1"));
        for local in [
            "http://localhost:11434/v1",
            "http://LOCALHOST:1234/v1/",
            "http://127.0.0.1:8765/v1",
            "https://[::1]:8080/v1",
            " http://user:pw@127.0.0.1/v1 ",
        ] {
            assert!(runs_locally(OpenAiCompatible, local), "{local}");
        }
        for remote in [
            "",
            "localhost:11434/v1",
            "http://192.168.1.20:8765/v1",
            "https://api.openai.com/v1",
            "http://localhost.example.com/v1",
            "http://127.0.0.1.example.com/v1",
            "http://localhost@evil.example.com/v1",
            "http://[::2]/v1",
            "http://10.0.0.1/localhost",
        ] {
            assert!(!runs_locally(OpenAiCompatible, remote), "{remote}");
        }
    }
}
