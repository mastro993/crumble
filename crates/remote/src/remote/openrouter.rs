//! OpenRouter, which passes OpenAI-style requests on to many companies' Models: chat, and audio
//! transcriptions.

use serde_json::Value;

use super::{Chat, Check, KeyCheckRequest, Models, ModelsRequest, Remote, SignIn, Speech};
use crate::{KeyFormat, KeyHeader, Listed};

pub(crate) static REMOTE: Remote = Remote {
    key_url: Some("https://openrouter.ai/keys"),
    base_url: Some("https://openrouter.ai/api/v1"),
    sign_in: SignIn {
        api_key: Some(KeyFormat {
            pattern: Some(r"^sk-or-v1-[0-9a-f]{64}$"),
            starts: &["sk-or-v1-"],
            refused: &[],
            required: true,
            header: KeyHeader::Bearer,
        }),
        // Its OAuth makes an API key, which could go here later.
        subscription: None,
    },
    // Names Crumble in the user's OpenRouter activity.
    headers: &[
        ("http-referer", "https://github.com/mastro993/crumble"),
        ("x-openrouter-title", "Crumble"),
    ],
    chat: Some(Chat::OpenAiChat),
    speech: Some(Speech::OpenAiTranscriptions),
    streams: true,
    check: Check {
        // The Model list is public, so the key is checked on its own, at no cost.
        key: Some(KeyCheckRequest {
            path: "/key",
            unusable: |_| None,
        }),
        models: Some(ModelsRequest {
            path: "/models",
            keyed: false,
            forbidden_refuses_key: false,
            read: listed,
        }),
    },
    models: Models {
        chat: &[
            "google/gemini-3.8-flash",
            "anthropic/claude-sonnet-5.5",
            "openai/gpt-6.1-sol",
        ],
        // Whisper Models time their segments; others answer only `json`.
        speech: &["openai/whisper-large-v3", "openai/whisper-large-v3-turbo"],
        intelligence: "anthropic/claude-sonnet-5.5",
    },
};

/// The Models in OpenRouter's list that read and write text.
fn listed(answer: &Value) -> Listed {
    let has = |model: &Value, key: &str, modality: &str| {
        (model["architecture"][key].as_array().into_iter())
            .flatten()
            .any(|value| value == modality)
    };
    let mut chat: Vec<String> = (answer["data"].as_array().into_iter().flatten())
        .filter(|model| {
            has(model, "input_modalities", "text") && has(model, "output_modalities", "text")
        })
        .filter_map(|model| model["id"].as_str())
        .map(str::to_owned)
        .collect();
    chat.sort();
    chat.dedup();
    Listed {
        chat,
        speech: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::listed;
    use serde_json::json;

    #[test]
    fn the_list_keeps_models_that_write_text() {
        let text = json!({"input_modalities": ["text", "image"], "output_modalities": ["text"]});
        let answer = json!({"data": [
            {"id": "google/gemini-3.8-flash", "architecture": text},
            {"id": "openai/gpt-image-2", "architecture": {"input_modalities": ["text"], "output_modalities": ["image"]}},
            {"id": "openai/whisper-large-v3", "architecture": {"input_modalities": ["audio"], "output_modalities": ["transcription"]}},
        ]});
        assert_eq!(listed(&answer).chat, ["google/gemini-3.8-flash"]);
    }
}
