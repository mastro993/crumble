//! Remote Providers over HTTP: the Claude API and OpenAI-compatible servers. Each gets the
//! whole Transcript in one request. An API key goes only into its request's header: never into
//! an error, a log, or anything else this module returns.

use std::time::Duration;

use anyhow::{anyhow, bail};
use serde_json::{Value, json};

use crate::{Provider, Summarizer, prompt, url_parts};

/// The Claude API's address.
const CLAUDE_URL: &str = "https://api.anthropic.com/v1/messages";
/// The Messages API version Crumble speaks.
const CLAUDE_VERSION: &str = "2023-06-01";
/// The most tokens Claude may spend on a Summary, any thinking included. An OpenAI-compatible
/// server gets no cap: the parameter's name differs between servers, and a Summary that runs out
/// fails either way.
const CLAUDE_MAX_TOKENS: u32 = 16_000;
/// The longest Transcript sent, about 150,000 tokens: under the context of current Claude
/// Models and large local ones. ponytail: a fixed limit; ask each server for its context if a
/// smaller one turns long Transcripts away.
const MAX_CHARS: usize = 600_000;
/// How long a request may take in all; a local Model can take minutes on an hour's speech.
const TIMEOUT: Duration = Duration::from_secs(600);

/// The Claude API, with the user's API key.
pub struct Claude {
    /// The key, from the Keychain.
    pub api_key: String,
    /// The Model id, such as `claude-sonnet-5-5`.
    pub model: String,
}

/// A server that speaks OpenAI's chat completions, such as Ollama or LM Studio.
pub struct OpenAiCompatible {
    /// Its address up to `/chat/completions`, such as `http://localhost:11434/v1`.
    pub base_url: String,
    /// The key, from the Keychain, if the server wants one.
    pub api_key: Option<String>,
    /// The Model's name on that server.
    pub model: String,
}

impl Summarizer for Claude {
    fn provider(&self) -> Provider {
        Provider::Claude
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn summarize(&self, transcript: &str, progress: &mut dyn FnMut(f32)) -> anyhow::Result<String> {
        fits(transcript)?;
        let response = self.ask(
            &prompt::summary_instructions(false),
            &prompt::prompt(transcript, false),
            CLAUDE_MAX_TOKENS,
        )?;
        let summary = claude_text(&response)?;
        progress(1.0);
        Ok(summary)
    }
}

impl Claude {
    /// Sends a tiny prompt, to check the key and the Model: any answer will do.
    pub fn check(&self) -> anyhow::Result<()> {
        self.ask("Reply with OK.", "OK?", 16).map(drop)
    }

    /// Sends the prompt and returns the response's body, or why it failed.
    fn ask(&self, instructions: &str, prompt: &str, max_tokens: u32) -> anyhow::Result<String> {
        if self.api_key.trim().is_empty() {
            bail!("Add a Claude API key in Settings.");
        }
        if self.model.trim().is_empty() {
            bail!("Enter a Claude Model in Settings.");
        }
        let body = claude_body(&self.model, instructions, prompt, max_tokens);
        let headers = [
            ("x-api-key", self.api_key.as_str()),
            ("anthropic-version", CLAUDE_VERSION),
        ];
        let (status, text) = post(CLAUDE_URL, &headers, &body).map_err(|error| {
            eprintln!("Could not reach Claude: {error}");
            anyhow!("Couldn't reach Claude. Check the internet connection.")
        })?;
        if status != 200 {
            return Err(failure(Provider::Claude, &self.model, status, &text));
        }
        Ok(text)
    }
}

impl Summarizer for OpenAiCompatible {
    fn provider(&self) -> Provider {
        Provider::OpenAiCompatible
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn summarize(&self, transcript: &str, progress: &mut dyn FnMut(f32)) -> anyhow::Result<String> {
        fits(transcript)?;
        let response = self.ask(
            &prompt::summary_instructions(false),
            &prompt::prompt(transcript, false),
        )?;
        let summary = openai_text(&response)?;
        progress(1.0);
        Ok(summary)
    }
}

impl OpenAiCompatible {
    /// Sends a tiny prompt, to check the address, the key and the Model: any answer will do.
    pub fn check(&self) -> anyhow::Result<()> {
        self.ask("Reply with OK.", "OK?").map(drop)
    }

