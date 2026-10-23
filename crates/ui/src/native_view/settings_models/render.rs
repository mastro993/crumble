//! Drawing the Transcription, Summary and Intelligence sections: their Provider and Model
//! selects, who gets what, and the links to more Models and to Settings › Providers.

use super::super::NativeView;
use super::super::controls::{Inactive, button};
use super::super::settings_page::SettingsSection;
use super::super::settings_page::form::{Group, Setting, groups};
use crumble_remote::{Job, Provider};
use crumble_summarize::{Availability, apple_availability};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Sizable, h_flex};
use gpui_kit::{Context, Div, ParentElement, Styled, Window, prelude::FluentBuilder};
use std::time::{Duration, Instant};

/// How often Settings reads again whether Apple's Model can run.
const AVAILABILITY_INTERVAL: Duration = Duration::from_secs(1);

impl NativeView {
    /// Follows Apple Intelligence being turned on or off while Crumble runs: Apple's row of the
    /// Summary and Intelligence Provider selects, and the backfill once its Model can run. Reads it
    /// at most once a second, as each read opens a file macOS keeps.
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
            // Apple's row may have been shown as picked only because it could run.
            self.refresh_model_selects(cx);
            if apple == Availability::Available {
                // Its failures may be only that it could not run; backfill once out of render.
                cx.defer_in(window, |view, _, cx| {
                    view.forget_summary_failures(Provider::Apple);
                    view.summarize_unsummarized(cx);
                });
            }
        }
    }

    /// The Summary section: the Summary Model's Provider and Model, and who gets the
    /// Transcript's text.
    pub(in super::super) fn render_summary_settings(&mut self, cx: &mut Context<Self>) -> Div {
        self.list_picked_models(Job::Summary, cx);
        groups().child(
            Group::new("settings-summary-model")
                .heading("Model")
                .child(
                    Setting::new("Provider")
                        .description("Remote Providers receive the Transcript's text.")
                        .descriptions(self.apple_unavailable())
                        .description(add_provider("summary-add-provider", cx))
                        .control(self.models.summary.provider.clone()),
                )
                .children(self.model_row(Job::Summary)),
        )
    }

    /// The Intelligence section: the Provider and Model Crumble's Chat talks to, and who gets
    /// what is asked.
    pub(in super::super) fn render_intelligence_settings(&mut self, cx: &mut Context<Self>) -> Div {
        self.list_picked_models(Job::Intelligence, cx);
        groups().child(
            Group::new("settings-intelligence-model")
                .heading("Model")
                .child(
                    Setting::new("Provider")
                        .description(
                            "The Model Crumble's Chat talks to. Remote Providers receive what \
                             you ask.",
                        )
                        .descriptions(self.apple_unavailable())
                        .description(add_provider("intelligence-add-provider", cx))
                        .control(self.models.intelligence.provider.clone()),
                )
                .children(self.model_row(Job::Intelligence)),
        )
    }

    /// Lists, once a run, the Models of a remote Provider picked for `job`.
    fn list_picked_models(&mut self, job: Job, cx: &mut Context<Self>) {
        if let Some(provider) = self.picked_provider(job) {
            self.list_models(provider, cx);
        }
    }

    /// The Model row under `job`'s Provider select, while a remote Provider or Local Models are
    /// picked in it, saying why the Provider's Models couldn't be listed when they couldn't.
    fn model_row(&self, job: Job) -> Option<Setting> {
        let provider = self.picked_provider(job)?;
        let row = Setting::new("Model");
        if provider == Provider::Local {
            let none = self.local_models.downloaded().is_empty();
            return Some(
                row.when(none, |row| row.description("No Local Model is downloaded."))
                    .control(self.models.local_model.clone()),
            );
        }
        provider.remote()?;
        let failure =
            (self.listing_failure(provider)).map(|why| format!("Couldn't list its Models. {why}"));
        Some(
            row.descriptions(failure)
                .control(self.models.job(job).model.clone()),
        )
    }

    /// Why Apple's Model can't run, when it can't.
    fn apple_unavailable(&self) -> Option<String> {
        (self.models.apple.reason()).map(|reason| format!("Apple isn't available: {reason}"))
    }

    /// The Transcription section: the Transcription Model, its language, and transcribing every
    /// Recording without a Transcript, which nothing else does on its own.
    pub(in super::super) fn render_transcription_settings(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Div {
        self.list_picked_models(Job::Transcription, cx);
        let untranscribed = self.untranscribed_count();
        let provider = self.transcription_provider();
        groups()
            .child(
                Group::new("settings-transcription-model")
                    .heading("Model")
                    .child(
                        Setting::new("Provider")
                            .description(
                                "Local Models keep the audio on this Mac. Remote Providers \
                                 receive the Recording's audio.",
                            )
                            .description(
                                h_flex()
                                    .gap_3()
                                    .flex_wrap()
                                    .child(
                                        link("transcription-get-models", "Get more Models")
                                            .on_click(cx.listener(|view, _, _, cx| {
                                                view.open_settings(SettingsSection::LocalModels, cx)
                                            })),
                                    )
                                    .child(add_provider("transcription-add-provider", cx)),
                            )
                            .control(self.models.transcription.provider.clone()),
                    )
                    .children(self.model_row(Job::Transcription))
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
                                .inactive(
                                    untranscribed == 0
                                        || !self.ready_for(provider, Job::Transcription),
                                )
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.transcribe_untranscribed(window, cx);
                                    cx.notify();
                                })),
                        ),
                    ),
            )
    }
}

/// A small link button labeled `label`.
fn link(id: &'static str, label: &'static str) -> Button {
    button(id).link().small().label(label)
}

/// "Add a Provider", which opens Settings › Providers.
fn add_provider(id: &'static str, cx: &mut Context<NativeView>) -> Button {
    link(id, "Add a Provider")
        .on_click(cx.listener(|view, _, _, cx| view.open_settings(SettingsSection::Providers, cx)))
}
