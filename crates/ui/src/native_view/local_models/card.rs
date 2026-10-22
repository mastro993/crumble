//! One Model's row in the Local Models section: its name, starred when Handy recommends it,
//! with its badge at the top right,
//! what it is, then its accuracy and speed and its details, with its size and buttons at the
//! bottom right. The rows of a group share one card, each drawing its part of it.

use super::super::NativeView;
use super::super::controls::{app_icon, button};
use super::super::keyboard::focusable;
use crumble_local_models::{Model, size_label};
use crumble_remote::Provider;
use crumble_theme::SURFACE_RADIUS;
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Div, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement,
    ParentElement, SharedString, Stateful, StatefulInteractiveElement, Styled, div,
    prelude::FluentBuilder, px, relative,
};

/// Width of Use and Download, which a download's progress keeps.
const BUTTON_WIDTH: f32 = 84.0;
/// Height of a small button.
const BUTTON_HEIGHT: f32 = 24.0;
/// Space above and below a row's content.
const ROW_PADDING: f32 = 13.0;
/// Height of the line between a row's scores and its details.
const DIVIDER_HEIGHT: f32 = 12.0;
/// Size of the star after a recommended Model's name.
const STAR_SIZE: f32 = 12.0;
/// Diameter of a score's dot.
const DOT_SIZE: f32 = 5.0;
/// How many dots a score has.
const DOTS: u8 = 5;

/// A Model the section lists.
#[derive(Clone, PartialEq)]
pub(super) enum Card {
    /// Downloaded, and whether the list still offers it.
    Downloaded(Box<Model>, bool),
    Available(&'static Model),
}

/// Where a row sits in its group's card: the first draws its top, the last its bottom.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct Place {
    pub(super) first: bool,
    pub(super) last: bool,
}

/// A row's parts, before it is drawn.
struct Parts {
    /// Tells the row's elements apart from other rows'.
    key: SharedString,
    name: SharedString,
    /// Whether a star after the name says Handy recommends it.
    recommended: bool,
    badge: Option<Tag>,
    details: Vec<String>,
    description: SharedString,
    failure: Option<String>,
    scores: Option<(u8, u8)>,
    actions: Vec<AnyElement>,
}

impl NativeView {
    /// The row of a Local Model.
    pub(super) fn render_card(
        &self,
        card: &Card,
        place: Place,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (model, downloaded, offered) = match card {
            Card::Downloaded(model, offered) => (&**model, true, *offered),
            Card::Available(model) => (*model, false, true),
        };
        let in_use = downloaded
            && self.transcription_provider() == Provider::Local
            && self.store.local_model() == Some(model.slug.as_str());
        let parts = Parts {
            key: model.slug.clone().into(),
            name: model.name.clone().into(),
            recommended: model.recommended.is_some() && !downloaded,
            badge: (!offered).then(|| Tag::warning().small().child("No longer offered")),
            details: details(model),
            description: model.description.clone().into(),
            failure: self.local_models.failures.get(&model.slug).cloned(),
            scores: Some((model.accuracy, model.speed)),
            actions: self.card_actions(model, downloaded, in_use, cx),
        };
        row(parts, place, cx)
    }

    /// The row of Apple's built-in Model, which needs no download and cannot be deleted.
    pub(super) fn render_apple_card(&self, place: Place, cx: &mut Context<Self>) -> AnyElement {
        let in_use = self.transcription_provider() == Provider::Apple;
        let action = if in_use {
            in_use_tag(cx)
        } else {
            button("local-model-use-apple")
                .outline()
                .small()
                .w(px(BUTTON_WIDTH))
                .label("Use")
                .on_click(cx.listener(|view, _, _, cx| view.use_apple_model(cx)))
                .into_any_element()
        };
        let parts = Parts {
            key: "apple".into(),
            name: "Apple SpeechAnalyzer".into(),
            recommended: false,
            badge: None,
            details: vec!["Built into macOS".to_owned()],
            description: "Built into macOS, which downloads each language's model when first \
                          needed."
                .into(),
            failure: None,
            scores: None,
            actions: vec![action],
        };
        row(parts, place, cx)
    }

