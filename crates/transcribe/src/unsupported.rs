//! Platforms without SpeechAnalyzer: nothing transcribes.

use std::path::Path;

use anyhow::bail;

use crate::ModelStatus;

/// `en_US`, as there is no system locale to ask.
pub fn current_locale() -> String {
    "en_US".to_owned()
}

/// `locale` itself.
pub fn language_name(locale: &str) -> String {
    locale.to_owned()
}

/// Always [`ModelStatus::Unsupported`].
pub fn model_status(_locale: &str) -> ModelStatus {
    ModelStatus::Unsupported
}

/// Always errors.
pub fn install_model(_locale: &str, _progress: impl FnMut(f32) + Send) -> anyhow::Result<()> {
    bail!("transcription needs macOS")
}

pub(crate) fn transcribe_file(
    _path: &Path,
    _locale: &str,
    _progress: &mut (dyn FnMut(f32) + Send),
) -> anyhow::Result<String> {
    bail!("transcription needs macOS")
}
