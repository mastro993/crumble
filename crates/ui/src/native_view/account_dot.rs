//! The color dot and checkbox that mark an Account, and the names and tones of its colors.

use crate::settings::AccountColor;
use crumble_theme::sidebar::{CHECK_RADIUS, CHECK_SIZE, DOT_SIZE, ICON_SIZE};
use gpui_kit::component::{ActiveTheme, Icon, IconName};
use gpui_kit::{App, Div, Hsla, ParentElement, Rgba, Styled, div, prelude::FluentBuilder, px, rgb};

/// The dot centred in an icon-sized slot, so labels line up with icon rows.
pub(super) fn dot(color: AccountColor, cx: &App) -> Div {
    slot().child(div().size(px(DOT_SIZE)).rounded_full().bg(tint(color, cx)))
}

/// A checkbox filled with the Account's color while `checked`, else outlined in it, centred in
/// an icon-sized slot as the dot is. It only draws the box; the caller owns the toggle.
pub(super) fn check(color: AccountColor, checked: bool, cx: &App) -> Div {
    let tint = tint(color, cx);
    slot().child(
        div()
            .flex()
            .items_center()
            .justify_center()
            .size(px(CHECK_SIZE))
            .rounded(px(CHECK_RADIUS))
            .border_1()
            .border_color(tint)
            .when(checked, |check| {
                // The mark takes the sidebar's color, so it reads as cut out of the fill.
                check.bg(tint).child(
                    Icon::new(IconName::Check)
                        .size(px(CHECK_SIZE - 4.0))
                        .text_color(cx.theme().sidebar),
                )
            }),
    )
}

/// An empty ring in the dot's place, for a color not picked yet.
pub(super) fn ring(cx: &App) -> Div {
    slot().child(
        div()
            .size(px(DOT_SIZE))
            .rounded_full()
            .border_1()
            .border_color(cx.theme().muted_foreground),
    )
}

/// An icon-sized slot that centres its child.
fn slot() -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(ICON_SIZE))
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
        AccountColor::Custom(_) => "Custom",
    }
}

/// How `color` is drawn in the current appearance. Palette colors use SkillBinder's tones that
/// keep contrast on the dark and light sidebar surfaces; a custom color is the same in both.
pub(super) fn tint(color: AccountColor, cx: &App) -> Hsla {
    let (dark_tone, light_tone) = match color {
        AccountColor::Gray => (0xa3a3a3, 0x737373),
        AccountColor::Red => (0xf87171, 0xdc2626),
        AccountColor::Orange => (0xfb923c, 0xea580c),
        AccountColor::Yellow => (0xfacc15, 0xca8a04),
        AccountColor::Green => (0x4ade80, 0x16a34a),
        AccountColor::Blue => (0x60a5fa, 0x2563eb),
        AccountColor::Purple => (0xc084fc, 0x9333ea),
        AccountColor::Pink => (0xf472b6, 0xdb2777),
        AccountColor::Custom(srgb) => (srgb, srgb),
    };
    rgb(if cx.theme().mode.is_dark() {
        dark_tone
    } else {
        light_tone
    })
    .into()
}

/// `color` as an opaque sRGB `0xRRGGBB`; any transparency is dropped.
pub(super) fn srgb(color: Hsla) -> u32 {
    let Rgba { r, g, b, .. } = color.into();
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    channel(r) << 16 | channel(g) << 8 | channel(b)
}

#[cfg(test)]
mod tests {
    use super::srgb;
    use gpui_kit::{Hsla, hsla, rgb};

    #[test]
    fn a_color_converts_back_to_the_srgb_it_came_from() {
        for value in [0x000000, 0xffffff, 0x3366ff, 0xdc2626, 0x0a0b0c] {
            assert_eq!(srgb(Hsla::from(rgb(value))), value);
        }
        // Transparency is dropped.
        assert_eq!(srgb(hsla(0.0, 1.0, 0.5, 0.2)), 0xff0000);
    }
}
