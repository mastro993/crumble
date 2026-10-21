//! Application chrome: the borderless header with back, forward, and the sidebar toggle pinned
//! beside the traffic lights, the rail, and the panel the page sits in.

use super::controls::{Inactive, app_icon, button};
use super::rail::PRESS_OPACITY;
use super::{NativeView, Route};
use crumble_theme::{CONTENT_PADDING, HEADER_HEIGHT, SURFACE_RADIUS};
use gpui_kit::component::button::{ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Icon, IconName, Sizable, TitleBar, h_flex};
use gpui_kit::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};

gpui_kit::actions!(
    crumble,
    [
        /// Shows or hides the page's second-level sidebar, such as the Agenda list.
        ToggleSidebar,
        /// Returns to the previous route.
        GoBack,
        /// Reopens the route left by going back.
        GoForward
    ]
);

/// Controls offset beside the traffic lights: the first glyph sits one light-spacing past the
/// green light.
const CONTROLS_LEFT_MACOS: f32 = 93.0;
/// Small button size of each header control.
const CONTROL_SIZE: f32 = 24.0;
const CONTROL_GAP: f32 = 4.0;
const HEADER_GAP: f32 = 8.0;
/// Strength of the secondary ink in a header control's icon while it has nowhere to go.
const QUIET_INK: f32 = 0.2;
/// Space between the panel and the window's trailing and bottom edges, so the panel's corners
/// sit concentric inside the window's.
const PANEL_INSET: f32 = 4.0;

/// macOS draws the traffic lights over app content, so the chrome reserves their row.
fn reserves_traffic_lights(window: &Window) -> bool {
    cfg!(target_os = "macos") && !window.is_fullscreen()
}

/// Where the header controls start.
fn controls_left(window: &Window) -> f32 {
    if reserves_traffic_lights(window) {
        CONTROLS_LEFT_MACOS
    } else {
        HEADER_GAP
    }
}

/// Leading header space that keeps the page's toolbar clear of `controls` header controls
/// starting at `left`.
fn header_inset(left: f32, controls: usize) -> f32 {
    let controls = controls as f32;
    left + controls * CONTROL_SIZE + (controls - 1.0).max(0.0) * CONTROL_GAP + HEADER_GAP
}

impl NativeView {
    pub(super) fn render_shell(
        &self,
        page: AnyElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        // One device pixel: a hairline on Retina instead of 1pt's two pixels.
        let hairline = px(1.0 / window.scale_factor());
        let (edge, background) = (theme.sidebar_border, theme.background);
        let panel = div()
            .id("main-panel")
            .relative()
            .flex()
            .flex_1()
            .min_w_0()
            // Stretched by the row rather than `h_full`, which would push the bottom inset
            // past the window.
            .overflow_hidden()
            .mr(px(PANEL_INSET))
            .mb(px(PANEL_INSET))
            .rounded(px(SURFACE_RADIUS))
            .shadow(crumble_theme::panel_shadow())
            .border(hairline)
            .border_color(edge)
            .bg(background)
            .child(
                div()
                    .id("main-content")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    // The Agenda list and the Settings sections run from the panel's leading edge
                    // to its bottom.
                    .when(!self.route.fills_panel(), |content| {
                        content.p(px(CONTENT_PADDING)).pb_6()
                    })
                    .child(page),
            );
        div()
            .id("app-shell")
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .child(self.render_header(window, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_rail(window, cx))
                    .child(panel),
            )
            .child(self.render_header_controls(window, cx))
            .when(self.route == Route::Calendar, |shell| {
                // Centered on the window rather than on the toolbar, which starts after the
                // header controls. A plain div takes no clicks, so the header still drags.
                shell.child(
                    h_flex()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .h(px(HEADER_HEIGHT))
                        .justify_center()
                        .child(self.render_calendar_navigation(cx)),
                )
            })
            .into_any_element()
    }

