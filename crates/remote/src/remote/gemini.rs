//! Google's Gemini API: its own `generateContent` for chat, and a transcribe Model through
//! the Files API and Interactions.

use serde_json::Value;

use super::{Chat, Check, Models, ModelsRequest, Remote, SignIn, Speech};
use crate::{KeyFormat, KeyHeader, Listed};

pub(crate) static REMOTE: Remote = Remote {
    key_url: Some("https://aistudio.google.com/apikey"),
    base_url: Some("https://generativelanguage.googleapis.com"),
    sign_in: SignIn {
        // AI Studio's auth keys (`AQ.`), and the older standard keys (`AIza` and 35 more).
        api_key: Some(KeyFormat {
            pattern: Some(r"^(?:AIza[0-9A-Za-z_-]{35}|AQ\.[0-9A-Za-z_.-]{20,})$"),
            starts: &["AQ.", "AIza"],
            refused: &[],
            required: true,
            header: KeyHeader::Named("x-goog-api-key"),
        }),
        // Google doesn't allow other apps to use Gemini subscriptions.
        subscription: None,
    },
    headers: &[],
    chat: Some(Chat::GeminiGenerateContent),
    speech: Some(Speech::GeminiInteractions),
    streams: true,
    check: Check {
        key: None,
        models: Some(ModelsRequest {
            path: "/v1beta/models?pageSize=1000",
            keyed: true,
            // Its keys have no scopes: a 403 is a key whose project can't use the API, or one
            // Google blocked, such as a leaked one.
            forbidden_refuses_key: true,
            read: listed,
        }),
    },
    models: Models {
        chat: &[
            "gemini-3.8-flash",
            "gemini-4-argon",
            "gemini-3.5-flash-lite",
        ],
        speech: &["gemini-3.5-transcribe"],
        intelligence: "gemini-3.8-flash",
    },
};

/// Words in the names of Gemini Models that generate content but don't chat in text.
const NOT_CHAT: [&str; 13] = [
    "tts",
    "image",
    "embedding",
    "live",
    "transcribe",
    "veo",
    "imagen",
    "robotics",
    "research",
    "antigravity",
    "omni",
    "nano-banana",
    "aqa",
];

/// The Models in a `models.list` answer: those that generate content and chat in text, and
/// the transcribe Models for recorded audio.
fn listed(answer: &Value) -> Listed {
    let mut listed = Listed::default();
    for model in answer["models"].as_array().into_iter().flatten() {
        let Some(id) = model["name"]
            .as_str()
            .map(|name| name.trim_start_matches("models/"))
        else {
            continue;
        };
        let generates = (model["supportedGenerationMethods"].as_array().into_iter())
            .flatten()
            .any(|method| method == "generateContent");
        if id.contains("transcribe") && !id.contains("live") {
            listed.speech.push(id.to_owned());
        } else if generates && !NOT_CHAT.iter().any(|word| id.contains(word)) {
            listed.chat.push(id.to_owned());
        }
    }
    listed.chat.sort();
    listed.speech.sort();
    listed
}

#[cfg(test)]
mod tests {
    use super::listed;
    use serde_json::json;

    #[test]
    fn the_list_splits_chat_from_transcribe_models() {
        let answer = json!({"models": [
            {"name": "models/gemini-3.8-flash", "supportedGenerationMethods": ["generateContent", "countTokens"]},
            {"name": "models/gemini-3.5-transcribe", "supportedGenerationMethods": ["generateContent"]},
            {"name": "models/gemini-3.5-transcribe-live", "supportedGenerationMethods": ["bidiGenerateContent"]},
            {"name": "models/gemini-embedding-2", "supportedGenerationMethods": ["embedContent"]},
            {"name": "models/gemini-3.8-flash-image", "supportedGenerationMethods": ["generateContent"]},
            {"name": "models/gemini-4-argon", "supportedGenerationMethods": ["generateContent"]},
        ]});
        let listed = listed(&answer);
        assert_eq!(listed.chat, ["gemini-3.8-flash", "gemini-4-argon"]);
        assert_eq!(listed.speech, ["gemini-3.5-transcribe"]);
    }
}
