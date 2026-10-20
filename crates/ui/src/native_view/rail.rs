//! The rail: an icon button per page, the Calendars popover's button, and Settings at the bottom.

use super::controls::{app_icon, button};
use super::{NativeView, Route};
use crumble_eventkit::Access;
use crumble_theme::rail::{
    BUTTON_SIZE, GAP, ICON_SIZE, RADIUS, SELECTED_INK_DARK, SELECTED_INK_LIGHT, WIDTH,
};
use gpui_kit::base::Placement;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Selectable};
use gpui_kit::{
    App, Context, InteractiveElement, IntoElement, ParentElement, Styled, Window, div,
    prelude::FluentBuilder, px,
};

/// Strength of the ink over the rail while a button is pressed, a step past the hover fill.
const PRESS_OPACITY: f32 = 0.15;
/// Strength of the secondary ink in the separator above the Calendars button; the border
/// color barely shows on the rail.
const SEPARATOR_OPACITY: f32 = 0.3;

impl NativeView {
    /// The rail runs from below the header to the window's bottom, on the sidebar color.
    pub(super) fn render_rail(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // One device pixel: a hairline on Retina instead of 1pt's two pixels.
        let hairline = px(1.0 / window.scale_factor());
        let calendars = self.calendar_sync.read(cx).access() != Access::Unsupported;
        div()
            .id("rail")
            .flex()
            .flex_col()
            .flex_shrink_0()
            .items_center()
            .gap(px(GAP))
            .w(px(WIDTH))
            .h_full()
            .py(px(GAP))
            .text_color(cx.theme().sidebar_foreground)
            .child(self.route_button(Route::Home, "home", cx))
            .child(self.route_button(Route::Agenda, "agenda", cx))
            .child(self.route_button(Route::Calendar, "calendar", cx))
            .child(self.route_button(Route::Chat, "chat", cx))
            .when(calendars, |rail| {
                rail.child(
                    div()
                        .w(px(BUTTON_SIZE - GAP))
                        .h(hairline)
                        .bg(cx.theme().muted_foreground.opacity(SEPARATOR_OPACITY)),
                )
                .child(self.render_calendars_button(window, cx))
            })
            .child(div().flex_1())
            .child(self.route_button(Route::Settings, "settings", cx))
    }

    /// The button that opens `route`, named by a tooltip.
    fn route_button(&self, route: Route, icon: &'static str, cx: &mut Context<Self>) -> Button {
        let label = route.title();
        rail_button(
            format!("nav-{}", label.to_lowercase()),
            icon,
            self.route.rail_route() == route,
            cx,
        )
        .accessibility_label(label)
        .tooltip(label)
        .tooltip_placement(Placement::Right)
        .on_click(cx.listener(move |view, _, _, cx| view.navigate(route.clone(), cx)))
    }
}

/// A square icon button that keeps its hover fill, and shows the solid `icon`, while `selected`. The fill is the full sidebar
/// accent, a step stronger than a sidebar row's, so it stands out on the rail.
pub(super) fn rail_button(
    id: impl Into<gpui_kit::ElementId>,
    icon: &str,
    selected: bool,
    cx: &App,
) -> Button {
    let theme = cx.theme();
    let ink = if selected {
        gpui_kit::rgba(if theme.mode.is_dark() {
            SELECTED_INK_DARK
        } else {
            SELECTED_INK_LIGHT
        })
        .into()
    } else {
        crumble_theme::faint_foreground(theme)
    };
    // GPUI Kit draws a custom button's resting color at a fifth of its strength, so the selected
    // fill comes through its selected state, which draws the active color as is.
    let fill = theme.sidebar_accent;
    let variant = ButtonCustomVariant::new(cx)
        .color(gpui_kit::transparent_black())
        .foreground(ink)
        .hover(fill)
        .active(if selected {
            fill
        } else {
            theme.muted_foreground.opacity(PRESS_OPACITY)
        });
    // The icon is a child, not the button's icon, which GPUI Kit sizes to three quarters of the
    // button.
    button(id)
        .custom(variant)
        .selected(selected)
        .rounded(px(RADIUS))
        .size(px(BUTTON_SIZE))
        .p_0()
        .justify_center()
        .child(
            app_icon(&if selected {
                format!("{icon}-selected")
            } else {
                icon.to_owned()
            })
            .size(px(ICON_SIZE))
            .text_color(ink),
        )
}