    /// Back, forward, then the sidebar toggle on a page with a second-level sidebar, pinned
    /// beside the traffic lights above the header.
    fn render_header_controls(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The rail's faint ink, or a trace of it with nowhere to go. The icons carry their own
        // color, as GPUI Kit draws every disabled button in one fixed ink.
        let theme = cx.theme();
        let (ink, quiet) = (
            crumble_theme::faint_foreground(theme),
            theme.muted_foreground.opacity(QUIET_INK),
        );
        // The rail's hover and press fills: GPUI Kit draws a ghost button's dark mode hover at
        // half strength, too faint on the shell.
        let variant = ButtonCustomVariant::new(cx)
            .color(gpui_kit::transparent_black())
            .foreground(ink)
            .hover(theme.secondary)
            .active(theme.muted_foreground.opacity(PRESS_OPACITY));
        let control = |id: &'static str, icon: Icon, label: &'static str, inactive: bool| {
            let icon = icon.size_4().text_color(if inactive { quiet } else { ink });
            button(id)
                .custom(variant)
                .small()
                .icon(icon)
                .accessibility_label(label)
                .inactive(inactive)
        };
        let collapsed = self.sidebar_collapsed;
        h_flex()
            .absolute()
            .top(px((HEADER_HEIGHT - CONTROL_SIZE) / 2.0))
            .left(px(controls_left(window)))
            .gap(px(CONTROL_GAP))
            .child(
                control(
                    "go-back",
                    app_icon("chevron-left"),
                    "Back",
                    !self.history.can_go_back(),
                )
                .on_click(cx.listener(|view, _, _, cx| view.go_back(cx))),
            )
            .child(
                control(
                    "go-forward",
                    app_icon("chevron-right"),
                    "Forward",
                    !self.history.can_go_forward(),
                )
                .on_click(cx.listener(|view, _, _, cx| view.go_forward(cx))),
            )
            .when(self.route.has_sidebar(), |controls| {
                let (icon, label) = if collapsed {
                    (IconName::PanelLeftOpen, "Show sidebar")
                } else {
                    (IconName::PanelLeftClose, "Hide sidebar")
                };
                // Kit's SidebarToggleButton exposes no styling, so the hand cursor needs a plain
                // button.
                controls.child(
                    control("collapse", Icon::new(icon), label, false)
                        .on_click(cx.listener(|view, _, _, cx| view.toggle_sidebar(cx))),
                )
            })
    }

    /// Borderless, transparent header; the Agenda, Calendar, and Event pages put their toolbars
    /// here, clear of the header controls. The Recording pill sits at its trailing end on every
    /// page while a Recording runs.
    fn render_header(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let controls = if self.route.has_sidebar() { 3 } else { 2 };
        let inset = div()
            .flex_shrink_0()
            .w(px(header_inset(controls_left(window), controls)));
        let toolbar = match &self.route {
            Route::Agenda => Some(self.render_agenda_toolbar(cx).into_any_element()),
            Route::Calendar => Some(self.render_calendar_toolbar(cx).into_any_element()),
            Route::Event(event) => Some(self.render_event_toolbar(event, cx)),
            // Pushes the pill to the trailing end, as the toolbars do.
            _ => Some(div().flex_1().into_any_element()),
        };
        let pill = self.render_recording_pill(cx);
        if cfg!(target_os = "macos") {
            TitleBar::new()
                .border_0()
                .bg(gpui_kit::transparent_black())
                .h(px(HEADER_HEIGHT))
                .pl_0()
                .pr(px(CONTENT_PADDING))
                .child(inset)
                .children(toolbar)
                .children(pill)
                .into_any_element()
        } else {
            h_flex()
                .id("header")
                .flex_shrink_0()
                .h(px(HEADER_HEIGHT))
                .pr(px(CONTENT_PADDING))
                .child(inset)
                .children(toolbar)
                .children(pill)
                .into_any_element()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::header_inset;

    #[test]
    fn header_clears_every_control() {
        assert_eq!(header_inset(93.0, 2), 93.0 + 24.0 + 4.0 + 24.0 + 8.0);
        assert_eq!(header_inset(93.0, 3), 93.0 + 3.0 * 24.0 + 2.0 * 4.0 + 8.0);
        assert_eq!(header_inset(8.0, 0), 16.0);
    }
}
