//! The sidebar row: icon, truncated label, and a selection fill.

use std::rc::Rc;

use super::NativeView;
use crumble_theme::sidebar::{ICON_GAP, ICON_SIZE, LABEL_SIZE, RADIUS, ROW_HEIGHT, ROW_PADDING};
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::{
    App, Context, Div, ElementId, FontWeight, InteractiveElement, KeyDownEvent, ParentElement,
    SharedString, Stateful, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px,
};

/// Unselected icons sit a step below their label.
const ICON_OPACITY: f32 = 0.65;

/// A row whose icon uses the label color while selected.
pub(super) fn row(
    id: impl Into<ElementId>,
    icon: Icon,
    label: impl Into<SharedString>,
    selected: bool,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let text = if selected {
        theme.sidebar_accent_foreground
    } else {
        theme.muted_foreground
    };
    let icon_color = if selected {
        text
    } else {
        theme.muted_foreground.opacity(ICON_OPACITY)
    };
    let hover = theme.sidebar_accent.opacity(0.5);
    div()
        .id(id.into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(ICON_GAP))
        .h(px(ROW_HEIGHT))
        .px(px(ROW_PADDING))
        .rounded(px(RADIUS))
        .cursor_pointer()
        .text_size(px(LABEL_SIZE))
        .font_weight(FontWeight::MEDIUM)
        .text_color(text)
        .when(selected, |row| row.bg(theme.sidebar_accent))
        .when(!selected, |row| row.hover(move |style| style.bg(hover)))
        .child(
            icon.size(px(ICON_SIZE))
                .flex_shrink_0()
                .text_color(icon_color),
        )
        .child(div().flex_1().min_w_0().truncate().child(label.into()))
}

/// Runs `action` on click, and on Enter or Space while the row has keyboard focus.
pub(super) fn activate(
    row: Stateful<Div>,
    label: impl Into<SharedString>,
    cx: &mut Context<NativeView>,
    action: impl Fn(&mut NativeView, &mut Window, &mut Context<NativeView>) + 'static,
) -> Stateful<Div> {
    let action = Rc::new(action);
    let click = action.clone();
    row.tab_index(0)
        .aria_label(label)
        .on_click(cx.listener(move |view, _, window, cx| click(view, window, cx)))
        .on_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                action(view, window, cx);
                cx.stop_propagation();
            }
        }))
}
