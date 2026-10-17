//! Who runs a Model, whether it keeps what it reads on this Mac, and the HTTP that reaches the
//! remote ones, shared by Summaries (ADR 0007) and Transcripts (ADR 0008). An API key goes only
//! into its request's header: never into an error, a log, or anything else this crate returns.
//! No GPUI.

use std::time::Duration;

use anyhow::{anyhow, bail};
use serde_json::Value;
use ureq::http::Uri;

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
        Provider::OpenAiCompatible => is_loopback(openai_base_url),
    }
}

/// Whether `url` can be sent to and its host is `localhost`, `127.0.0.1` or `::1`.
fn is_loopback(url: &str) -> bool {
    host(url).is_some_and(|host| host == "localhost" || host == "127.0.0.1" || host == "::1")
}

/// `url`'s host in lowercase, without a port, credentials or IPv6 brackets: `api.openai.com`
/// for `https://api.openai.com/v1`. `None` when requests cannot go to `url`.
pub fn host(url: &str) -> Option<String> {
    let uri = parse(url)?;
    let host = uri.host()?;
    Some(
        host.trim_start_matches('[')
            .trim_end_matches(']')
            .to_ascii_lowercase(),
    )
}

/// `url` with its spaces and trailing `/` trimmed, for a path to follow; or, for the user, why
/// requests cannot go to it.
pub fn base_url(url: &str) -> anyhow::Result<&str> {
    let base = url.trim().trim_end_matches('/');
    if base.is_empty() {
        bail!("Enter the server's address in Settings.");
    }
    if !base.contains("://") {
        bail!("Start the server's address with http:// or https://.");
    }
    if parse(base).is_none() {
        bail!("Check the server's address in Settings.");
    }
    Ok(base)
}

/// `url` read as the HTTP client reads it, so what is judged local or asked about is where a
/// request goes: `None` unless it has an `http` or `https` scheme and a host.
fn parse(url: &str) -> Option<Uri> {
    let uri: Uri = url.trim().parse().ok()?;
    let sendable = matches!(uri.scheme_str(), Some("http" | "https")) && uri.host().is_some();
    sendable.then_some(uri)
}

/// `url` without the `user:password@` before its host, to show in errors and logs, even when
/// it cannot be sent to.
pub fn without_userinfo(url: &str) -> String {
    let [scheme, _, authority, rest] = url_parts(url);
    format!("{scheme}{authority}{rest}")
}

