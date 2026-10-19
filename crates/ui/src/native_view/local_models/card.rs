//! One Model's card in the Local Models section: its name and badge, what it is, its accuracy
//! and speed, what it can do, its size, and what can be done with it.

use super::super::NativeView;
use super::super::controls::{app_icon, button};
use crumble_local_models::{Model, size_label};
use crumble_remote::Provider;
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::progress::Progress;
use gpui_kit::component::separator::Separator;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, ElementId, Hsla, IntoElement, ParentElement, SharedString, Styled, div, px,
};

/// Width of an accuracy or speed bar.
const BAR_WIDTH: f32 = 72.0;
/// Width of a download's progress bar.
const PROGRESS_WIDTH: f32 = 120.0;

/// A Model the section lists.
pub(super) enum Card {
    /// Downloaded, and whether the list still offers it.
    Downloaded(Model, bool),
    Available(Model),
}

impl NativeView {
    /// The card of a Local Model.
    pub(super) fn render_card(&self, card: Card, cx: &mut Context<Self>) -> AnyElement {
        let (model, downloaded, offered) = match card {
            Card::Downloaded(model, offered) => (model, true, offered),
            Card::Available(model) => (model, false, true),
        };
        let in_use = downloaded
            && self.transcription_provider() == Provider::Local
            && self.store.local_model() == Some(model.slug.as_str());
        let badge = if in_use {
            Some(Tag::primary().small().child("In use"))
        } else if !offered {
            Some(Tag::warning().small().child("No longer offered"))
        } else if model.recommended.is_some() && !downloaded {
            Some(Tag::secondary().small().child("Recommended"))
        } else {
            None
        };
        let facts = facts(&model);
        let actions = self.card_actions(&model, downloaded, in_use, cx);
        frame(
            &model.slug,
            &model.name,
            badge,
            &model.description,
            Some((model.accuracy, model.speed)),
            facts,
            actions,
            in_use,
            cx,
        )
    }

    /// The card of Apple's built-in Model, which needs no download and cannot be deleted.
    pub(super) fn render_apple_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let in_use = self.transcription_provider() == Provider::Apple;
        let action = if in_use {
            div().into_any_element()
        } else {
            button("local-model-use-apple")
                .outline()
                .small()
                .label("Use")
                .on_click(cx.listener(|view, _, window, cx| view.use_apple_model(window, cx)))
                .into_any_element()
        };
        frame(
            "apple",
            "Apple SpeechAnalyzer",
            in_use.then(|| Tag::primary().small().child("In use")),
            "Built into macOS, which downloads each language's model when first needed.",
            None,
            vec![fact("check", "Built into macOS".into())],
            vec![action],
            in_use,
            cx,
        )
    }

    /// Download, its progress and Cancel, or Use and Delete.
    fn card_actions(
        &self,
        model: &Model,
        downloaded: bool,
        in_use: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let theme = cx.theme();
        let (muted, danger) = (theme.muted_foreground, theme.danger);
        let slug = model.slug.clone();
        let id = |action: &str| ElementId::Name(format!("local-model-{action}-{slug}").into());
        let size = size_label(model.file.size);
        if let Some(download) = self.local_models.downloads.get(&slug) {
            let fraction = download.held as f32 / download.total.max(1) as f32;
            let slug = slug.clone();
            return vec![
                Progress::new(id("progress"))
                    .small()
                    .w(px(PROGRESS_WIDTH))
                    .value(fraction * 100.0)
                    .accessibility_label(format!("Downloading {}", model.name))
                    .into_any_element(),
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(format!("{:.0}%", fraction * 100.0))
                    .into_any_element(),
                button(id("cancel"))
                    .ghost()
                    .small()
                    .label("Cancel")
                    .on_click(cx.listener(move |view, _, _, cx| view.cancel_download(&slug, cx)))
                    .into_any_element(),
            ];
        }
        let mut actions = Vec::new();
        if let Some(message) = self.local_models.failures.get(&slug) {
            actions.push(
                div()
                    .text_xs()
                    .text_color(danger)
                    .child(message.clone())
                    .into_any_element(),
            );
        }
        if !downloaded {
            let model = model.clone();
            actions.push(
                button(id("download"))
                    .outline()
                    .small()
                    .icon(app_icon("download").size_4())
                    .label(size)
                    .tooltip(format!("Download {}", model.name))
                    .accessibility_label(format!("Download {}", model.name))
                    .on_click(
                        cx.listener(move |view, _, _, cx| view.download_local_model(&model, cx)),
                    )
                    .into_any_element(),
            );
            return actions;
        }
        actions.push(
            h_flex()
                .gap_1()
                .text_xs()
                .text_color(muted)
                .child(app_icon("hard-drive").size_4())
                .child(size)
                .into_any_element(),
        );
        if !in_use {
            let slug = slug.clone();
            actions.push(
                button(id("use"))
                    .outline()
                    .small()
                    .label("Use")
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.use_local_model(&slug, window, cx)
                    }))
                    .into_any_element(),
            );
        }
        let model = model.clone();
        actions.push(
            button(id("delete"))
                .ghost()
                .small()
                .icon(app_icon("delete").size_4())
                .tooltip(format!("Delete {}", model.name))
                .accessibility_label(format!("Delete {}", model.name))
                .on_click(cx.listener(move |view, _, window, cx| {
                    view.confirm_delete_local_model(&model, window, cx)
                }))
                .into_any_element(),
        );
        actions
    }
}

