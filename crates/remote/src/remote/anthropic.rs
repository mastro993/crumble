//! Anthropic's Claude API: chat only, through its Messages API.

use super::{Chat, Check, Models, ModelsRequest, Remote, SignIn, data_ids};
use crate::{KeyFormat, KeyHeader, Listed};

pub(crate) static REMOTE: Remote = Remote {
    key_url: Some("https://platform.claude.com/settings/keys"),
    base_url: Some("https://api.anthropic.com"),
    sign_in: SignIn {
        // A workspace key is `sk-ant-api03-` and 95 more characters; a personal key starts
        // `sk-ant-usr` and a service account's `sk-ant-svc`. Any kind but those refused below
        // passes, so one Anthropic adds later does too, and the ping proves it.
        api_key: Some(KeyFormat {
            pattern: Some(r"^sk-ant-[a-z]+\d*-[A-Za-z0-9_-]{20,}$"),
            starts: &["sk-ant-"],
            refused: &[
                (
                    r"^sk-ant-admin\d{2}-",
                    "This is an Admin key, which can't make Summaries. Use an API key.",
                ),
                // Claude.ai's own sign-in tokens, which Anthropic allows only in its own apps.
                (
                    r"^sk-ant-o[ar]t\d{2}-",
                    "This is a Claude subscription token. Use an API key.",
                ),
            ],
            required: true,
            header: KeyHeader::Named("x-api-key"),
        }),
        // Anthropic doesn't allow other apps to use Claude subscriptions.
        subscription: None,
    },
    headers: &[("anthropic-version", "2023-06-01")],
    chat: Some(Chat::AnthropicMessages),
    speech: None,
    streams: true,
    check: Check {
        key: None,
        models: Some(ModelsRequest {
            path: "/v1/models?limit=1000",
            keyed: true,
            forbidden_refuses_key: false,
            read: |answer| Listed {
                chat: data_ids(answer, |_| true),
                speech: Vec::new(),
            },
        }),
    },
    models: Models {
        chat: &["claude-sonnet-5-5", "claude-opus-5-5", "claude-haiku-4-5"],
        speech: &[],
        intelligence: "claude-sonnet-5-5",
    },
};
