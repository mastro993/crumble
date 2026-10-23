//! Google's Gemini API: `POST {base}/v1beta/models/{model}:generateContent`.

use anyhow::{Context as _, bail};
use serde_json::{Value, json};

use super::answered;

/// The `finishReason`s of a candidate Gemini stopped writing because of what it held: safety,
/// prohibited content, recitation, blocklisted terms, or personal data.
const DECLINED: [&str; 5] = [
    "SAFETY",
    "PROHIBITED_CONTENT",
    "RECITATION",
    "BLOCKLIST",
    "SPII",
];

/// Where a request for `model` goes below `base`, and what it sends. It sets no token cap.
pub(super) fn request(
    base: &str,
    model: &str,
    instructions: &str,
    prompt: &str,
) -> (String, Value) {
    let body = json!({
        "systemInstruction": { "parts": [{ "text": instructions }] },
        "contents": [{ "role": "user", "parts": [{ "text": prompt }] }],
    });
    (
        format!("{base}/v1beta/models/{model}:generateContent"),
        body,
    )
}

/// The Summary in `who`'s response, or why there is none: the first candidate's text, without
/// the parts that are the Model's thinking.
pub(super) fn text(who: &str, response: &str) -> anyhow::Result<String> {
    let response: Value = serde_json::from_str(response)
        .with_context(|| format!("{who}'s answer wasn't understood."))?;
    // Set when the prompt itself was blocked, and then there is no candidate.
    if !response["promptFeedback"]["blockReason"].is_null() {
        bail!("{who} declined to summarize this Transcript.");
    }
    let candidate = &response["candidates"][0];
    match candidate["finishReason"].as_str() {
        Some("MAX_TOKENS") => bail!("{who} ran out of room before finishing the Summary."),
        Some(reason) if DECLINED.contains(&reason) => {
            bail!("{who} declined to summarize this Transcript.")
        }
        _ => {}
    }
    let text: String = (candidate["content"]["parts"]
        .as_array()
        .into_iter()
        .flatten())
    .filter(|part| part["thought"] != true)
    .filter_map(|part| part["text"].as_str())
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
    fn a_request_names_the_model_in_its_path_and_carries_the_instructions_and_prompt() {
        let (url, body) = request(
            "https://generativelanguage.googleapis.com",
            "gemini-3.8-flash",
            "Be brief.",
            "Transcript:\n\nYou: Hi.",
        );
        assert_eq!(
            url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.8-flash:generateContent"
        );
        assert_eq!(
            body,
            json!({
                "systemInstruction": { "parts": [{ "text": "Be brief." }] },
                "contents": [{
                    "role": "user",
                    "parts": [{ "text": "Transcript:\n\nYou: Hi." }],
                }],
            })
        );
    }

    #[test]
    fn a_response_gives_its_text_parts_without_the_thinking() {
        let answer = r#"{"candidates":[{"content":{"role":"model","parts":[
            {"text":"The user wants a summary.","thought":true},
            {"text":"Planned the launch."},{"text":"\n\n## Decisions\n"}]},
            "finishReason":"STOP","index":0}],
            "usageMetadata":{"promptTokenCount":10}}"#;
        assert_eq!(
            text("Google Gemini", answer).ok().as_deref(),
            Some("Planned the launch.\n\n## Decisions")
        );
    }

    #[test]
    fn a_block_a_cut_or_an_empty_answer_is_an_error() {
        let declined = Some("Google Gemini declined to summarize this Transcript.");
        let blocked = r#"{"promptFeedback":{"blockReason":"PROHIBITED_CONTENT"}}"#;
        assert_eq!(message(text("Google Gemini", blocked)).as_deref(), declined);
        for reason in [
            "SAFETY",
            "PROHIBITED_CONTENT",
            "RECITATION",
            "BLOCKLIST",
            "SPII",
        ] {
            let stopped = format!(
                r#"{{"candidates":[{{"content":{{"parts":[{{"text":"Plan"}}]}},"finishReason":"{reason}"}}]}}"#
            );
            assert_eq!(
                message(text("Google Gemini", &stopped)).as_deref(),
                declined
            );
        }
        let cut = r#"{"candidates":[{"content":{"parts":[{"text":"Plan"}]},
            "finishReason":"MAX_TOKENS"}]}"#;
        assert_eq!(
            message(text("Google Gemini", cut)).as_deref(),
            Some("Google Gemini ran out of room before finishing the Summary.")
        );
        let thought_only = r#"{"candidates":[{"content":{"parts":[
            {"text":"Hmm.","thought":true}]},"finishReason":"STOP"}]}"#;
        assert_eq!(
            message(text("Google Gemini", thought_only)).as_deref(),
            Some("The Model answered with nothing.")
        );
        assert!(text("Google Gemini", r#"{"candidates":[]}"#).is_err());
        assert_eq!(
            message(text("Google Gemini", "<html>")).as_deref(),
            Some("Google Gemini's answer wasn't understood.")
        );
    }
}
