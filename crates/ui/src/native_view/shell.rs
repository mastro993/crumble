//! Application chrome: sidebar, borderless header, and fixed sidebar toggle.

use std::rc::Rc;

use super::NativeView;
use super::controls::button;
use super::sidebar::{SidebarDrag, clamp_sidebar_width, visible_width};
use crumble_theme::HEADER_HEIGHT;
use gpui_kit::component::{Icon, IconName, Sizable, TitleBar, button::ButtonVariants, h_flex};
use gpui_kit::{
    AnyElement, Context, DragMoveEvent, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};

gpui_kit::actions!(
    crumble,
    [
        /// Shows or hides the sidebar.
        ToggleSidebar,
        /// Returns to the previous sidebar route.
        GoBack,
        /// Reopens the route left by going back.
        GoForward
    ]
);

/// Toggle offset beside the traffic lights: its glyph sits one light-spacing past the green light.
const TOGGLE_LEFT_MACOS: f32 = 93.0;
const TOGGLE_SIZE: f32 = 24.0;
const HEADER_GAP: f32 = 8.0;

/// macOS draws the traffic lights over app content, so the chrome reserves their row.
fn reserves_traffic_lights(window: &Window) -> bool {
    cfg!(target_os = "macos") && !window.is_fullscreen()
}

/// The sidebar toggle stays at this offset whether the sidebar is open or closed.
fn toggle_left(window: &Window) -> f32 {
    if reserves_traffic_lights(window) {
        TOGGLE_LEFT_MACOS
    } else {
        HEADER_GAP
    }
}

/// Leading header space that keeps header content clear of the fixed toggle.
fn header_inset(toggle_left: f32, sidebar_visible: f32) -> f32 {
    (toggle_left + TOGGLE_SIZE + HEADER_GAP - sidebar_visible).max(HEADER_GAP)
}

impl NativeView {
    pub(super) fn render_shell(
        &self,
        page: AnyElement,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let collapsed = self.sidebar_collapsed;
        div()
            .id("app-shell")
            .relative()
            .size_full()
            .flex()
            .on_drag_move(
                cx.listener(|view, event: &DragMoveEvent<Rc<SidebarDrag>>, _, cx| {
                    if event.drag(cx).0 != cx.entity_id() {
                        return;
                    }
                    view.sidebar_width = clamp_sidebar_width(f32::from(
                        event.event.position.x - event.bounds.left(),
                    ));
                    cx.notify();
                }),
            )
            .when(!collapsed || self.sidebar_closing, |shell| {
                shell.child(self.render_sidebar_frame(window, cx))
            })
            .when(!collapsed, |shell| {
                shell.child(self.render_resize_control(cx))
            })
            .child(
                div()
                    .id("main-column")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(self.render_header(window))
                    .child(
                        div()
                            .id("main-content")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .px_6()
                            .pb_6()
                            .child(page),
                    ),
            )
            .child(self.render_sidebar_toggle(window, cx))
            .into_any_element()
    }

    /// Pinned beside the traffic lights, above both the sidebar and the header.
    fn render_sidebar_toggle(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let collapsed = self.sidebar_collapsed;
        div()
            .absolute()
            .top(px((HEADER_HEIGHT - TOGGLE_SIZE) / 2.0))
            .left(px(toggle_left(window)))
            .child(
                // Kit's SidebarToggleButton exposes no styling, so the hand cursor needs a plain button.
                button("collapse")
                    .ghost()
                    .small()
                    .icon(
                        Icon::new(if collapsed {
                            IconName::PanelLeftOpen
                        } else {
                            IconName::PanelLeftClose
                        })
                        .size_4(),
                    )
                    .accessibility_label(if collapsed {
                        "Show sidebar"
                    } else {
                        "Hide sidebar"
                    })
                    .on_click(cx.listener(|view, _, _, cx| view.toggle_sidebar(cx))),
            )
    }

    /// Borderless, transparent header.
    fn render_header(&self, window: &Window) -> AnyElement {
        let (width, collapsed, left) = (
            self.sidebar_width,
            self.sidebar_collapsed,
            toggle_left(window),
        );
        let inset =
            self.sidebar_motion_width(div().flex_shrink_0(), "header-inset", move |delta| {
                header_inset(left, visible_width(width, collapsed, delta))
            });
        if cfg!(target_os = "macos") {
            TitleBar::new()
                .border_0()
                .bg(gpui_kit::transparent_black())
                .h(px(HEADER_HEIGHT))
                .pl_0()
                .pr_4()
                .child(inset)
                .into_any_element()
        } else {
            h_flex()
                .id("header")
                .flex_shrink_0()
                .h(px(HEADER_HEIGHT))
                .pr_4()
                .child(inset)
                .into_any_element()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HEADER_GAP, header_inset};

    #[test]
    fn header_clears_the_toggle_only_while_the_sidebar_is_narrow() {
        assert_eq!(header_inset(93.0, 0.0), 125.0);
        assert_eq!(header_inset(93.0, 60.0), 65.0);
        assert_eq!(header_inset(93.0, 232.0), HEADER_GAP);
    }
}
