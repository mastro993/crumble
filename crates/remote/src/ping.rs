//! The ping Settings sends once a pasted key looks right: it proves the key without spending
//! tokens, and lists the Models the Provider offers.

use std::time::Duration;

use anyhow::{Context as _, anyhow, bail};
use serde_json::Value;

use crate::http::{said, who};
use crate::{Listed, Provider, get, no_response, without_userinfo};

/// How long a ping may take: it asks for a key's details or a list, never for an answer.
const TIMEOUT: Duration = Duration::from_secs(20);

/// Checks `key` with the remote `provider` at `base_url`, the address requests go to, without
/// spending tokens, and returns the Models it lists; empty when it lists none or doesn't let
/// this key read them. Blocks. Its error is a sentence for the user, and never holds the key.
pub fn ping(provider: Provider, base_url: &str, key: Option<&str>) -> anyhow::Result<Listed> {
    let remote =
        (provider.remote()).with_context(|| format!("{} isn't remote.", provider.name()))?;
    let base = crate::base_url(base_url)?;
    let key = key.map(str::trim).filter(|key| !key.is_empty());
    if key.is_none() && remote.api_key().is_some_and(|format| format.required()) {
        bail!("Enter an API key.");
    }
    let keyed = remote.headers(key);
    let keyed: Vec<(&str, &str)> = keyed
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    if let Some(check) = &remote.check.key {
        // A key limited to some uses, such as an ElevenLabs key for Speech to Text alone, may
        // not read its own details, but it is a key.
        let answer = answer(provider, base, check.path, &keyed, key, false)?;
        if let Some(why) = answer.as_ref().and_then(check.unusable) {
            bail!(why);
        }
    }
    let Some(models) = &remote.check.models else {
        return Ok(Listed::default());
    };
    let public = remote.headers(None);
    let public: Vec<(&str, &str)> = public
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    let headers = if models.keyed { &keyed } else { &public };
    // A key that may use Models but not list them is still a key: its Models are typed into
    // the Model selects.
    let refuses_on_403 = models.forbidden_refuses_key;
    Ok(
        answer(provider, base, models.path, headers, key, refuses_on_403)?
            .map(|answer| (models.read)(&answer))
            .unwrap_or_default(),
    )
}

/// `provider`'s answer to `GET {base}{path}`, as [`read`] reads it.
fn answer(
    provider: Provider,
    base: &str,
    path: &str,
    headers: &[(&str, &str)],
    key: Option<&str>,
    refuses_on_403: bool,
) -> anyhow::Result<Option<Value>> {
    let (status, body) = get(&format!("{base}{path}"), headers, TIMEOUT).map_err(|error| {
        // The address may hold a user name and password.
        no_response(
            &address(provider, base),
            "Check the address and the internet connection.",
            TIMEOUT,
            &error,
        )
    })?;
    read(provider, status, &body, key, refuses_on_403)
}

/// What `provider`'s answer to a check, `status` and `body`, says: its JSON; `None` when it
/// refused this key the listing but not the key itself, with a 403 unless `refuses_on_403`;
/// or why the key or address won't do. A server that repeats `key` in its message has it taken
/// out.
fn read(
    provider: Provider,
    status: u16,
    body: &str,
    key: Option<&str>,
    refuses_on_403: bool,
) -> anyhow::Result<Option<Value>> {
    let who = who(provider);
    let said = said(body, key);
    match status {
        200..=299 => serde_json::from_str(body).map(Some).map_err(|_| {
            anyhow!("{who} answered with something other than its API. Check the address.")
        }),
        // No check follows a redirect, so a key goes only to the address given.
        300..=399 => Err(anyhow!(
            "{who} answered from another address. Check the address."
        )),
        // A personal or service account key made for more than one workspace needs an
        // `anthropic-workspace-id` header, which Crumble doesn't send.
        400 if provider == Provider::Anthropic && said.contains("anthropic-workspace-id") => Err(
            anyhow!("This key isn't limited to one workspace. Make a key for one workspace."),
        ),
        403 if !refuses_on_403 => Ok(None),
        // Google, xAI and ElevenLabs refuse a malformed key with 400.
        400 | 401 | 403 if said.is_empty() => Err(anyhow!("{who} didn't accept this key.")),
        400 | 401 | 403 => Err(anyhow!("{who} didn't accept this key: {said}")),
        404 => Err(anyhow!(
            "{who} has nothing at that address. Check the address."
        )),
        429 => Err(anyhow!(
            "{who} is getting too many requests. Try again in a minute."
        )),
        500.. => Err(anyhow!("{who} had a problem ({status}). Try again later.")),
        _ if !said.is_empty() => Err(anyhow!("{who} turned the check down: {said}")),
        _ => Err(anyhow!("{who} turned the check down ({status}).")),
    }
}

