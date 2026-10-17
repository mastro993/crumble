//! Settings › Models: the Summary Model, and what each remote Provider needs: Claude's API key
//! and Model id, and an OpenAI-compatible server's address, optional key and Model. Choices are
//! kept through `Store`; keys go to the Keychain through `secrets` and are never shown again.

use super::NativeView;
use super::controls::{Inactive, button};
use crumble_store::{Store, secrets};
use crumble_summarize::{Availability, Claude, OpenAiCompatible, Provider, apple_availability};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectItem, SelectState};
use gpui_kit::component::{ActiveTheme, IndexPath, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::{
    AppContext, Context, Div, Entity, IntoElement, ParentElement, SharedString, Styled,
    Subscription, Task, Window, div, px,
};
use std::collections::{HashMap, HashSet};

/// Width of the Summary Model select and of each field.
const FIELD_WIDTH: f32 = 240.0;

/// The Models section's fields, and what it knows of the Keychain.
pub(super) struct ModelSettings {
    provider: Entity<SelectState<Vec<ProviderItem>>>,
    /// Apple's availability the select's items were made with.
    apple: Availability,
    claude_key: Entity<InputState>,
    claude_model: Entity<InputState>,
    openai_url: Entity<InputState>,
    openai_key: Entity<InputState>,
    openai_model: Entity<InputState>,
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

fn provider_items(apple: Availability) -> Vec<ProviderItem> {
    Provider::ALL
        .into_iter()
        .map(|provider| ProviderItem {
            provider,
            disabled: provider == Provider::Apple && apple != Availability::Available,
        })
        .collect()
}

/// The select's row for `provider`.
fn index_of(provider: Option<Provider>) -> Option<IndexPath> {
    let provider = provider?;
    Provider::ALL
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
        store
            .summary_provider(apple == Availability::Available)
            .and_then(Provider::from_id),
    );
    let provider = cx.new(|cx| SelectState::new(provider_items(apple), selected, window, cx));
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
    let subscriptions = vec![
        cx.subscribe_in(
            &provider,
            window,
            |view, _, event: &SelectEvent<_>, _, cx| {
                let SelectEvent::Confirm(Some(provider)) = event else {
                    return;
                };
                view.store.set_summary_provider(provider.id());
                view.summarize_unsummarized(cx);
                cx.notify();
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
        claude_key,
        claude_model,
        openai_url,
        openai_key,
        openai_model,
        saved_keys,
        checks: HashMap::new(),
        _subscriptions: subscriptions,
    }
}

/// Keeps `input`'s text through `save` as it is typed, and forgets `provider`'s Test result,
/// which no longer says anything about it. Enter or leaving the field backfills Summaries when
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
                cx.notify();
            }
            // A finished address or Model may make the Summary Model ready to backfill.
            InputEvent::PressEnter { .. } | InputEvent::Blur => view.summarize_unsummarized(cx),
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
                field.update(cx, |input, cx| input.set_value("", window, cx));
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
                    Ok(()) => Check::Passed,
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

    /// The Models section: the Summary Model, then Claude's and the OpenAI-compatible server's
    /// settings.
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
                select.set_selected_index(index_of(selected), window, cx);
            });
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
            .child(subtitle("Claude"))
            .child(self.key_row(Provider::Claude, cx))
            .child(row("Model", Input::new(&self.models.claude_model).small()))
            .child(self.test_row(Provider::Claude, cx))
            .child(subtitle("OpenAI-compatible server"))
            .child(row("Address", Input::new(&self.models.openai_url).small()))
            .child(self.key_row(Provider::OpenAiCompatible, cx))
            .child(row("Model", Input::new(&self.models.openai_model).small()))
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
