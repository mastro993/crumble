//! The dialog Set up and Edit open for a remote Provider: its API key and its address, and for
//! a server at a loopback address a line saying it is on this Mac. Its Models are picked in the
//! sections of their jobs. Everything in it is kept as it changes, so Done, Escape and the close
//! button only close it; a key still being checked is kept once its ping passes. It fades in
//! over the frosted backdrop every dialog uses, titled with the Provider's logo and name, with
//! Check again and Done in its footer.
//!
//! `ProviderDialog` owns the fields and the checks it runs. What the user changes goes to its
//! owner, `NativeView`, as a `ProviderDialogEvent`; the owner keeps it through `Store` and
//! `secrets` and passes the dialog what Settings now holds, `Saved`. The API key row is in
//! `key`, the address row in `address`, the checks with the saved key in `check`, and what each
//! says in `content`.

mod address;
mod check;
mod content;
mod key;
#[cfg(test)]
mod tests;

use super::super::NativeView;
use super::super::controls::{Inactive, app_icon, button};
use super::super::settings_page::form::{Group, description};
use super::logo::logo_tile;
use address::AddressField;
use check::Check;
use crumble_remote::{Listed, Provider, Remote, runs_locally};
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::dialog::{DialogEntrance, DialogFooter};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{IconName, Sizable, WindowExt, h_flex, v_flex};
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, InteractiveElement, IntoElement,
    ParentElement, Render, Role, SharedString, StatefulInteractiveElement, Styled, Subscription,
    Window, div, prelude::FluentBuilder, px,
};
use key::KeyField;
use std::cell::Cell;

/// Width of a Provider's dialog.
const WIDTH: f32 = 520.0;

/// What Settings holds of a remote Provider, as its dialog shows it.
#[derive(Clone, Debug)]
struct Saved {
    /// Whether its key is in the Keychain.
    has_key: bool,
    /// The address saved for it, "" for its usual one.
    base_url: String,
}

/// A change made in a Provider's dialog, for its owner to keep; the owner then passes the
/// dialog what is `Saved` again.
enum ProviderDialogEvent {
    /// Keep `key`, which a ping just passed listing `listed`, in the Keychain, then answer with
    /// `ProviderDialog::key_saved`.
    SaveKey { key: String, listed: Listed },
    /// Delete the saved key, then answer with `ProviderDialog::key_removed`.
    RemoveKey,
    /// Keep the address as typed so far.
    SetAddress(String),
    /// A check with the saved key, or with none, passed and listed these Models.
    Checked(Listed),
}

