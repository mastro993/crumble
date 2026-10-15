//! Sidebar, back/forward controls, resizing, and its open/close motion.

use std::{rc::Rc, time::Duration};

use super::controls::{Inactive, app_icon, button};
use super::sidebar_row::{activate, row};
use super::{NativeView, Route};
use crumble_theme::sidebar::{INSET, ROW_GAP, SECTION_GAP};
use gpui_kit::component::{ActiveTheme, Sizable, button::ButtonVariants, h_flex};
use gpui_kit::{
    Animation, AnimationExt, AnyElement, AppContext, Axis, Context, Div, ElementId, EmptyView,
    InteractiveElement, IntoElement, ParentElement, Stateful, StatefulInteractiveElement, Styled,
    Window, div, prelude::FluentBuilder, px,
};

pub(super) const DEFAULT_WIDTH: f32 = 232.0;
const SIDEBAR_MIN_WIDTH: f32 = 192.0;
const SIDEBAR_MAX_WIDTH: f32 = 400.0;
const SIDEBAR_MOTION: Duration = Duration::from_millis(220);
/// Small ghost button size for the back and forward controls.
const CONTROL_SIZE: f32 = 24.0;

/// Drag payload; the resize handle hands it to GPUI wrapped in an `Rc`.
pub(super) struct SidebarDrag(pub(super) gpui_kit::EntityId);

pub(super) fn clamp_sidebar_width(width: f32) -> f32 {
    width.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH)
}

/// Width of the sidebar still on screen after `delta` of the transition toward `collapsed`.
pub(super) fn visible_width(width: f32, collapsed: bool, delta: f32) -> f32 {
    width * if collapsed { 1.0 - delta } else { delta }
}

