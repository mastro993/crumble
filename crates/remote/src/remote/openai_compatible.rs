//! A server the user picks that speaks OpenAI's API, such as Ollama or LM Studio on the Mac, or
//! a Whisper server: chat completions and audio transcriptions, at the address the user gives.

use super::{Chat, Check, Models, ModelsRequest, Remote, SignIn, Speech, data_ids};
use crate::{KeyFormat, KeyHeader, Listed};

pub(crate) static REMOTE: Remote = Remote {
    key_url: None,
    base_url: None,
    sign_in: SignIn {
        // Any key, if the server wants one.
        api_key: Some(KeyFormat {
            pattern: None,
            starts: &[],
            refused: &[],
            required: false,
            header: KeyHeader::Bearer,
        }),
        subscription: None,
    },
    headers: &[],
    chat: Some(Chat::OpenAiChat),
    speech: Some(Speech::OpenAiTranscriptions),
    streams: true,
    check: Check {
        key: None,
        // Ollama, LM Studio, llama.cpp and speaches answer it; its ids say nothing of the job,
        // so every one is offered for each.
        models: Some(ModelsRequest {
            path: "/models",
            keyed: true,
            forbidden_refuses_key: false,
            read: |answer| {
                let ids = data_ids(answer, |_| true);
                Listed {
                    chat: ids.clone(),
                    speech: ids,
                }
            },
        }),
    },
    models: Models {
        chat: &[],
        speech: &["whisper-1"],
        intelligence: "",
    },
};
