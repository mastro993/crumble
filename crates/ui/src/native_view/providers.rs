//! Settings › Providers: which remote Providers have a key in the Keychain and the Models each
//! listed this run, whether a Provider is set up or ready for a job, and the changes Settings
//! makes to one: its key through `secrets`, its address and Models through `Store`, each kept
//! as it changes. The section's list is drawn in `list`, each Provider's logo in `logo` and the
//! line saying what it does in `detail`; Set up and Edit open its dialog, in `dialog`, which
//! sets up its key and address. Its Models are picked in the sections of their jobs, which list
//! them through `listing`. A key is never read back to show: only a check or a job reads it
//! (ADR 0011).

mod detail;
mod dialog;
mod list;
mod listing;
mod logo;

pub(super) use logo::provider_logo;

use super::NativeView;
use crumble_remote::{Job, Listed, Provider, Remote, runs_locally};
use crumble_store::{ProviderSettings, secrets};
use crumble_summarize::{Availability, apple_availability};
use gpui_kit::Context;
use listing::Listing;
use std::collections::{HashMap, HashSet};

/// What Settings knows of the remote Providers besides what `Store` keeps.
pub(super) struct Providers {
    /// The Providers with a key in the Keychain: listed at launch without reading a key, then
    /// kept as keys are saved and removed.
    saved_keys: HashSet<Provider>,
    /// The Models each Provider listed when it was last checked this run.
    listed: HashMap<Provider, Listed>,
    /// The listings of Models running, and those that failed since Settings last showed a
    /// section.
    listing: HashMap<Provider, Listing>,
}

impl Providers {
    /// Moves the keys an older Crumble kept, then lists which Providers have one, reading none.
    pub(super) fn read() -> Self {
        // First, so a moved key counts.
        if let Err(error) = secrets::move_old_keys() {
            eprintln!("Could not move the old API keys: {error:#}");
        }
        let accounts = secrets::saved().unwrap_or_else(|error| {
            eprintln!("Could not read which API keys the Keychain holds: {error:#}");
            Default::default()
        });
        Self {
            saved_keys: with_keys(accounts),
            listed: HashMap::new(),
            listing: HashMap::new(),
        }
    }

    /// Whether `provider` has a key in the Keychain.
    pub(super) fn has_key(&self, provider: Provider) -> bool {
        self.saved_keys.contains(&provider)
    }
}

/// The remote Providers among `accounts`, the Keychain's; an id this Crumble doesn't know is
/// left out.
fn with_keys(accounts: impl IntoIterator<Item = String>) -> HashSet<Provider> {
    (accounts.into_iter())
        .filter_map(|id| Provider::from_id(&id))
        .filter(|provider| provider.remote().is_some())
        .collect()
}

/// The Model `settings` keep for `job`; `None` for the Provider's default.
pub(super) fn saved_model(settings: &ProviderSettings, job: Job) -> Option<&str> {
    match job {
        Job::Transcription => settings.transcription_model.as_deref(),
        Job::Summary => settings.summary_model.as_deref(),
        Job::Intelligence => settings.intelligence_model.as_deref(),
    }
}

/// Whether the remote Provider `remote`, with `settings` and a key in the Keychain when
/// `has_key`, is ready for `job`: it has a key if it needs one, an address requests can go to,
/// and a Model for `job`, which it must do.
fn remote_ready(remote: &Remote, job: Job, has_key: bool, settings: &ProviderSettings) -> bool {
    let saved_base_url = settings.base_url.as_deref().unwrap_or_default();
    remote.is_set_up(has_key, saved_base_url)
        && remote.model(job, saved_model(settings, job)).is_some()
}

/// `provider`'s key from the Keychain, for a check or a job about to start; `None` when it has
/// none. The error is for the user.
pub(super) fn saved_key(provider: Provider) -> Result<Option<String>, String> {
    secrets::get(provider.id()).map_err(|error| {
        eprintln!("Could not read the API key: {error:#}");
        "Couldn't read the API key from the Keychain.".to_owned()
    })
}

/// A remote Provider set up for a job, as the Provider selects list it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SetUp {
    pub(super) provider: Provider,
    /// Whether its address is on this Mac, so what it gets stays here.
    pub(super) on_this_mac: bool,
}

