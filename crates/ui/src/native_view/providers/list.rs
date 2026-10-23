//! The Providers section: the remote Providers set up, under Connected, above the rest, under
//! Available, each a card listing its Providers with their logo, name and what it does, each
//! with the button that opens its dialog: Set up until it has a key, if it needs one, and an
//! address requests can go to, and Edit after. Only the button acts; the row itself is not
//! clickable.

use super::super::NativeView;
use super::super::controls::button;
use super::super::settings_page::form::{Group, groups};
use super::detail::detail_line;
use super::logo::logo_tile;
use crumble_remote::Provider;
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::{
    Context, Div, FontWeight, InteractiveElement, ParentElement, Stateful, Styled, div, px,
};

/// Space above and below each row, as a setting has.
const ROW_PADDING: f32 = 12.0;
/// Size of a Provider's name.
const NAME_SIZE: f32 = 15.0;
/// Size of the line under the name.
const DETAIL_SIZE: f32 = 13.0;

impl NativeView {
    /// The Providers section: the remote Providers set up, then the rest, each in Settings'
    /// order.
    pub(in super::super) fn render_providers(&self, cx: &mut Context<Self>) -> Div {
        let (connected, available): (Vec<Provider>, Vec<Provider>) =
            (Provider::REMOTE.into_iter()).partition(|&provider| self.is_set_up(provider));
        let connected =
            self.provider_group("settings-providers-connected", "Connected", connected, cx);
        let available =
            self.provider_group("settings-providers-available", "Available", available, cx);
        groups().children(connected).children(available)
    }

    /// A card headed `heading` with a row for each of `providers`; `None` when there are none.
    fn provider_group(
        &self,
        id: &'static str,
        heading: &'static str,
        providers: Vec<Provider>,
        cx: &mut Context<Self>,
    ) -> Option<Group> {
        let rows: Vec<_> = (providers.into_iter())
            .map(|provider| self.provider_row(provider, cx))
            .collect();
        (!rows.is_empty()).then(|| Group::new(id).heading(heading).children(rows))
    }

    /// `provider`'s row: its logo, name and what it does, and Set up or Edit.
    fn provider_row(&self, provider: Provider, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = provider.id();
        let name = provider.name();
        let saved = self.store.provider(id).base_url.unwrap_or_default();
        let set_up = self.is_set_up(provider);
        let action = if set_up { "Edit" } else { "Set up" };
        h_flex()
            .id(format!("settings-provider-{id}"))
            .w_full()
            .py(px(ROW_PADDING))
            .gap_3()
            .child(logo_tile(provider, cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(NAME_SIZE))
                            .font_weight(FontWeight::MEDIUM)
                            .child(name),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(DETAIL_SIZE))
                            .text_color(cx.theme().muted_foreground)
                            .child(detail_line(provider, set_up, &saved)),
                    ),
            )
            .child(
                button(format!("settings-provider-{id}-edit"))
                    .outline()
                    .small()
                    .label(action)
                    .accessibility_label(format!("{action} {name}"))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.open_provider_dialog(provider, window, cx)
                    })),
            )
    }
}
