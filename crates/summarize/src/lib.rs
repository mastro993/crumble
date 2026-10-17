//! Summaries of Transcripts, made by a Model the user picks: Apple's on-device Foundation
//! Models through a small Swift bridge, the Claude API, or any OpenAI-compatible server such as
//! Ollama or LM Studio (ADR 0007). Each is a [`Summarizer`]; `crumble-remote` names its
//! [`Provider`](crumble_remote::Provider). No GPUI.

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

use crumble_remote::Provider;

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
    use super::{Speaker, transcript_text};

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
}
