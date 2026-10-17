//! Remote Providers over HTTP, through `crumble-remote`: the Claude API and OpenAI-compatible
//! servers. Each gets the whole Transcript in one request. An API key goes only into its
//! request's header: never into an error, a log, or anything else this module returns.

use anyhow::{Context as _, anyhow, bail};
use crumble_remote::{Provider, failure, post_json, without_userinfo};
use serde_json::{Value, json};

use crate::{Summarizer, prompt};

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
        let (status, text) = post_json(CLAUDE_URL, &headers, &body).map_err(|error| {
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
        let (status, text) = post_json(&url, &headers, &body).map_err(|error| {
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

/// Errors when `transcript` is longer than any remote Model reads.
fn fits(transcript: &str) -> anyhow::Result<()> {
    if transcript.len() > MAX_CHARS {
        bail!("The Transcript is too long to summarize in one request.");
    }
    Ok(())
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
    let response: Value =
        serde_json::from_str(response).context("Claude's answer wasn't understood.")?;
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
    let response: Value =
        serde_json::from_str(response).context("The server's answer wasn't understood.")?;
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

#[cfg(test)]
mod tests {
    use super::{
        Claude, OpenAiCompatible, claude_body, claude_text, fits, openai_body, openai_text,
    };
    use crate::Summarizer;
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
        assert_eq!(
            openai_text("<html>")
                .map_err(|error| error.to_string())
                .err()
                .as_deref(),
            Some("The server's answer wasn't understood.")
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
