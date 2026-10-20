//! The second-level sidebar, such as the Agenda list: its surface, its open/close motion and
//! resizing.

use std::rc::Rc;
use std::time::{Duration, Instant};

use super::NativeView;
use crumble_theme::SURFACE_RADIUS;
use gpui_kit::component::{ActiveTheme, h_flex};
use gpui_kit::{
    Animation, AnimationExt, AnyElement, App, AppContext, Axis, Context, Div, DragMoveEvent,
    ElementId, EmptyView, Hsla, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};

pub(super) const DEFAULT_WIDTH: f32 = 320.0;
const SIDEBAR_MIN_WIDTH: f32 = 240.0;
const SIDEBAR_MAX_WIDTH: f32 = 480.0;
const SIDEBAR_MOTION: Duration = Duration::from_millis(220);
/// Width of the edge's highlight while the sidebar is resized.
const HIGHLIGHT_WIDTH: f32 = 3.0;
/// Strength of the ring color in that highlight.
const HIGHLIGHT_OPACITY: f32 = 0.5;

/// Drag payload; the resize handle hands it to GPUI wrapped in an `Rc`.
struct SidebarDrag(pub(super) gpui_kit::EntityId);

fn clamp_sidebar_width(width: f32) -> f32 {
    width.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH)
}

/// Whether a toggle at `toggled` is still sliding at `now`.
fn sliding(toggled: Option<Instant>, now: Instant) -> bool {
    toggled.is_some_and(|toggled| now.saturating_duration_since(toggled) < SIDEBAR_MOTION)
}

/// Width of the sidebar still on screen after `delta` of the transition toward `collapsed`.
pub(super) fn visible_width(width: f32, collapsed: bool, delta: f32) -> f32 {
    width * if collapsed { 1.0 - delta } else { delta }
}

/// A second-level column's opaque surface: the sidebar over the page, so the column sits
/// between them. Dark mode leans toward the page, keeping the column dark beside the light
/// sidebar.
pub(super) fn surface(cx: &App) -> Hsla {
    let theme = cx.theme();
    let strength = if theme.is_dark() { 0.35 } else { 0.5 };
    theme.background.blend(theme.sidebar.opacity(strength))
}

/// A second-level column filling its parent, with a hairline on its trailing edge. The sidebar
/// around it paints its surface; see `rounded_surface`.
pub(super) fn column(window: &Window, cx: &App) -> Div {
    // One device pixel, matching the panel's hairline.
    let hairline = px(1.0 / window.scale_factor());
    div()
        .size_full()
        .border_r(hairline)
        .border_color(cx.theme().border)
}

/// `sidebar` on the column surface, rounded to the panel's leading corners. The panel does not
/// clip its children to its corners, so the sidebar's visible leading edge rounds itself: a
/// sliding sidebar's column sits partly outside its frame, where its own corners would not show.
fn rounded_surface(sidebar: Div, cx: &App) -> Div {
    sidebar.rounded_l(px(SURFACE_RADIUS)).bg(surface(cx))
}

impl NativeView {
    /// Shows or hides the second-level sidebar; does nothing on a page without one.
    pub(super) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        if !self.route.has_sidebar() {
            return;
        }
        self.sidebar_collapsed = !self.sidebar_collapsed;
        self.sidebar_motion += 1;
        self.sidebar_toggled = Some(Instant::now());
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

    /// Sizes `element` along the current sidebar transition; static once it has finished.
    pub(super) fn sidebar_motion_width(
        &self,
        element: impl Styled + IntoElement + 'static,
        name: &'static str,
        width_at: impl Fn(f32) -> f32 + 'static,
    ) -> AnyElement {
        if !sliding(self.sidebar_toggled, Instant::now()) {
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
    pub(super) fn render_sidebar_frame(&self, sidebar: impl IntoElement, cx: &App) -> AnyElement {
        let (width, collapsed) = (self.sidebar_width, self.sidebar_collapsed);
        let frame = rounded_surface(div(), cx)
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
    /// open.
    pub(super) fn render_sidebar(
        &self,
        sidebar: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let collapsed = self.sidebar_collapsed;
        let shown = !collapsed || self.sidebar_closing;
        self.resizable(cx)
            .when(shown, |row| {
                row.child(self.render_sidebar_frame(sidebar, cx))
            })
            .when(!collapsed, |row| row.child(self.render_resize_control(cx)))
            .into_any_element()
    }

    /// A second-level sidebar that stays open, such as the Settings sections: `sidebar` at the
    /// width every sidebar shares, then its resize handle.
    pub(super) fn render_open_sidebar(
        &self,
        sidebar: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.resizable(cx)
            .child(
                rounded_surface(div(), cx)
                    .flex_shrink_0()
                    .w(px(self.sidebar_width))
                    .h_full()
                    .child(sidebar),
            )
            .child(self.render_resize_control(cx))
            .into_any_element()
    }

    /// The row holding a sidebar and its resize handle. Dragging the handle measures from this
    /// row's leading edge.
    fn resizable(&self, cx: &mut Context<Self>) -> Div {
        h_flex().h_full().flex_shrink_0().on_drag_move(cx.listener(
            |view, event: &DragMoveEvent<Rc<SidebarDrag>>, _, cx| {
                if event.drag(cx).0 != cx.entity_id() {
                    return;
                }
                view.sidebar_width =
                    clamp_sidebar_width(f32::from(event.event.position.x - event.bounds.left()));
                cx.notify();
            },
        ))
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
                    // At rest the sidebar's own hairline border is the divider; the kit line
                    // would double it at 1pt. While pressed or dragged, a wider line in a light
                    // tint of the ring color covers the hairline, centred on the handle's 1pt
                    // line.
                    .with_appearance(Rc::new(|handle, _, cx| {
                        let line = div().flex_none();
                        Some(if handle.is_active() {
                            line.h_full()
                                .w(px(HIGHLIGHT_WIDTH))
                                .ml(px((1.0 - HIGHLIGHT_WIDTH) / 2.0))
                                .bg(cx.theme().ring.opacity(HIGHLIGHT_OPACITY))
                                .into_any_element()
                        } else {
                            line.into_any_element()
                        })
                    }))
                    .on_drag(SidebarDrag(cx.entity_id()), |_, _, _, cx| {
                        cx.new(|_| EmptyView)
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{SIDEBAR_MOTION, sliding, visible_width};
    use std::time::Instant;

    #[test]
    fn visible_width_runs_toward_the_target_state() {
        assert_eq!(visible_width(300.0, false, 0.0), 0.0);
        assert_eq!(visible_width(300.0, false, 1.0), 300.0);
        assert_eq!(visible_width(300.0, true, 0.0), 300.0);
        assert_eq!(visible_width(300.0, true, 1.0), 0.0);
    }

    #[test]
    fn only_a_recent_toggle_slides() {
        let toggled = Instant::now();
        assert!(!sliding(None, toggled), "launch");
        assert!(sliding(Some(toggled), toggled));
        // A page mounting the sidebar after the slide shows it still.
        assert!(!sliding(Some(toggled), toggled + SIDEBAR_MOTION));
    }
}
