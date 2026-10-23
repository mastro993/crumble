//! ElevenLabs: Speech to Text only, through its Scribe Models.

use super::{Check, KeyCheckRequest, Models, Remote, SignIn, Speech};
use crate::{KeyFormat, KeyHeader};

pub(crate) static REMOTE: Remote = Remote {
    key_url: Some("https://elevenlabs.io/app/developers/api-keys"),
    // EU, India and Singapore accounts have their own address, such as
    // `https://api.eu.residency.elevenlabs.io`.
    base_url: Some("https://api.elevenlabs.io"),
    sign_in: SignIn {
        // `sk_` and 48 hex digits; older keys are 32 hex digits alone.
        api_key: Some(KeyFormat {
            pattern: Some(r"^(?:sk_[0-9a-f]{48}|[0-9a-f]{32})$"),
            starts: &["sk_"],
            refused: &[],
            required: true,
            header: KeyHeader::Named("xi-api-key"),
        }),
        subscription: None,
    },
    headers: &[],
    chat: None,
    speech: Some(Speech::ElevenLabsSpeechToText),
    streams: false,
    check: Check {
        // Free, and its list holds only voice Models, so the Scribe ones are suggested below.
        key: Some(KeyCheckRequest {
            path: "/v1/user",
            unusable: |_| None,
        }),
        models: None,
    },
    models: Models {
        chat: &[],
        speech: &["scribe_v2"],
        intelligence: "",
    },
};
