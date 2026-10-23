//! Remote Providers over HTTP, through `crumble-remote`: any whose Models chat, each through the
//! API its [`Chat`] names, one module per API in `remote/`. Each gets the whole Transcript in one
//! request. An API key goes only into its request's header: never into an error, a log, or
//! anything else this module returns.

mod anthropic;
mod gemini;
mod openai;

use anyhow::{Context as _, bail};
use crumble_remote::{
    Chat, Provider, TIMEOUT, base_url, failure, no_response, post_json, without_userinfo,
};

use crate::{Summarizer, prompt};

/// The longest Transcript sent, about 150,000 tokens: under the context of current remote
/// Models and large local ones. ponytail: a fixed limit; ask each server for its context if a
/// smaller one turns long Transcripts away.
const MAX_CHARS: usize = 600_000;

/// A remote Provider's Model, reached through the chat API `crumble-remote` names for it.
pub struct Remote {
    /// Who runs the Model: one of [`Provider::REMOTE`] whose Models chat.
    pub provider: Provider,
    /// The address requests go to, such as `https://api.anthropic.com` or
    /// `http://localhost:11434/v1`: the caller's `provider.remote().base_url(saved)`, the one
    /// the user saved or else the Provider's usual one.
    pub base_url: String,
    /// The API key, from the Keychain; `None` when there is none, which only a server the user
    /// runs may accept.
    pub api_key: Option<String>,
    /// The Model id, such as `claude-sonnet-5-5`.
    pub model: String,
}

impl Summarizer for Remote {
    fn provider(&self) -> Provider {
        self.provider
    }

    fn model(&self) -> &str {
        self.model.trim()
    }

    fn summarize(&self, transcript: &str, progress: &mut dyn FnMut(f32)) -> anyhow::Result<String> {
        fits(transcript)?;
        let summary = self.ask(
            &prompt::summary_instructions(false),
            &prompt::prompt(transcript, false),
        )?;
        progress(1.0);
        Ok(summary)
    }
}

impl Remote {
    /// Sends the prompt and returns the Model's answer, or why there is none. Settings missing
    /// for the request fail before it is sent.
    fn ask(&self, instructions: &str, prompt: &str) -> anyhow::Result<String> {
        let provider = self.provider;
        let name = provider.name();
        let remote = (provider.remote()).with_context(|| format!("{name} isn't remote."))?;
        let chat = (remote.chat()).with_context(|| format!("{name} doesn't make Summaries."))?;
        let key = (self.api_key.as_deref()).filter(|key| !key.trim().is_empty());
        if key.is_none() && remote.api_key().is_some_and(|format| format.required()) {
            bail!("Add an API key for {name} in Settings.");
        }
        let base = base_url(&self.base_url)?;
        let model = self.model.trim();
        if model.is_empty() {
            match provider {
                Provider::OpenAiCompatible => bail!("Enter the server's Model in Settings."),
                _ => bail!("Choose a Model for {name} in Settings."),
            }
        }
        let (url, body) = match chat {
            Chat::AnthropicMessages => anthropic::request(base, model, instructions, prompt),
            Chat::OpenAiChat => openai::request(base, model, instructions, prompt),
            Chat::GeminiGenerateContent => gemini::request(base, model, instructions, prompt),
        };
        let headers = remote.headers(key);
        let headers: Vec<(&str, &str)> = (headers.iter())
            .map(|(header, value)| (*header, value.as_str()))
            .collect();
        let (status, answer) =
            post_json(&url, &headers, &body).map_err(|error| match provider {
                // The address may hold a user name and password.
                Provider::OpenAiCompatible => no_response(
                    &without_userinfo(base),
                    "Check that the server is running.",
                    TIMEOUT,
                    &error,
                ),
                _ => no_response(name, "Check the internet connection.", TIMEOUT, &error),
            })?;
        if status != 200 {
            return Err(failure(provider, model, status, &answer, key));
        }
        // Named as `failure` names it.
        let who = match provider {
            Provider::OpenAiCompatible => "The server",
            _ => name,
        };
        match chat {
            Chat::AnthropicMessages => anthropic::text(who, &answer),
            Chat::OpenAiChat => openai::text(who, &answer),
            Chat::GeminiGenerateContent => gemini::text(who, &answer),
        }
    }
}

