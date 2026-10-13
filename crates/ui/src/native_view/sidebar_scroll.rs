//! The sidebar's scrolling sections and their scrollbar, which fades in while the pointer is
//! over the sidebar. Copied from SkillBinder's sidebar.

use std::time::Duration;

use super::NativeView;
use crumble_theme::sidebar::{INSET, SECTION_GAP};
use gpui_kit::component::scroll::{Scrollbar, ScrollbarMode};
use gpui_kit::{
    Animation, AnimationExt, AnyElement, Context, ElementId, InteractiveElement, IntoElement,
    ParentElement, ScrollHandle, StatefulInteractiveElement, Styled, div, px,
};

/// GPUI Kit's scrollbar fade timings.
const SCROLLBAR_ENTER: Duration = Duration::from_millis(300);
const SCROLLBAR_EXIT: Duration = Duration::from_millis(500);
/// The shown scrollbar's opacity, which keeps it quieter than the rows beside it.
const SCROLLBAR_OPACITY: f32 = 0.5;

impl NativeView {
    /// The sections, scrolling together between the top rows and Settings.
    pub(super) fn render_sections(
        &self,
        sections: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .id("sidebar-sections")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.sidebar_scroll)
                    // Scrolls with the sections, mirroring the top rows' half gap.
                    .pb(px(SECTION_GAP / 2.0))
                    .child(sections),
            )
            .child(self.render_sections_scrollbar(cx))
            .into_any_element()
    }

    /// The scrollbar sits in the sidebar's trailing inset, visible only while the sidebar is
    /// hovered.
    fn render_sections_scrollbar(&self, cx: &Context<Self>) -> AnyElement {
        let shown = self.sidebar_hovered;
        let overlay = div()
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .right(px(-INSET))
            .child(
                Scrollbar::vertical(&self.sidebar_scroll)
                    .id("sidebar-scrollbar")
                    .mode(ScrollbarMode::Always)
                    .viewport_from_layout(),
            );
        if self.sidebar_hover_motion == 0 || cx.reduce_motion() {
            return overlay
                .opacity(if shown { SCROLLBAR_OPACITY } else { 0.0 })
                .into_any_element();
        }
        // ponytail: each fade starts from its far end, so leaving mid-fade-in jumps to the shown
        // opacity first; track the current opacity if quick passes over the sidebar look wrong.
        let (from, to, duration) = if shown {
            (0.0, SCROLLBAR_OPACITY, SCROLLBAR_ENTER)
        } else {
            (SCROLLBAR_OPACITY, 0.0, SCROLLBAR_EXIT)
        };
        overlay
            .with_animation(
                ElementId::NamedInteger("sidebar-scrollbar-fade".into(), self.sidebar_hover_motion),
                Animation::new(duration).with_easing(gpui_kit::base::animation::ease_in_out_cubic),
                move |overlay, delta| overlay.opacity(from + (to - from) * delta),
            )
            .into_any_element()
    }

    /// Records whether the pointer is over the sidebar, which fades the scrollbar in or out.
    pub(super) fn hover_sidebar(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if self.sidebar_hovered != hovered {
            self.sidebar_hovered = hovered;
            self.sidebar_hover_motion += 1;
            cx.notify();
        }
    }
}

/// Layout leaves rounding crumbs in the scroll range, so less than this counts as no overflow.
const EDGE_SLACK: f32 = 0.5;

/// Whether sections are scrolled out of view above and below the sidebar's scroll area.
pub(super) fn scrolled_edges(scroll: &ScrollHandle) -> (bool, bool) {
    edges(
        -f32::from(scroll.offset().y),
        f32::from(scroll.max_offset().y),
    )
}

fn edges(offset: f32, max: f32) -> (bool, bool) {
    (offset > EDGE_SLACK, offset < max - EDGE_SLACK)
}

#[cfg(test)]
mod tests {
    use super::edges;

    #[test]
    fn rounding_crumbs_do_not_count_as_overflow() {
        assert_eq!(edges(0.0, 0.0001), (false, false));
        assert_eq!(edges(0.0, 120.0), (false, true));
        assert_eq!(edges(60.0, 120.0), (true, true));
        assert_eq!(edges(120.0, 120.0), (true, false));
    }
}
