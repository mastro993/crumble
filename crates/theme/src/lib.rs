//! Application appearance and typography.

use std::rc::Rc;

use gpui_kit::component::{Theme, ThemeConfig, ThemeConfigColors, ThemeMode};
use gpui_kit::{App, BoxShadow, Hsla, SharedString, Window, hsla, px};

/// Height of the window header row.
pub const HEADER_HEIGHT: f32 = 42.0;

/// Top-left of the 14 pt macOS traffic lights, vertically centred in the header row.
pub const TRAFFIC_LIGHTS_ORIGIN: (f32, f32) = (16.0, (HEADER_HEIGHT - 14.0) / 2.0);

/// Horizontal padding of the main column, shared by the header and the page so their
/// edges line up.
pub const CONTENT_PADDING: f32 = 24.0;

/// Corner radius of large content surfaces such as the Calendar grids, one tier softer than
/// the theme's `radius_lg` controls they sit beside.
pub const SURFACE_RADIUS: f32 = 12.0;

/// The page panel's shadow over the header and rail: a soft wide blur and a tighter one
/// beneath it, so the panel reads as raised above the chrome.
pub fn panel_shadow() -> Vec<BoxShadow> {
    vec![
        BoxShadow::new(px(0.), px(1.), hsla(0., 0., 0., 0.04))
            .blur_radius(px(8.))
            .spread_radius(px(0.)),
        BoxShadow::new(px(0.), px(0.), hsla(0., 0., 0., 0.03))
            .blur_radius(px(2.))
            .spread_radius(px(0.)),
    ]
}

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

/// Rail metrics in points: the icon column always on the window's leading edge.
pub mod rail {
    /// Width of the rail.
    pub const WIDTH: f32 = 52.0;
    /// Side of a rail button, centred in the rail.
    pub const BUTTON_SIZE: f32 = 36.0;
    /// Corner radius of a rail button's fill.
    pub const RADIUS: f32 = 10.0;
    /// Rail button icon size.
    pub const ICON_SIZE: f32 = 20.0;
    /// Space between rail buttons, and above the first and below the last.
    pub const GAP: f32 = 8.0;
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
    /// Space above and below a row with a description.
    pub const DESCRIPTION_PADDING: f32 = 6.0;
    /// Tallest a select's menu is drawn before its rows scroll.
    pub const MAX_HEIGHT: f32 = 320.0;
    /// Space between a select and the menu under it.
    pub const SELECT_GAP: f32 = 6.0;

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

/// The hairline between the rows of a card: the ink at 6%, or 8% in dark mode.
pub fn divider(theme: &Theme) -> Hsla {
    theme
        .foreground
        .opacity(if theme.is_dark() { 0.08 } else { 0.06 })
}

/// Unselected icons and hints, a step below secondary text: the second ink at about 55%, or
/// 63% in dark mode.
pub fn faint_foreground(theme: &Theme) -> Hsla {
    theme.muted_foreground.opacity(0.76)
}

/// Applies the system font and installs the light and dark palettes, which
/// [`follow_window`] loads.
pub fn init(cx: &mut App) {
    Theme::update(cx, |theme| {
        theme.font_family = system_font().into();
        theme.light_theme = Rc::new(palette(ThemeMode::Light));
        theme.dark_theme = Rc::new(palette(ThemeMode::Dark));
    });
}

/// Native system font.
pub fn system_font() -> &'static str {
    ".SystemUIFont"
}

/// Tracks the native window appearance.
pub fn follow_window(window: &mut Window, cx: &mut App) {
    Theme::change(window.appearance(), None, cx);
    window
        .observe_window_appearance(|window, cx| Theme::change(window.appearance(), None, cx))
        .detach();
}

