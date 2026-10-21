//! The Transcription, Summary and Providers sections' fields, made with the stored choices, and
//! what each change to them keeps through `Store` and starts again. Picking a Transcription
//! Model or Language transcribes nothing on its own (ADR 0010).

use super::super::NativeView;
use super::super::notes::TRANSCRIBERS;
use super::super::select::{Picked, Select};
use super::{
    ModelSettings, Transcriber, index_of, language_items, languages_for, picked_transcriber,
    provider_items, transcriber_items,
};
use crumble_local_models::Downloaded;
use crumble_remote::Provider;
use crumble_store::{Store, secrets};
use crumble_summarize::{Availability, apple_availability};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext, Context, Entity, Subscription, Window};
use std::collections::HashMap;
use std::time::Instant;

/// Makes the sections' fields with the stored choices and the `downloaded` Local Models, and
/// reads which keys are saved.
pub(in super::super) fn model_settings(
    store: &Store,
    downloaded: &[Downloaded],
    window: &mut Window,
    cx: &mut Context<NativeView>,
) -> ModelSettings {
    let apple = apple_availability();
    let selected = index_of(
        &Provider::SUMMARIZERS,
        store
            .summary_provider(apple == Availability::Available)
            .and_then(Provider::from_id),
    );
    let provider = cx.new(|cx| {
        Select::new("Summary Model", provider_items(apple), selected, cx)
            .placeholder("Choose a Model")
    });
    let transcribers = transcriber_items(downloaded);
    let picked = picked_transcriber(store);
    let selected = (transcribers.iter()).position(|item| item.transcriber == picked);
    let transcription = cx.new(|cx| Select::new("Transcription Model", transcribers, selected, cx));
    let apple_languages = language_items();
    let local = match &picked {
        Transcriber::Local(slug) => (downloaded.iter())
            .find(|downloaded| &downloaded.model.slug == slug)
            .map(|downloaded| &downloaded.model),
        Transcriber::Provider(_) => None,
    };
    let (languages, selected) = languages_for(store, local, &apple_languages);
    let language = cx.new(|cx| Select::new("Language", languages, Some(selected), cx).searchable());
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
        cx.subscribe(
            &provider,
            |view, _, Picked(provider): &Picked<Provider>, cx| {
                // What the previous Summary Model failed at is the new one's to try.
                if let Some(previous) = view.summary_provider() {
                    view.forget_summary_failures(previous);
                }
                view.store.set_summary_provider(provider.id());
                view.summarize_unsummarized(cx);
                cx.notify();
            },
        ),
        cx.subscribe(
            &transcription,
            |view, _, Picked(transcriber): &Picked<Transcriber>, cx| {
                // What the previous Transcription Model failed at is the new one's to try.
                view.forget_transcription_failures(view.transcription_provider());
                match transcriber {
                    Transcriber::Provider(provider) => {
                        view.store.set_transcription_provider(provider.id());
                    }
                    Transcriber::Local(slug) => {
                        view.store.set_transcription_provider(Provider::Local.id());
                        view.store.set_local_model(Some(slug));
                    }
                }
                // Its languages may differ.
                view.refresh_transcription_selects(cx);
                cx.notify();
            },
        ),
        cx.subscribe(
            &language,
            |view, _, Picked(locale): &Picked<Option<String>>, cx| {
                view.store.set_transcription_locale(locale.as_deref());
                // A language that failed, or one Apple's Model lacked, may work now.
                TRANSCRIBERS
                    .into_iter()
                    .for_each(|provider| view.forget_transcription_failures(provider));
                cx.notify();
            },
        ),
        // Not a Summary setting, so Summary checks and failures stay.
        cx.subscribe_in(&transcription_model, window, |view, input, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                let value = input.read(cx).value();
                view.store.set_transcription_model(&value);
                view.forget_transcription_failures(Provider::OpenAiCompatible);
                cx.notify();
            }
        }),
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
        cx.subscribe_in(&openai_url, window, |view, _, event, _, _| {
            if matches!(event, InputEvent::Change) {
                view.forget_transcription_failures(Provider::OpenAiCompatible);
            }
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
        apple_read: Instant::now(),
        transcription,
        language,
        apple_languages,
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