/// Who a ping that got no answer names: a company's name, or a server's address without
/// credentials.
fn address(provider: Provider, base: &str) -> String {
    match provider {
        Provider::OpenAiCompatible => without_userinfo(base),
        _ => provider.name().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{ping, read};
    use crate::Provider;

    /// What a check answered `status` and `body` tells the user, or that it passed, listing
    /// nothing.
    fn told(provider: Provider, status: u16, body: &str, refuses_on_403: bool) -> String {
        match read(provider, status, body, Some("k1"), refuses_on_403) {
            Ok(None) => "passed, listing nothing".to_owned(),
            Ok(Some(answer)) => answer.to_string(),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn a_ping_fails_before_any_request_without_what_it_needs() {
        let message = |provider, base, key| {
            ping(provider, base, key)
                .map_err(|error| error.to_string())
                .err()
        };
        assert_eq!(
            message(Provider::Apple, "", None).as_deref(),
            Some("Apple, on this Mac isn't remote.")
        );
        assert_eq!(
            message(Provider::OpenAiCompatible, "", None).as_deref(),
            Some("Enter the server's address in Settings.")
        );
        assert_eq!(
            message(Provider::OpenAi, "https://api.openai.com/v1", Some("  ")).as_deref(),
            Some("Enter an API key.")
        );
    }

    #[test]
    fn a_403_passes_a_key_that_may_not_list_models_but_refuses_a_gemini_key() {
        assert_eq!(
            told(
                Provider::OpenAi,
                403,
                r#"{"error":{"message":"Missing scopes"}}"#,
                false
            ),
            "passed, listing nothing"
        );
        let leaked = r#"{"error":{"code":403,"message":"Your API key was reported as leaked. Please use another API key.","status":"PERMISSION_DENIED"}}"#;
        assert_eq!(
            told(Provider::Gemini, 403, leaked, true),
            "Google Gemini didn't accept this key: Your API key was reported as leaked. \
             Please use another API key."
        );
        assert_eq!(
            told(Provider::Gemini, 403, "", true),
            "Google Gemini didn't accept this key."
        );
    }

    #[test]
    fn a_check_says_what_its_answer_means() {
        assert_eq!(
            told(Provider::OpenAi, 200, r#"{"data":[]}"#, false),
            r#"{"data":[]}"#
        );
        assert_eq!(
            told(Provider::OpenAi, 200, "<html>", false),
            "OpenAI answered with something other than its API. Check the address."
        );
        assert_eq!(
            told(
                Provider::OpenAi,
                401,
                r#"{"error":{"message":"Incorrect key k1"}}"#,
                false
            ),
            "OpenAI didn't accept this key: Incorrect key …"
        );
        let unscoped = r#"{"type":"error","error":{"type":"invalid_request_error","message":"This API key is not scoped to a workspace, so this request must include the anthropic-workspace-id header."}}"#;
        assert_eq!(
            told(Provider::Anthropic, 400, unscoped, false),
            "This key isn't limited to one workspace. Make a key for one workspace."
        );
        assert_eq!(
            told(Provider::Gemini, 302, "", true),
            "Google Gemini answered from another address. Check the address."
        );
    }

    #[test]
    fn an_unreachable_server_is_named_without_its_credentials_or_the_key() {
        // Port 9 on loopback is discard, so nothing answers.
        let error = ping(
            Provider::OpenAiCompatible,
            "http://me:secret@127.0.0.1:9/v1",
            Some("sk-key-that-must-not-show"),
        )
        .map_err(|error| error.to_string())
        .err()
        .unwrap_or_default();
        assert!(error.contains("http://127.0.0.1:9/v1"), "{error}");
        assert!(
            !error.contains("secret") && !error.contains("sk-key"),
            "{error}"
        );
    }
}
