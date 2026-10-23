//! The rows of the Transcription, Summary and Intelligence Provider selects: Apple's, Local
//! Models' for Transcripts, then each remote Provider set up for the job, each led by the
//! Provider's logo. A picked remote Provider no longer set up, such as one whose key was
//! removed, stays as a row of its own, so the pick isn't silently replaced; its jobs then say
//! what it lacks.

use super::super::providers::{SetUp, provider_logo};
use super::super::select::{Choice, Leading};
use crumble_remote::{Job, Provider};
use crumble_theme::sidebar::ICON_SIZE;
use gpui_kit::SharedString;
use std::rc::Rc;

/// What a picked remote Provider that is no longer set up says in its row.
const NOT_SET_UP: &str = "Not set up";
/// What a set-up remote Provider at a loopback address says in its row.
const ON_THIS_MAC: &str = "On this Mac";

/// A row of a Provider select; Apple's is disabled while its Model can't make Summaries or
/// answer.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ProviderItem {
    provider: Provider,
    description: Option<SharedString>,
    disabled: bool,
}

impl ProviderItem {
    /// `provider`'s row, enabled; Apple's and Local Models' names say where they run.
    fn new(provider: Provider, description: Option<&'static str>) -> Self {
        Self {
            provider,
            description: description.map(SharedString::new_static),
            disabled: false,
        }
    }
}

impl Choice for ProviderItem {
    type Value = Provider;

    fn title(&self) -> SharedString {
        self.provider.name().into()
    }

    fn value(&self) -> &Provider {
        &self.provider
    }

    fn description(&self) -> Option<SharedString> {
        self.description.clone()
    }

    fn disabled(&self) -> bool {
        self.disabled
    }

    fn leading(&self) -> Option<Leading> {
        let provider = self.provider;
        Some(Rc::new(move |cx| provider_logo(provider, ICON_SIZE, cx)))
    }
}

/// The Provider select's rows for `job`, and the row of `picked`: Apple's, disabled for
/// Summaries and Intelligence unless `apple_runs`; for Transcripts, Local Models' when
/// `local_models` are downloaded or it is picked; then each of `set_up`; then `picked` when it
/// is a remote Provider that does `job` but is no longer set up.
pub(super) fn provider_items(
    job: Job,
    apple_runs: bool,
    local_models: bool,
    set_up: &[SetUp],
    picked: Option<Provider>,
) -> (Vec<ProviderItem>, Option<usize>) {
    let transcribes = job == Job::Transcription;
    // Apple's Model always offers to transcribe.
    let mut items = vec![ProviderItem {
        disabled: !transcribes && !apple_runs,
        ..ProviderItem::new(Provider::Apple, None)
    }];
    if transcribes && (local_models || picked == Some(Provider::Local)) {
        items.push(ProviderItem::new(Provider::Local, None));
    }
    items.extend((set_up.iter()).map(|set_up| {
        ProviderItem::new(set_up.provider, set_up.on_this_mac.then_some(ON_THIS_MAC))
    }));
    let selected = picked.and_then(|picked| {
        let found = items.iter().position(|item| item.provider == picked);
        found.or_else(|| {
            (picked.remote().is_some() && picked.does(job)).then(|| {
                items.push(ProviderItem::new(picked, Some(NOT_SET_UP)));
                items.len() - 1
            })
        })
    });
    (items, selected)
}

#[cfg(test)]
mod tests {
    use super::provider_items;
    use crate::native_view::providers::SetUp;
    use crate::native_view::select::Choice;
    use crumble_remote::{Job, Provider};

    fn set_up(provider: Provider, on_this_mac: bool) -> SetUp {
        SetUp {
            provider,
            on_this_mac,
        }
    }

    /// Each row's title, description and whether it is disabled, as the select shows them.
    fn shown<C: Choice>(items: &[C]) -> Vec<(String, Option<String>, bool)> {
        (items.iter())
            .map(|item| {
                let description = item.description().map(|line| line.to_string());
                (item.title().to_string(), description, item.disabled())
            })
            .collect()
    }

    fn row(
        title: &str,
        description: Option<&str>,
        disabled: bool,
    ) -> (String, Option<String>, bool) {
        (title.to_owned(), description.map(str::to_owned), disabled)
    }

    #[test]
    fn summary_and_intelligence_rows_are_apple_then_each_set_up_provider_with_its_logo() {
        let set_up = [
            set_up(Provider::Anthropic, false),
            set_up(Provider::OpenAiCompatible, true),
        ];
        let picked = Some(Provider::OpenAiCompatible);
        for job in [Job::Summary, Job::Intelligence] {
            let (items, selected) = provider_items(job, false, true, &set_up, picked);
            assert_eq!(
                shown(&items),
                [
                    row("Apple, on this Mac", None, true),
                    row("Anthropic", None, false),
                    row("OpenAI-compatible server", Some("On this Mac"), false),
                ],
                "{job:?}"
            );
            assert_eq!(selected, Some(2));
            assert!(items.iter().all(|item| item.leading().is_some()));
        }
        // Apple's row is enabled once its Model can run.
        let (items, _) = provider_items(Job::Intelligence, true, false, &set_up, None);
        assert!(!items[0].disabled());
    }

    #[test]
    fn a_picked_provider_no_longer_set_up_keeps_its_row() {
        let (items, selected) = provider_items(Job::Summary, true, false, &[], Some(Provider::Xai));
        assert_eq!(selected, Some(1));
        assert_eq!(shown(&items)[1], row("xAI", Some("Not set up"), false));
        // Nothing to keep: none picked, or one that can't do the job.
        assert_eq!(provider_items(Job::Summary, true, false, &[], None).1, None);
        let picked = Some(Provider::ElevenLabs);
        let (items, selected) = provider_items(Job::Summary, true, false, &[], picked);
        assert_eq!((items.len(), selected), (1, None));
    }

    #[test]
    fn transcription_rows_are_apple_local_models_then_set_up_providers() {
        let set_up = [set_up(Provider::OpenAi, false)];
        let picked = Some(Provider::OpenAi);
        let (items, selected) = provider_items(Job::Transcription, false, true, &set_up, picked);
        assert_eq!(
            shown(&items),
            [
                row("Apple, on this Mac", None, false),
                row("Crumble, on this Mac", None, false),
                row("OpenAI", None, false),
            ]
        );
        assert_eq!(selected, Some(2));
        assert!(items.iter().all(|item| item.leading().is_some()));
        // Local Models show once one is downloaded, or while picked.
        let picked = Some(Provider::Local);
        let (items, selected) = provider_items(Job::Transcription, true, false, &[], picked);
        assert_eq!((items.len(), selected), (2, Some(1)));
        let (items, _) = provider_items(Job::Transcription, true, false, &[], None);
        assert_eq!(items.len(), 1);
    }
}
