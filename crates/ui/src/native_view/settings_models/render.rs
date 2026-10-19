//! Drawing the Transcription and Summary sections, their selects, and the Providers section:
//! each remote Provider's fields, key row, and Test.

use super::super::NativeView;
use super::super::controls::{Inactive, button};
use super::super::settings_page::SettingsSection;
use super::{Check, FIELD_WIDTH, index_of, provider_items};
use crumble_remote::Provider;
use crumble_summarize::{Availability, apple_availability};
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::input::Input;
use gpui_kit::component::select::Select;
use gpui_kit::component::{ActiveTheme, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::{Context, Div, IntoElement, ParentElement, Styled, Window, div, px};

impl NativeView {
    /// Follows Apple Intelligence being turned on or off while Crumble runs: Apple's row of the
    /// Summary Model select, and the backfill once its Model can run.
    pub(in super::super) fn notice_apple_availability(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let apple = apple_availability();
        if apple != self.models.apple {
            self.models.apple = apple;
            let selected = self.summary_provider();
            self.models.provider.update(cx, |select, cx| {
                select.set_items(provider_items(apple), window, cx);
                // Apple's row may have been shown as picked only because it could run.
                select.set_selected_index(index_of(&Provider::SUMMARIZERS, selected), window, cx);
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
    pub(in super::super) fn render_summary_settings(&self, cx: &Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
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
                self.models
                    .apple
                    .reason()
                    .map(|reason| note(format!("Apple isn't available: {reason}"), muted)),
            )
            .child(note(
                "Claude and remote servers receive the Transcript's text.".to_owned(),
                muted,
            ))
    }

    /// The Transcription section: the Transcription Model, its language, and transcribing every
    /// Recording without a Transcript, which nothing else does on its own.
    pub(in super::super) fn render_transcription_settings(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Div {
        let muted = cx.theme().muted_foreground;
        let untranscribed = self.untranscribed_count();
        let provider = self.transcription_provider();
        v_flex()
            .gap_4()
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
                    .child(
                        h_flex()
                            .gap_2()
                            .justify_between()
                            .child(
                                note(
                                    "Local Models keep the audio on this Mac. A remote server \
                                     receives the Recording's audio."
                                        .to_owned(),
                                    muted,
                                )
                                .flex_1()
                                .min_w_0(),
                            )
                            .child(
                                button("transcription-get-models")
                                    .flex_shrink_0()
                                    .link()
                                    .xsmall()
                                    .label("Get more Models")
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.open_settings(SettingsSection::LocalModels, cx)
                                    })),
                            ),
                    ),
            )
            .child(row(
                "Language",
                Select::new(&self.models.language)
                    .small()
                    .menu_width(px(FIELD_WIDTH))
                    .accessibility_label("Language"),
            ))
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .justify_between()
                            .gap_4()
                            .text_sm()
                            .child(match untranscribed {
                                0 => "Every Recording has a Transcript.".to_owned(),
                                1 => "1 Recording has no Transcript.".to_owned(),
                                count => format!("{count} Recordings have no Transcript."),
                            })
                            .child(
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
                    )
                    .child(note(
                        format!(
                            "Recordings are transcribed when they stop. Transcribe all uses {}.",
                            self.transcriber_name(provider)
                        ),
                        muted,
                    )),
            )
    }

    /// The Providers section: Claude's and the OpenAI-compatible server's settings.
    pub(in super::super) fn render_provider_settings(&self, cx: &mut Context<Self>) -> Div {
        v_flex()
            .gap_4()
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