/// What `model` can do: its languages, streaming, whether its times are rough, its license.
fn facts(model: &Model) -> Vec<AnyElement> {
    let languages = match model.languages.as_slice() {
        [only] => format!("{} only", crumble_transcribe::language_name(only)),
        all => format!("{} languages", all.len()),
    };
    let mut facts = vec![fact("globe", languages.into())];
    if model.streaming {
        facts.push(fact("audio-wave", "Streaming".into()));
    }
    if model.rough_timestamps {
        facts.push(fact("clock", "Rough timestamps".into()));
    }
    facts.push(fact("key", model.license.to_uppercase().into()));
    facts
}

/// One thing a Model can do, with its icon.
fn fact(icon: &str, text: SharedString) -> AnyElement {
    h_flex()
        .gap_1()
        .child(app_icon(icon).size_4())
        .child(text)
        .into_any_element()
}

/// A card: the name, badge and description beside the accuracy and speed bars, then a line
/// of facts and the actions. The Model in use is outlined in the accent color.
#[allow(
    clippy::too_many_arguments,
    reason = "one card's parts, each drawn once"
)]
fn frame(
    key: &str,
    name: &str,
    badge: Option<Tag>,
    description: &str,
    scores: Option<(u8, u8)>,
    facts: Vec<AnyElement>,
    actions: Vec<AnyElement>,
    in_use: bool,
    cx: &Context<NativeView>,
) -> AnyElement {
    let theme = cx.theme();
    let (muted, border, accent) = (theme.muted_foreground, theme.border, theme.primary);
    v_flex()
        .gap_3()
        .p_4()
        .rounded_lg()
        .border_1()
        .border_color(if in_use { accent } else { border })
        .child(
            h_flex()
                .gap_4()
                .items_start()
                .justify_between()
                .child(
                    v_flex()
                        .gap_1()
                        .min_w_0()
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                        .child(name.to_owned()),
                                )
                                .children(badge),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(muted)
                                .child(description.to_owned()),
                        ),
                )
                .children(scores.map(|(accuracy, speed)| {
                    v_flex()
                        .gap_1()
                        .flex_shrink_0()
                        .child(bar(key, "Accuracy", accuracy, muted))
                        .child(bar(key, "Speed", speed, muted))
                })),
        )
        .child(Separator::horizontal())
        .child(
            h_flex()
                .gap_3()
                .justify_between()
                .child(
                    h_flex()
                        .gap_3()
                        .flex_wrap()
                        .min_w_0()
                        .text_xs()
                        .text_color(muted)
                        .children(facts),
                )
                .child(h_flex().gap_2().flex_shrink_0().children(actions)),
        )
        .into_any_element()
}

/// A score from 0 to 100 as a labelled bar.
fn bar(key: &str, label: &'static str, score: u8, muted: Hsla) -> impl IntoElement {
    h_flex()
        .gap_2()
        .justify_end()
        .text_xs()
        .text_color(muted)
        .child(label)
        .child(
            Progress::new(ElementId::Name(format!("local-model-{label}-{key}").into()))
                .small()
                .w(px(BAR_WIDTH))
                .value(f32::from(score))
                .accessibility_label(format!("{label} {score} of 100")),
        )
}
