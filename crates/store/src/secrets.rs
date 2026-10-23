//! API keys in the macOS Keychain, one generic password per Provider: service
//! `io.github.mastro993.crumble.api-keys`, account the Provider's id (ADR 0011). A key is never
//! written to the database, a log, or an error.
//!
//! Keys saved before Provider ids came from `crumble-remote` sit under the service
//! `io.github.mastro993.crumble`, by their old ids, until [`move_old_keys`] moves them. The new
//! service keeps the OpenAI-compatible server's old `openai` key from ever being read as OpenAI's.

use std::collections::BTreeSet;

/// The Keychain service Crumble's keys are filed under.
#[cfg(target_os = "macos")]
const SERVICE: &str = "io.github.mastro993.crumble.api-keys";
/// The service keys were filed under by their old ids.
#[cfg(target_os = "macos")]
const OLD_SERVICE: &str = "io.github.mastro993.crumble";
/// `errSecItemNotFound`: no key is saved.
#[cfg(target_os = "macos")]
const NOT_FOUND: i32 = -25300;
/// The name of `kSecAttrAccount` in a search result's attributes: the Provider's id.
#[cfg(target_os = "macos")]
const ACCOUNT: &str = "acct";

/// Each old Provider id with a key in [`OLD_SERVICE`], and the id its key moves to, as
/// migration 000011 renamed them in the database.
#[cfg(any(target_os = "macos", test))]
const MOVES: [(&str, &str); 2] = [("claude", "anthropic"), ("openai", "openai-compatible")];

/// The key saved for `provider`, a Provider's id, if there is one.
#[cfg(target_os = "macos")]
pub fn get(provider: &str) -> anyhow::Result<Option<String>> {
    get_in(SERVICE, provider)
}

/// Saves `key` for `provider`, a Provider's id, replacing any key it had.
#[cfg(target_os = "macos")]
pub fn set(provider: &str, key: &str) -> anyhow::Result<()> {
    set_in(SERVICE, provider, key)
}

/// Removes the key saved for `provider`, a Provider's id; nothing saved is not an error.
#[cfg(target_os = "macos")]
pub fn delete(provider: &str) -> anyhow::Result<()> {
    delete_in(SERVICE, provider)
}

/// The ids of the Providers that have a key saved, read without reading any key.
#[cfg(target_os = "macos")]
pub fn saved() -> anyhow::Result<BTreeSet<String>> {
    saved_in(SERVICE)
}

/// Moves the keys saved under the old Provider ids to their new ids, then removes the old ones.
/// A key already saved under a new id stays. Safe to run at every launch: once moved, nothing is
/// left to move.
#[cfg(target_os = "macos")]
pub fn move_old_keys() -> anyhow::Result<()> {
    move_keys(OLD_SERVICE, SERVICE)
}

#[cfg(target_os = "macos")]
fn get_in(service: &str, provider: &str) -> anyhow::Result<Option<String>> {
    use security_framework::passwords::get_generic_password;
    match get_generic_password(service, provider) {
        Ok(key) => Ok(Some(String::from_utf8(key).map_err(|_| {
            anyhow::anyhow!("the Keychain item for {provider} is not text")
        })?)),
        Err(error) if error.code() == NOT_FOUND => Ok(None),
        Err(error) => Err(anyhow::anyhow!(
            "could not read the Keychain item for {provider}: {error}"
        )),
    }
}

#[cfg(target_os = "macos")]
fn set_in(service: &str, provider: &str, key: &str) -> anyhow::Result<()> {
    security_framework::passwords::set_generic_password(service, provider, key.as_bytes())
        .map_err(|error| anyhow::anyhow!("could not save the key for {provider}: {error}"))
}

#[cfg(target_os = "macos")]
fn delete_in(service: &str, provider: &str) -> anyhow::Result<()> {
    match security_framework::passwords::delete_generic_password(service, provider) {
        Err(error) if error.code() != NOT_FOUND => Err(anyhow::anyhow!(
            "could not remove the key for {provider}: {error}"
        )),
        _ => Ok(()),
    }
}