    /// The Model's size, then Download, which shows the download's progress once pressed, or
    /// Delete and Use, or In use in Use's place.
    fn card_actions(
        &self,
        model: &Model,
        downloaded: bool,
        in_use: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let slug = model.slug.clone();
        let id = |action: &str| ElementId::Name(format!("local-model-{action}-{slug}").into());
        let size = div()
            .text_xs()
            .text_color(crumble_theme::faint_foreground(cx.theme()))
            .child(size_label(model.file.size))
            .into_any_element();
        if let Some(download) = self.local_models.downloads.get(&slug) {
            let fraction = download.held as f32 / download.total.max(1) as f32;
            let slug = slug.clone();
            return vec![
                size,
                progress(id("progress"), &model.name, fraction, cx)
                    .on_click(cx.listener(move |view, _, _, cx| view.cancel_download(&slug, cx)))
                    .into_any_element(),
            ];
        }
        if !downloaded {
            let model = model.clone();
            return vec![
                size,
                button(id("download"))
                    .primary()
                    .small()
                    .w(px(BUTTON_WIDTH))
                    .label("Download")
                    .accessibility_label(format!("Download {}", model.name))
                    .on_click(
                        cx.listener(move |view, _, _, cx| view.download_local_model(&model, cx)),
                    )
                    .into_any_element(),
            ];
        }
        let delete = {
            let model = model.clone();
            button(id("delete"))
                .ghost()
                .small()
                .icon(app_icon("delete").size_4())
                .tooltip(format!("Delete {}", model.name))
                .accessibility_label(format!("Delete {}", model.name))
                .on_click(cx.listener(move |view, _, window, cx| {
                    view.confirm_delete_local_model(&model, window, cx)
                }))
                .into_any_element()
        };
        let use_button = if in_use {
            in_use_tag(cx)
        } else {
            button(id("use"))
                .outline()
                .small()
                .w(px(BUTTON_WIDTH))
                .label("Use")
                .on_click(cx.listener(move |view, _, _, cx| view.use_local_model(&slug, cx)))
                .into_any_element()
        };
        vec![size, delete, use_button]
    }
}

/// In use, in green with a check, where Use would be and as wide: solid green in light mode,
/// a green tint in dark mode.
fn in_use_tag(cx: &App) -> AnyElement {
    let theme = cx.theme();
    let green = theme.success;
    let tag = if theme.is_dark() {
        Tag::custom(green.opacity(0.12), green, green.opacity(0.))
    } else {
        Tag::custom(green, gpui_kit::white(), green)
    };
    tag.w(px(BUTTON_WIDTH))
        .h(px(BUTTON_HEIGHT))
        .justify_center()
        .gap_1()
        .rounded(cx.theme().radius)
        .child(app_icon("check").size_3p5())
        .child("In use")
        .into_any_element()
}

/// What `model` can do: its languages, streaming, whether its times are rough.
fn details(model: &Model) -> Vec<String> {
    let languages = match model.languages.as_slice() {
        [only] => format!("{} only", crumble_transcribe::language_name(only)),
        all => format!("{} languages", all.len()),
    };
    let mut details = vec![languages];
    if model.streaming {
        details.push("Streaming".to_owned());
    }
    if model.rough_timestamps {
        details.push("Rough timestamps".to_owned());
    }
    details
}

/// A download's progress in place of Download, as wide: the button's fill grows from the left
/// and the percentage reads across it. Under the pointer it offers to cancel instead.
fn progress(id: ElementId, name: &str, fraction: f32, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let (fill, ink, track) = (
        theme.primary,
        theme.primary_foreground,
        theme.sidebar_accent,
    );
    let percent: SharedString = format!("{:.0}%", fraction * 100.0).into();
    let group: SharedString = format!("{id}").into();
    // Pinned to the button's left edge at its full width, so the fill clips it in place.
    let label = |text: SharedString, color: Hsla| {
        div()
            .absolute()
            .top_0()
            .left_0()
            .w(px(BUTTON_WIDTH))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(color)
            .child(text)
    };
    focusable(
        div().id(id),
        format!("Cancel downloading {name}, {percent} done"),
    )
    .group(group.clone())
    .relative()
    .flex_shrink_0()
    .w(px(BUTTON_WIDTH))
    .h(px(BUTTON_HEIGHT))
    .rounded(theme.radius)
    .overflow_hidden()
    .bg(track)
    .text_xs()
    .cursor_pointer()
    .child(
        // The percentage in the fill's color over the track, and in the button ink over the
        // fill.
        div()
            .absolute()
            .size_full()
            .group_hover(group.clone(), |style| style.invisible())
            .child(label(percent.clone(), fill))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .h_full()
                    .w(relative(fraction.clamp(0., 1.)))
                    .overflow_hidden()
                    // A clip is square, so the fill rounds the button's corners itself.
                    .rounded_l(theme.radius)
                    .bg(fill)
                    .child(label(percent, ink)),
            ),
    )
    .child(
        label("Cancel".into(), fill)
            .invisible()
            .group_hover(group, |style| style.visible()),
    )
}