/// The warm palette: cream surfaces and brown ink. Light mode steps from the shell behind the
/// rail to a lighter page, with cards a hair above it; the sidebar row fill sits a step below
/// its column, as the rail fill does below the shell. Dark mode puts cream ink on warm
/// charcoals, stepping up from the page through cards to the shell, with one fill above them
/// for rows, selections and borders. Colors left out are derived by gpui-kit from these.
fn palette(mode: ThemeMode) -> ThemeConfig {
    const CREAM: &str = "#faf5ef";
    const ESPRESSO: &str = "#1f150a";
    const BROWN: &str = "#3e2b13";
    const SHELL: &str = "#f7f0e7";
    const PAGE: &str = "#fefcf9";
    const CARD: &str = "#ffffff";
    const FILL: &str = "#f2e5d5";
    const ROW_FILL: &str = "#f4eadd";
    const DARK_PAGE: &str = "#0d0a07";
    const DARK_CARD: &str = "#201b15";
    const DARK_SIDEBAR: &str = "#221c15";
    const DARK_FILL: &str = "#3a3228";

    // Secondary text and icons, focus rings, and text selection are tints of the second ink,
    // the last at a fifth strength. Light secondary text is at 72%, which keeps 4.5:1 on the
    // fills, and its focus ring at 54% clears 3:1 on them. Dark secondary text is at 83% and
    // its ring at half.
    // The edge of cards and controls is the ink at 6%, or 7% in dark mode.
    let (background, foreground, sidebar, row_fill, fill, card, primary, inks, edge) =
        if mode.is_dark() {
            let inks = ["#f0e2d0d3", "#f0e2d080", "#f0e2d033"];
            // One fill for sidebar rows, the rail, the Agenda list and borders.
            (
                DARK_PAGE,
                CREAM,
                DARK_SIDEBAR,
                DARK_FILL,
                DARK_FILL,
                DARK_CARD,
                CREAM,
                inks,
                "#faf5ef12",
            )
        } else {
            let inks = ["#3e2b13b8", "#3e2b138a", "#3e2b1333"];
            (
                PAGE,
                ESPRESSO,
                SHELL,
                ROW_FILL,
                FILL,
                CARD,
                BROWN,
                inks,
                "#1f150a0f",
            )
        };
    let [muted_text, ring, selection] = inks;
    // The backdrop behind a dialog: the page at 70%, a frosted wash in place of a blur, which
    // GPUI cannot draw.
    let overlay = if mode.is_dark() {
        "#14100cb3"
    } else {
        "#fcf8f3b3"
    };
    let color = |value: &str| Some(SharedString::new(value));
    // Built field by field: the struct has private fields, so `..Default::default()` is out.
    let mut colors = ThemeConfigColors::default();
    colors.background = color(background);
    colors.foreground = color(foreground);
    colors.sidebar = color(sidebar);
    colors.sidebar_accent = color(row_fill);
    // Cards inside pages, such as the Note card and the Year view's months.
    colors.group_box = color(card);
    colors.primary = color(primary);
    colors.primary_foreground = color(background);
    colors.secondary = color(fill);
    colors.muted = color(fill);
    colors.muted_foreground = color(muted_text);
    colors.border = color(fill);
    // gpui-kit draws its fields' and buttons' edges in `input`, and Crumble its cards' and
    // selects'.
    colors.input = color(edge);
    // gpui-kit derives button fills from `input`; keep them on the fill.
    let tint = |alpha: &str| color(&format!("{fill}{alpha}"));
    colors.button_hover = tint("80");
    colors.button_active = tint("b3");
    if mode.is_dark() {
        colors.button = tint("4d");
    }
    colors.ring = color(ring);
    colors.selection = color(selection);
    colors.overlay = color(overlay);
    ThemeConfig {
        name: "Crumble".into(),
        mode,
        colors,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::palette;
    use gpui_kit::Hsla;
    use gpui_kit::component::{Theme, ThemeMode, try_parse_color};
    use std::rc::Rc;

    fn theme(mode: ThemeMode) -> Theme {
        let mut theme = Theme::default();
        theme.apply_config(&Rc::new(palette(mode)));
        theme
    }

    #[test]
    fn every_color_parses() {
        // GPUI Kit silently falls back to its own color for a value it cannot parse.
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let colors = palette(mode).colors;
            let values = [
                colors.background,
                colors.foreground,
                colors.sidebar,
                colors.sidebar_accent,
                colors.group_box,
                colors.primary,
                colors.primary_foreground,
                colors.secondary,
                colors.muted,
                colors.muted_foreground,
                colors.border,
                colors.input,
                colors.button,
                colors.button_hover,
                colors.button_active,
                colors.ring,
                colors.selection,
                colors.overlay,
            ];
            for value in values.into_iter().flatten() {
                assert!(try_parse_color(&value).is_ok(), "{mode:?}: {value}");
            }
        }
    }

    #[test]
    fn surfaces_keep_their_order() {
        let light = theme(ThemeMode::Light);
        let lightening = [
            light.secondary,
            light.sidebar_accent,
            light.sidebar,
            light.background,
            light.group_box,
        ];
        assert!(lightening.windows(2).all(|pair| pair[0].l < pair[1].l));
        let dark = theme(ThemeMode::Dark);
        let lightening = [
            dark.background,
            dark.group_box,
            dark.sidebar,
            dark.secondary,
        ];
        assert!(lightening.windows(2).all(|pair| pair[0].l < pair[1].l));
    }

    /// WCAG's relative luminance of an opaque color.
    fn luminance(color: Hsla) -> f32 {
        let rgb = color.to_rgb();
        let linear = |channel: f32| {
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(rgb.r) + 0.7152 * linear(rgb.g) + 0.0722 * linear(rgb.b)
    }

    #[test]
    fn secondary_text_keeps_4_5_to_1_on_every_surface() {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let theme = theme(mode);
            let surfaces = [
                theme.background,
                theme.group_box,
                theme.sidebar,
                theme.sidebar_accent,
                theme.secondary,
            ];
            for surface in surfaces {
                let text = luminance(surface.blend(theme.muted_foreground));
                let surface_luminance = luminance(surface);
                let contrast =
                    (text.max(surface_luminance) + 0.05) / (text.min(surface_luminance) + 0.05);
                assert!(contrast >= 4.5, "{mode:?}: {contrast:.2} on {surface:?}");
            }
        }
    }
}
