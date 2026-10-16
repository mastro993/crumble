//! API keys in the macOS Keychain, one generic password per Provider: service
//! `io.github.mastro993.crumble`, account the Provider's id (ADR 0007). A key is never written
//! to the database, a log, or an error.

/// The Keychain service Crumble's keys are filed under.
#[cfg(target_os = "macos")]
const SERVICE: &str = "io.github.mastro993.crumble";
/// `errSecItemNotFound`: no key is saved.
#[cfg(target_os = "macos")]
const NOT_FOUND: i32 = -25300;

/// The key saved for `provider`, a Provider's id, if there is one.
#[cfg(target_os = "macos")]
pub fn get(provider: &str) -> anyhow::Result<Option<String>> {
    use security_framework::passwords::get_generic_password;
    match get_generic_password(SERVICE, provider) {
        Ok(key) => Ok(Some(String::from_utf8(key).map_err(|_| {
            anyhow::anyhow!("the Keychain item for {provider} is not text")
        })?)),
        Err(error) if error.code() == NOT_FOUND => Ok(None),
        Err(error) => Err(anyhow::anyhow!(
            "could not read the Keychain item for {provider}: {error}"
        )),
    }
}

/// Saves `key` for `provider`, a Provider's id, replacing any key it had.
#[cfg(target_os = "macos")]
pub fn set(provider: &str, key: &str) -> anyhow::Result<()> {
    security_framework::passwords::set_generic_password(SERVICE, provider, key.as_bytes())
        .map_err(|error| anyhow::anyhow!("could not save the key for {provider}: {error}"))
}

/// Removes the key saved for `provider`, a Provider's id; nothing saved is not an error.
#[cfg(target_os = "macos")]
pub fn delete(provider: &str) -> anyhow::Result<()> {
    match security_framework::passwords::delete_generic_password(SERVICE, provider) {
        Err(error) if error.code() != NOT_FOUND => Err(anyhow::anyhow!(
            "could not remove the key for {provider}: {error}"
        )),
        _ => Ok(()),
    }
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
