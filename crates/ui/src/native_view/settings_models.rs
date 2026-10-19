//! Settings › Transcription: the Transcription Model, its language, and transcribing the
//! Recordings without a Transcript; Settings › Summary: the Summary Model; and Settings ›
//! Providers: what each remote Provider needs, Claude's API key and Model id, and an
//! OpenAI-compatible server's address, optional key, and a Model for each job. Choices are kept
//! through `Store`; keys go to the Keychain through `secrets` and are never shown again.
//! The fields and what their changes do are in `fields`; the sections are drawn in `render`.

mod fields;
mod render;

pub(super) use fields::model_settings;

use super::NativeView;
use crumble_local_models::{Downloaded, Model};
use crumble_remote::Provider;
use crumble_store::Store;
use crumble_store::secrets;
use crumble_summarize::{Availability, Claude, OpenAiCompatible};
use gpui_kit::component::IndexPath;
use gpui_kit::component::input::InputState;
use gpui_kit::component::select::{SelectItem, SelectState};
use gpui_kit::{Context, Entity, SharedString, Subscription, Task, Window};
use std::collections::{HashMap, HashSet};

/// Width of each select and field.
const FIELD_WIDTH: f32 = 240.0;

/// The Transcription, Summary and Providers sections' fields, and what they know of the
/// Keychain.
pub(super) struct ModelSettings {
    provider: Entity<SelectState<Vec<ProviderItem>>>,
    /// Apple's availability the select's items were made with.
    apple: Availability,
    transcription: Entity<SelectState<Vec<TranscriberItem>>>,
    language: Entity<SelectState<Vec<LanguageItem>>>,
    /// SpeechAnalyzer's locales by name, read once.
    apple_languages: Vec<LanguageItem>,
    claude_key: Entity<InputState>,
    claude_model: Entity<InputState>,
    openai_url: Entity<InputState>,
    openai_key: Entity<InputState>,
    openai_model: Entity<InputState>,
    transcription_model: Entity<InputState>,
    /// The Providers with a key in the Keychain: read at launch, then kept as keys are saved and
    /// removed here.
    saved_keys: HashSet<Provider>,
    /// The last Test of each remote Provider.
    checks: HashMap<Provider, Check>,
    _subscriptions: Vec<Subscription>,
}

impl ModelSettings {
    /// Whether `provider` has a key in the Keychain.
    pub(super) fn has_key(&self, provider: Provider) -> bool {
        self.saved_keys.contains(&provider)
    }

    /// The ISO 639 code of each locale SpeechAnalyzer transcribes.
    pub(super) fn apple_languages(&self) -> impl Iterator<Item = &str> {
        (self.apple_languages.iter())
            .filter_map(|item| item.locale.as_deref())
            .map(|locale| locale.split(['_', '-']).next().unwrap_or_default())
    }
}

enum Check {
    /// Holds the Test; dropping it drops its result.
    Running {
        _task: Task<()>,
    },
    Passed,
    Failed(String),
}

/// A row of the Summary Model select; Apple's is disabled when its model can't run.
#[derive(Clone)]
struct ProviderItem {
    provider: Provider,
    disabled: bool,
}

impl SelectItem for ProviderItem {
    type Value = Provider;

    fn title(&self) -> SharedString {
        self.provider.name().into()
    }

    fn value(&self) -> &Provider {
        &self.provider
    }

    fn disabled(&self) -> bool {
        self.disabled
    }
}

/// What makes Transcripts: a Provider, or a downloaded Local Model by its slug.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Transcriber {
    Provider(Provider),
    Local(String),
}

/// A row of the Transcription Model select.
#[derive(Clone)]
struct TranscriberItem {
    transcriber: Transcriber,
    name: SharedString,
}

impl SelectItem for TranscriberItem {
    type Value = Transcriber;

    fn title(&self) -> SharedString {
        self.name.clone()
    }

    fn value(&self) -> &Transcriber {
        &self.transcriber
    }
}

/// Apple's Model, each downloaded Local Model, then the OpenAI-compatible server.
fn transcriber_items(downloaded: &[Downloaded]) -> Vec<TranscriberItem> {
    let provider = |provider: Provider| TranscriberItem {
        transcriber: Transcriber::Provider(provider),
        name: provider.name().into(),
    };
    let mut items = vec![provider(Provider::Apple)];
    items.extend(downloaded.iter().map(|downloaded| TranscriberItem {
        transcriber: Transcriber::Local(downloaded.model.slug.clone()),
        name: downloaded.model.name.clone().into(),
    }));
    items.push(provider(Provider::OpenAiCompatible));
    items
}

/// The Transcription Model picked in `store`.
fn picked_transcriber(store: &Store) -> Transcriber {
    match Provider::from_id(store.transcription_provider()) {
        Some(Provider::Local) => {
            Transcriber::Local(store.local_model().unwrap_or_default().to_owned())
        }
        Some(provider) => Transcriber::Provider(provider),
        None => Transcriber::Provider(Provider::Apple),
    }
}

