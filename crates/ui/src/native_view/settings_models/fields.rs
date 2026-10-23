//! The Transcription, Summary and Intelligence sections' selects, and what picking in each keeps
//! through `Store` and starts again. Their rows come from `NativeView::refresh_model_selects`.
//! Picking a Provider, a Model or a Language transcribes nothing on its own (ADR 0010).

use super::super::NativeView;
use super::super::select::{Picked, Select};
use super::{JobSelects, ModelSettings, language_items};
use crumble_remote::{Job, Provider};
use crumble_summarize::apple_availability;
use gpui_kit::{AppContext, Context, Subscription};
use std::time::Instant;

/// The name of `job`'s Model select, and of the Local Model select for Transcripts.
pub(super) fn model_title(job: Job) -> &'static str {
    match job {
        Job::Transcription => "Transcription Model",
        Job::Summary => "Summary Model",
        Job::Intelligence => "Intelligence Model",
    }
}

/// The name of `job`'s Provider select.
fn provider_title(job: Job) -> &'static str {
    match job {
        Job::Transcription => "Transcription Provider",
        Job::Summary => "Summary Provider",
        Job::Intelligence => "Intelligence Provider",
    }
}

/// Makes the sections' selects, empty until `NativeView::refresh_model_selects` lists their
/// rows.
pub(in super::super) fn model_settings(cx: &mut Context<NativeView>) -> ModelSettings {
    let mut subscriptions = Vec::new();
    let transcription = job_selects(Job::Transcription, cx, &mut subscriptions);
    let summary = job_selects(Job::Summary, cx, &mut subscriptions);
    let intelligence = job_selects(Job::Intelligence, cx, &mut subscriptions);
    let local_model = cx.new(|cx| {
        Select::new(model_title(Job::Transcription), Vec::new(), None, cx)
            .placeholder("Choose a Model")
    });
    let language = cx.new(|cx| Select::new("Language", Vec::new(), None, cx).searchable());
    subscriptions.push(cx.subscribe(
        &local_model,
        |view, _, Picked(slug): &Picked<String>, cx| view.use_local_model(slug, cx),
    ));
    subscriptions.push(cx.subscribe(
        &language,
        |view, _, Picked(locale): &Picked<Option<String>>, cx| {
            view.store.set_transcription_locale(locale.as_deref());
            // A language that failed, or one Apple's Model lacked, may work now.
            (Provider::ALL.into_iter())
                .filter(|provider| provider.does(Job::Transcription))
                .for_each(|provider| view.forget_transcription_failures(provider));
            cx.notify();
        },
    ));
    ModelSettings {
        transcription,
        summary,
        intelligence,
        local_model,
        apple: apple_availability(),
        apple_read: Instant::now(),
        language,
        apple_languages: language_items(),
        _subscriptions: subscriptions,
    }
}

/// `job`'s Provider select, and the Model select of a remote Provider picked in it, a combo box
/// where a Model the Provider doesn't list can be typed.
fn job_selects(
    job: Job,
    cx: &mut Context<NativeView>,
    subscriptions: &mut Vec<Subscription>,
) -> JobSelects {
    let provider = cx.new(|cx| {
        Select::new(provider_title(job), Vec::new(), None, cx).placeholder("Choose a Provider")
    });
    let model = cx.new(|cx| {
        Select::new(model_title(job), Vec::new(), None, cx)
            .placeholder("Choose a Model")
            .searchable()
    });
    subscriptions.push(cx.subscribe(
        &provider,
        move |view, _, Picked(provider): &Picked<Provider>, cx| {
            view.pick_provider(job, *provider, cx)
        },
    ));
    subscriptions.push(cx.subscribe(
        &model,
        move |view, _, Picked(model): &Picked<String>, cx| {
            let picked = view.picked_provider(job);
            if let Some(provider) = picked.filter(|picked| picked.remote().is_some()) {
                view.set_provider_model(provider, job, model, cx);
            }
        },
    ));
    JobSelects { provider, model }
}

impl NativeView {
    /// Makes `provider` do `job`. What the previous one failed at is the new one's to try; Local
    /// Models start with the first downloaded one unless one is picked, and a Summary Model may
    /// now summarize older Transcripts.
    fn pick_provider(&mut self, job: Job, provider: Provider, cx: &mut Context<Self>) {
        match job {
            Job::Transcription => {
                self.forget_transcription_failures(self.transcription_provider());
                self.store.set_transcription_provider(provider.id());
                let first = (self.local_models.downloaded().first())
                    .map(|downloaded| downloaded.model.slug.clone());
                if let Some(first) = first
                    .filter(|_| provider == Provider::Local && self.picked_local_model().is_none())
                {
                    self.store.set_local_model(Some(&first));
                }
            }
            Job::Summary => {
                if let Some(previous) = self.summary_provider() {
                    self.forget_summary_failures(previous);
                }
                self.store.set_summary_provider(provider.id());
            }
            Job::Intelligence => self.store.set_intelligence_provider(provider.id()),
        }
        // A Provider kept only because it was picked leaves the list, and the Model select
        // lists the new one's Models and Transcripts its languages.
        self.refresh_model_selects(cx);
        if job == Job::Summary {
            self.summarize_unsummarized(cx);
        }
        cx.notify();
    }
}
