//! Who runs a Model, whether it keeps what it reads on this Mac, and the HTTP that reaches the
//! remote ones, shared by Summaries (ADR 0007) and Transcripts (ADR 0008). An API key goes only
//! into its request's header: never into an error, a log, or anything else this crate returns.
//! No GPUI.

use std::time::Duration;

use anyhow::anyhow;
use serde_json::Value;

/// Where a Model runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Provider {
    /// Apple's on-device models: Foundation Models for Summaries, SpeechAnalyzer for
    /// Transcripts.
    Apple,
    /// Anthropic's Claude API.
    Claude,
    /// A server that speaks OpenAI's API: chat completions, and audio transcriptions.
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

/// Whether `provider` keeps what it reads on this Mac: Apple's Models, or an OpenAI-compatible
/// server at `openai_base_url` whose host is `localhost`, `127.0.0.1` or `::1`. Only such a
/// Provider works on older Recordings and Transcripts without being asked, and only a remote
/// one needs consent before it gets audio (ADRs 0007 and 0008).
pub fn runs_locally(provider: Provider, openai_base_url: &str) -> bool {
    match provider {
        Provider::Apple => true,
        Provider::Claude => false,
        Provider::OpenAiCompatible => {
            let host = host(openai_base_url);
            host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
        }
    }
}

/// `url`'s host, without a port, credentials or IPv6 brackets: `api.openai.com` for
/// `https://api.openai.com/v1`. Empty when there is none.
pub fn host(url: &str) -> &str {
    let url = url.trim();
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    // Credentials before an `@` are not the host.
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    match authority.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next().unwrap_or_default(),
        None => authority.split(':').next().unwrap_or_default(),
    }
}

/// How long a request may take in all: a local Model can take minutes on an hour's speech, and
/// an upload of a long track takes a while too.
const TIMEOUT: Duration = Duration::from_secs(600);

/// POSTs `body` as JSON. See [`post`].
pub fn post_json(
    url: &str,
    headers: &[(&str, &str)],
    body: &Value,
) -> Result<(u16, String), ureq::Error> {
    post(
        url,
        headers,
        "application/json",
        body.to_string().as_bytes(),
    )
}

/// POSTs `body` as `content_type` and returns the status and the response's text, whatever the
/// status, within 10 minutes. Only a request that got no response errors; its error holds no
/// header.
pub fn post(
    url: &str,
    headers: &[(&str, &str)],
    content_type: &str,
    body: &[u8],
) -> Result<(u16, String), ureq::Error> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    let mut request = agent.post(url).header("content-type", content_type);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let mut response = request.send(body)?;
    let status = response.status().as_u16();
    Ok((status, response.body_mut().read_to_string()?))
}

/// What to tell the user when `provider` answered a request for `model` with `status` and
/// `body`: a sentence they can act on.
pub fn failure(provider: Provider, model: &str, status: u16, body: &str) -> anyhow::Error {
    let who = match provider {
        Provider::Claude => "Claude",
        _ => "The server",
    };
    // OpenAI's and Claude's APIs put a message under `error`; Ollama and others may put a bare
    // string there.
    let body: Value = serde_json::from_str(body).unwrap_or_default();
    let said = body["error"]["message"]
        .as_str()
        .or_else(|| body["error"].as_str())
        .unwrap_or_default();
    match status {
        401 => anyhow!("{who} didn't accept the API key. Check it in Settings."),
        403 => anyhow!("{who} didn't allow this API key to use {model}."),
        404 => anyhow!("{who} has no Model named {model}. Check the Model in Settings."),
        413 => anyhow!("{who} turned the request down as too large."),
        429 => anyhow!("{who} is getting too many requests. Try again in a minute."),
        500.. => anyhow!("{who} had a problem ({status}). Try again later."),
        _ if !said.is_empty() => anyhow!("{who} turned the request down: {said}"),
        _ => anyhow!("{who} turned the request down ({status})."),
    }
}

#[cfg(test)]
mod tests {
    use super::{Provider, failure, host, runs_locally};

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
            "localhost:11434/v1",
            " http://user:pw@127.0.0.1/v1 ",
        ] {
            assert!(runs_locally(OpenAiCompatible, local), "{local}");
        }
        for remote in [
            "",
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

    #[test]
    fn a_host_drops_scheme_port_path_and_credentials() {
        assert_eq!(host("https://api.openai.com/v1"), "api.openai.com");
        assert_eq!(host(" http://user:pw@whisper.lan:8000/v1/ "), "whisper.lan");
        assert_eq!(host("https://[::1]:8080/v1"), "::1");
        assert_eq!(host("localhost:11434"), "localhost");
        assert_eq!(host("http://example.com?x=1"), "example.com");
        assert_eq!(host(""), "");
    }

    #[test]
    fn failures_say_what_to_do() {
        let message = |provider, status, body| failure(provider, "m1", status, body).to_string();
        let claude_error = r#"{"type":"error","error":{"type":"invalid_request_error",
            "message":"max_tokens: too large"}}"#;
        assert_eq!(
            message(Provider::Claude, 401, claude_error),
            "Claude didn't accept the API key. Check it in Settings."
        );
        assert_eq!(
            message(
                Provider::OpenAiCompatible,
                404,
                r#"{"error":"model 'm1' not found"}"#
            ),
            "The server has no Model named m1. Check the Model in Settings."
        );
        assert_eq!(
            message(Provider::OpenAiCompatible, 413, ""),
            "The server turned the request down as too large."
        );
        assert_eq!(
            message(Provider::Claude, 429, ""),
            "Claude is getting too many requests. Try again in a minute."
        );
        assert_eq!(
            message(Provider::Claude, 529, ""),
            "Claude had a problem (529). Try again later."
        );
        assert_eq!(
            message(Provider::Claude, 400, claude_error),
            "Claude turned the request down: max_tokens: too large"
        );
        assert_eq!(
            message(Provider::OpenAiCompatible, 400, r#"{"error":"bad input"}"#),
            "The server turned the request down: bad input"
        );
        assert_eq!(
            message(Provider::OpenAiCompatible, 400, "nope"),
            "The server turned the request down (400)."
        );
    }
}
