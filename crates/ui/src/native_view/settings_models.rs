//! Settings › Transcription: the Transcription Model, its language, and transcribing the
//! Recordings without a Transcript; Settings › Summary: the Summary Model; and Settings ›
//! Intelligence: the Model Crumble's Chat talks to. Each picks a Provider, from Apple's, Local
//! Models' for Transcripts, and the remote Providers set up for its job, then that Provider's
//! Model: one it lists or one typed, or a downloaded Local Model. Choices are kept through
//! `Store`. The selects and what picking does are in `fields`, the Provider selects' rows in
//! `rows` and the Model selects' in `models`, and the sections are drawn in `render`.

mod fields;
mod models;
mod render;
mod rows;

pub(super) use fields::model_settings;

use super::NativeView;
use super::providers::saved_model;
use super::select::{Choice, Select};
use crumble_local_models::Model;
use crumble_remote::{Job, Provider};
use crumble_store::Store;
use crumble_summarize::Availability;
use gpui_kit::{Context, Entity, SharedString, Subscription};
use models::{LocalModelItem, ModelItem, local_model_choices, model_choices};
use rows::{ProviderItem, provider_items};
use std::time::Instant;

/// A job's Provider select, and the Model select of a remote Provider picked in it.
pub(super) struct JobSelects {
    provider: Entity<Select<ProviderItem>>,
    model: Entity<Select<ModelItem>>,
}

/// The Transcription, Summary and Intelligence sections' selects.
pub(super) struct ModelSettings {
    transcription: JobSelects,
    summary: JobSelects,
    intelligence: JobSelects,
    /// The Model select while Local Models make Transcripts.
    local_model: Entity<Select<LocalModelItem>>,
    /// Apple's availability the selects' items were made with, and when it was last read.
    apple: Availability,
    apple_read: Instant,
    language: Entity<Select<LanguageItem>>,
    /// SpeechAnalyzer's locales by name, read once.
    apple_languages: Vec<LanguageItem>,
    _subscriptions: Vec<Subscription>,
}

impl ModelSettings {
    /// `job`'s selects.
    fn job(&self, job: Job) -> &JobSelects {
        match job {
            Job::Transcription => &self.transcription,
            Job::Summary => &self.summary,
            Job::Intelligence => &self.intelligence,
        }
    }

    /// The ISO 639 code of each locale SpeechAnalyzer transcribes.
    pub(super) fn apple_languages(&self) -> impl Iterator<Item = &str> {
        (self.apple_languages.iter())
            .filter_map(|item| item.locale.as_deref())
            .map(|locale| locale.split(['_', '-']).next().unwrap_or_default())
    }
}

/// The Language select's rows for the picked Transcription Model, and the row picked in
/// `store`: a Local Model's languages, with "Detect automatically" when it can, or else
/// SpeechAnalyzer's locales, which a remote Provider is asked in too. A stored locale the Model
/// does not list stays, as an extra row, so its Transcripts say why they fail.
fn languages_for(
    store: &Store,
    local: Option<&Model>,
    apple: &[LanguageItem],
) -> (Vec<LanguageItem>, usize) {
    let mut items = match local {
        Some(model) => local_language_items(model),
        None => apple.to_vec(),
    };
    let stored = store.transcription_locale();
    // A Local Model reads only a locale's language, so `it_IT` picks Italian.
    let same = |item: &LanguageItem| match (item.locale.as_deref(), stored) {
        (Some(item), Some(stored)) if local.is_some() => {
            item == stored || Some(item) == stored.split(['_', '-']).next()
        }
        (item, stored) => item == stored,
    };
    let selected = items.iter().position(same).unwrap_or_else(|| {
        let locale = stored.unwrap_or_default().to_owned();
        let name = match locale.as_str() {
            crumble_transcribe::DETECT => "Detect automatically".into(),
            _ => crumble_transcribe::language_name(&locale).into(),
        };
        items.push(LanguageItem {
            locale: Some(locale),
            name,
        });
        items.len() - 1
    });
    (items, selected)
}

