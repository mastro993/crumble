//! Crumble's select: a button showing the picked choice, with an optional icon before it and a
//! chevron after it, that opens the app's menu under it with the picked row checked. A
//! searchable select is a combo box: its menu has a search field.
//!
//! GPUI Kit's `Select` draws its own popup, which cannot take the menu's corners, color, rows
//! or place under the trigger's trailing edge, so Crumble's reuses its menu instead.

use super::controls::app_icon;
use super::menu::{MenuItem, OpenMenu, Place};
use crumble_theme::menu::{HIGHLIGHT_INK, TRAILING_SIZE};
use crumble_theme::sidebar::ICON_GAP;
use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    Bounds, Context, EventEmitter, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent,
    ParentElement, Pixels, Render, Role, SharedString, StatefulInteractiveElement, Styled, Window,
    canvas, div, prelude::FluentBuilder, px,
};
use std::cell::Cell;
use std::rc::Rc;

/// Height of a select.
const HEIGHT: f32 = 28.0;
/// Widest a select is drawn; a longer choice is cut short.
const MAX_WIDTH: f32 = 240.0;
/// Space between a select's edge and what it shows.
const PADDING: f32 = 10.0;
/// Corner radius of a select.
const RADIUS: f32 = 8.0;

/// One of a select's choices.
pub(super) trait Choice: 'static {
    type Value: Clone + 'static;

    /// The name shown in the select and in its row, and searched by a combo box.
    fn title(&self) -> SharedString;

    fn value(&self) -> &Self::Value;

    /// A muted line under the title in the choice's row.
    fn description(&self) -> Option<SharedString> {
        None
    }

    /// Whether the choice's row is greyed out and cannot be picked.
    fn disabled(&self) -> bool {
        false
    }
}

impl Choice for &'static str {
    type Value = &'static str;

    fn title(&self) -> SharedString {
        SharedString::new_static(self)
    }

    fn value(&self) -> &&'static str {
        self
    }
}

/// A choice picked from the menu, even the one already picked.
pub(super) struct Picked<V>(pub(super) V);

/// A select's choices, the picked one, and its open menu.
pub(super) struct Select<C: Choice> {
    items: Vec<C>,
    selected: Option<usize>,
    /// The select's name for a screen reader.
    label: SharedString,
    /// Shown while nothing is picked.
    placeholder: SharedString,
    /// Shown before the picked choice.
    icon: Option<&'static str>,
    searchable: bool,
    focus: FocusHandle,
    /// Where the select was last drawn; its menu opens under it.
    bounds: Rc<Cell<Bounds<Pixels>>>,
    menu: Option<OpenMenu>,
}

impl<C: Choice> EventEmitter<Picked<C::Value>> for Select<C> {}

impl<C: Choice> Select<C> {
    /// A select named `label`, listing `items` with row `selected` picked.
    pub(super) fn new(
        label: impl Into<SharedString>,
        items: Vec<C>,
        selected: Option<usize>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            items,
            selected,
            label: label.into(),
            placeholder: SharedString::default(),
            icon: None,
            searchable: false,
            focus: cx.focus_handle().tab_stop(true),
            bounds: Rc::default(),
            menu: None,
        }
    }

    pub(super) fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Shows the app icon `name` before the picked choice.
    pub(super) fn icon(mut self, name: &'static str) -> Self {
        self.icon = Some(name);
        self
    }

    /// Adds a search field to the menu.
    pub(super) fn searchable(mut self) -> Self {
        self.searchable = true;
        self
    }

    /// Lists `items` instead, with row `selected` picked.
    pub(super) fn set_items(
        &mut self,
        items: Vec<C>,
        selected: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.items = items;
        self.set_selected_index(selected, cx);
    }

    pub(super) fn set_selected_index(&mut self, selected: Option<usize>, cx: &mut Context<Self>) {
        self.selected = selected.filter(|&ix| ix < self.items.len());
        cx.notify();
    }

    pub(super) fn selected_value(&self) -> Option<&C::Value> {
        Some(self.items.get(self.selected?)?.value())
    }

    /// Opens the menu under the select, on the picked row.
    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.is_some() {
            return;
        }
        let (select, id) = (cx.entity().downgrade(), cx.entity_id());
        let items = (self.items.iter().enumerate())
            .map(|(ix, choice)| {
                let select = select.clone();
                let item = MenuItem::new(format!("select-{id}-{ix}"), choice.title())
                    .checked(self.selected == Some(ix))
                    .disabled(choice.disabled())
                    .on_select(move |_, cx| {
                        let _ = select.update(cx, |select, cx| select.pick(ix, cx));
                    });
                match choice.description() {
                    Some(description) => item.description(description),
                    None => item,
                }
            })
            .collect();
        let menu = OpenMenu::new(
            format!("select-{id}"),
            items,
            Place::Below(self.bounds.get()),
            self.focus.clone(),
            window,
            cx,
            |select: &mut Self| &mut select.menu,
        )
        .highlight(self.selected, cx);
        self.menu = Some(if self.searchable {
            menu.searchable(window, cx)
        } else {
            menu
        });
    }

    fn pick(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(value) = self.items.get(ix).map(|choice| choice.value().clone()) else {
            return;
        };
        self.selected = Some(ix);
        cx.emit(Picked(value));
        cx.notify();
    }
}

impl<C: Choice> Render for Select<C> {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (muted, fill) = (
            theme.muted_foreground,
            theme.foreground.alpha(HIGHLIGHT_INK),
        );
        let ring = theme.ring;
        let title = self
            .selected
            .and_then(|ix| self.items.get(ix))
            .map(C::title);
        let bounds = self.bounds.clone();
        let icon = |name| {
            app_icon(name)
                .size(px(TRAILING_SIZE))
                .flex_shrink_0()
                .text_color(muted)
        };
        div()
            .id("select")
            .track_focus(&self.focus)
            .role(Role::ComboBox)
            .aria_label(self.label.clone())
            .aria_expanded(self.menu.is_some())
            .relative()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(ICON_GAP))
            .h(px(HEIGHT))
            .max_w(px(MAX_WIDTH))
            .px(px(PADDING))
            .rounded(px(RADIUS))
            .border_1()
            .border_color(theme.input)
            .text_sm()
            .cursor_pointer()
            .hover(|style| style.bg(fill))
            .when(self.menu.is_some(), |select| select.bg(fill))
            .focus_visible(|style| style.border_color(ring))
            // Enter and Space click it too.
            .on_click(cx.listener(|select, _, window, cx| select.open(window, cx)))
            .on_key_down(cx.listener(|select, event: &KeyDownEvent, window, cx| {
                let stroke = &event.keystroke;
                if matches!(stroke.key.as_str(), "up" | "down") && !stroke.modifiers.modified() {
                    select.open(window, cx);
                    cx.stop_propagation();
                }
            }))
            .children(self.icon.map(icon))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .when(title.is_none(), |title| title.text_color(muted))
                    .child(title.unwrap_or_else(|| self.placeholder.clone())),
            )
            .child(icon("chevron-down"))
            .child(
                canvas(move |select, _, _| bounds.set(select), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            .children(self.menu.as_ref().map(OpenMenu::render))
    }
}
