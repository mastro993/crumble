//! What the user changed for each remote Provider in Settings: its address and the Model each job
//! uses. Provider ids are opaque here; `crumble-remote` gives them and each setting's default.

use crate::{Store, db};

/// What the user changed for one remote Provider. `None` is the Provider's own default, as
/// `crumble_remote::Remote` gives it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProviderSettings {
    /// Its API's address; `None` is its usual one.
    pub base_url: Option<String>,
    /// The Model that makes Summaries; `None` is its default.
    pub summary_model: Option<String>,
    /// The Model that makes Transcripts; `None` is its default.
    pub transcription_model: Option<String>,
    /// The Model that answers in Crumble's chat, for Intelligence; `None` is its default.
    pub intelligence_model: Option<String>,
}

impl ProviderSettings {
    /// Each value trimmed, and a blank one `None`.
    fn trimmed(self) -> Self {
        let trim = |value: Option<String>| {
            value
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        };
        Self {
            base_url: trim(self.base_url),
            summary_model: trim(self.summary_model),
            transcription_model: trim(self.transcription_model),
            intelligence_model: trim(self.intelligence_model),
        }
    }
}

impl Store {
    /// What the user changed for the Provider with the id `id`, as `crumble_remote::Provider::id`
    /// gives it; all `None` until something is.
    pub fn provider(&self, id: &str) -> ProviderSettings {
        self.state.providers.get(id).cloned().unwrap_or_default()
    }

    /// Keeps `settings` for the Provider with the id `id`, each value trimmed and a blank one
    /// `None`. Settings back to all `None` delete the Provider's row, so the table holds only the
    /// Providers the user changed.
    pub fn set_provider(&mut self, id: &str, settings: ProviderSettings) {
        let settings = settings.trimmed();
        if self.provider(id) == settings {
            return;
        }
        if settings == ProviderSettings::default() {
            self.state.providers.remove(id);
        } else {
            self.state.providers.insert(id.to_owned(), settings);
        }
        self.write(|connection, state| db::write_provider(connection, state, id));
    }
}
