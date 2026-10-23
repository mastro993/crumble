//! DeepSeek's API: chat only, through OpenAI's chat completions.

use super::{Chat, Check, Models, ModelsRequest, Remote, SignIn, data_ids};
use crate::{KeyFormat, KeyHeader, Listed};

pub(crate) static REMOTE: Remote = Remote {
    key_url: Some("https://platform.deepseek.com/api_keys"),
    base_url: Some("https://api.deepseek.com"),
    sign_in: SignIn {
        api_key: Some(KeyFormat {
            pattern: Some(r"^sk-[0-9a-f]{32}$"),
            starts: &["sk-"],
            // Other Providers' keys that start `sk-` too, which would otherwise look
            // incomplete.
            refused: &[
                (r"^sk-ant-", "This is an Anthropic key."),
                (r"^sk-or-", "This is an OpenRouter key."),
                (
                    r"^sk-(?:proj-|svcacct-|admin-)|^sk-[A-Za-z0-9_-]{40,}$",
                    "This is an OpenAI key.",
                ),
            ],
            required: true,
            header: KeyHeader::Bearer,
        }),
        subscription: None,
    },
    headers: &[],
    chat: Some(Chat::OpenAiChat),
    speech: None,
    streams: true,
    check: Check {
        key: None,
        models: Some(ModelsRequest {
            path: "/models",
            keyed: true,
            forbidden_refuses_key: false,
            read: |answer| Listed {
                chat: data_ids(answer, |_| true),
                speech: Vec::new(),
            },
        }),
    },
    models: Models {
        chat: &["deepseek-flash", "deepseek-v4-pro"],
        speech: &[],
        intelligence: "deepseek-v4-pro",
    },
};