/// `url` in four parts: its scheme with `://` (empty without one), any `user:password@`, its host
/// and port, and the rest.
fn url_parts(url: &str) -> [&str; 4] {
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

/// How long a JSON request may take in all, and the least an upload gets: a local Model can
/// take minutes on an hour's speech.
pub const TIMEOUT: Duration = Duration::from_secs(600);

/// POSTs `body` as JSON, within [`TIMEOUT`]. See [`post`].
pub fn post_json(
    url: &str,
    headers: &[(&str, &str)],
    body: &Value,
) -> Result<(u16, String), ureq::Error> {
    post(
        url,
        headers,
        ("application/json", body.to_string().as_bytes()),
        TIMEOUT,
    )
}

/// POSTs `body`, a content type and its bytes, and returns the status and the response's text,
/// whatever the status, within `timeout`. Only a request that got no response errors; its
/// error holds no header.
pub fn post(
    url: &str,
    headers: &[(&str, &str)],
    (content_type, body): (&str, &[u8]),
    timeout: Duration,
) -> Result<(u16, String), ureq::Error> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
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

/// What to tell the user when a request to `who` got no response, with `hint` saying what to
/// check; the cause is logged. One that ran out of its `timeout` says so, as a slow Model is no
/// unreachable server.
pub fn no_response(who: &str, hint: &str, timeout: Duration, error: &ureq::Error) -> anyhow::Error {
    eprintln!("Could not reach {who}: {error}");
    match error {
        ureq::Error::Timeout(_) => anyhow!(
            "{who} didn't answer within {} minutes. Try again later.",
            timeout.as_secs() / 60
        ),
        _ => anyhow!("Couldn't reach {who}. {hint}"),
    }
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
        // Blame the Model only when the server names it; a bare 404 is an address with nothing
        // behind it.
        404 if said.contains(model) => {
            anyhow!("{who} has no Model named {model}. Check the Model in Settings.")
        }
        404 if !said.is_empty() => anyhow!("{who} turned the request down: {said}"),
        404 => anyhow!("{who} has nothing at that address. Check the address in Settings."),
        413 => anyhow!("{who} turned the request down as too large."),
        429 => anyhow!("{who} is getting too many requests. Try again in a minute."),
        500.. => anyhow!("{who} had a problem ({status}). Try again later."),
        _ if !said.is_empty() => anyhow!("{who} turned the request down: {said}"),
        _ => anyhow!("{who} turned the request down ({status})."),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Provider, TIMEOUT, base_url, failure, host, no_response, runs_locally, without_userinfo,
    };
    use std::time::Duration;

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
            "evil.example.com/p?u=http://localhost/v1",
            "http://evil.example.com\\@localhost/v1",
            "ftp://localhost/v1",
        ] {
            assert!(!runs_locally(OpenAiCompatible, remote), "{remote}");
        }
    }

    #[test]
    fn a_host_drops_scheme_port_path_and_credentials() {
        let host = |url| host(url);
        assert_eq!(
            host("https://API.openai.com/v1").as_deref(),
            Some("api.openai.com")
        );
        assert_eq!(
            host(" http://user:pw@whisper.lan:8000/v1/ ").as_deref(),
            Some("whisper.lan")
        );
        assert_eq!(host("https://[::1]:8080/v1").as_deref(), Some("::1"));
        assert_eq!(
            host("http://example.com?x=1").as_deref(),
            Some("example.com")
        );
        for unsendable in [
            "",
            "localhost:11434",
            "evil.example.com/p?u=http://localhost/v1",
        ] {
            assert_eq!(host(unsendable), None, "{unsendable}");
        }
    }

    #[test]
    fn a_base_url_must_be_sendable() {
        assert_eq!(
            base_url(" http://localhost:8000/v1/ ").ok(),
            Some("http://localhost:8000/v1")
        );
        let error = |url| base_url(url).map_err(|error| error.to_string()).err();
        assert_eq!(
            error(" ").as_deref(),
            Some("Enter the server's address in Settings.")
        );
        assert_eq!(
            error("whisper.lan:8000/v1").as_deref(),
            Some("Start the server's address with http:// or https://.")
        );
        assert_eq!(
            error("evil.example.com/p?u=http://localhost/v1").as_deref(),
            Some("Check the server's address in Settings.")
        );
    }

    #[test]
    fn an_address_loses_its_user_name_and_password() {
        assert_eq!(
            without_userinfo("http://me:pw@127.0.0.1:11434/v1"),
            "http://127.0.0.1:11434/v1"
        );
        assert_eq!(
            without_userinfo("http://localhost:11434/v1/a@b"),
            "http://localhost:11434/v1/a@b"
        );
        assert_eq!(without_userinfo("me@host/v1"), "host/v1");
    }

    #[test]
    fn no_response_tells_a_timeout_apart() {
        let timeout = ureq::Error::Timeout(ureq::Timeout::Global);
        assert_eq!(
            no_response(
                "http://whisper.lan/v1",
                "Check that the server is running.",
                Duration::from_secs(80 * 60),
                &timeout
            )
            .to_string(),
            "http://whisper.lan/v1 didn't answer within 80 minutes. Try again later."
        );
        let refused = ureq::Error::ConnectionFailed;
        assert_eq!(
            no_response(
                "Claude",
                "Check the internet connection.",
                TIMEOUT,
                &refused
            )
            .to_string(),
            "Couldn't reach Claude. Check the internet connection."
        );
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
            message(
                Provider::OpenAiCompatible,
                404,
                r#"{"error":{"message":"Invalid URL (POST /chat/completions)"}}"#
            ),
            "The server turned the request down: Invalid URL (POST /chat/completions)"
        );
        assert_eq!(
            message(Provider::OpenAiCompatible, 404, "404 page not found"),
            "The server has nothing at that address. Check the address in Settings."
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