impl NativeView {
    /// The address requests to `provider` go to: the one saved in Settings, else its usual
    /// one; "" for a server not given one yet, and for Apple's and Local Models.
    pub(super) fn provider_address(&self, provider: Provider) -> String {
        let Some(remote) = provider.remote() else {
            return String::new();
        };
        let saved = self.store.provider(provider.id()).base_url;
        remote
            .base_url(saved.as_deref().unwrap_or_default())
            .to_owned()
    }

    /// The Model the remote `provider` uses for `job`: the one saved in Settings, else its
    /// default; `None` when it doesn't do `job` or has neither.
    pub(super) fn provider_model(&self, provider: Provider, job: Job) -> Option<String> {
        let remote = provider.remote()?;
        let settings = self.store.provider(provider.id());
        remote
            .model(job, saved_model(&settings, job))
            .map(str::to_owned)
    }

    /// Whether the remote `provider` has what every request needs: a key if it requires one,
    /// and an address requests can go to. Its Models are then picked in their jobs' sections.
    pub(super) fn is_set_up(&self, provider: Provider) -> bool {
        provider.remote().is_some_and(|remote| {
            let saved = self.store.provider(provider.id()).base_url;
            remote.is_set_up(
                self.providers.has_key(provider),
                saved.as_deref().unwrap_or_default(),
            )
        })
    }

    /// Whether `provider` can do `job` with what Settings holds. Apple's Models always offer to
    /// transcribe, and do the rest while Apple Intelligence can run; a Local Model transcribes
    /// once picked and downloaded; a remote Provider needs a key if it takes one, an address
    /// requests can go to, and a Model for `job`.
    pub(super) fn ready_for(&self, provider: Provider, job: Job) -> bool {
        if !provider.does(job) {
            return false;
        }
        match provider.remote() {
            Some(remote) => remote_ready(
                remote,
                job,
                self.providers.has_key(provider),
                &self.store.provider(provider.id()),
            ),
            None if provider == Provider::Local => self.picked_local_model().is_some(),
            None => job == Job::Transcription || apple_availability() == Availability::Available,
        }
    }

    /// The remote Providers set up for `job`, in Settings' order.
    pub(super) fn set_up_remotes(&self, job: Job) -> Vec<SetUp> {
        (Provider::REMOTE.into_iter())
            .filter(|&provider| provider.does(job) && self.is_set_up(provider))
            .map(|provider| SetUp {
                provider,
                on_this_mac: runs_locally(provider, &self.provider_address(provider)),
            })
            .collect()
    }

    /// The Models `provider` listed when it was last checked this run.
    pub(super) fn listed(&self, provider: Provider) -> Option<&Listed> {
        self.providers.listed.get(&provider)
    }

    /// Keeps the Models a check of `provider` listed, for the Model selects.
    fn keep_listed(&mut self, provider: Provider, listed: Listed, cx: &mut Context<Self>) {
        self.providers.keep_listed(provider, listed);
        self.refresh_model_selects(cx);
        cx.notify();
    }

    /// Keeps `key` in the Keychain as `provider`'s once a check passed with it, with the
    /// Models that check `listed`. The error is for the user.
    fn save_key(
        &mut self,
        provider: Provider,
        key: &str,
        listed: Listed,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let Err(error) = secrets::set(provider.id(), key.trim()) {
            eprintln!("Could not save the key: {error:#}");
            return Err("Couldn't save the key.".to_owned());
        }
        self.providers.saved_keys.insert(provider);
        self.providers.keep_listed(provider, listed);
        self.provider_changed(provider, cx);
        Ok(())
    }

    /// Deletes `provider`'s key from the Keychain, and the Models it listed. The error is for
    /// the user.
    fn remove_key(&mut self, provider: Provider, cx: &mut Context<Self>) -> Result<(), String> {
        if let Err(error) = secrets::delete(provider.id()) {
            eprintln!("Could not remove the key: {error:#}");
            return Err("Couldn't remove the key.".to_owned());
        }
        self.providers.saved_keys.remove(&provider);
        self.providers.forget_listed(provider);
        self.provider_changed(provider, cx);
        Ok(())
    }

    /// Keeps `address` as where `provider`'s requests go, "" for its usual address, as it is
    /// typed. The Models it listed and its failures were about the old address.
    fn set_provider_address(&mut self, provider: Provider, address: &str, cx: &mut Context<Self>) {
        let mut settings = self.store.provider(provider.id());
        settings.base_url = Some(address.to_owned());
        self.store.set_provider(provider.id(), settings);
        self.providers.forget_listed(provider);
        self.forget_summary_failures(provider);
        self.forget_transcription_failures(provider);
        self.refresh_model_selects(cx);
        cx.notify();
    }

