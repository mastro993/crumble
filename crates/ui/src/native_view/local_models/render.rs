//! The Local Models section: a search field and filters over two groups, the downloaded
//! Models with Apple's built-in one first, then those available to download. All of it is one
//! list, which draws only the rows in view.

use super::super::NativeView;
use super::super::controls::{app_icon, button};
use super::super::list_rows::splice;
use super::super::select::{Picked, Select};
use super::super::settings_page::form::{
    GROUP_GAP, HEADING_GAP, PAGE_PADDING, TITLE_GAP, heading, search_field,
};
use super::super::settings_page::{SettingsSection, centered, title};
use super::card::{Card, Place};
use super::{LanguageFilter, Row, language_filters};
use crumble_local_models::{Downloaded, Filter, Model, catalog};
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
                // The rows of a card touch; other rows sit a heading's gap apart, and the last
                // its padding from the bottom.
                let joined = matches!(row, Row::Apple(place) | Row::Card(_, place) if !place.last);
                let row = centered(view.render_local_model_row(row, cx));
                if index + 1 == rows.len() {
                    row.pb(px(PAGE_PADDING))
                } else if joined {
                    row
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

    /// The section's title, then the search field and filters.
    fn render_local_model_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let models = &self.local_models;
        let filters = h_flex()
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
            .children(models.language.clone())
            .child(models.sort.clone());
        v_flex()
            .gap(px(TITLE_GAP))
            .pt(px(PAGE_PADDING))
            .child(title(SettingsSection::LocalModels.title()))
            .child(filters)
            .into_any_element()
    }

    /// The rows the search field and filters leave, each group in the picked order: the
    /// downloaded Models with Apple's first, then the others.
    fn local_model_rows(
        &self,
        language: &Entity<Select<LanguageFilter>>,
        cx: &Context<Self>,
    ) -> Vec<Row> {
        let models = &self.local_models;
        let query = models.search.read(cx).value().to_string();
        let code = language.read(cx).selected_value().cloned().flatten();
        let (sort, descending) = models.sorted;
        let filter = Filter {
            query: &query,
            streaming: models.streaming,
            language: code.as_deref(),
        };
        // `None` is Apple's built-in Model.
        let mut downloaded: Vec<Option<Card>> = (self.apple_matches(&filter))
            .then_some(None)
            .into_iter()
            .collect();
        let mut matching: Vec<&Downloaded> = (models.downloaded.iter())
            .filter(|downloaded| downloaded.model.matches(&filter))
            .collect();
        sort.apply(descending, &mut matching, |downloaded| &downloaded.model);
        downloaded.extend(matching.into_iter().map(|downloaded| {
            Some(Card::Downloaded(
                Box::new(downloaded.model.clone()),
                downloaded.offered,
            ))
        }));
        let any_downloaded = !downloaded.is_empty();
        let mut rows = vec![Row::Header, Row::Label("Downloaded")];
        push_group(&mut rows, downloaded);
        let mut available: Vec<&'static Model> = catalog()
            .iter()
            .filter(|model| {
                model.matches(&filter)
                    && !(models.downloaded.iter()).any(|d| d.model.slug == model.slug)
            })
            .collect();
        sort.apply(descending, &mut available, |model| model);
        let shown = any_downloaded || !available.is_empty();
        rows.push(Row::Label("Available to download"));
        push_group(
            &mut rows,
            (available.into_iter())
                .map(|model| Some(Card::Available(model)))
                .collect(),
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
            Row::Apple(place) => self.render_apple_card(*place, cx),
            Row::Card(card, place) => self.render_card(card, *place, cx),
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

/// Adds `group`'s rows, which share one card, `None` being Apple's built-in Model.
fn push_group(rows: &mut Vec<Row>, group: Vec<Option<Card>>) {
    let last = group.len().saturating_sub(1);
    rows.extend(group.into_iter().enumerate().map(|(ix, card)| {
        let place = Place {
            first: ix == 0,
            last: ix == last,
        };
        match card {
            None => Row::Apple(place),
            Some(card) => Row::Card(card, place),
        }
    }));
}
