//! The sidebar row: icon, truncated label, and a selection fill.

use crumble_theme::sidebar::{ICON_GAP, ICON_SIZE, LABEL_SIZE, RADIUS, ROW_HEIGHT, ROW_PADDING};
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::{
    App, Context, Div, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement,
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
    let icon_color = if selected {
        theme.sidebar_accent_foreground
    } else {
        theme.muted_foreground.opacity(ICON_OPACITY)
    };
    let icon = icon
        .size(px(ICON_SIZE))
        .flex_shrink_0()
        .text_color(icon_color);
    row_with(id, icon, label, selected, cx)
}

/// A row with any `leading` element in the icon slot.
pub(super) fn row_with(
    id: impl Into<ElementId>,
    leading: impl IntoElement,
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
    let hover = hover_fill(cx);
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
        .child(leading)
        .child(div().flex_1().min_w_0().truncate().child(label.into()))
}

/// The fill of a row under the pointer, or whose context menu is open.
pub(super) fn hover_fill(cx: &App) -> Hsla {
    cx.theme().sidebar_accent.opacity(0.5)
}

/// Runs `action` on click, and on Enter or Space while the row has keyboard focus, which GPUI
/// turns into a click on a focusable element.
pub(super) fn activate<V: 'static>(
    row: Stateful<Div>,
    label: impl Into<SharedString>,
    cx: &Context<V>,
    action: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
) -> Stateful<Div> {
    row.tab_index(0)
        .aria_label(label)
        .on_click(cx.listener(move |view, _, window, cx| action(view, window, cx)))
}

#[cfg(test)]
mod tests {
    use super::activate;
    use gpui_kit::test::{TestSupportExt, TestWindowExt};
    use gpui_kit::{
        AppContext, Bounds, Context, InteractiveElement, IntoElement, ParentElement, Render,
        Styled, TestAppContext, Window, WindowBounds, WindowOptions, div, px, size,
    };

    #[derive(Default)]
    struct Row {
        activations: usize,
    }

    impl Render for Row {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            activate(
                div().id("row").size_full().child("Work"),
                "Work",
                cx,
                |row: &mut Row, _, _| row.activations += 1,
            )
            .test_support()
        }
    }

    #[gpui_kit::test]
    fn each_click_or_key_on_a_focused_row_runs_it_once(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (handle, row) = cx
            .update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            Default::default(),
                            size(px(200.), px(32.)),
                        ))),
                        ..Default::default()
                    },
                    cx,
                    |_, cx| cx.new(|_| Row::default()),
                )
            })
            .unwrap_or_else(|error| panic!("open test window: {error}"));

        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            // The click also focuses the row.
            window.click("row", cx);
            window.press("enter", cx);
            window.press("space", cx);
        })
        .unwrap_or_else(|error| panic!("update test window: {error}"));

        cx.update(|cx| assert_eq!(row.read(cx).activations, 3));
    }
}