    /// Sends the prompt and returns the response's body, or why it failed.
    fn ask(&self, instructions: &str, prompt: &str) -> anyhow::Result<String> {
        let base = self.base_url.trim().trim_end_matches('/');
        if base.is_empty() {
            bail!("Enter the server's address in Settings.");
        }
        if !base.contains("://") {
            bail!("Start the server's address with http:// or https://.");
        }
        if self.model.trim().is_empty() {
            bail!("Enter the server's Model in Settings.");
        }
        let url = format!("{base}/chat/completions");
        let body = openai_body(&self.model, instructions, prompt);
        let authorization = self.api_key.as_ref().map(|key| format!("Bearer {key}"));
        let headers: Vec<(&str, &str)> = authorization
            .iter()
            .map(|value| ("authorization", value.as_str()))
            .collect();
        let (status, text) = post(&url, &headers, &body).map_err(|error| {
            // The address may hold a user name and password.
            let base = without_userinfo(base);
            eprintln!("Could not reach {base}: {error}");
            anyhow!("Couldn't reach {base}. Check that the server is running.")
        })?;
        if status != 200 {
            return Err(failure(
                Provider::OpenAiCompatible,
                &self.model,
                status,
                &text,
            ));
        }
        Ok(text)
    }
}

/// `url` without the `user:password@` before its host.
fn without_userinfo(url: &str) -> String {
    let [scheme, _, authority, rest] = url_parts(url);
    format!("{scheme}{authority}{rest}")
}

/// Errors when `transcript` is longer than any remote Model reads.
fn fits(transcript: &str) -> anyhow::Result<()> {
    if transcript.len() > MAX_CHARS {
        bail!("The Transcript is too long to summarize in one request.");
    }
    Ok(())
}

/// POSTs `body` as JSON and returns the status and the response's text, whatever the status.
/// Only a request that got no response errors; its error holds no header.
fn post(url: &str, headers: &[(&str, &str)], body: &Value) -> Result<(u16, String), ureq::Error> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    let mut request = agent.post(url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let mut response = request.send_json(body)?;
    let status = response.status().as_u16();
    Ok((status, response.body_mut().read_to_string()?))
}

fn claude_body(model: &str, instructions: &str, prompt: &str, max_tokens: u32) -> Value {
    json!({
        "model": model,
        "max_tokens": max_tokens,
        "system": instructions,
        "messages": [{ "role": "user", "content": prompt }],
    })
}

fn openai_body(model: &str, instructions: &str, prompt: &str) -> Value {
    json!({
        "model": model,
        "messages": [
            { "role": "system", "content": instructions },
            { "role": "user", "content": prompt },
        ],
    })
}

/// The text of a Messages API response.
fn claude_text(response: &str) -> anyhow::Result<String> {
    let response: Value = serde_json::from_str(response)?;
    match response["stop_reason"].as_str() {
        Some("refusal") => bail!("Claude declined to summarize this Transcript."),
        Some("max_tokens") => bail!("Claude ran out of room before finishing the Summary."),
        _ => {}
    }
    let text: String = response["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .collect();
    answered(text)
}

/// The text of a chat completions response.
fn openai_text(response: &str) -> anyhow::Result<String> {
    let response: Value = serde_json::from_str(response)?;
    if response["choices"][0]["finish_reason"] == "length" {
        bail!("The Model ran out of room before finishing the Summary.");
    }
    let text = response["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or_default();
    answered(text.to_owned())
}

fn answered(text: String) -> anyhow::Result<String> {
    let text = text.trim();
    if text.is_empty() {
        bail!("The Model answered with nothing.");
    }
    Ok(text.to_owned())
}

/// What to tell the user when `provider` answered `status` with `body`.
fn failure(provider: Provider, model: &str, status: u16, body: &str) -> anyhow::Error {
    let who = match provider {
        Provider::Claude => "Claude",
        _ => "The server",
    };
    // Both APIs put a message under `error`; Ollama and others may put a bare string there.
    let body: Value = serde_json::from_str(body).unwrap_or_default();
    let said = body["error"]["message"]
        .as_str()
        .or_else(|| body["error"].as_str())
        .unwrap_or_default();
    match status {
        401 => anyhow!("{who} didn't accept the API key. Check it in Settings."),
        403 => anyhow!("{who} didn't allow this API key to use {model}."),
        404 => anyhow!("{who} has no Model named {model}. Check the Model in Settings."),
        429 => anyhow!("{who} is getting too many requests. Try again in a minute."),
        500.. => anyhow!("{who} had a problem ({status}). Try again later."),
        _ if !said.is_empty() => anyhow!("{who} turned the request down: {said}"),
        _ => anyhow!("{who} turned the request down ({status})."),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Claude, OpenAiCompatible, claude_body, claude_text, failure, fits, openai_body,
        openai_text, without_userinfo,
    };
    use crate::{Provider, Summarizer};
    use serde_json::json;

    #[test]
    fn bodies_carry_the_model_instructions_and_prompt() {
        assert_eq!(
            claude_body(
                "claude-sonnet-5-5",
                "Be brief.",
                "Transcript:\n\nYou: Hi.",
                64
            ),
            json!({
                "model": "claude-sonnet-5-5",
                "max_tokens": 64,
                "system": "Be brief.",
                "messages": [{ "role": "user", "content": "Transcript:\n\nYou: Hi." }],
            })
        );
        assert_eq!(
            openai_body("llama3.2", "Be brief.", "Hi"),
            json!({
                "model": "llama3.2",
                "messages": [
                    { "role": "system", "content": "Be brief." },
                    { "role": "user", "content": "Hi" },
                ],
            })
        );
    }

    #[test]
    fn responses_give_their_text() {
        let claude = r#"{"id":"msg_1","type":"message","role":"assistant","content":[
            {"type":"text","text":"Planned the launch."},{"type":"text","text":"\n\n## Decisions"}],
            "stop_reason":"end_turn"}"#;
        assert_eq!(
            claude_text(claude).ok().as_deref(),
            Some("Planned the launch.\n\n## Decisions")
        );
        let refused = r#"{"content":[],"stop_reason":"refusal"}"#;
        assert!(claude_text(refused).is_err());
        let cut = r#"{"content":[{"type":"text","text":"Planned"}],"stop_reason":"max_tokens"}"#;
        assert!(claude_text(cut).is_err());
        let openai = r#"{"choices":[{"index":0,"message":{"role":"assistant",
            "content":" Planned the launch.\n"},"finish_reason":"stop"}]}"#;
        assert_eq!(
            openai_text(openai).ok().as_deref(),
            Some("Planned the launch.")
        );
        assert!(openai_text(r#"{"choices":[]}"#).is_err());
        let cut = r#"{"choices":[{"message":{"content":"Planned"},"finish_reason":"length"}]}"#;
        assert!(openai_text(cut).is_err());
        assert!(openai_text("<html>").is_err());
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

    #[test]
    fn a_transcript_must_fit_one_request() {
        assert!(fits("You: Hi.").is_ok());
        assert!(fits(&"a".repeat(super::MAX_CHARS + 1)).is_err());
    }

    #[test]
    fn missing_settings_fail_before_any_request() {
        let claude = Claude {
            api_key: "sk-test-secret".into(),
            model: " ".into(),
        };
        let error = claude.summarize("You: Hi.", &mut |_| {});
        assert_eq!(
            error.map_err(|error| error.to_string()).err().as_deref(),
            Some("Enter a Claude Model in Settings.")
        );
        let server = OpenAiCompatible {
            base_url: String::new(),
            api_key: None,
            model: "llama3.2".into(),
        };
        assert!(server.check().is_err());
        let schemeless = OpenAiCompatible {
            base_url: "localhost:11434/v1".into(),
            api_key: None,
            model: "llama3.2".into(),
        };
        assert_eq!(
            schemeless
                .check()
                .map_err(|error| error.to_string())
                .err()
                .as_deref(),
            Some("Start the server's address with http:// or https://.")
        );
        let keyless = Claude {
            api_key: String::new(),
            model: "claude-sonnet-5-5".into(),
        };
        assert_eq!(
            keyless
                .check()
                .map_err(|error| error.to_string())
                .err()
                .as_deref(),
            Some("Add a Claude API key in Settings.")
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
    fn an_unreachable_server_never_shows_the_key() {
        // Nothing listens on port 9 of this Mac.
        let server = OpenAiCompatible {
            base_url: "http://me:pw@127.0.0.1:9/v1/".into(),
            api_key: Some("sk-test-secret".into()),
            model: "llama3.2".into(),
        };
        let error = server.check().map_err(|error| format!("{error:#}")).err();
        assert_eq!(
            error.as_deref(),
            Some("Couldn't reach http://127.0.0.1:9/v1. Check that the server is running.")
        );
    }
}
