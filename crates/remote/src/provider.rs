//! The Providers, the jobs their Models do, and which of them keep what they read on the Mac.

use crate::address::is_loopback;
use crate::remote::{self, Remote};

/// Where a Model runs. Its [`Provider::id`] is what Crumble stores; a remote Provider's
/// [`Remote`] says how to reach it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Provider {
    /// Apple's on-device Models: SpeechAnalyzer for Transcripts, Foundation Models for
    /// Summaries and Intelligence.
    Apple,
    /// Crumble itself, running a Local Model it downloaded (ADR 0010). Transcripts only.
    Local,
    /// Anthropic's Claude API.
    Anthropic,
    /// OpenAI's API.
    OpenAi,
    /// Google's Gemini API.
    Gemini,
    /// ElevenLabs' Speech to Text.
    ElevenLabs,
    /// OpenRouter, which passes requests on to many companies' Models.
    OpenRouter,
    /// DeepSeek's API.
    DeepSeek,
    /// xAI's API.
    Xai,
    /// A server the user picks that speaks OpenAI's API, such as Ollama or LM Studio on the
    /// Mac, or a Whisper server.
    OpenAiCompatible,
}

/// What a Model does for Crumble.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Job {
    /// Turns a Recording into a Transcript.
    Transcription,
    /// Turns a Transcript into a Summary.
    Summary,
    /// Answers in general: Crumble's chat.
    Intelligence,
}

impl Provider {
    /// Every Provider: the two on the Mac, then [`Provider::REMOTE`].
    pub const ALL: [Provider; 10] = [
        Provider::Apple,
        Provider::Local,
        Provider::Anthropic,
        Provider::OpenAi,
        Provider::Gemini,
        Provider::ElevenLabs,
        Provider::OpenRouter,
        Provider::DeepSeek,
        Provider::Xai,
        Provider::OpenAiCompatible,
    ];

    /// The remote Providers, in the order Settings lists them: the companies, then a server
    /// the user picks.
    pub const REMOTE: [Provider; 8] = [
        Provider::Anthropic,
        Provider::OpenAi,
        Provider::Gemini,
        Provider::ElevenLabs,
        Provider::OpenRouter,
        Provider::DeepSeek,
        Provider::Xai,
        Provider::OpenAiCompatible,
    ];

    /// Its stable id, as stored and as its Keychain item's account. Never change one: Notes,
    /// choices and consents keep it.
    pub fn id(self) -> &'static str {
        match self {
            Provider::Apple => "apple",
            Provider::Local => "local",
            Provider::Anthropic => "anthropic",
            Provider::OpenAi => "openai",
            Provider::Gemini => "gemini",
            Provider::ElevenLabs => "elevenlabs",
            Provider::OpenRouter => "openrouter",
            Provider::DeepSeek => "deepseek",
            Provider::Xai => "xai",
            Provider::OpenAiCompatible => "openai-compatible",
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
            Provider::Local => "Crumble, on this Mac",
            Provider::Anthropic => "Anthropic",
            Provider::OpenAi => "OpenAI",
            Provider::Gemini => "Google Gemini",
            Provider::ElevenLabs => "ElevenLabs",
            Provider::OpenRouter => "OpenRouter",
            Provider::DeepSeek => "DeepSeek",
            Provider::Xai => "xAI",
            Provider::OpenAiCompatible => "OpenAI-compatible server",
        }
    }

    /// How to reach it, for a remote Provider; `None` for Apple's Models and Local Models.
    pub fn remote(self) -> Option<&'static Remote> {
        match self {
            Provider::Apple | Provider::Local => None,
            Provider::Anthropic => Some(&remote::anthropic::REMOTE),
            Provider::OpenAi => Some(&remote::openai::REMOTE),
            Provider::Gemini => Some(&remote::gemini::REMOTE),
            Provider::ElevenLabs => Some(&remote::elevenlabs::REMOTE),
            Provider::OpenRouter => Some(&remote::openrouter::REMOTE),
            Provider::DeepSeek => Some(&remote::deepseek::REMOTE),
            Provider::Xai => Some(&remote::xai::REMOTE),
            Provider::OpenAiCompatible => Some(&remote::openai_compatible::REMOTE),
        }
    }

    /// Whether its Models do `job`: Apple's do all three, Local Models transcribe, and a
    /// remote Provider does what its API offers.
    pub fn does(self, job: Job) -> bool {
        match (self, self.remote()) {
            (_, Some(remote)) => remote.does(job),
            (Provider::Local, None) => job == Job::Transcription,
            (_, None) => true,
        }
    }
}

/// Whether `provider` keeps what it reads on this Mac: Apple's Models, Local Models, or a server
/// the user picks, one with no usual address, whose address, `base_url`, has the host
/// `localhost`, `127.0.0.1` or `::1`, such as Ollama. Only such a Provider summarizes older
/// Transcripts without being asked, and only one that doesn't needs consent before it gets
/// audio (ADRs 0007, 0008, 0010 and 0011). Any other address counts as remote, even one that
/// happens to be this Mac. A company's Models never run on the Mac, so its API counts as remote
/// even through a proxy at a loopback address, which passes what it reads on to the company.
pub fn runs_locally(provider: Provider, base_url: &str) -> bool {
    match provider.remote() {
        None => true,
        Some(remote) => remote.default_base_url().is_none() && is_loopback(base_url),
    }
}

#[cfg(test)]
mod tests {
    use super::{Job, Provider, runs_locally};

    #[test]
    fn provider_ids_round_trip_and_never_take_the_old_ones() {
        for provider in Provider::ALL {
            assert_eq!(Provider::from_id(provider.id()), Some(provider));
        }
        // Migration 000011 renamed these; nothing may read them as a Provider again.
        assert_eq!(Provider::from_id("claude"), None);
        assert_eq!(Provider::from_id("copilot"), None);
    }

    #[test]
    fn all_is_the_mac_s_two_then_remote() {
        assert_eq!(Provider::ALL[..2], [Provider::Apple, Provider::Local]);
        assert_eq!(Provider::ALL[2..], Provider::REMOTE);
    }

    #[test]
    fn remote_lists_exactly_the_providers_with_an_api() {
        for provider in Provider::ALL {
            assert_eq!(
                provider.remote().is_some(),
                Provider::REMOTE.contains(&provider),
                "{provider:?}"
            );
        }
    }

    #[test]
    fn apple_does_every_job_and_local_models_only_transcribe() {
        for job in [Job::Transcription, Job::Summary, Job::Intelligence] {
            assert!(Provider::Apple.does(job));
            assert_eq!(Provider::Local.does(job), job == Job::Transcription);
        }
    }

    #[test]
    fn only_the_mac_s_models_and_a_server_at_a_loopback_address_run_locally() {
        assert!(runs_locally(Provider::Apple, ""));
        assert!(runs_locally(Provider::Local, "https://api.openai.com/v1"));
        assert!(runs_locally(
            Provider::OpenAiCompatible,
            "http://localhost:11434/v1"
        ));
        // A proxy on this Mac passes a company's requests on to the company.
        for company in Provider::REMOTE {
            if company != Provider::OpenAiCompatible {
                assert!(
                    !runs_locally(company, "http://127.0.0.1:4000"),
                    "{company:?}"
                );
            }
        }
        assert!(!runs_locally(
            Provider::Anthropic,
            "https://api.anthropic.com"
        ));
        assert!(!runs_locally(Provider::OpenAiCompatible, ""));
        assert!(!runs_locally(
            Provider::OpenAiCompatible,
            "http://192.168.1.20:8765/v1"
        ));
    }
}
