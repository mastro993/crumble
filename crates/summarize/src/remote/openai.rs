//! OpenAI's chat completions, `POST {base}/chat/completions`, which OpenRouter, DeepSeek, xAI and
//! servers such as Ollama and LM Studio also speak.

use anyhow::{Context as _, bail};
use serde_json::{Value, json};

use super::answered;

/// Where a request for `model` goes below `base`, and what it sends. It sets no token cap: the
/// parameter's name differs between servers.
pub(super) fn request(
    base: &str,
    model: &str,
    instructions: &str,
    prompt: &str,
) -> (String, Value) {
    let body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": instructions },
            { "role": "user", "content": prompt },
        ],
    });
    (format!("{base}/chat/completions"), body)
}

/// The Summary in `who`'s response, or why there is none. Only the message's `content` is the
/// Summary: a Model that thinks first, such as DeepSeek's, writes its thinking apart, in
/// `reasoning_content`.
pub(super) fn text(who: &str, response: &str) -> anyhow::Result<String> {
    let response: Value = serde_json::from_str(response)
        .with_context(|| format!("{who}'s answer wasn't understood."))?;
    let choice = &response["choices"][0];
    match choice["finish_reason"].as_str() {
        Some("length") => bail!("{who} ran out of room before finishing the Summary."),
        Some("content_filter") => bail!("{who} declined to summarize this Transcript."),
        _ => {}
    }
    answered(choice["message"]["content"].as_str().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::{request, text};
    use serde_json::json;

    fn message(result: anyhow::Result<String>) -> Option<String> {
        result.map_err(|error| error.to_string()).err()
    }

    #[test]
    fn a_request_carries_the_model_instructions_and_prompt() {
        let (url, body) = request("http://localhost:11434/v1", "llama3.2", "Be brief.", "Hi");
        assert_eq!(url, "http://localhost:11434/v1/chat/completions");
        assert_eq!(
            body,
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
    fn a_response_gives_its_content_without_any_thinking() {
        let answer = r#"{"choices":[{"index":0,"message":{"role":"assistant",
            "content":" Planned the launch.\n"},"finish_reason":"stop"}]}"#;
        assert_eq!(
            text("The server", answer).ok().as_deref(),
            Some("Planned the launch.")
        );
        let deepseek = r#"{"choices":[{"index":0,"message":{"role":"assistant",
            "reasoning_content":"The user wants a summary.","content":"Planned the launch."},
            "finish_reason":"stop"}]}"#;
        assert_eq!(
            text("DeepSeek", deepseek).ok().as_deref(),
            Some("Planned the launch.")
        );
    }

    #[test]
    fn a_cut_a_filtered_or_an_empty_answer_is_an_error() {
        let cut = r#"{"choices":[{"message":{"content":"Planned"},"finish_reason":"length"}]}"#;
        assert_eq!(
            message(text("OpenAI", cut)).as_deref(),
            Some("OpenAI ran out of room before finishing the Summary.")
        );
        let filtered =
            r#"{"choices":[{"message":{"content":null},"finish_reason":"content_filter"}]}"#;
        assert_eq!(
            message(text("xAI", filtered)).as_deref(),
            Some("xAI declined to summarize this Transcript.")
        );
        let thought_only = r#"{"choices":[{"message":{"reasoning_content":"Hmm.",
            "content":""},"finish_reason":"stop"}]}"#;
        assert_eq!(
            message(text("DeepSeek", thought_only)).as_deref(),
            Some("The Model answered with nothing.")
        );
        assert!(text("The server", r#"{"choices":[]}"#).is_err());
        assert_eq!(
            message(text("The server", "<html>")).as_deref(),
            Some("The server's answer wasn't understood.")
        );
    }
}
