//! The Local Models section: a search field and filters over two groups, the downloaded
//! Models with Apple's built-in one first, then those available to download. All of it is one
//! list, which draws only the rows in view.

use super::super::NativeView;
use super::super::controls::{app_icon, button};
use super::super::list_rows::splice;
use super::super::select::{Picked, Select};
use super::super::settings_page::form::{
    GROUP_GAP, HEADING_GAP, PAGE_PADDING, TITLE_GAP, description, heading, search_field,
};
use super::super::settings_page::{SettingsSection, centered, title};
use super::card::Card;
use super::{LanguageFilter, Row, language_filters};
use crumble_local_models::{Filter, Model, catalog};
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{ActiveTheme, Selectable, Sizable, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AppContext, Context, Entity, ParentElement, Styled, div, list, prelude::*, px,
};
use std::rc::Rc;

/// Where Handy's model list comes from, credited on the page.
const HANDY: &str = "https://github.com/cjpais/Handy";
/// The name Apple's Model is searched by.
const APPLE_NAME: &str = "Apple SpeechAnalyzer";

impl NativeView {
    /// The Local Models section beside the Settings sidebar: one list of its title, search
    /// field and filters, and the cards, scrolling together.
    pub(in super::super) fn render_local_model_settings(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let language = self.language_filter(cx);
        let rows = self.local_model_rows(&language, cx);
        let models = &mut self.local_models;
        if rows != *models.rows {
            let spliced = splice(&models.list, &models.rows, &rows);
            // Measures the new rows too, so the scrollbar spans every row.
            models.list.remeasure_items(spliced);
            models.rows = Rc::new(rows);
        }
        let rows = models.rows.clone();
        let list = list(
            models.list.clone(),
            cx.processor(move |view, index, _, cx| {
                let Some(row) = rows.get(index) else {
                    return div().into_any_element();
                };
                let row = centered(view.render_local_model_row(row, cx));
                // Rows sit a heading's gap apart, and the last its padding from the bottom.
                if index + 1 == rows.len() {
                    row.pb(px(PAGE_PADDING))
                } else {
                    row.pb(px(HEADING_GAP))
                }
                .into_any_element()
            }),
        )
        .size_full();
        div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(list)
            .vertical_scrollbar(&models.list)
            .into_any_element()
    }

    /// The section's title, its description, and the search field and filters.
    fn render_local_model_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let models = &self.local_models;
        let filters = v_flex()
            .gap_4()
            .child(description(
                "Download Models that transcribe on this Mac. Each has its own accuracy, speed \
                 and languages.",
                cx,
            ))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(search_field(&models.search, cx)),
                    )
                    .child(
                        button("local-models-streaming")
                            .ghost()
                            .small()
                            .icon(app_icon("audio-wave").size_4())
                            .selected(models.streaming)
                            .tooltip("Only Models that can stream")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.local_models.streaming = !view.local_models.streaming;
                                view.local_models.filtered(cx);
                            })),
                    )
                    .children(models.language.clone()),
            );
        v_flex()
            .gap(px(TITLE_GAP))
            .pt(px(PAGE_PADDING))
            .child(title(SettingsSection::LocalModels.title()))
            .child(filters)
            .into_any_element()
    }

    /// The rows the search field and filters leave: the downloaded Models with Apple's first,
    /// then the others, Handy's recommended ones first.
    fn local_model_rows(
        &self,
        language: &Entity<Select<LanguageFilter>>,
        cx: &Context<Self>,
    ) -> Vec<Row> {
        let models = &self.local_models;
        let query = models.search.read(cx).value().to_string();
        let code = language.read(cx).selected_value().cloned().flatten();
        let filter = Filter {
            query: &query,
            streaming: models.streaming,
            language: code.as_deref(),
        };
        let apple = self.apple_matches(&filter);
        let mut rows = vec![Row::Header, Row::Label("Downloaded")];
        rows.extend(apple.then_some(Row::Apple));
        rows.extend(
            (models.downloaded.iter())
                .filter(|downloaded| downloaded.model.matches(&filter))
                .map(|downloaded| {
                    Row::Card(Card::Downloaded(
                        Box::new(downloaded.model.clone()),
                        downloaded.offered,
                    ))
                }),
        );
        let mut available: Vec<&'static Model> = catalog()
            .iter()
            .filter(|model| {
                model.matches(&filter)
                    && !(models.downloaded.iter()).any(|d| d.model.slug == model.slug)
            })
            .collect();
        // Handy's recommended Models first, in its order; the rest stay in the list's order.
        available.sort_by_key(|model| model.recommended.unwrap_or(u32::MAX));
        let shown = rows.len() > 2 || !available.is_empty();
        rows.push(Row::Label("Available to download"));
        rows.extend(
            available
                .into_iter()
                .map(|model| Row::Card(Card::Available(model))),
        );
        if !shown {
            rows.push(Row::Nothing);
        }
        rows.push(Row::Credit);
        rows
    }

    fn render_local_model_row(&self, row: &Row, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        match row {
            Row::Header => self.render_local_model_header(cx),
            // Set apart from the cards above, as a group is from the one before it.
            Row::Label(text) => heading(*text)
                .pt(px(GROUP_GAP - HEADING_GAP))
                .into_any_element(),
            Row::Apple => self.render_apple_card(cx),
            Row::Card(card) => self.render_card(card, cx),
            Row::Nothing => div()
                .text_sm()
                .text_color(muted)
                .child("No Model matches.")
                .into_any_element(),
            Row::Credit => h_flex()
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
                .child("· Speed measured on Handy's reference laptop.")
                .into_any_element(),
        }
    }

    /// The language filter, made the first time the section shows, as naming every language
    /// takes a moment.
    fn language_filter(&mut self, cx: &mut Context<Self>) -> Entity<Select<LanguageFilter>> {
        if let Some(select) = &self.local_models.language {
            return select.clone();
        }
        let select = cx.new(|cx| {
            Select::new("Language", language_filters(), Some(0), cx)
                .icon("globe")
                .searchable()
        });
        let subscription = cx.subscribe(&select, |view, _, _: &Picked<Option<String>>, cx| {
            view.local_models.filtered(cx)
        });
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
