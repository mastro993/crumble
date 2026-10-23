//! What a Provider's dialog says, apart from drawing it: the placeholders and lines of its
//! fields.

use crumble_remote::{Provider, Remote};

/// The address the server's field suggests: Ollama's, on this Mac.
const SERVER_ADDRESS: &str = "http://localhost:11434/v1";

/// What the API key field says while empty; a key the Provider doesn't require is optional.
pub(super) fn key_placeholder(provider: Provider, required: bool) -> String {
    if required {
        format!("Paste your {} API key", provider.name())
    } else {
        "API key (optional)".to_owned()
    }
}

/// What the address field says while empty: the Provider's usual address, or, for a server the
/// user picks, Ollama's.
pub(super) fn address_placeholder(remote: &Remote) -> &'static str {
    remote.default_base_url().unwrap_or(SERVER_ADDRESS)
}

/// The line under the address field.
pub(super) fn address_description(remote: &Remote) -> String {
    match remote.default_base_url() {
        Some(usual) => format!("Leave empty to use {usual}."),
        None => "Where the server's API is, such as Ollama's or LM Studio's.".to_owned(),
    }
}

/// Why requests can't go to `typed`, the address as typed; `None` while it is empty, as the
/// line above says what an empty field means.
pub(super) fn address_error(typed: &str) -> Option<String> {
    if typed.trim().is_empty() {
        return None;
    }
    crumble_remote::base_url(typed)
        .err()
        .map(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{address_description, address_error, address_placeholder, key_placeholder};
    use crumble_remote::{Provider, Remote};

    fn remote(provider: Provider) -> &'static Remote {
        provider
            .remote()
            .unwrap_or_else(|| panic!("{provider:?} isn't remote"))
    }

    #[test]
    fn a_required_key_names_its_provider_and_an_optional_one_says_so() {
        assert_eq!(
            key_placeholder(Provider::Anthropic, true),
            "Paste your Anthropic API key"
        );
        assert_eq!(
            key_placeholder(Provider::OpenAiCompatible, false),
            "API key (optional)"
        );
    }

    #[test]
    fn a_company_s_address_defaults_to_its_api_and_a_server_s_to_ollama() {
        let openai = remote(Provider::OpenAi);
        assert_eq!(address_placeholder(openai), "https://api.openai.com/v1");
        assert_eq!(
            address_description(openai),
            "Leave empty to use https://api.openai.com/v1."
        );
        let server = remote(Provider::OpenAiCompatible);
        assert_eq!(address_placeholder(server), "http://localhost:11434/v1");
        assert_eq!(
            address_description(server),
            "Where the server's API is, such as Ollama's or LM Studio's."
        );
    }

    #[test]
    fn only_an_address_typed_without_its_scheme_is_wrong() {
        assert_eq!(address_error("  "), None);
        assert_eq!(address_error("http://localhost:11434/v1"), None);
        assert_eq!(
            address_error("localhost:11434/v1").as_deref(),
            Some("Start the server's address with http:// or https://.")
        );
    }
}
