//! The address row of a Provider's dialog: where its requests go, kept by the owner as it is
//! typed, empty for the Provider's usual address. Once it is finished, with Enter or by leaving
//! the field after a change, the Provider is checked again there, and so is a key being typed;
//! nothing is sent to an address half typed.

use super::super::super::settings_page::form::{Setting, field};
use super::check::Check;
use super::{ProviderDialog, ProviderDialogEvent, content};
use crumble_remote::Remote;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::{
    App, AppContext, Context, Entity, ParentElement, SharedString, Styled, Subscription, Window,
    div,
};

/// The address row's field.
pub(super) struct AddressField {
    input: Entity<InputState>,
    /// Whether it changed since it was last finished.
    changed: bool,
}

impl AddressField {
    /// The address field of `remote`, showing `saved`, the address saved for it, "" for its
    /// usual one.
    pub(super) fn new(
        remote: &Remote,
        saved: &str,
        window: &mut Window,
        cx: &mut Context<ProviderDialog>,
        subscriptions: &mut Vec<Subscription>,
    ) -> Self {
        let placeholder = content::address_placeholder(remote);
        let saved = saved.to_owned();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .default_value(saved)
        });
        subscriptions.push(cx.subscribe(
            &input,
            |dialog, input, event: &InputEvent, cx| match event {
                InputEvent::Change => {
                    let typed = input.read(cx).value().to_string();
                    dialog.address_edited(typed, cx);
                }
                InputEvent::PressEnter { .. } => dialog.address_finished(true, cx),
                InputEvent::Blur => dialog.address_finished(false, cx),
                InputEvent::Focus => {}
            },
        ));
        Self {
            input,
            changed: false,
        }
    }
}

impl ProviderDialog {
    /// The address field's text.
    pub(super) fn typed_address(&self, cx: &App) -> SharedString {
        self.address.input.read(cx).value()
    }

    /// The address requests go to: the one typed, else the Provider's usual one; "" for a
    /// server not given one yet.
    pub(super) fn address(&self, cx: &App) -> String {
        (self.remote.base_url(&self.typed_address(cx))).to_owned()
    }

    /// The address field while it is empty for a server, which has no usual address, to focus
    /// as the dialog opens.
    pub(super) fn needed_address_field(&self, cx: &App) -> Option<&Entity<InputState>> {
        let needed = self.remote.default_base_url().is_none() && self.typed_address(cx).is_empty();
        needed.then_some(&self.address.input)
    }

    /// Keeps the address as typed, and drops what was checked at the old one; a key being
    /// typed waits for the address to be finished.
    fn address_edited(&mut self, typed: String, cx: &mut Context<Self>) {
        self.address.changed = true;
        self.check = Check::Idle;
        cx.emit(ProviderDialogEvent::SetAddress(typed));
        self.drop_key_check(cx);
        cx.notify();
    }

    /// Checks a key being typed at a finished address, on Enter or on leaving the field after a
    /// change, and the Provider too when it has a key or needs none.
    fn address_finished(&mut self, entered: bool, cx: &mut Context<Self>) {
        if !entered && !self.address.changed {
            return;
        }
        self.address.changed = false;
        self.check_key(cx);
        if self.is_set_up(cx) {
            self.start_check(cx);
        }
    }

    /// The address row: the field, what an empty one means, and why requests can't go to what
    /// is typed.
    pub(super) fn address_row(&self, cx: &App) -> Setting {
        let error = content::address_error(&self.typed_address(cx));
        let danger = cx.theme().danger;
        Setting::new("Address")
            .description(content::address_description(self.remote))
            .descriptions(error.map(|why| div().text_color(danger).child(why)))
            .control(field(
                Input::new(&self.address.input)
                    .id("provider-address")
                    .small(),
            ))
    }
}
