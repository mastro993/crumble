//! Listing a remote Provider's Models for the Model select of a section that picked it: once a
//! run, in the background, with the saved key, or with none for a server that takes none. A
//! failure shows under the select until the Provider's key or address changes, or Settings
//! shows a section again. Also the ping every check sends, off the main thread.

use super::super::NativeView;
use super::{Providers, saved_key};
use crumble_remote::{Listed, Provider};
use gpui_kit::{App, Context, Task};

/// The key a ping sends.
pub(super) enum CheckKey {
    /// One typed in a Provider's dialog, not kept yet.
    Typed(String),
    /// The one in the Keychain, read off the main thread.
    Saved,
    /// None, for a server that takes none.
    NoKey,
}

/// Pings `provider` at `address` with `key`, off the main thread, where a saved key is read.
/// The Task gives the Models the Provider lists, or why its key or address won't do, for the
/// user; dropping it drops the result.
pub(super) fn ping(
    provider: Provider,
    address: String,
    key: CheckKey,
    cx: &App,
) -> Task<Result<Listed, String>> {
    cx.background_executor().spawn(async move {
        let key = match key {
            CheckKey::Typed(key) => Some(key),
            CheckKey::Saved => saved_key(provider)?,
            CheckKey::NoKey => None,
        };
        crumble_remote::ping(provider, &address, key.as_deref()).map_err(|error| error.to_string())
    })
}

/// A listing of a Provider's Models.
pub(super) enum Listing {
    /// Running; dropping it drops the result.
    Running { _task: Task<()> },
    /// Failed, and why, for the user.
    Failed(String),
}

impl Providers {
    /// Whether to list `provider`'s Models once it is `set_up`: it didn't list them this run,
    /// and no listing of them runs or failed.
    fn lists(&self, provider: Provider, set_up: bool) -> bool {
        set_up && !self.listed.contains_key(&provider) && !self.listing.contains_key(&provider)
    }

    /// Keeps the Models `provider` listed, which ends its listing.
    pub(super) fn keep_listed(&mut self, provider: Provider, listed: Listed) {
        self.listed.insert(provider, listed);
        self.listing.remove(&provider);
    }

    /// Forgets the Models `provider` listed, and its listing, as its key or address changed.
    pub(super) fn forget_listed(&mut self, provider: Provider) {
        self.listed.remove(&provider);
        self.listing.remove(&provider);
    }

    /// Why `provider`'s Models couldn't be listed, when its last listing failed.
    fn failure(&self, provider: Provider) -> Option<&str> {
        match self.listing.get(&provider) {
            Some(Listing::Failed(why)) => Some(why),
            _ => None,
        }
    }

    /// Forgets the listings that failed, leaving those running.
    fn forget_failures(&mut self) {
        (self.listing).retain(|_, listing| matches!(listing, Listing::Running { .. }));
    }
}

impl NativeView {
    /// Lists the Models of `provider`, in the background, once it is set up, unless it listed
    /// them this run or a listing of them runs or failed.
    pub(in super::super) fn list_models(&mut self, provider: Provider, cx: &mut Context<Self>) {
        if !(self.providers).lists(provider, self.is_set_up(provider)) {
            return;
        }
        let key = if self.providers.has_key(provider) {
            CheckKey::Saved
        } else {
            CheckKey::NoKey
        };
        let ping = ping(provider, self.provider_address(provider), key, cx);
        let task = cx.spawn(async move |view, cx| {
            let listed = ping.await;
            let _ = view.update(cx, |view, cx| view.models_listed(provider, listed, cx));
        });
        (self.providers.listing).insert(provider, Listing::Running { _task: task });
    }

    /// Follows a listing of `provider`'s Models: keeps what it listed, or why it failed. One that
    /// passed found the Provider answering at its address, so a Summary Model there may
    /// backfill.
    fn models_listed(
        &mut self,
        provider: Provider,
        listed: Result<Listed, String>,
        cx: &mut Context<Self>,
    ) {
        match listed {
            Ok(listed) => {
                self.keep_listed(provider, listed, cx);
                self.summarize_unsummarized(cx);
            }
            Err(why) => {
                (self.providers.listing).insert(provider, Listing::Failed(why));
                cx.notify();
            }
        }
    }

    /// Why `provider`'s Models couldn't be listed, when its last listing failed.
    pub(in super::super) fn listing_failure(&self, provider: Provider) -> Option<&str> {
        self.providers.failure(provider)
    }

    /// Forgets the listings that failed, so showing a section lists their Models again.
    pub(in super::super) fn forget_listing_failures(&mut self) {
        self.providers.forget_failures();
    }
}

#[cfg(test)]
mod tests {
    use super::{Listing, Providers};
    use crumble_remote::{Listed, Provider};
    use gpui_kit::Task;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn models_are_listed_once_a_run_and_again_once_a_failure_is_forgotten() {
        let mut providers = Providers {
            saved_keys: HashSet::new(),
            listed: HashMap::new(),
            listing: HashMap::new(),
        };
        let server = Provider::OpenAiCompatible;
        assert!(!providers.lists(server, false));
        assert!(providers.lists(server, true));

        // Showing a section again leaves a listing running.
        let running = Listing::Running {
            _task: Task::ready(()),
        };
        providers.listing.insert(server, running);
        providers.forget_failures();
        assert!(!providers.lists(server, true));

        // A failure stays until it is forgotten, then the Models are listed again.
        providers
            .listing
            .insert(server, Listing::Failed("No answer.".to_owned()));
        assert_eq!(providers.failure(server), Some("No answer."));
        assert!(!providers.lists(server, true));
        providers.forget_failures();
        assert_eq!(providers.failure(server), None);
        assert!(providers.lists(server, true));

        // Listed, they are not listed again until the key or the address changes.
        providers.keep_listed(server, Listed::default());
        assert!(!providers.lists(server, true));
        providers
            .listing
            .insert(server, Listing::Failed("No answer.".to_owned()));
        providers.forget_listed(server);
        assert_eq!(providers.failure(server), None);
        assert!(providers.lists(server, true));
    }
}
