//! The second-level sidebar, such as the Agenda list: its open/close motion and resizing.

use std::{rc::Rc, time::Duration};

use super::NativeView;
use gpui_kit::component::h_flex;
use gpui_kit::{
    Animation, AnimationExt, AnyElement, AppContext, Axis, Context, DragMoveEvent, ElementId,
    EmptyView, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, prelude::FluentBuilder, px,
};

pub(super) const DEFAULT_WIDTH: f32 = 320.0;
const SIDEBAR_MIN_WIDTH: f32 = 240.0;
const SIDEBAR_MAX_WIDTH: f32 = 480.0;
const SIDEBAR_MOTION: Duration = Duration::from_millis(220);

/// Drag payload; the resize handle hands it to GPUI wrapped in an `Rc`.
struct SidebarDrag(pub(super) gpui_kit::EntityId);

fn clamp_sidebar_width(width: f32) -> f32 {
    width.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH)
}

/// Width of the sidebar still on screen after `delta` of the transition toward `collapsed`.
pub(super) fn visible_width(width: f32, collapsed: bool, delta: f32) -> f32 {
    width * if collapsed { 1.0 - delta } else { delta }
}

impl NativeView {
    /// Shows or hides the second-level sidebar; does nothing on a page without one.
    pub(super) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        if !self.route.has_sidebar() {
            return;
        }
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

    /// The frame expands or shrinks while `sidebar` inside stays pinned to its trailing edge at
    /// full width, so the content slides in and out instead of reflowing.
    pub(super) fn render_sidebar_frame(&self, sidebar: impl IntoElement) -> AnyElement {
        let (width, collapsed) = (self.sidebar_width, self.sidebar_collapsed);
        let frame = div()
            .id("sidebar-frame")
            .flex()
            .flex_shrink_0()
            .justify_end()
            .h_full()
            .overflow_hidden()
            .child(div().flex_shrink_0().w(px(width)).h_full().child(sidebar));
        self.sidebar_motion_width(frame, "sidebar-motion", move |delta| {
            visible_width(width, collapsed, delta)
        })
    }

    /// The second-level sidebar, `sidebar`, sliding open and shut, then its resize handle while
    /// open. Dragging the handle measures from the leading edge of this element's parent.
    pub(super) fn render_sidebar(
        &self,
        sidebar: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let collapsed = self.sidebar_collapsed;
        let shown = !collapsed || self.sidebar_closing;
        h_flex()
            .h_full()
            .flex_shrink_0()
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
            .when(shown, |row| row.child(self.render_sidebar_frame(sidebar)))
            .when(!collapsed, |row| row.child(self.render_resize_control(cx)))
            .into_any_element()
    }

    fn render_resize_control(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        div()
            .id("sidebar-resize-control")
            // The handle's grab band spans this element's height.
            .h_full()
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
