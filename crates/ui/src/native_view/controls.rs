//! App buttons: GPUI Kit keeps the arrow cursor on buttons except link and text variants.

use gpui_kit::component::{Disableable, Icon, button::Button};
use gpui_kit::{ElementId, InteractiveElement, MouseButton, Styled, prelude::FluentBuilder};

/// Button that shows a hand cursor while it is enabled.
///
/// Its presses stay inside the button: the title bar zooms the window on a double click and
/// drags it on a press, and neither should happen when the press lands on a header button.
pub(super) fn button(id: impl Into<ElementId>) -> Button {
    Button::new(id)
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

/// Retained Hugeicons artwork mapped under `icons/` in the asset source.
pub(super) fn app_icon(name: &str) -> Icon {
    Icon::empty().path(format!("icons/{name}.svg"))
}

pub(super) trait Inactive {
    /// Disables the button and restores the arrow cursor while it is disabled.
    fn inactive(self, inactive: bool) -> Self;
}

impl Inactive for Button {
    fn inactive(self, inactive: bool) -> Self {
        self.disabled(inactive)
            .when(inactive, |button| button.cursor_default())
    }
}

#[cfg(test)]
mod tests {
    use super::button;
    use gpui_kit::base::InteractiveElementExt;
    use gpui_kit::test::{TestSupportExt, TestWindowExt};
    use gpui_kit::{
        AppContext, Bounds, Context, InteractiveElement, IntoElement, ParentElement, Render,
        Styled, TestAppContext, Window, WindowBounds, WindowOptions, div, px, size,
    };

    /// Stands in for the title bar, which zooms the window from the same double-click listener.
    #[derive(Default)]
    struct Header {
        button_clicks: usize,
        header_double_clicks: usize,
    }

    impl Render for Header {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .id("header")
                .size_full()
                .on_double_click(cx.listener(|header, _, _, cx| {
                    header.header_double_clicks += 1;
                    cx.notify();
                }))
                .child(
                    button("today")
                        .label("Today")
                        .on_click(cx.listener(|header, _, _, cx| {
                            header.button_clicks += 1;
                            cx.notify();
                        })),
                )
                .test_support()
        }
    }

    #[gpui_kit::test]
    fn double_clicking_a_button_does_not_reach_the_header(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (handle, header) = cx
            .update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            Default::default(),
                            size(px(400.), px(100.)),
                        ))),
                        ..Default::default()
                    },
                    cx,
                    |_, cx| cx.new(|_| Header::default()),
                )
            })
            .unwrap_or_else(|error| panic!("open test window: {error}"));

        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.double_click("today", cx);
            // Control: the same gesture on bare header space still reaches the header.
            window.double_click("header", cx);
        })
        .unwrap_or_else(|error| panic!("update test window: {error}"));

        cx.update(|cx| {
            let header = header.read(cx);
            assert_eq!(header.button_clicks, 2);
            assert_eq!(header.header_double_clicks, 1);
        });
    }
}
