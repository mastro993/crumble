//! The Models section's fields, made with the stored choices, and what each change to them
//! keeps through `Store` and starts again.

use super::super::NativeView;
use super::super::notes::TRANSCRIBERS;
use super::{LanguageItem, ModelSettings, ProviderItem, index_of, language_items, provider_items};
use crumble_remote::Provider;
use crumble_store::{Store, secrets};
use crumble_summarize::{Availability, apple_availability};
use gpui_kit::component::IndexPath;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::select::{SelectEvent, SelectState};
use gpui_kit::{AppContext, Context, Entity, Subscription, Window};
use std::collections::HashMap;

/// Makes the section's fields with the stored choices, and reads which keys are saved.
pub(in super::super) fn model_settings(
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
        // The server's address serves Transcripts too.
        cx.subscribe_in(&openai_url, window, |view, _, event, _, cx| match event {
            InputEvent::Change => view.forget_transcription_failures(Provider::OpenAiCompatible),
            InputEvent::PressEnter { .. } | InputEvent::Blur => view.transcribe_untranscribed(cx),
            InputEvent::Focus => {}
        }),
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
                cx.notify();
            }
            // A finished address or Model may make the Summary Model ready to backfill.
            InputEvent::PressEnter { .. } | InputEvent::Blur => view.summarize_unsummarized(cx),
            InputEvent::Focus => {}
        },
    )
}
