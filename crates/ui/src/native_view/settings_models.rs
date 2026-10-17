//! Settings › Models: the Summary Model, the Transcription Model and its language, and what each
//! remote Provider needs: Claude's API key and Model id, and an OpenAI-compatible server's
//! address, optional key, and a Model for each job. Choices are kept through `Store`; keys go to
//! the Keychain through `secrets` and are never shown again.

use super::NativeView;
use super::controls::{Inactive, button};
use super::notes::TRANSCRIBERS;
use crumble_remote::Provider;
use crumble_store::{Store, secrets};
use crumble_summarize::{Availability, Claude, OpenAiCompatible, apple_availability};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectItem, SelectState};
use gpui_kit::component::{ActiveTheme, IndexPath, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::{
    AppContext, Context, Div, Entity, IntoElement, ParentElement, SharedString, Styled,
    Subscription, Task, Window, div, px,
};
use std::collections::{HashMap, HashSet};

/// Width of each select and field.
const FIELD_WIDTH: f32 = 240.0;

/// The Models section's fields, and what it knows of the Keychain.
pub(super) struct ModelSettings {
    provider: Entity<SelectState<Vec<ProviderItem>>>,
    /// Apple's availability the select's items were made with.
    apple: Availability,
    transcription: Entity<SelectState<Vec<ProviderItem>>>,
    language: Entity<SelectState<Vec<LanguageItem>>>,
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
    Provider::ALL
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

/// Makes the section's fields with the stored choices, and reads which keys are saved.
pub(super) fn model_settings(
    store: &Store,
    window: &mut Window,
    cx: &mut Context<NativeView>,
) -> ModelSettings {
    let apple = apple_availability();
    let selected = index_of(
        &Provider::ALL,
        store
            .summary_provider(apple == Availability::Available)
            .and_then(Provider::from_id),
    );
    let provider = cx.new(|cx| SelectState::new(provider_items(apple), selected, window, cx));
    let transcribers = TRANSCRIBERS
        .into_iter()
        .map(|provider| ProviderItem {
            provider,
            disabled: false,
        })
        .collect();
    let selected = index_of(
        &TRANSCRIBERS,
        Provider::from_id(store.transcription_provider()),
    );
    let transcription = cx.new(|cx| SelectState::new(transcribers, selected, window, cx));
    let mut languages = language_items();
    let stored = store.transcription_locale();
    let selected = (languages.iter())
        .position(|item| item.locale.as_deref() == stored)
        .unwrap_or_else(|| {
            // A locale SpeechAnalyzer no longer lists still applies, so it shows as picked.
            let locale = stored.unwrap_or_default().to_owned();
            let name = crumble_transcribe::language_name(&locale).into();
            languages.push(LanguageItem {
                locale: Some(locale),
                name,
            });
            languages.len() - 1
        });
    let language = cx.new(|cx| {
        SelectState::new(languages, Some(IndexPath::new(selected)), window, cx).searchable(true)
    });
    let field = |value: &str, placeholder: &str, window: &mut Window, cx: &mut Context<_>| {
        let (value, placeholder) = (value.to_owned(), placeholder.to_owned());
        cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .default_value(value)
        })
    };
    let key_field = |placeholder: &str, window: &mut Window, cx: &mut Context<_>| {
        let placeholder = placeholder.to_owned();
        cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .masked(true)
        })
    };
    let claude_key = key_field("API key", window, cx);
    let claude_model = field(store.claude_model(), "claude-sonnet-5-5", window, cx);
    let openai_url = field(
        store.openai_base_url(),
        "http://localhost:11434/v1",
        window,
        cx,
    );
    let openai_key = key_field("API key (optional)", window, cx);
    let openai_model = field(store.openai_model(), "llama3.2", window, cx);
    let transcription_model = field(store.transcription_model(), "whisper-1", window, cx);
    let subscriptions = vec![
        cx.subscribe_in(
            &provider,
            window,
            |view, _, event: &SelectEvent<_>, _, cx| {
                let SelectEvent::Confirm(Some(provider)) = event else {
                    return;
                };
                // What the previous Summary Model failed at is the new one's to try.
                if let Some(previous) = view.summary_provider() {
                    view.forget_summary_failures(previous);
                }
                view.store.set_summary_provider(provider.id());
                view.summarize_unsummarized(cx);
                cx.notify();
            },
        ),
        cx.subscribe_in(
            &transcription,
            window,
            |view, _, event: &SelectEvent<Vec<ProviderItem>>, _, cx| {
                let SelectEvent::Confirm(Some(provider)) = event else {
                    return;
                };
                // What the previous Transcription Model failed at is the new one's to try.
                view.forget_transcription_failures(view.transcription_provider());
                view.store.set_transcription_provider(provider.id());
                view.transcribe_untranscribed(cx);
                cx.notify();
            },
        ),
        cx.subscribe_in(
            &language,
            window,
            |view, _, event: &SelectEvent<Vec<LanguageItem>>, _, cx| {
                if let SelectEvent::Confirm(Some(locale)) = event {
                    view.store.set_transcription_locale(locale.as_deref());
                    // A language that failed, or one Apple's Model lacked, may work now.
                    TRANSCRIBERS
                        .into_iter()
                        .for_each(|provider| view.forget_transcription_failures(provider));
                    view.transcribe_untranscribed(cx);
                    cx.notify();
                }
            },
        ),
        // Not a Summary setting, so Summary checks and failures stay.
        cx.subscribe_in(
            &transcription_model,
            window,
            |view, input, event, _, cx| match event {
                InputEvent::Change => {
                    let value = input.read(cx).value();
                    view.store.set_transcription_model(&value);
                    view.forget_transcription_failures(Provider::OpenAiCompatible);
                    cx.notify();
                }
                InputEvent::PressEnter { .. } | InputEvent::Blur => {
                    view.transcribe_untranscribed(cx);
                }
                InputEvent::Focus => {}
            },
        ),
        saved_on_change(
            &claude_model,
            Provider::Claude,
            window,
            cx,
            |store, value| store.set_claude_model(value),
        ),
        saved_on_change(
            &openai_url,
            Provider::OpenAiCompatible,
            window,
            cx,
            |store, value| store.set_openai_base_url(value),
        ),
        saved_on_change(
            &openai_model,
            Provider::OpenAiCompatible,
            window,
            cx,
            |store, value| store.set_openai_model(value),
        ),
        cx.subscribe_in(&claude_key, window, |view, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                view.save_key(Provider::Claude, window, cx);
            }
        }),
        cx.subscribe_in(&openai_key, window, |view, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                view.save_key(Provider::OpenAiCompatible, window, cx);
            }
        }),
    ];
    let saved_keys = [Provider::Claude, Provider::OpenAiCompatible]
        .into_iter()
        .filter(|provider| match secrets::get(provider.id()) {
            Ok(key) => key.is_some(),
            Err(error) => {
                eprintln!("Could not read the Keychain: {error:#}");
                false
            }
        })
        .collect();
    ModelSettings {
        provider,
        apple,
        transcription,
        language,
        claude_key,
        claude_model,
        openai_url,
        openai_key,
        openai_model,
        transcription_model,
        saved_keys,
        checks: HashMap::new(),
        _subscriptions: subscriptions,
    }
}

