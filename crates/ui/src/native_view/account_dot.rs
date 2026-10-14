//! The color dot that marks an Account, and the names of its colors.

use crate::settings::AccountColor;
use crumble_theme::sidebar::{DOT_SIZE, ICON_SIZE};
use gpui_kit::component::ActiveTheme;
use gpui_kit::{App, Div, Hsla, ParentElement, Styled, div, px, rgb};

/// The dot centred in an icon-sized slot, so labels line up with icon rows.
pub(super) fn dot(color: AccountColor, cx: &App) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(ICON_SIZE))
        .child(
            div()
                .size(px(DOT_SIZE))
                .rounded_full()
                .bg(tint(color, cx.theme().mode.is_dark())),
        )
}

pub(super) fn name(color: AccountColor) -> &'static str {
    match color {
        AccountColor::Gray => "Gray",
        AccountColor::Red => "Red",
        AccountColor::Orange => "Orange",
        AccountColor::Yellow => "Yellow",
        AccountColor::Green => "Green",
        AccountColor::Blue => "Blue",
        AccountColor::Purple => "Purple",
        AccountColor::Pink => "Pink",
    }
}

/// Palette tones that keep contrast on the dark and light sidebar surfaces, from SkillBinder.
fn tint(color: AccountColor, dark: bool) -> Hsla {
    let (dark_tone, light_tone) = match color {
        AccountColor::Gray => (0xa3a3a3, 0x737373),
        AccountColor::Red => (0xf87171, 0xdc2626),
        AccountColor::Orange => (0xfb923c, 0xea580c),
        AccountColor::Yellow => (0xfacc15, 0xca8a04),
        AccountColor::Green => (0x4ade80, 0x16a34a),
        AccountColor::Blue => (0x60a5fa, 0x2563eb),
        AccountColor::Purple => (0xc084fc, 0x9333ea),
        AccountColor::Pink => (0xf472b6, 0xdb2777),
    };
    rgb(if dark { dark_tone } else { light_tone }).into()
}