/// A row in its group's card: the card's edge at its sides, its top and corners on the first
/// row and its bottom on the last, and a line above every row but the first.
fn row(parts: Parts, place: Place, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let (muted, faint) = (
        theme.muted_foreground,
        crumble_theme::faint_foreground(theme),
    );
    let radius = px(SURFACE_RADIUS);
    let key = parts.key;
    let top = h_flex()
        .gap_2p5()
        .child(
            h_flex()
                .min_w_0()
                .gap_1()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(parts.name),
                )
                .when(parts.recommended, |name| {
                    name.child(
                        div()
                            .id(ElementId::Name(
                                format!("local-model-recommended-{key}").into(),
                            ))
                            .flex_shrink_0()
                            .aria_label("Recommended")
                            .tooltip(|window, cx| Tooltip::new("Recommended").build(window, cx))
                            .child(app_icon("star").size(px(STAR_SIZE)).text_color(muted)),
                    )
                }),
        )
        .children(parts.badge.map(|badge| badge.ml_auto().flex_shrink_0()));
    let scored = parts.scores.is_some();
    let bottom = h_flex()
        .gap_4()
        .mt_1p5()
        .text_xs()
        .text_color(faint)
        .children(parts.scores.map(|(accuracy, speed)| {
            h_flex()
                .gap_4()
                .child(score(&key, "Accuracy", accuracy, cx))
                .child(score(&key, "Speed", speed, cx))
        }))
        .when(scored, |bottom| {
            bottom.child(
                div()
                    .flex_shrink_0()
                    .w(px(1.))
                    .h(px(DIVIDER_HEIGHT))
                    .bg(crumble_theme::divider(theme)),
            )
        })
        .child(
            div()
                .min_w_0()
                .truncate()
                .child(parts.details.join("  ·  ")),
        )
        .child(
            h_flex()
                .ml_auto()
                .flex_shrink_0()
                .gap_1p5()
                .children(parts.actions),
        );
    div()
        .px_4()
        .bg(theme.group_box)
        .border_color(theme.input)
        .border_l_1()
        .border_r_1()
        .when(place.first, |card| card.border_t_1().rounded_t(radius))
        .when(place.last, |card| card.border_b_1().rounded_b(radius))
        .child(
            v_flex()
                .py(px(ROW_PADDING))
                .when(!place.first, |row| {
                    row.border_t_1().border_color(crumble_theme::divider(theme))
                })
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(top)
                        .child(
                            div()
                                .mt_0p5()
                                .text_sm()
                                .text_color(muted)
                                .child(parts.description),
                        )
                        .children(parts.failure.map(|failure| {
                            div()
                                .mt_0p5()
                                .text_xs()
                                .text_color(theme.danger)
                                .child(failure)
                        }))
                        .child(bottom),
                ),
        )
        .into_any_element()
}

/// Rounds a score from 0 to 100 to how many of the five dots are filled.
fn filled_dots(score: u8) -> u8 {
    ((u16::from(score.min(100)) * u16::from(DOTS) + 50) / 100) as u8
}

/// A score as its label and five dots.
fn score(key: &str, label: &'static str, score: u8, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let (on, off) = (theme.primary, theme.foreground.opacity(0.1));
    let filled = filled_dots(score);
    h_flex().gap_1p5().child(label).child(
        h_flex()
            .id(ElementId::Name(format!("local-model-{label}-{key}").into()))
            .gap(px(2.))
            .aria_label(format!("{label} {score} of 100"))
            .children((0..DOTS).map(|dot| {
                div()
                    .size(px(DOT_SIZE))
                    .rounded_full()
                    .bg(if dot < filled { on } else { off })
            })),
    )
}

#[cfg(test)]
mod tests {
    use super::filled_dots;

    #[test]
    fn a_score_rounds_to_the_nearest_dot() {
        assert_eq!(filled_dots(0), 0);
        assert_eq!(filled_dots(9), 0);
        assert_eq!(filled_dots(10), 1);
        assert_eq!(filled_dots(50), 3);
        assert_eq!(filled_dots(72), 4);
        assert_eq!(filled_dots(100), 5);
        assert_eq!(filled_dots(255), 5);
    }
}
