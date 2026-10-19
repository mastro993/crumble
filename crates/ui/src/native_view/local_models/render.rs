//! The Local Models section: a search field and filters over two groups, the downloaded Models
//! with Apple's built-in one first, then those available to download.

use super::super::NativeView;
use super::super::controls::{app_icon, button};
use super::card::Card;
use super::{LanguageFilter, language_filters};
use crumble_local_models::{Filter, Model, catalog};
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::input::Input;
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{ActiveTheme, IndexPath, Selectable, Sizable, h_flex, v_flex};
use gpui_kit::{AppContext, Context, Div, ParentElement, Styled, Window, div, prelude::*, px};

/// Where Handy's model list comes from, credited on the page.
const HANDY: &str = "https://github.com/cjpais/Handy";
/// Width of the language filter.
const LANGUAGE_WIDTH: f32 = 180.0;
/// The name Apple's Model is searched by.
const APPLE_NAME: &str = "Apple SpeechAnalyzer";

impl NativeView {
    /// The Local Models section.
    pub(in super::super) fn render_local_model_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let language = self.language_filter(window, cx);
        let models = &self.local_models;
        let query = models.search.read(cx).value().to_string();
        let code = language.read(cx).selected_value().cloned().flatten();
        let filter = Filter {
            query: &query,
            streaming: models.streaming,
            language: code.as_deref(),
        };
        let muted = cx.theme().muted_foreground;

        let apple = self
            .apple_matches(&filter)
            .then(|| self.render_apple_card(cx));
        let downloaded: Vec<_> = (models.downloaded.iter())
            .filter(|downloaded| downloaded.model.matches(&filter))
            .map(|downloaded| Card::Downloaded(downloaded.model.clone(), downloaded.offered))
            .collect();
        let mut available: Vec<&Model> = catalog()
            .iter()
            .filter(|model| {
                model.matches(&filter)
                    && !(models.downloaded.iter()).any(|d| d.model.slug == model.slug)
            })
            .collect();
        // Handy's recommended Models first, in its order; the rest stay in the list's order.
        available.sort_by_key(|model| model.recommended.unwrap_or(u32::MAX));
        let available: Vec<_> = available
            .into_iter()
            .map(|model| Card::Available(model.clone()))
            .collect();
        let nothing = apple.is_none() && downloaded.is_empty() && available.is_empty();

        v_flex()
            .gap_4()
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Download Models that transcribe on this Mac. Each has its own accuracy, speed and languages."),
            )
            .child(
                Input::new(&models.search)
                    .small()
                    .cleanable(true)
                    .prefix(app_icon("search").size_4().text_color(muted)),
            )
            .child(
                h_flex()
                    .gap_2()
                    .justify_between()
                    .child(group_label("Downloaded", cx))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                button("local-models-streaming")
                                    .ghost()
                                    .small()
                                    .icon(app_icon("audio-wave").size_4())
                                    .selected(models.streaming)
                                    .tooltip("Only Models that can stream")
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.local_models.streaming = !view.local_models.streaming;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div().w(px(LANGUAGE_WIDTH)).child(
                                    Select::new(&language)
                                        .small()
                                        .icon(app_icon("globe").size_4())
                                        .menu_width(px(LANGUAGE_WIDTH))
                                        .accessibility_label("Language"),
                                ),
                            ),
                    ),
            )
            .children(apple)
            .children(downloaded.into_iter().map(|card| self.render_card(card, cx)))
            .child(group_label("Available to download", cx))
            .children(available.into_iter().map(|card| self.render_card(card, cx)))
            .when(nothing, |page| {
                page.child(div().text_sm().text_color(muted).child("No Model matches."))
            })
            .child(
                h_flex()
                    .gap_1()
                    .text_xs()
                    .text_color(muted)
                    .child("Model list and scores from")
                    .child(
                        button("local-models-handy")
                            .link()
                            .xsmall()
                            .label("Handy")
                            .on_click(|_, _, cx| cx.open_url(HANDY)),
                    )
                    .child("· Speed measured on Handy's reference laptop."),
            )
    }

    /// The language filter, made the first time the section shows, as naming every language
    /// takes a moment.
    fn language_filter(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Entity<SelectState<Vec<LanguageFilter>>> {
        if let Some(select) = &self.local_models.language {
            return select.clone();
        }
        let select = cx.new(|cx| {
            SelectState::new(language_filters(), Some(IndexPath::new(0)), window, cx)
                .searchable(true)
        });
        let subscription = cx.subscribe_in(
            &select,
            window,
            |_, _, _: &SelectEvent<Vec<LanguageFilter>>, _, cx| cx.notify(),
        );
        self.local_models._subscriptions.push(subscription);
        self.local_models.language = Some(select.clone());
        select
    }

    /// Whether Apple's built-in Model shows under `filter`: it does not stream, and it
    /// transcribes the languages of SpeechAnalyzer's locales.
    fn apple_matches(&self, filter: &Filter) -> bool {
        let name = APPLE_NAME.to_lowercase();
        let query = filter.query.trim().to_lowercase();
        query.split_whitespace().all(|word| name.contains(word))
            && !filter.streaming
            && filter
                .language
                .is_none_or(|language| self.models.apple_languages().any(|code| code == language))
    }
}

/// A group's heading.
fn group_label(text: &'static str, cx: &Context<NativeView>) -> Div {
    div()
        .text_sm()
        .font_weight(gpui_kit::FontWeight::MEDIUM)
        .text_color(cx.theme().muted_foreground)
        .child(text)
}
