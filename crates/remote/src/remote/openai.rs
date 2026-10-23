//! OpenAI's API: chat completions, and audio transcriptions.

use super::{Chat, Check, Models, ModelsRequest, Remote, SignIn, Speech, data_ids};
use crate::{KeyFormat, KeyHeader, Listed};

pub(crate) static REMOTE: Remote = Remote {
    key_url: Some("https://platform.openai.com/api-keys"),
    base_url: Some("https://api.openai.com/v1"),
    sign_in: SignIn {
        // Project (`sk-proj-`), service account (`sk-svcacct-`) and user keys, 48 to about 160
        // characters after `sk-`.
        api_key: Some(KeyFormat {
            pattern: Some(r"^sk-(?:(?:proj|svcacct|None)-)?[A-Za-z0-9_-]{40,}$"),
            starts: &["sk-"],
            refused: &[
                (
                    r"^sk-admin-",
                    "This is an Admin key, which can't use Models. Use a project API key.",
                ),
                (r"^sk-ant-", "This is an Anthropic key."),
                (r"^sk-or-", "This is an OpenRouter key."),
                // Too short to be OpenAI's, which have 40 or more characters after `sk-`.
                (r"^sk-[0-9a-f]{32}$", "This is a DeepSeek key."),
            ],
            required: true,
            header: KeyHeader::Bearer,
        }),
        // "Sign in with ChatGPT" would go here; Crumble offers API keys only for now.
        subscription: None,
    },
    headers: &[],
    chat: Some(Chat::OpenAiChat),
    speech: Some(Speech::OpenAiTranscriptions),
    streams: true,
    check: Check {
        key: None,
        models: Some(ModelsRequest {
            path: "/models",
            keyed: true,
            forbidden_refuses_key: false,
            read: |answer| Listed {
                chat: data_ids(answer, chats),
                speech: data_ids(answer, transcribes),
            },
        }),
    },
    models: Models {
        chat: &["gpt-6.1-sol", "gpt-6-luna", "gpt-6-astra"],
        // `whisper-1` is the one that times its segments; `gpt-transcribe` writes better text
        // but times only each piece it is sent.
        speech: &["whisper-1", "gpt-transcribe"],
        intelligence: "gpt-6.1-sol",
    },
};

/// Words in the ids of OpenAI's Models that don't chat in text.
const NOT_CHAT: [&str; 11] = [
    "tts",
    "transcribe",
    "whisper",
    "realtime",
    "live",
    "audio",
    "image",
    "embedding",
    "moderation",
    "search",
    "computer-use",
];

/// Whether OpenAI's Model `id` chats in text: its list says nothing of what a Model does, so
/// the id tells.
fn chats(id: &str) -> bool {
    let family = id.starts_with("gpt-")
        || id.starts_with("chatgpt-")
        || (id.starts_with('o') && id[1..].starts_with(|c: char| c.is_ascii_digit()));
    family && !NOT_CHAT.iter().any(|word| id.contains(word))
}

/// Whether OpenAI's Model `id` transcribes recorded audio, rather than live audio.
fn transcribes(id: &str) -> bool {
    (id.starts_with("whisper") || id.contains("transcribe"))
        && !id.contains("realtime")
        && !id.contains("live")
}

#[cfg(test)]
mod tests {
    use super::{chats, transcribes};

    #[test]
    fn ids_tell_chat_and_speech_models_apart() {
        for chat in ["gpt-6.1-sol", "gpt-6-luna", "o4-mini", "chatgpt-4o-latest"] {
            assert!(chats(chat), "{chat}");
            assert!(!transcribes(chat), "{chat}");
        }
        for speech in ["whisper-1", "gpt-transcribe", "gpt-4o-transcribe-diarize"] {
            assert!(transcribes(speech), "{speech}");
            assert!(!chats(speech), "{speech}");
        }
        for neither in [
            "gpt-image-2",
            "text-embedding-3-large",
            "tts-1",
            "gpt-realtime",
            "gpt-realtime-transcribe",
            "omni-moderation-latest",
            "dall-e-3",
        ] {
            assert!(!chats(neither) && !transcribes(neither), "{neither}");
        }
    }
}