/// The accounts of `service`'s generic passwords, from their attributes alone: their data, the
/// keys, is never loaded.
#[cfg(target_os = "macos")]
fn saved_in(service: &str) -> anyhow::Result<BTreeSet<String>> {
    use security_framework::item::{ItemClass, ItemSearchOptions, Limit};
    let found = ItemSearchOptions::new()
        .class(ItemClass::generic_password())
        .service(service)
        .load_attributes(true)
        .limit(Limit::All)
        .search();
    match found {
        Ok(items) => Ok(items
            .iter()
            .filter_map(|item| item.simplify_dict()?.remove(ACCOUNT))
            .collect()),
        Err(error) if error.code() == NOT_FOUND => Ok(BTreeSet::new()),
        Err(error) => Err(anyhow::anyhow!("could not list the saved keys: {error}")),
    }
}

/// Moves each key of [`MOVES`] from `from`, under its old id, to `to`, under its new one, unless
/// `to` already has a key there, then removes it from `from`.
#[cfg(target_os = "macos")]
fn move_keys(from: &str, to: &str) -> anyhow::Result<()> {
    let saved = saved_in(to)?;
    for (old, new) in MOVES {
        let Some(key) = get_in(from, old)? else {
            continue;
        };
        if !saved.contains(new) {
            set_in(to, new, &key)?;
        }
        delete_in(from, old)?;
    }
    Ok(())
}

/// No Keychain here: no key is ever saved.
#[cfg(not(target_os = "macos"))]
pub fn get(_provider: &str) -> anyhow::Result<Option<String>> {
    Ok(None)
}

/// No Keychain here: saving fails.
#[cfg(not(target_os = "macos"))]
pub fn set(_provider: &str, _key: &str) -> anyhow::Result<()> {
    anyhow::bail!("keys are kept in the macOS Keychain")
}

/// No Keychain here: nothing to remove.
#[cfg(not(target_os = "macos"))]
pub fn delete(_provider: &str) -> anyhow::Result<()> {
    Ok(())
}

/// No Keychain here: no Provider has a key.
#[cfg(not(target_os = "macos"))]
pub fn saved() -> anyhow::Result<BTreeSet<String>> {
    Ok(BTreeSet::new())
}

/// No Keychain here: nothing to move.
#[cfg(not(target_os = "macos"))]
pub fn move_old_keys() -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::MOVES;

    #[test]
    fn no_old_key_moves_to_an_id_an_old_key_had() {
        for (_, new) in MOVES {
            assert!(MOVES.iter().all(|(old, _)| *old != new), "{new}");
        }
        // The OpenAI-compatible server's key, never OpenAI's.
        assert!(MOVES.contains(&("openai", "openai-compatible")));
    }

    /// Moves keys between two made-up services in the login Keychain, so no real key is touched.
    /// Run it by hand: `cargo test -p crumble-store --locked -- --ignored`. It leaves no item
    /// behind.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "writes to the login Keychain"]
    fn old_keys_move_once_and_never_replace_a_new_one() {
        use super::{delete_in, get_in, move_keys, saved_in, set_in};
        use std::collections::BTreeSet;

        const OLD: &str = "io.github.mastro993.crumble.keychain-test-old";
        const NEW: &str = "io.github.mastro993.crumble.keychain-test-new";
        fn ok<T>(result: anyhow::Result<T>) -> T {
            result.unwrap_or_else(|error| panic!("{error:#}"))
        }
        let clean = || {
            for (old, new) in MOVES {
                ok(delete_in(OLD, old));
                ok(delete_in(NEW, new));
            }
        };
        clean();
        assert_eq!(ok(saved_in(NEW)), BTreeSet::new());
        ok(set_in(OLD, "claude", "not-a-real-key-1"));
        ok(set_in(OLD, "openai", "not-a-real-key-2"));
        ok(set_in(NEW, "openai-compatible", "not-a-real-key-3"));

        ok(move_keys(OLD, NEW));
        ok(move_keys(OLD, NEW));
        let moved = (
            ok(get_in(NEW, "anthropic")),
            ok(get_in(NEW, "openai-compatible")),
            ok(saved_in(NEW)),
            ok(saved_in(OLD)),
        );
        clean();
        assert_eq!(moved.0.as_deref(), Some("not-a-real-key-1"));
        assert_eq!(moved.1.as_deref(), Some("not-a-real-key-3"));
        assert_eq!(
            moved.2,
            BTreeSet::from(["anthropic".to_owned(), "openai-compatible".to_owned()])
        );
        assert_eq!(moved.3, BTreeSet::new());
    }
}
