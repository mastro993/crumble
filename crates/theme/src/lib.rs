//! Application appearance and typography.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Window};

/// Height of the window header row.
pub const HEADER_HEIGHT: f32 = 50.0;

/// Top-left of the 14 pt macOS traffic lights, vertically centred in the header row.
pub const TRAFFIC_LIGHTS_ORIGIN: (f32, f32) = (16.0, (HEADER_HEIGHT - 14.0) / 2.0);

/// Horizontal padding of the main column, shared by the header and the page so their
/// edges line up.
pub const CONTENT_PADDING: f32 = 24.0;

/// Corner radius of large content surfaces such as the Calendar grids, one tier softer than
/// the theme's `radius_lg` controls they sit beside.
pub const SURFACE_RADIUS: f32 = 12.0;

/// Sidebar metrics in points.
pub mod sidebar {
    /// Height of a navigation row.
    pub const ROW_HEIGHT: f32 = 32.0;
    /// Vertical space between rows.
    pub const ROW_GAP: f32 = 1.0;
    /// Space between the sidebar edges and a row's selection fill.
    pub const INSET: f32 = 8.0;
    /// Horizontal padding inside a row.
    pub const ROW_PADDING: f32 = 8.0;
    /// Row icon size.
    pub const ICON_SIZE: f32 = 16.0;
    /// Space between a row icon and its label.
    pub const ICON_GAP: f32 = 8.0;
    /// Row label size.
    pub const LABEL_SIZE: f32 = 14.0;
    /// Section title size.
    pub const DETAIL_SIZE: f32 = 12.0;
    /// Height of a section title row.
    pub const SECTION_HEIGHT: f32 = 28.0;
    /// Space above a section title.
    pub const SECTION_GAP: f32 = 8.0;
    /// Corner radius of the selection fill.
    pub const RADIUS: f32 = 8.0;
    /// Diameter of an Account's color dot, centred in the icon slot.
    pub const DOT_SIZE: f32 = 8.0;
    /// Side of an Account's checkbox, centred in the icon slot.
    pub const CHECK_SIZE: f32 = 14.0;
    /// Corner radius of an Account's checkbox.
    pub const CHECK_RADIUS: f32 = 4.0;
}

/// Context menu metrics in points. Rows reuse the [`sidebar`] row height, padding, and icon
/// metrics, the corners are [`SURFACE_RADIUS`], and a highlighted row is rounded to the theme
/// radius.
pub mod menu {
    use gpui_kit::component::Theme;
    use gpui_kit::{BoxShadow, Hsla, hsla, px};

    /// Space between the menu's edge and its rows; separators are inset by the same amount.
    pub const PADDING: f32 = 8.0;
    /// Row label size, the macOS menu size, a step below the sidebar's.
    pub const LABEL_SIZE: f32 = 13.0;
    /// Narrowest a menu or submenu is drawn.
    pub const MIN_WIDTH: f32 = 180.0;
    /// Size of the check and of a submenu row's chevron.
    pub const TRAILING_SIZE: f32 = 14.0;
    /// Space above and below a separator.
    pub const SEPARATOR_MARGIN: f32 = 4.0;
    /// Foreground alpha of the menu's edge and its separators.
    pub const LINE_INK: f32 = 0.1;
    /// Foreground alpha of the highlighted row's fill.
    pub const HIGHLIGHT_INK: f32 = 0.08;
    /// Opacity of a disabled row.
    pub const DISABLED_OPACITY: f32 = 0.4;
    /// How far a submenu overlaps the edge of its parent.
    pub const SUBMENU_OVERLAP: f32 = 4.0;
    /// Closest a menu comes to the window edges.
    pub const WINDOW_MARGIN: f32 = 8.0;

    /// The menu surface, a step above the sidebar it opens over. The dark popover is darker
    /// than the dark sidebar, so dark mode lifts it to the muted color.
    pub fn surface(theme: &Theme) -> Hsla {
        if theme.is_dark() {
            theme.muted
        } else {
            theme.popover
        }
    }

    /// The menu's edge, a hairline `ring` just outside it, then a soft drop shadow, all at
    /// `strength` so an opening menu can fade them in. A border would be drawn over a submenu
    /// that overlaps the menu; a shadow is drawn under it.
    pub fn shadow(ring: Hsla, strength: f32) -> Vec<BoxShadow> {
        let strength = strength.clamp(0., 1.);
        let ink = hsla(0., 0., 0., 0.1 * strength);
        vec![
            BoxShadow::new(px(0.), px(0.), ring.opacity(strength))
                .blur_radius(px(0.))
                .spread_radius(px(1.)),
            BoxShadow::new(px(0.), px(10.), ink)
                .blur_radius(px(15.))
                .spread_radius(px(-3.)),
            BoxShadow::new(px(0.), px(4.), ink)
                .blur_radius(px(6.))
                .spread_radius(px(-4.)),
        ]
    }
}

/// Applies the system font.
pub fn init(cx: &mut App) {
    Theme::update(cx, |theme| theme.font_family = system_font().into());
}

/// Native system font.
pub fn system_font() -> &'static str {
    ".SystemUIFont"
}

/// Tracks the native window appearance.
pub fn follow_window(window: &mut Window, cx: &mut App) {
    apply_mode(ThemeMode::from(window.appearance()), cx);
    window
        .observe_window_appearance(|window, cx| {
            apply_mode(ThemeMode::from(window.appearance()), cx)
        })
        .detach();
}

/// Loads the mode's colors, then applies the sidebar surfaces.
///
/// The light sidebar stays slightly darker than the main surface.
fn apply_mode(mode: ThemeMode, cx: &mut App) {
    Theme::change(mode, None, cx);
    Theme::update(cx, |theme| {
        if mode.is_dark() {
            theme.sidebar = gpui_kit::rgb(0x161616).into();
            theme.sidebar_accent = gpui_kit::rgb(0x2f2f2f).into();
            theme.sidebar_accent_foreground = gpui_kit::rgb(0xebebeb).into();
        } else {
            let background = theme.background;
            theme.sidebar = gpui_kit::Hsla {
                l: (background.l - 0.02).max(0.0),
                ..background
            };
        }
    });
}
