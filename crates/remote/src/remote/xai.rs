//! xAI's API: chat completions, and its own speech to text.

use super::{
    Chat, Check, KeyCheckRequest, Models, ModelsRequest, Remote, SignIn, Speech, data_ids,
};
use crate::{KeyFormat, KeyHeader, Listed};

pub(crate) static REMOTE: Remote = Remote {
    key_url: Some("https://console.x.ai/team/default/api-keys"),
    base_url: Some("https://api.x.ai/v1"),
    sign_in: SignIn {
        // `xai-` and 80 more characters today.
        api_key: Some(KeyFormat {
            pattern: Some(r"^xai-[A-Za-z0-9_-]{40,}$"),
            starts: &["xai-"],
            refused: &[],
            required: true,
            header: KeyHeader::Bearer,
        }),
        // xAI lets only its partners use Grok subscriptions.
        subscription: None,
    },
    headers: &[],
    chat: Some(Chat::OpenAiChat),
    speech: Some(Speech::XaiSpeechToText),
    streams: true,
    check: Check {
        // Free, and says whether the key or its team is blocked.
        key: Some(KeyCheckRequest {
            path: "/api-key",
            unusable: |answer| {
                if answer["api_key_blocked"] == true || answer["api_key_disabled"] == true {
                    Some("This key is disabled. Turn it on or make another in xAI's console.")
                } else if answer["team_blocked"] == true {
                    Some("This key's team is blocked. Check it in xAI's console.")
                } else {
                    None
                }
            },
        }),
        // Lists the chat Models only, not the speech ones.
        models: Some(ModelsRequest {
            path: "/models",
            keyed: true,
            forbidden_refuses_key: false,
            read: |answer| Listed {
                chat: data_ids(answer, |id| !id.contains("image") && !id.contains("video")),
                speech: Vec::new(),
            },
        }),
    },
    models: Models {
        chat: &["grok-4.3", "grok-4.7"],
        speech: &["grok-voice-transcribe-2.0"],
        intelligence: "grok-4.7",
    },
};