impl NativeView {
    pub(super) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        self.sidebar_motion += 1;
        // A closing sidebar stays mounted until its slide-out finishes.
        self.sidebar_closing = self.sidebar_collapsed && !cx.reduce_motion();
        if self.sidebar_closing {
            let motion = self.sidebar_motion;
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(SIDEBAR_MOTION).await;
                let _ = this.update(cx, |view, cx| {
                    if view.sidebar_motion == motion {
                        view.sidebar_closing = false;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    /// Sizes `element` along the current sidebar transition; static until the first toggle.
    pub(super) fn sidebar_motion_width(
        &self,
        element: impl Styled + IntoElement + 'static,
        name: &'static str,
        width_at: impl Fn(f32) -> f32 + 'static,
    ) -> AnyElement {
        if self.sidebar_motion == 0 {
            return element.w(px(width_at(1.0))).into_any_element();
        }
        element
            .with_animation(
                ElementId::NamedInteger(name.into(), self.sidebar_motion),
                Animation::new(SIDEBAR_MOTION)
                    .with_easing(gpui_kit::base::animation::ease_in_out_cubic),
                move |element, delta| element.w(px(width_at(delta))),
            )
            .into_any_element()
    }

    /// The frame expands or shrinks while the full-width sidebar inside stays pinned to its
    /// trailing edge, so the content slides in and out instead of reflowing.
    pub(super) fn render_sidebar_frame(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (width, collapsed) = (self.sidebar_width, self.sidebar_collapsed);
        let frame = div()
            .id("sidebar-frame")
            .flex()
            .flex_shrink_0()
            .justify_end()
            .h_full()
            .overflow_hidden()
            .child(self.render_sidebar(window, cx));
        self.sidebar_motion_width(frame, "sidebar-motion", move |delta| {
            visible_width(width, collapsed, delta)
        })
    }

    fn render_sidebar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let (background, foreground, border) = (
            theme.sidebar,
            theme.sidebar_foreground,
            theme.sidebar_border,
        );
        // One device pixel: a hairline on Retina instead of 1pt's two pixels.
        let hairline = px(1.0 / window.scale_factor());
        let sections = self.render_calendars_section(window, cx);
        let (above, below) = match sections {
            Some(_) => super::sidebar_scroll::scrolled_edges(&self.sidebar_scroll),
            None => (false, false),
        };
        // The dividers show only while sections are scrolled out of view behind them.
        let divider = |shown: bool| {
            if shown {
                border
            } else {
                gpui_kit::transparent_black()
            }
        };
        div()
            .id("sidebar")
            .relative()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(self.sidebar_width))
            .h_full()
            .pt(px(crumble_theme::HEADER_HEIGHT))
            .px(px(INSET))
            .pb(px(INSET))
            .bg(background)
            .text_color(foreground)
            .border_r(hairline)
            .border_color(border)
            .on_hover(cx.listener(|view, hovered: &bool, _, cx| view.hover_sidebar(*hovered, cx)))
            .child(self.render_history_controls(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(ROW_GAP))
                    .pb(px(SECTION_GAP / 2.0))
                    .border_b(hairline)
                    .border_color(divider(above))
                    .child(self.nav_row(Route::Home, "home", cx))
                    .child(self.nav_row(Route::Calendar, "calendar", cx))
                    .child(self.nav_row(Route::Chat, "chat", cx)),
            )
            .map(|sidebar| match sections {
                Some(sections) => sidebar.child(self.render_sections(sections, cx)),
                None => sidebar.child(div().flex_1()),
            })
            .child(
                div()
                    .pt(px(SECTION_GAP / 2.0))
                    .border_t(hairline)
                    .border_color(divider(below))
                    .child(self.nav_row(Route::Settings, "settings", cx)),
            )
    }

    fn nav_row(&self, route: Route, icon: &str, cx: &mut Context<Self>) -> Stateful<Div> {
        let label = route.title();
        activate(
            row(
                format!("nav-{}", label.to_lowercase()),
                app_icon(icon),
                label,
                self.route.sidebar_row() == route,
                cx,
            ),
            label,
            cx,
            move |view, _, cx| view.navigate(route.clone(), cx),
        )
    }

    /// Back and forward, right-aligned in the sidebar's header row.
    fn render_history_controls(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let control = |id: &'static str, icon: &str, label: &'static str, enabled: bool| {
            button(id)
                .ghost()
                .small()
                .icon(app_icon(icon).size_4())
                .accessibility_label(label)
                .inactive(!enabled)
        };
        h_flex()
            .absolute()
            .top(px((crumble_theme::HEADER_HEIGHT - CONTROL_SIZE) / 2.0))
            .right(px(INSET))
            .child(
                control(
                    "go-back",
                    "chevron-left",
                    "Back",
                    self.history.can_go_back(),
                )
                .on_click(cx.listener(|view, _, _, cx| view.go_back(cx))),
            )
            .child(
                control(
                    "go-forward",
                    "chevron-right",
                    "Forward",
                    self.history.can_go_forward(),
                )
                .on_click(cx.listener(|view, _, _, cx| view.go_forward(cx))),
            )
    }

    pub(super) fn render_resize_control(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        div()
            .id("sidebar-resize-control")
            .tab_index(0)
            .aria_label("Resize sidebar")
            .on_key_down(cx.listener(|view, event: &gpui_kit::KeyDownEvent, _, cx| {
                let next = match event.keystroke.key.as_str() {
                    "left" => view.sidebar_width - 8.0,
                    "right" => view.sidebar_width + 8.0,
                    "home" => SIDEBAR_MIN_WIDTH,
                    "end" => SIDEBAR_MAX_WIDTH,
                    _ => return,
                };
                view.sidebar_width = clamp_sidebar_width(next);
                cx.stop_propagation();
                cx.notify();
            }))
            .child(
                gpui_kit::base::resize_handle("sidebar-resize", Axis::Horizontal)
                    // The sidebar's own hairline border is the divider; the kit line would
                    // double it at 1pt.
                    .with_appearance(Rc::new(|_, _, _| Some(div().into_any_element())))
                    .on_drag(SidebarDrag(cx.entity_id()), |_, _, _, cx| {
                        cx.new(|_| EmptyView)
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::visible_width;

    #[test]
    fn visible_width_runs_toward_the_target_state() {
        assert_eq!(visible_width(300.0, false, 0.0), 0.0);
        assert_eq!(visible_width(300.0, false, 1.0), 300.0);
        assert_eq!(visible_width(300.0, true, 0.0), 300.0);
        assert_eq!(visible_width(300.0, true, 1.0), 0.0);
    }
}
