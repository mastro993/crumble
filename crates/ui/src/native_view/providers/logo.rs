//! Each remote Provider's logo, from `assets/lobehub/`, on a small rounded tile so names beside
//! logos of any shape line up. A mono logo is tinted with the text ink, as the app's icons are;
//! a color one keeps its colors. The OpenAI-compatible server, which stands for any server, has
//! one of the app's icons instead.

use super::super::controls::app_icon;
use crumble_remote::Provider;
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::{App, Div, ParentElement, Styled, div, img, px};

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

/// `provider`'s logo; `None` for Apple's and Local Models, which have no Providers row.
fn logo(provider: Provider) -> Option<Logo> {
    Some(match provider {
        Provider::Apple | Provider::Local => return None,
        Provider::Anthropic => Logo::Mono("logos/anthropic.svg"),
        Provider::OpenAi => Logo::Mono("logos/openai.svg"),
        Provider::Gemini => Logo::Color("logos/gemini.svg"),
        Provider::ElevenLabs => Logo::Mono("logos/elevenlabs.svg"),
        Provider::OpenRouter => Logo::Mono("logos/openrouter.svg"),
        Provider::DeepSeek => Logo::Color("logos/deepseek.svg"),
        Provider::Xai => Logo::Mono("logos/xai.svg"),
        Provider::OpenAiCompatible => Logo::AppIcon("globe"),
    })
}

/// `provider`'s logo on its tile.
pub(super) fn logo_tile(provider: Provider, cx: &App) -> Div {
    let theme = cx.theme();
    let ink = theme.foreground;
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
    match logo(provider) {
        Some(Logo::Mono(path)) => {
            tile.child(Icon::empty().path(path).size(px(LOGO_SIZE)).text_color(ink))
        }
        Some(Logo::Color(path)) => tile.child(img(path).size(px(LOGO_SIZE))),
        Some(Logo::AppIcon(name)) => tile.child(app_icon(name).size(px(LOGO_SIZE)).text_color(ink)),
        None => tile,
    }
}

#[cfg(test)]
mod tests {
    use super::{Logo, logo};
    use crate::Assets;
    use crumble_remote::Provider;
    use gpui_kit::AssetSource;

    #[test]
    fn every_remote_provider_but_the_server_has_a_logo_that_loads() {
        for provider in Provider::REMOTE {
            let path = match logo(provider) {
                Some(Logo::Mono(path) | Logo::Color(path)) => {
                    assert!(
                        path == format!("logos/{}.svg", provider.id()),
                        "{provider:?}'s logo is {path}"
                    );
                    path.to_owned()
                }
                Some(Logo::AppIcon(name)) => {
                    assert_eq!(provider, Provider::OpenAiCompatible, "{provider:?}");
                    format!("icons/{name}.svg")
                }
                None => panic!("{provider:?} has no logo"),
            };
            assert!(
                matches!(Assets.load(&path), Ok(Some(_))),
                "{path} doesn't load"
            );
        }
    }
}
