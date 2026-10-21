//! The parts every Settings section is built from: groups of settings on a card under a
//! heading, each setting its title and description beside its control, with a hairline
//! between settings.

use super::super::controls::app_icon;
use crumble_theme::SURFACE_RADIUS;
use crumble_theme::sidebar::ICON_SIZE;
use gpui_kit::component::group_box::{GroupBox, GroupBoxVariants};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::separator::Separator;
use gpui_kit::component::{ActiveTheme, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, ElementId, Entity, FontWeight, IntoElement, ParentElement, RenderOnce,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder, px,
};

/// Width of a field beside its setting.
const FIELD_WIDTH: f32 = 240.0;
/// Space around a section: above its title, at its sides, and below its last group.
pub(in super::super) const PAGE_PADDING: f32 = 40.0;
/// Space between a section's title and its first group.
pub(in super::super) const TITLE_GAP: f32 = 32.0;
/// Space between one group and the next.
pub(in super::super) const GROUP_GAP: f32 = 44.0;
/// Space between a group's heading and its card.
pub(in super::super) const HEADING_GAP: f32 = 16.0;
/// Space above and below each setting on a card.
const ROW_PADDING: f32 = 14.0;
/// Height of a search field.
const SEARCH_HEIGHT: f32 = 36.0;
/// Size of a search field's text and placeholder.
const SEARCH_TEXT_SIZE: f32 = 15.0;
/// How faded a setting is while it cannot be changed.
const DISABLED_OPACITY: f32 = 0.5;

/// A section's groups, one under another.
pub(in super::super) fn groups() -> Div {
    v_flex().gap(px(GROUP_GAP))
}

/// A group's heading, above its card; also heads a run of cards that are not one group.
pub(in super::super) fn heading(child: impl IntoElement) -> Div {
    div()
        .text_base()
        .font_weight(FontWeight::MEDIUM)
        .child(child)
}

/// A line of secondary text, under a setting's title or a section's.
pub(in super::super) fn description(child: impl IntoElement, cx: &App) -> Div {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(child)
}

/// `control` at the width every field beside a setting has.
pub(in super::super) fn field(control: impl IntoElement) -> Div {
    // An Input fills its parent, so a fixed-width box sizes it.
    div().w(px(FIELD_WIDTH)).child(control)
}

/// `element` drawn as a card: the theme's card color, a hairline edge and rounded corners.
pub(in super::super) fn card<E: Styled>(element: E, cx: &App) -> E {
    element
        .bg(cx.theme().group_box)
        .border_1()
        .border_color(cx.theme().input)
        .rounded(px(SURFACE_RADIUS))
}

/// A search field: a pill in a selected sidebar row's fill, with a search icon, a button that
/// clears it, and the edge every control has.
pub(in super::super) fn search_field(state: &Entity<InputState>, cx: &App) -> Input {
    let fill = cx.theme().sidebar_accent;
    Input::new(state)
        .cleanable(true)
        .prefix(
            app_icon("search")
                .size(px(ICON_SIZE))
                .text_color(cx.theme().muted_foreground),
        )
        .h(px(SEARCH_HEIGHT))
        .px_4()
        .gap_2()
        .rounded_full()
        .bg(fill)
        .border_color(cx.theme().input)
        .text_size(px(SEARCH_TEXT_SIZE))
}

/// Settings that belong together, on one card under an optional heading.
#[derive(IntoElement)]
pub(in super::super) struct Group {
    id: ElementId,
    heading: Option<AnyElement>,
    rows: Vec<AnyElement>,
}

impl Group {
    pub(in super::super) fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            heading: None,
            rows: Vec::new(),
        }
    }

    pub(in super::super) fn heading(mut self, heading: impl IntoElement) -> Self {
        self.heading = Some(heading.into_any_element());
        self
    }
}

impl ParentElement for Group {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.rows.extend(elements);
    }
}

impl RenderOnce for Group {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut content = StyleRefinement::default().px_4().py_0().gap_0();
        content = card(content, cx);
        let divider = crumble_theme::divider(cx.theme());
        let mut rows = Vec::with_capacity(self.rows.len() * 2);
        for (index, row) in self.rows.into_iter().enumerate() {
            if index > 0 {
                rows.push(Separator::horizontal().color(divider).into_any_element());
            }
            rows.push(row);
        }
        GroupBox::new()
            .id(self.id)
            .fill()
            .gap(px(HEADING_GAP))
            .when_some(self.heading, |group, title| group.title(heading(title)))
            .title_style(StyleRefinement::default().text_color(cx.theme().foreground))
            .content_style(content)
            .children(rows)
    }
}

/// One setting: its title and description, and its control at the trailing edge.
#[derive(IntoElement)]
pub(in super::super) struct Setting {
    title: AnyElement,
    descriptions: Vec<AnyElement>,
    control: Option<AnyElement>,
    disabled: bool,
}

impl Setting {
    pub(in super::super) fn new(title: impl IntoElement) -> Self {
        Self {
            title: title.into_any_element(),
            descriptions: Vec::new(),
            control: None,
            disabled: false,
        }
    }

    /// Adds a line under the title; each call adds another.
    pub(in super::super) fn description(mut self, description: impl IntoElement) -> Self {
        self.descriptions.push(description.into_any_element());
        self
    }

    /// Adds a line under the title for each of `descriptions`.
    pub(in super::super) fn descriptions<E: IntoElement>(
        mut self,
        descriptions: impl IntoIterator<Item = E>,
    ) -> Self {
        (self.descriptions).extend(descriptions.into_iter().map(IntoElement::into_any_element));
        self
    }

    pub(in super::super) fn control(mut self, control: impl IntoElement) -> Self {
        self.control = Some(control.into_any_element());
        self
    }

    /// Fades the setting while it cannot be changed; its control disables itself.
    pub(in super::super) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for Setting {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .w_full()
            .py(px(ROW_PADDING))
            .gap_6()
            .justify_between()
            .when(self.disabled, |row| row.opacity(DISABLED_OPACITY))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .items_start()
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(self.title),
                    )
                    .children((self.descriptions.into_iter()).map(|line| description(line, cx))),
            )
            .children(
                self.control
                    .map(|control| div().flex_shrink_0().child(control)),
            )
    }
}