/// Errors when `transcript` is longer than any remote Model reads.
fn fits(transcript: &str) -> anyhow::Result<()> {
    if transcript.len() > MAX_CHARS {
        bail!("The Transcript is too long to summarize in one request.");
    }
    Ok(())
}

/// `text` without the spaces around it, or an error when the Model wrote nothing.
fn answered(text: &str) -> anyhow::Result<String> {
    let text = text.trim();
    if text.is_empty() {
        bail!("The Model answered with nothing.");
    }
    Ok(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{MAX_CHARS, Remote, fits};
    use crate::Summarizer;
    use crumble_remote::Provider;
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    fn remote(provider: Provider, base_url: &str, api_key: Option<&str>, model: &str) -> Remote {
        Remote {
            provider,
            base_url: base_url.to_owned(),
            api_key: api_key.map(str::to_owned),
            model: model.to_owned(),
        }
    }

    /// What summarizing a short Transcript with `remote` fails with, or `None` if it worked.
    fn failure(remote: &Remote) -> Option<String> {
        (remote.summarize("You: Hi.", &mut |_| {}))
            .map_err(|error| format!("{error:#}"))
            .err()
    }

    /// Answers one request on this Mac with 200 and `answer`. Gives the address to send it to,
    /// and the request as received, in lowercase.
    fn serve_once(answer: &'static str) -> (String, JoinHandle<String>) {
        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(error) => panic!("no port to listen on: {error}"),
        };
        let address = match listener.local_addr() {
            Ok(address) => address,
            Err(error) => panic!("no address: {error}"),
        };
        let served = thread::spawn(move || {
            let mut stream = match listener.accept() {
                Ok((stream, _)) => stream,
                Err(error) => panic!("no request: {error}"),
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
            let mut request = Vec::new();
            let mut buffer = [0; 8192];
            while !received_all(&request) {
                match stream.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => request.extend_from_slice(&buffer[..read]),
                }
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
                 connection: close\r\n\r\n{answer}",
                answer.len()
            );
            let _ = stream.write_all(response.as_bytes());
            String::from_utf8_lossy(&request).to_lowercase()
        });
        (format!("http://{address}"), served)
    }

    /// Whether `request` holds its whole head and the body its `content-length` announces.
    fn received_all(request: &[u8]) -> bool {
        let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            return false;
        };
        let head = String::from_utf8_lossy(&request[..end]).to_lowercase();
        let length = (head.lines())
            .find_map(|line| line.strip_prefix("content-length:"))
            .and_then(|length| length.trim().parse().ok())
            .unwrap_or(0);
        request.len() >= end + 4 + length
    }

    /// Summarizes with `provider`'s Model `m1`, at a server on this Mac that answers `answer`,
    /// and checks that the request starts with `line` and carries `headers`, in lowercase.
    /// Gives the request.
    fn assert_served(
        provider: Provider,
        key: Option<&str>,
        answer: &'static str,
        line: &str,
        headers: &[&str],
    ) -> String {
        let (base, served) = serve_once(answer);
        let remote = remote(provider, &base, key, " m1 ");
        assert_eq!(remote.model(), "m1");
        match remote.summarize("You: Hi.", &mut |_| {}) {
            Ok(summary) => assert_eq!(summary, "Planned.", "{provider:?}"),
            // The server still waits for a request, so don't join it.
            Err(error) => panic!("{provider:?}: {error:#}"),
        }
        let request = match served.join() {
            Ok(request) => request,
            Err(_) => panic!("{provider:?}: the server failed"),
        };
        assert!(request.starts_with(line), "{provider:?}: {request}");
        for header in headers {
            assert!(
                request.contains(header),
                "{provider:?}: {header} in {request}"
            );
        }
        request
    }

    #[test]
    fn each_chat_api_gets_its_path_headers_and_answer() {
        let anthropic =
            r#"{"content":[{"type":"text","text":"Planned."}],"stop_reason":"end_turn"}"#;
        let gemini =
            r#"{"candidates":[{"content":{"parts":[{"text":"Planned."}]},"finishReason":"STOP"}]}"#;
        let openai = r#"{"choices":[{"message":{"content":"Planned."},"finish_reason":"stop"}]}"#;
        assert_served(
            Provider::Anthropic,
            Some("sk-test"),
            anthropic,
            "post /v1/messages ",
            &["x-api-key: sk-test", "anthropic-version: 2023-06-01"],
        );
        assert_served(
            Provider::Gemini,
            Some("sk-test"),
            gemini,
            "post /v1beta/models/m1:generatecontent ",
            &["x-goog-api-key: sk-test"],
        );
        assert_served(
            Provider::OpenRouter,
            Some("sk-test"),
            openai,
            "post /chat/completions ",
            &[
                "authorization: bearer sk-test",
                "x-openrouter-title: crumble",
            ],
        );
        let keyless = assert_served(
            Provider::OpenAiCompatible,
            None,
            openai,
            "post /chat/completions ",
            &[],
        );
        assert!(!keyless.contains("authorization"), "{keyless}");
    }

    #[test]
    fn a_transcript_must_fit_one_request() {
        assert!(fits("You: Hi.").is_ok());
        assert!(fits(&"a".repeat(MAX_CHARS + 1)).is_err());
    }

    #[test]
    fn missing_settings_fail_before_any_request() {
        let anthropic = "https://api.anthropic.com";
        let cases = [
            (
                remote(Provider::Apple, "", None, "m1"),
                "Apple, on this Mac isn't remote.",
            ),
            (
                remote(
                    Provider::ElevenLabs,
                    "https://api.elevenlabs.io",
                    None,
                    "m1",
                ),
                "ElevenLabs doesn't make Summaries.",
            ),
            (
                remote(Provider::Anthropic, anthropic, None, "claude-sonnet-5-5"),
                "Add an API key for Anthropic in Settings.",
            ),
            (
                remote(
                    Provider::Anthropic,
                    anthropic,
                    Some(" "),
                    "claude-sonnet-5-5",
                ),
                "Add an API key for Anthropic in Settings.",
            ),
            (
                remote(Provider::Anthropic, anthropic, Some("sk-test-secret"), " "),
                "Choose a Model for Anthropic in Settings.",
            ),
            (
                remote(Provider::OpenAiCompatible, "", None, "llama3.2"),
                "Enter the server's address in Settings.",
            ),
            (
                remote(
                    Provider::OpenAiCompatible,
                    "localhost:11434/v1",
                    None,
                    "llama3.2",
                ),
                "Start the server's address with http:// or https://.",
            ),
            (
                remote(
                    Provider::OpenAiCompatible,
                    "http://localhost:11434/v1",
                    None,
                    "",
                ),
                "Enter the server's Model in Settings.",
            ),
        ];
        for (remote, expected) in cases {
            assert_eq!(failure(&remote).as_deref(), Some(expected));
        }
        let long = remote(Provider::Anthropic, anthropic, Some("sk-test"), "m1");
        let error = long.summarize(&"a".repeat(MAX_CHARS + 1), &mut |_| {});
        assert_eq!(
            error.map_err(|error| error.to_string()).err().as_deref(),
            Some("The Transcript is too long to summarize in one request.")
        );
    }

    #[test]
    fn an_unreachable_provider_never_shows_the_key() {
        // Nothing listens on port 9 of this Mac.
        let server = remote(
            Provider::OpenAiCompatible,
            "http://me:pw@127.0.0.1:9/v1/",
            Some("sk-test-secret"),
            "llama3.2",
        );
        assert_eq!(
            failure(&server).as_deref(),
            Some("Couldn't reach http://127.0.0.1:9/v1. Check that the server is running.")
        );
        let company = remote(
            Provider::Anthropic,
            "http://127.0.0.1:9",
            Some("sk-test-secret"),
            "claude-sonnet-5-5",
        );
        assert_eq!(
            failure(&company).as_deref(),
            Some("Couldn't reach Anthropic. Check the internet connection.")
        );
    }
}