    /// Keeps `model` as the Model `provider` uses for `job`, and forgets what its old Model
    /// failed at; a Summary Model may now summarize older Transcripts.
    pub(super) fn set_provider_model(
        &mut self,
        provider: Provider,
        job: Job,
        model: &str,
        cx: &mut Context<Self>,
    ) {
        let mut settings = self.store.provider(provider.id());
        let saved = match job {
            Job::Transcription => &mut settings.transcription_model,
            Job::Summary => &mut settings.summary_model,
            Job::Intelligence => &mut settings.intelligence_model,
        };
        *saved = Some(model.to_owned());
        self.store.set_provider(provider.id(), settings);
        match job {
            Job::Transcription => self.forget_transcription_failures(provider),
            Job::Summary => {
                self.forget_summary_failures(provider);
                self.summarize_unsummarized(cx);
            }
            Job::Intelligence => {}
        }
        self.refresh_model_selects(cx);
        cx.notify();
    }

    /// Follows a key saved or removed for `provider`: its failures may not happen again, the
    /// Provider and Model selects list it as it now is, and it may now summarize older
    /// Transcripts.
    fn provider_changed(&mut self, provider: Provider, cx: &mut Context<Self>) {
        self.forget_summary_failures(provider);
        self.forget_transcription_failures(provider);
        self.refresh_model_selects(cx);
        self.summarize_unsummarized(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{remote_ready, saved_model, with_keys};
    use crumble_remote::{Job, Provider, Remote};
    use crumble_store::ProviderSettings;
    use std::collections::HashSet;

    fn remote(provider: Provider) -> &'static Remote {
        provider
            .remote()
            .unwrap_or_else(|| panic!("{provider:?} isn't remote"))
    }

    #[test]
    fn keys_count_only_for_the_remote_providers_this_crumble_knows() {
        let accounts = [
            "anthropic",
            "openai-compatible",
            "apple",
            "claude",
            "copilot",
        ];
        assert_eq!(
            with_keys(accounts.map(str::to_owned)),
            HashSet::from([Provider::Anthropic, Provider::OpenAiCompatible])
        );
    }

    #[test]
    fn each_job_reads_its_own_saved_model() {
        let settings = ProviderSettings {
            summary_model: Some("summary".to_owned()),
            transcription_model: Some("transcription".to_owned()),
            ..ProviderSettings::default()
        };
        assert_eq!(saved_model(&settings, Job::Summary), Some("summary"));
        assert_eq!(
            saved_model(&settings, Job::Transcription),
            Some("transcription")
        );
        assert_eq!(saved_model(&settings, Job::Intelligence), None);
    }

    #[test]
    fn a_company_is_ready_with_its_key_for_the_jobs_it_does() {
        let anthropic = remote(Provider::Anthropic);
        let defaults = ProviderSettings::default();
        assert!(remote_ready(anthropic, Job::Summary, true, &defaults));
        assert!(remote_ready(anthropic, Job::Intelligence, true, &defaults));
        assert!(!remote_ready(anthropic, Job::Summary, false, &defaults));
        // Its Models don't transcribe.
        assert!(!remote_ready(
            anthropic,
            Job::Transcription,
            true,
            &defaults
        ));
        let unsendable = ProviderSettings {
            base_url: Some("api.anthropic.com".to_owned()),
            ..ProviderSettings::default()
        };
        assert!(!remote_ready(anthropic, Job::Summary, true, &unsendable));
    }

    #[test]
    fn a_server_needs_an_address_and_a_model_but_no_key() {
        let server = remote(Provider::OpenAiCompatible);
        let defaults = ProviderSettings::default();
        assert!(!remote_ready(server, Job::Transcription, true, &defaults));
        let addressed = ProviderSettings {
            base_url: Some("http://localhost:11434/v1".to_owned()),
            ..ProviderSettings::default()
        };
        // Its Transcription Model has a default; its chat Models are typed in.
        assert!(remote_ready(server, Job::Transcription, false, &addressed));
        assert!(!remote_ready(server, Job::Summary, false, &addressed));
        let with_model = ProviderSettings {
            summary_model: Some("llama3.2".to_owned()),
            ..addressed
        };
        assert!(remote_ready(server, Job::Summary, false, &with_model));
        assert!(!remote_ready(server, Job::Intelligence, false, &with_model));
    }
}