/// Keeps `input`'s text through `save` as it is typed, and forgets `provider`'s Test result and
/// the Summary failures it caused, which no longer say anything about it. Enter or leaving the field backfills Summaries when
/// the Summary Model is now ready to.
fn saved_on_change(
    input: &Entity<InputState>,
    provider: Provider,
    window: &mut Window,
    cx: &mut Context<NativeView>,
    save: fn(&mut Store, &str),
) -> Subscription {
    cx.subscribe_in(
        input,
        window,
        move |view, input, event, _, cx| match event {
            InputEvent::Change => {
                let value = input.read(cx).value();
                save(&mut view.store, &value);
                view.models.checks.remove(&provider);
                view.forget_summary_failures(provider);
                // The server's address serves Transcripts too.
                view.forget_transcription_failures(provider);
                cx.notify();
            }
            // A finished address or Model may make a Model ready to backfill.
            InputEvent::PressEnter { .. } | InputEvent::Blur => {
                view.summarize_unsummarized(cx);
                view.transcribe_untranscribed(cx);
            }
            InputEvent::Focus => {}
        },
    )
}

impl NativeView {
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
                self.transcribe_untranscribed(cx);
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

    /// The Models section: the Summary Model, the Transcription Model and its language, then
    /// Claude's and the OpenAI-compatible server's settings.
    pub(super) fn render_model_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        // Apple Intelligence can be turned on or off while Crumble runs.
        let apple = apple_availability();
        if apple != self.models.apple {
            self.models.apple = apple;
            let selected = self.summary_provider();
            self.models.provider.update(cx, |select, cx| {
                select.set_items(provider_items(apple), window, cx);
                // Apple's row may have been shown as picked only because it could run.
                select.set_selected_index(index_of(&Provider::ALL, selected), window, cx);
            });
            if apple == Availability::Available {
                // Its failures may be only that it could not run; backfill once out of render.
                cx.defer_in(window, |view, _, cx| {
                    view.forget_summary_failures(Provider::Apple);
                    view.summarize_unsummarized(cx);
                });
            }
        }
        let muted = cx.theme().muted_foreground;
        v_flex()
            .gap_4()
            .child(super::settings_page::group_title("Models", cx))
            .child(
                v_flex()
                    .gap_1()
                    .child(row(
                        "Summary Model",
                        Select::new(&self.models.provider)
                            .small()
                            .placeholder("Choose a Model")
                            .menu_width(px(FIELD_WIDTH))
                            .accessibility_label("Summary Model"),
                    ))
                    .children(
                        apple
                            .reason()
                            .map(|reason| note(format!("Apple isn't available: {reason}"), muted)),
                    )
                    .child(note(
                        "Claude and remote servers receive the Transcript's text.".to_owned(),
                        muted,
                    )),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(row(
                        "Transcription Model",
                        Select::new(&self.models.transcription)
                            .small()
                            .menu_width(px(FIELD_WIDTH))
                            .accessibility_label("Transcription Model"),
                    ))
                    .child(note(
                        "A remote server receives the Recording's audio.".to_owned(),
                        muted,
                    )),
            )
            .child(row(
                "Language",
                Select::new(&self.models.language)
                    .small()
                    .menu_width(px(FIELD_WIDTH))
                    .accessibility_label("Language"),
            ))
            .child(subtitle("Claude"))
            .child(self.key_row(Provider::Claude, cx))
            .child(row("Model", Input::new(&self.models.claude_model).small()))
            .child(self.test_row(Provider::Claude, cx))
            .child(subtitle("OpenAI-compatible server"))
            .child(row("Address", Input::new(&self.models.openai_url).small()))
            .child(self.key_row(Provider::OpenAiCompatible, cx))
            .child(row(
                "Summary Model",
                Input::new(&self.models.openai_model).small(),
            ))
            .child(row(
                "Transcription Model",
                Input::new(&self.models.transcription_model).small(),
            ))
            .child(self.test_row(Provider::OpenAiCompatible, cx))
    }

    /// `provider`'s key: a masked field and Save, or, once saved, a line saying so and Remove.
    fn key_row(&self, provider: Provider, cx: &mut Context<Self>) -> Div {
        let id = provider.id();
        if self.models.has_key(provider) {
            return row(
                "API key",
                h_flex()
                    .gap_2()
                    .justify_end()
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child("Saved in Keychain"),
                    )
                    .child(
                        button(format!("{id}-key-remove"))
                            .outline()
                            .small()
                            .label("Remove")
                            .on_click(
                                cx.listener(move |view, _, _, cx| view.remove_key(provider, cx)),
                            ),
                    ),
            );
        }
        row(
            "API key",
            h_flex()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .child(Input::new(self.key_field(provider)).small()),
                )
                .child(
                    button(format!("{id}-key-save"))
                        .outline()
                        .small()
                        .label("Save")
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.save_key(provider, window, cx)
                        })),
                ),
        )
    }

    /// Test, and how the last Test went.
    fn test_row(&self, provider: Provider, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let (status, color) = match self.models.checks.get(&provider) {
            None => (String::new(), theme.muted_foreground),
            Some(Check::Running { .. }) => ("Testing…".to_owned(), theme.muted_foreground),
            Some(Check::Passed) => ("OK".to_owned(), theme.success),
            Some(Check::Failed(message)) => (message.clone(), theme.danger),
        };
        let running = matches!(
            self.models.checks.get(&provider),
            Some(Check::Running { .. })
        );
        h_flex()
            .gap_2()
            .text_sm()
            .justify_end()
            .child(div().min_w_0().text_color(color).child(status))
            .child(
                button(format!("{}-test", provider.id()))
                    .outline()
                    .small()
                    .label("Test")
                    .inactive(running)
                    .on_click(cx.listener(move |view, _, _, cx| view.test_provider(provider, cx))),
            )
    }
}

/// A setting: its label, then its control at a fixed width.
fn row(label: &'static str, control: impl IntoElement) -> Div {
    h_flex()
        .justify_between()
        .gap_4()
        .text_sm()
        .child(label)
        // Select and Input fill their parent, so a fixed-width box sizes them.
        .child(div().w(px(FIELD_WIDTH)).flex_shrink_0().child(control))
}

/// A Provider's heading inside the section.
fn subtitle(title: &'static str) -> Div {
    div().pt_2().text_sm().font_medium().child(title)
}

/// A line of secondary text under a setting.
fn note(text: String, color: gpui_kit::Hsla) -> Div {
    div().text_xs().text_color(color).child(text)
}
