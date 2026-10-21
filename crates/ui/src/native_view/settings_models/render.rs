//! Drawing the Transcription and Summary sections, their selects, and the Providers section:
//! each remote Provider's fields, key row, and Test.

use super::super::NativeView;
use super::super::controls::{Inactive, button};
use super::super::settings_page::SettingsSection;
use super::super::settings_page::form::{Group, Setting, field, groups};
use super::{Check, index_of, provider_items};
use crumble_remote::Provider;
use crumble_summarize::{Availability, apple_availability};
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::input::Input;
use gpui_kit::component::{ActiveTheme, Sizable, h_flex};
use gpui_kit::{Context, Div, ParentElement, Styled, Window, div};
use std::time::{Duration, Instant};

/// How often Settings reads again whether Apple's Model can run.
const AVAILABILITY_INTERVAL: Duration = Duration::from_secs(1);

impl NativeView {
    /// Follows Apple Intelligence being turned on or off while Crumble runs: Apple's row of the
    /// Summary Model select, and the backfill once its Model can run. Reads it at most once
    /// a second, as each read opens a file macOS keeps.
    pub(in super::super) fn notice_apple_availability(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.models.apple_read.elapsed() < AVAILABILITY_INTERVAL {
            return;
        }
        self.models.apple_read = Instant::now();
        let apple = apple_availability();
        if apple != self.models.apple {
            self.models.apple = apple;
            let selected = self.summary_provider();
            // Apple's row may have been shown as picked only because it could run.
            let selected = index_of(&Provider::SUMMARIZERS, selected);
            (self.models.provider).update(cx, |select, cx| {
                select.set_items(provider_items(apple), selected, cx)
            });
            if apple == Availability::Available {
                // Its failures may be only that it could not run; backfill once out of render.
                cx.defer_in(window, |view, _, cx| {
                    view.forget_summary_failures(Provider::Apple);
                    view.summarize_unsummarized(cx);
                });
            }
        }
    }

    /// The Summary section: the Summary Model, and who gets the Transcript's text.
    pub(in super::super) fn render_summary_settings(&self) -> Div {
        groups().child(
            Group::new("settings-summary-model").heading("Model").child(
                Setting::new("Summary Model")
                    .description("Claude and remote servers receive the Transcript's text.")
                    .descriptions(
                        self.models
                            .apple
                            .reason()
                            .map(|reason| format!("Apple isn't available: {reason}")),
                    )
                    .control(self.models.provider.clone()),
            ),
        )
    }

    /// The Transcription section: the Transcription Model, its language, and transcribing every
    /// Recording without a Transcript, which nothing else does on its own.
    pub(in super::super) fn render_transcription_settings(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Div {
        let untranscribed = self.untranscribed_count();
        let provider = self.transcription_provider();
        groups()
            .child(
                Group::new("settings-transcription-model")
                    .heading("Model")
                    .child(
                        Setting::new("Transcription Model")
                            .description(
                                "Local Models keep the audio on this Mac. A remote server \
                                 receives the Recording's audio.",
                            )
                            .description(
                                button("transcription-get-models")
                                    .link()
                                    .small()
                                    .label("Get more Models")
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.open_settings(SettingsSection::LocalModels, cx)
                                    })),
                            )
                            .control(self.models.transcription.clone()),
                    )
                    .child(
                        Setting::new("Language")
                            .description("The language Recordings are spoken in.")
                            .control(self.models.language.clone()),
                    ),
            )
            .child(
                Group::new("settings-transcription-recordings")
                    .heading("Recordings")
                    .child(
                        Setting::new(match untranscribed {
                            0 => "Every Recording has a Transcript.".to_owned(),
                            1 => "1 Recording has no Transcript.".to_owned(),
                            count => format!("{count} Recordings have no Transcript."),
                        })
                        .description(format!(
                            "Recordings are transcribed when they stop. Transcribe all uses {}.",
                            self.transcriber_name(provider)
                        ))
                        .control(
                            button("transcription-transcribe-all")
                                .outline()
                                .small()
                                .label("Transcribe all")
                                .inactive(untranscribed == 0 || !self.can_transcribe(provider))
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.transcribe_untranscribed(window, cx);
                                    cx.notify();
                                })),
                        ),
                    ),
            )
    }

    /// The Providers section: Claude's and the OpenAI-compatible server's settings.
    pub(in super::super) fn render_provider_settings(&self, cx: &mut Context<Self>) -> Div {
        groups()
            .child(
                Group::new("settings-claude")
                    .heading("Claude")
                    .child(self.key_setting(Provider::Claude, cx))
                    .child(
                        Setting::new("Model")
                            .control(field(Input::new(&self.models.claude_model).small())),
                    )
                    .child(self.test_setting(Provider::Claude, cx)),
            )
            .child(
                Group::new("settings-openai")
                    .heading("OpenAI-compatible server")
                    .child(
                        Setting::new("Address")
                            .control(field(Input::new(&self.models.openai_url).small())),
                    )
                    .child(self.key_setting(Provider::OpenAiCompatible, cx))
                    .child(
                        Setting::new("Summary Model")
                            .control(field(Input::new(&self.models.openai_model).small())),
                    )
                    .child(
                        Setting::new("Transcription Model")
                            .control(field(Input::new(&self.models.transcription_model).small())),
                    )
                    .child(self.test_setting(Provider::OpenAiCompatible, cx)),
            )
    }

    /// `provider`'s key: a masked field and Save, or, once saved, a line saying so and Remove.
    fn key_setting(&self, provider: Provider, cx: &mut Context<Self>) -> Setting {
        let id = provider.id();
        let setting = Setting::new("API key");
        if self.models.has_key(provider) {
            return setting.description("Saved in Keychain").control(
                button(format!("{id}-key-remove"))
                    .outline()
                    .small()
                    .label("Remove")
                    .on_click(cx.listener(move |view, _, _, cx| view.remove_key(provider, cx))),
            );
        }
        setting.control(
            h_flex()
                .gap_2()
                .child(field(Input::new(self.key_field(provider)).small()))
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
    fn test_setting(&self, provider: Provider, cx: &mut Context<Self>) -> Setting {
        let theme = cx.theme();
        let check = self.models.checks.get(&provider);
        let status = match check {
            None => None,
            Some(Check::Running { .. }) => Some(div().child("Testing…")),
            Some(Check::Passed) => Some(div().text_color(theme.success).child("OK")),
            Some(Check::Failed(message)) => {
                Some(div().text_color(theme.danger).child(message.clone()))
            }
        };
        Setting::new("Connection")
            .description("Test sends a tiny prompt with these settings.")
            .descriptions(status)
            .control(
                button(format!("{}-test", provider.id()))
                    .outline()
                    .small()
                    .label("Test")
                    .inactive(matches!(check, Some(Check::Running { .. })))
                    .on_click(cx.listener(move |view, _, _, cx| view.test_provider(provider, cx))),
            )
    }
}
