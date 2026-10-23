//! The line under each Provider's name in Settings › Providers: what it does, and where a
//! server the user runs is.

use crumble_remote::{Job, Provider, host, runs_locally};

/// The line under `provider`'s name, given whether it is `set_up` and the address saved for it,
/// "" for its usual one: "Intelligence" when it chats, for Summaries and Chat, and
/// "Transcription" when it transcribes. A server the user runs, once set up, adds its address's
/// host, saying when that is this Mac.
pub(super) fn detail_line(provider: Provider, set_up: bool, saved_base_url: &str) -> String {
    let Some(remote) = provider.remote() else {
        return String::new();
    };
    let jobs = [
        (Job::Intelligence, "Intelligence"),
        (Job::Transcription, "Transcription"),
    ];
    let mut parts: Vec<String> = (jobs.into_iter())
        .filter(|&(job, _)| remote.does(job))
        .map(|(_, name)| name.to_owned())
        .collect();
    if set_up && remote.default_base_url().is_none() {
        let address = remote.base_url(saved_base_url);
        parts.push(host(address).unwrap_or_default());
        if runs_locally(provider, address) {
            parts.push("On this Mac".to_owned());
        }
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::detail_line;
    use crumble_remote::Provider;

    #[test]
    fn a_company_shows_what_it_does_whether_set_up_or_not() {
        for set_up in [false, true] {
            assert_eq!(
                detail_line(Provider::OpenAi, set_up, ""),
                "Intelligence · Transcription"
            );
            assert_eq!(detail_line(Provider::Anthropic, set_up, ""), "Intelligence");
            assert_eq!(
                detail_line(Provider::ElevenLabs, set_up, ""),
                "Transcription"
            );
        }
        // A company's own address stays out, even when it was changed.
        assert_eq!(
            detail_line(Provider::OpenAi, true, "https://proxy.example.com/v1"),
            "Intelligence · Transcription"
        );
    }

    #[test]
    fn a_server_adds_its_host_once_set_up() {
        let server = Provider::OpenAiCompatible;
        assert_eq!(
            detail_line(server, false, ""),
            "Intelligence · Transcription"
        );
        assert_eq!(
            detail_line(server, true, "http://localhost:11434/v1"),
            "Intelligence · Transcription · localhost · On this Mac"
        );
        assert_eq!(
            detail_line(server, true, "https://me:secret@LLM.example.com/v1"),
            "Intelligence · Transcription · llm.example.com"
        );
    }

    #[test]
    fn apple_and_local_models_have_no_line() {
        assert_eq!(detail_line(Provider::Apple, true, ""), "");
        assert_eq!(detail_line(Provider::Local, true, ""), "");
    }
}
