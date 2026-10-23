//! Each Provider's logo, from `assets/lobehub/`: on a small rounded tile in the Providers list,
//! so names beside logos of any shape line up, and alone before its name in a Provider select's
//! row. A mono logo is tinted with the text ink, as the app's icons are; a color one keeps its
//! colors. The OpenAI-compatible server, which stands for any server, and Local Models, which
//! Crumble runs, have one of the app's icons instead.

use super::super::controls::app_icon;
use crumble_remote::Provider;
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::{
    AnyElement, App, Div, Hsla, IntoElement, ParentElement, Pixels, Styled, div, img, px,
};

/// Side of the tile a logo sits on.
const TILE_SIZE: f32 = 28.0;
/// Corner radius of the tile.
const TILE_RADIUS: f32 = 7.0;
/// Side of the logo on its tile.
const LOGO_SIZE: f32 = 20.0;

/// How a Provider's logo is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Logo {
    /// One color, `fill="currentColor"`, tinted with the text ink: the asset at this path.
    Mono(&'static str),
    /// Its own colors, drawn as an image: the asset at this path.
    Color(&'static str),
    /// One of the app's icons, by name, for a Provider with no logo of its own.
    AppIcon(&'static str),
}

/// `provider`'s logo.
fn logo(provider: Provider) -> Logo {
    match provider {
        Provider::Apple => Logo::Mono("logos/apple.svg"),
        Provider::Anthropic => Logo::Mono("logos/anthropic.svg"),
        Provider::OpenAi => Logo::Mono("logos/openai.svg"),
        Provider::Gemini => Logo::Color("logos/gemini.svg"),
        Provider::ElevenLabs => Logo::Mono("logos/elevenlabs.svg"),
        Provider::OpenRouter => Logo::Mono("logos/openrouter.svg"),
        Provider::DeepSeek => Logo::Color("logos/deepseek.svg"),
        Provider::Xai => Logo::Mono("logos/xai.svg"),
        Provider::OpenAiCompatible => Logo::AppIcon("globe"),
        // The icon of the Local Models section.
        Provider::Local => Logo::AppIcon("chip"),
    }
}

/// `provider`'s logo, `size` wide, a mono one tinted with `ink`.
fn mark(provider: Provider, size: Pixels, ink: Hsla) -> AnyElement {
    match logo(provider) {
        Logo::Mono(path) => (Icon::empty().path(path).size(size).flex_shrink_0())
            .text_color(ink)
            .into_any_element(),
        Logo::Color(path) => img(path).size(size).flex_shrink_0().into_any_element(),
        Logo::AppIcon(name) => (app_icon(name).size(size).flex_shrink_0())
            .text_color(ink)
            .into_any_element(),
    }
}

/// `provider`'s logo alone, `size` wide, as a menu row leads with an icon.
pub(in super::super) fn provider_logo(provider: Provider, size: f32, cx: &App) -> AnyElement {
    mark(provider, px(size), cx.theme().foreground)
}

/// `provider`'s logo on its tile.
pub(super) fn logo_tile(provider: Provider, cx: &App) -> Div {
    let theme = cx.theme();
    let tile = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(TILE_SIZE))
        .rounded(px(TILE_RADIUS))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background);
    tile.child(mark(provider, px(LOGO_SIZE), theme.foreground))
}

#[cfg(test)]
mod tests {
    use super::{Logo, logo};
    use crate::Assets;
    use crumble_remote::Provider;
    use gpui_kit::AssetSource;

    #[test]
    fn every_provider_has_a_logo_that_loads() {
        for provider in Provider::ALL {
            let path = match logo(provider) {
                Logo::Mono(path) | Logo::Color(path) => {
                    assert!(
                        path == format!("logos/{}.svg", provider.id()),
                        "{provider:?}'s logo is {path}"
                    );
                    path.to_owned()
                }
                Logo::AppIcon(name) => {
                    assert!(
                        matches!(provider, Provider::OpenAiCompatible | Provider::Local),
                        "{provider:?} has no logo of its own"
                    );
                    format!("icons/{name}.svg")
                }
            };
            assert!(
                matches!(Assets.load(&path), Ok(Some(_))),
                "{path} doesn't load"
            );
        }
    }
}
