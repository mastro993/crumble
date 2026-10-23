//! Anthropic's Messages API: `POST {base}/v1/messages`.

use anyhow::{Context as _, bail};
use serde_json::{Value, json};

use super::answered;

/// The most tokens a Summary may take, any thinking included. The Messages API needs a cap; the
/// other APIs get none, as the parameter's name differs between servers and a Summary that runs
/// out fails either way.
const MAX_TOKENS: u32 = 16_000;

/// Where a request for `model` goes below `base`, and what it sends.
pub(super) fn request(
    base: &str,
    model: &str,
    instructions: &str,
    prompt: &str,
) -> (String, Value) {
    let body = json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "system": instructions,
        "messages": [{ "role": "user", "content": prompt }],
    });
    (format!("{base}/v1/messages"), body)
}

/// The Summary in `who`'s response, or why there is none.
pub(super) fn text(who: &str, response: &str) -> anyhow::Result<String> {
    let response: Value = serde_json::from_str(response)
        .with_context(|| format!("{who}'s answer wasn't understood."))?;
    match response["stop_reason"].as_str() {
        Some("refusal") => bail!("{who} declined to summarize this Transcript."),
        Some("max_tokens") => bail!("{who} ran out of room before finishing the Summary."),
        _ => {}
    }
    let text: String = (response["content"].as_array().into_iter().flatten())
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .collect();
    answered(&text)
}

#[cfg(test)]
mod tests {
    use super::{request, text};
    use serde_json::json;

    fn message(result: anyhow::Result<String>) -> Option<String> {
        result.map_err(|error| error.to_string()).err()
    }

    #[test]
    fn a_request_carries_the_model_instructions_prompt_and_cap() {
        let (url, body) = request(
            "https://api.anthropic.com",
            "claude-sonnet-5-5",
            "Be brief.",
            "Transcript:\n\nYou: Hi.",
        );
        assert_eq!(url, "https://api.anthropic.com/v1/messages");
        assert_eq!(
            body,
            json!({
                "model": "claude-sonnet-5-5",
                "max_tokens": 16_000,
                "system": "Be brief.",
                "messages": [{ "role": "user", "content": "Transcript:\n\nYou: Hi." }],
            })
        );
    }

    #[test]
    fn a_response_gives_its_text_blocks() {
        let answer = r#"{"id":"msg_1","type":"message","role":"assistant","content":[
            {"type":"thinking","thinking":"Hmm."},
            {"type":"text","text":"Planned the launch."},{"type":"text","text":"\n\n## Decisions"}],
            "stop_reason":"end_turn"}"#;
        assert_eq!(
            text("Anthropic", answer).ok().as_deref(),
            Some("Planned the launch.\n\n## Decisions")
        );
    }

    #[test]
    fn a_refusal_a_cut_answer_or_an_empty_one_is_an_error() {
        assert_eq!(
            message(text(
                "Anthropic",
                r#"{"content":[],"stop_reason":"refusal"}"#
            ))
            .as_deref(),
            Some("Anthropic declined to summarize this Transcript.")
        );
        let cut = r#"{"content":[{"type":"text","text":"Planned"}],"stop_reason":"max_tokens"}"#;
        assert_eq!(
            message(text("Anthropic", cut)).as_deref(),
            Some("Anthropic ran out of room before finishing the Summary.")
        );
        assert_eq!(
            message(text(
                "Anthropic",
                r#"{"content":[],"stop_reason":"end_turn"}"#
            ))
            .as_deref(),
            Some("The Model answered with nothing.")
        );
        assert_eq!(
            message(text("Anthropic", "<html>")).as_deref(),
            Some("Anthropic's answer wasn't understood.")
        );
    }
}