/// A remote Provider's dialog: its fields, and the checks running.
struct ProviderDialog {
    provider: Provider,
    remote: &'static Remote,
    /// What Settings holds of the Provider, as the owner last passed it.
    saved: Saved,
    /// The API key row; `None` for a Provider that takes no key.
    key: Option<KeyField>,
    address: AddressField,
    /// The last check with the saved key.
    check: Check,
    /// Takes the focus when the focused key field goes, as a saved key's does, so Escape still
    /// reaches the dialog and closes it.
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ProviderDialogEvent> for ProviderDialog {}

impl ProviderDialog {
    /// The dialog of `provider`, reached as `remote` says, showing what is `saved`.
    fn new(
        provider: Provider,
        remote: &'static Remote,
        saved: Saved,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut subscriptions = Vec::new();
        let key = (remote.api_key())
            .map(|format| KeyField::new(provider, format, window, cx, &mut subscriptions));
        let address = AddressField::new(remote, &saved.base_url, window, cx, &mut subscriptions);
        Self {
            provider,
            remote,
            saved,
            key,
            address,
            check: Check::Idle,
            focus: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// Shows what is `saved`, which the owner passes after each change it keeps.
    fn set_saved(&mut self, saved: Saved, cx: &mut Context<Self>) {
        self.saved = saved;
        cx.notify();
    }
}

impl Render for ProviderDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let on_this_mac = runs_locally(self.provider, &self.address(cx));
        v_flex()
            .track_focus(&self.focus)
            .gap_6()
            .child(
                Group::new("provider-connection")
                    .children(self.key_row(cx))
                    .child(self.address_row(cx)),
            )
            .when(on_this_mac, |dialog| {
                dialog.child(description("This server is on this Mac.", cx))
            })
    }
}

/// Opens `dialog` over `window`. The field the Provider most needs takes the focus: the key a
/// company needs, or the address a server needs.
fn open(dialog: Entity<ProviderDialog>, window: &mut Window, cx: &mut App) {
    let provider = dialog.read(cx).provider;
    // The builder runs on every frame the dialog is drawn; it focuses only the first time.
    let focus = Cell::new(true);
    window.open_dialog(cx, move |modal, window, cx| {
        if focus.replace(false) {
            let this = dialog.read(cx);
            let needed = (this.needed_key_field()).or_else(|| this.needed_address_field(cx));
            if let Some(input) = needed.cloned() {
                input.update(cx, |input, cx| input.focus(window, cx));
            }
        }
        modal
            .entrance(DialogEntrance::Fade)
            .w(px(WIDTH))
            // Enter checks the key or the address it is pressed in; only Done closes.
            .on_ok(|_, _, _| false)
            // GPUI Kit's close button keeps the arrow cursor, so the title draws one with a hand.
            .close_button(false)
            .title(
                h_flex()
                    .gap_3()
                    .child(logo_tile(provider, cx))
                    .child(div().flex_1().min_w_0().child(provider.name()))
                    .child(
                        button("provider-dialog-close")
                            .ghost()
                            .small()
                            .icon(IconName::Close)
                            .accessibility_label("Close")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    ),
            )
            .child(dialog.clone())
            .footer(footer(&dialog, cx))
    });
}

/// The footer: how the last check the user asked for went, Check again while a key is saved or
/// none is needed, and Done.
fn footer(dialog: &Entity<ProviderDialog>, cx: &App) -> DialogFooter {
    let this = dialog.read(cx);
    let check_again = this.checks_again().then(|| {
        let dialog = dialog.clone();
        button("provider-check-again")
            .outline()
            .small()
            .label("Check again")
            .inactive(!this.is_set_up(cx) || this.is_checking())
            .on_click(move |_, _, cx| dialog.update(cx, |dialog, cx| dialog.start_check(cx)))
    });
    DialogFooter::new()
        .child(div().flex_1().min_w_0().children(this.check_line(cx)))
        .children(check_again)
        .child(
            button("provider-done")
                .primary()
                .small()
                .label("Done")
                .on_click(|_, window, cx| window.close_dialog(cx)),
        )
}

/// `text`, read out as a status, under `id` for tests.
fn status(id: &'static str, text: impl Into<SharedString>) -> impl IntoElement {
    let text = text.into();
    div()
        .id(id)
        .role(Role::Status)
        .aria_label(text.clone())
        .child(text)
        .test_support()
}

/// The spinner beside "Checking…".
fn spinner() -> Spinner {
    Spinner::new().icon(app_icon("refresh")).xsmall()
}

impl NativeView {
    /// Opens `provider`'s dialog, which keeps each change through this view as it is made.
    pub(super) fn open_provider_dialog(
        &mut self,
        provider: Provider,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(remote) = provider.remote() else {
            return;
        };
        let saved = self.provider_saved(provider);
        let dialog = cx.new(|cx| ProviderDialog::new(provider, remote, saved, window, cx));
        cx.subscribe_in(&dialog, window, move |view, dialog, event, window, cx| {
            view.keep_provider_change(provider, dialog, event, window, cx)
        })
        .detach();
        open(dialog, window, cx);
    }

    /// What Settings holds of `provider`, for its dialog.
    fn provider_saved(&self, provider: Provider) -> Saved {
        Saved {
            has_key: self.providers.has_key(provider),
            base_url: (self.store.provider(provider.id()).base_url).unwrap_or_default(),
        }
    }

    /// Keeps what was changed in `provider`'s dialog, then passes the dialog what Settings now
    /// holds, and whether a key was saved or removed.
    fn keep_provider_change(
        &mut self,
        provider: Provider,
        dialog: &Entity<ProviderDialog>,
        event: &ProviderDialogEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (mut saved, mut removed) = (None, None);
        match event {
            ProviderDialogEvent::SaveKey { key, listed } => {
                saved = Some(self.save_key(provider, key, listed.clone(), cx));
            }
            ProviderDialogEvent::RemoveKey => removed = Some(self.remove_key(provider, cx)),
            ProviderDialogEvent::SetAddress(address) => {
                self.set_provider_address(provider, address, cx);
            }
            ProviderDialogEvent::Checked(listed) => {
                self.keep_listed(provider, listed.clone(), cx);
                // The Provider answers at its address, so a Summary Model there may backfill.
                self.summarize_unsummarized(cx);
            }
        }
        let now = self.provider_saved(provider);
        dialog.update(cx, |dialog, cx| {
            dialog.set_saved(now, cx);
            if let Some(saved) = saved {
                dialog.key_saved(saved, window, cx);
            }
            if let Some(removed) = removed {
                dialog.key_removed(removed, window, cx);
            }
        });
    }
}