/// The Language select's rows for the picked Transcription Model, and the row picked in
/// `store`: a Local Model's languages, with "Detect automatically" when it can, or else
/// SpeechAnalyzer's locales, which a remote server is asked in too. A stored locale the Model
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

impl SelectItem for LanguageItem {
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

fn provider_items(apple: Availability) -> Vec<ProviderItem> {
    Provider::SUMMARIZERS
        .into_iter()
        .map(|provider| ProviderItem {
            provider,
            disabled: provider == Provider::Apple && apple != Availability::Available,
        })
        .collect()
}

/// The row of `provider` in a select listing `providers`.
fn index_of(providers: &[Provider], provider: Option<Provider>) -> Option<IndexPath> {
    let provider = provider?;
    providers
        .iter()
        .position(|known| *known == provider)
        .map(IndexPath::new)
}

impl NativeView {
    /// Makes the Transcription Model and Language selects show the downloaded Local Models
    /// and the picked Model's languages, as after a download, a Delete or a Use.
    pub(super) fn refresh_transcription_selects(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let downloaded = self.local_models.downloaded();
        let items = transcriber_items(downloaded);
        let picked = picked_transcriber(&self.store);
        let selected = items
            .iter()
            .position(|item| item.transcriber == picked)
            .map(IndexPath::new);
        self.models.transcription.update(cx, |select, cx| {
            select.set_items(items, window, cx);
            select.set_selected_index(selected, window, cx);
        });
        let local = self
            .picked_local_model()
            .map(|downloaded| &downloaded.model);
        let local = local.filter(|_| self.transcription_provider() == Provider::Local);
        let (languages, selected) = languages_for(&self.store, local, &self.models.apple_languages);
        self.models.language.update(cx, |select, cx| {
            select.set_items(languages, window, cx);
            select.set_selected_index(Some(IndexPath::new(selected)), window, cx);
        });
    }

    /// The key field of `provider`.
    fn key_field(&self, provider: Provider) -> &Entity<InputState> {
        match provider {
            Provider::Claude => &self.models.claude_key,
            _ => &self.models.openai_key,
        }
    }

    /// Moves the key typed for `provider` into the Keychain and empties its field.
    fn save_key(&mut self, provider: Provider, window: &mut Window, cx: &mut Context<Self>) {
        let field = self.key_field(provider).clone();
        let key = field.read(cx).value().trim().to_owned();
        if key.is_empty() {
            return;
        }
        match secrets::set(provider.id(), &key) {
            Ok(()) => {
                self.models.saved_keys.insert(provider);
                self.models.checks.remove(&provider);
                self.forget_summary_failures(provider);
                self.forget_transcription_failures(provider);
                field.update(cx, |input, cx| input.set_value("", window, cx));
                self.summarize_unsummarized(cx);
            }
            Err(error) => {
                eprintln!("Could not save the key: {error:#}");
                self.models
                    .checks
                    .insert(provider, Check::Failed("Couldn't save the key.".to_owned()));
            }
        }
        cx.notify();
    }

    fn remove_key(&mut self, provider: Provider, cx: &mut Context<Self>) {
        match secrets::delete(provider.id()) {
            Ok(()) => {
                self.models.saved_keys.remove(&provider);
                self.models.checks.remove(&provider);
                self.forget_summary_failures(provider);
                self.forget_transcription_failures(provider);
            }
            Err(error) => {
                eprintln!("Could not remove the key: {error:#}");
                self.models.checks.insert(
                    provider,
                    Check::Failed("Couldn't remove the key.".to_owned()),
                );
            }
        }
        cx.notify();
    }

    /// Sends a tiny prompt to `provider` with what Settings holds, and shows how it went.
    fn test_provider(&mut self, provider: Provider, cx: &mut Context<Self>) {
        let key = match secrets::get(provider.id()) {
            Ok(key) => key,
            Err(error) => {
                eprintln!("Could not read the API key: {error:#}");
                self.models.checks.insert(
                    provider,
                    Check::Failed("Couldn't read the API key from the Keychain.".to_owned()),
                );
                cx.notify();
                return;
            }
        };
        let store = &self.store;
        let work = match provider {
            Provider::Claude => {
                let claude = Claude {
                    api_key: key.unwrap_or_default(),
                    model: store.claude_model().to_owned(),
                };
                cx.background_executor()
                    .spawn(async move { claude.check() })
            }
            _ => {
                let server = OpenAiCompatible {
                    base_url: store.openai_base_url().to_owned(),
                    api_key: key,
                    model: store.openai_model().to_owned(),
                };
                cx.background_executor()
                    .spawn(async move { server.check() })
            }
        };
        let task = cx.spawn(async move |view, cx| {
            let result = work.await;
            let _ = view.update(cx, |view, cx| {
                let check = match result {
                    Ok(()) => {
                        // Settings may have been right all along, with the server not yet up.
                        view.forget_summary_failures(provider);
                        view.summarize_unsummarized(cx);
                        Check::Passed
                    }
                    Err(error) => Check::Failed(error.to_string()),
                };
                view.models.checks.insert(provider, check);
                cx.notify();
            });
        });
        self.models
            .checks
            .insert(provider, Check::Running { _task: task });
        cx.notify();
    }
}
