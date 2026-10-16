//! The colors an Account can take.

use serde::{Deserialize, Serialize};

/// The color of an Account's dot in the sidebar and in Settings, and of its Events: one of
/// SkillBinder's folder colors, or any color the user picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountColor {
    /// Palette gray, never picked by default.
    Gray,
    /// Palette red.
    Red,
    /// Palette orange.
    Orange,
    /// Palette yellow.
    Yellow,
    /// Palette green.
    Green,
    /// Palette blue.
    Blue,
    /// Palette purple.
    Purple,
    /// Palette pink.
    Pink,
    /// sRGB color as `0xRRGGBB`.
    Custom(u32),
}

impl AccountColor {
    /// SkillBinder's folder colors.
    pub const PALETTE: [Self; 8] = [
        Self::Gray,
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Blue,
        Self::Purple,
        Self::Pink,
    ];

    /// A color that looks random but stays the same for `id` across launches, so nothing has to
    /// be saved until the user picks one. Gray is left for the user to choose.
    pub fn default_for(id: &str) -> Self {
        // FNV-1a: tiny, and unlike std's hasher its output is fixed.
        let hash = id.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
        let colorful = &Self::PALETTE[1..];
        colorful[(hash % colorful.len() as u64) as usize]
    }

    /// The color's name, as the Color menu lists it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Gray => "Gray",
            Self::Red => "Red",
            Self::Orange => "Orange",
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::Purple => "Purple",
            Self::Pink => "Pink",
            Self::Custom(_) => "Custom",
        }
    }

    /// The `(dark, light)` sRGB `0xRRGGBB` tones the color is drawn in. Palette colors use
    /// SkillBinder's tones that keep contrast on the dark and light sidebar surfaces; a custom
    /// color is the same in both.
    pub fn tones(self) -> (u32, u32) {
        match self {
            Self::Gray => (0xa3a3a3, 0x737373),
            Self::Red => (0xf87171, 0xdc2626),
            Self::Orange => (0xfb923c, 0xea580c),
            Self::Yellow => (0xfacc15, 0xca8a04),
            Self::Green => (0x4ade80, 0x16a34a),
            Self::Blue => (0x60a5fa, 0x2563eb),
            Self::Purple => (0xc084fc, 0x9333ea),
            Self::Pink => (0xf472b6, 0xdb2777),
            Self::Custom(srgb) => (srgb, srgb),
        }
    }
}
