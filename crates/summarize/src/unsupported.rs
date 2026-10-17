//! Platforms without Foundation Models: Apple's model is never available.

use anyhow::bail;

use crate::Availability;

/// Always [`Availability::Unsupported`].
pub fn apple_availability() -> Availability {
    Availability::Unsupported
}

pub(crate) fn context_size() -> usize {
    0
}

pub(crate) fn token_counts(_texts: &[&str]) -> anyhow::Result<Vec<usize>> {
    bail!("Apple's on-device model needs macOS")
}

pub(crate) fn respond(
    _instructions: &str,
    _prompt: &str,
    _max_tokens: usize,
) -> anyhow::Result<String> {
    bail!("Apple's on-device model needs macOS")
}
