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
    /// Corner radius of the selection fill.
    pub const RADIUS: f32 = 8.0;
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