/// "System (<language>)", "Detect automatically" when `model` can, then its languages by name.
fn local_language_items(model: &Model) -> Vec<LanguageItem> {
    let system = crumble_transcribe::language_name(&crumble_transcribe::current_locale());
    let mut items = vec![LanguageItem {
        locale: None,
        name: format!("System ({system})").into(),
    }];
    if model.detects_language {
        items.push(LanguageItem {
            locale: Some(crumble_transcribe::DETECT.to_owned()),
            name: "Detect automatically".into(),
        });
    }
    let mut languages: Vec<LanguageItem> = (model.languages.iter())
        .map(|code| LanguageItem {
            name: crumble_transcribe::language_name(code).into(),
            locale: Some(code.to_owned()),
        })
        .collect();
    languages.sort_by(|a, b| a.name.cmp(&b.name));
    items.extend(languages);
    items
}

/// A row of the Language select: the Mac's locale, `None`, or one SpeechAnalyzer supports.
#[derive(Clone)]
struct LanguageItem {
    locale: Option<String>,
    name: SharedString,
}

impl Choice for LanguageItem {
    type Value = Option<String>;

    fn title(&self) -> SharedString {
        self.name.clone()
    }

    fn value(&self) -> &Option<String> {
        &self.locale
    }
}

/// "System (<language>)", then SpeechAnalyzer's locales by name.
// ponytail: read once at launch on the main thread, about 70 ms; read on the first visit to
// Settings if launch time matters.
fn language_items() -> Vec<LanguageItem> {
    let system = crumble_transcribe::language_name(&crumble_transcribe::current_locale());
    let mut locales: Vec<LanguageItem> = crumble_transcribe::supported_locales()
        .into_iter()
        .map(|locale| LanguageItem {
            name: crumble_transcribe::language_name(&locale).into(),
            locale: Some(locale),
        })
        .collect();
    locales.sort_by(|a, b| a.name.cmp(&b.name));
    let mut items = vec![LanguageItem {
        locale: None,
        name: format!("System ({system})").into(),
    }];
    items.extend(locales);
    items
}

impl NativeView {
    /// The Provider picked for `job`: Apple's for Transcripts until another is, and for
    /// Summaries while it can run, as last read once a second rather than asking macOS on each
    /// frame.
    fn picked_provider(&self, job: Job) -> Option<Provider> {
        match job {
            Job::Transcription => Some(self.transcription_provider()),
            Job::Summary => {
                let apple_runs = self.models.apple == Availability::Available;
                (self.store.summary_provider(apple_runs)).and_then(Provider::from_id)
            }
            Job::Intelligence => (self.store.intelligence_provider()).and_then(Provider::from_id),
        }
    }

    /// Makes the selects list what can do each job now, as after a download, a Delete or a Use,
    /// Apple Intelligence turning on or off, or a Provider's key, address, Models or Model
    /// changing: the Providers set up for it, the picked one's Models, and the picked
    /// Transcription Model's languages.
    pub(super) fn refresh_model_selects(&mut self, cx: &mut Context<Self>) {
        let apple_runs = self.models.apple == Availability::Available;
        let local_models = !self.local_models.downloaded().is_empty();
        for job in [Job::Transcription, Job::Summary, Job::Intelligence] {
            let picked = self.picked_provider(job);
            let set_up = self.set_up_remotes(job);
            let (items, selected) = provider_items(job, apple_runs, local_models, &set_up, picked);
            let selects = self.models.job(job);
            (selects.provider).update(cx, |select, cx| select.set_items(items, selected, cx));
            let Some((picked, remote)) = picked.and_then(|picked| Some((picked, picked.remote()?)))
            else {
                continue;
            };
            let settings = self.store.provider(picked.id());
            let saved = saved_model(&settings, job);
            let (items, selected) = model_choices(remote, job, saved, self.listed(picked));
            (selects.model).update(cx, |select, cx| select.set_items(items, selected, cx));
        }
        let (items, selected) =
            local_model_choices(self.local_models.downloaded(), self.store.local_model());
        (self.models.local_model).update(cx, |select, cx| select.set_items(items, selected, cx));
        let local = self
            .picked_local_model()
            .map(|downloaded| &downloaded.model);
        let local = local.filter(|_| self.transcription_provider() == Provider::Local);
        let (languages, selected) = languages_for(&self.store, local, &self.models.apple_languages);
        (self.models.language).update(cx, |select, cx| {
            select.set_items(languages, Some(selected), cx)
        });
    }
}
