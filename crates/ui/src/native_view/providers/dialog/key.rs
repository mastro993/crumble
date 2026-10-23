//! The API key row of a Provider's dialog. With no key saved, or after Replace, a masked field:
//! what is typed is held against the Provider's key format at once, and a key that looks right
//! is pinged once typing pauses, or at once on Enter or a paste; an edit of the address holds
//! that ping until the address is finished. A key that passes goes to the owner to keep in the
//! Keychain, and the field empties; the ping holds the dialog, so a key typed just before it
//! closes is still kept. With a key saved, the row says so, with Replace and Remove. A key is
//! never shown or read back; only its ping sends it.

use super::super::super::controls::button;
use super::super::super::settings_page::form::{Setting, field};
use super::super::listing::{CheckKey, ping};
use super::check::Check;
use super::{ProviderDialog, ProviderDialogEvent, content, spinner, status};
use crumble_remote::{KeyCheck, KeyFormat, Listed, Provider};
use gpui_kit::SharedString;
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::input::{Input, InputContentType, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AppContext, Context, Entity, Focusable, IntoElement, ParentElement, Styled,
    Subscription, Task, Window, div,
};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

/// How long typing pauses before a key that looks right is pinged.
pub(super) const PAUSE: Duration = Duration::from_millis(400);

/// The API key row's field and what it says.
pub(super) struct KeyField {
    input: Entity<InputState>,
    format: &'static KeyFormat,
    /// Whether the field shows although a key is saved, after Replace; the saved key stays
    /// until a new one passes its ping.
    replacing: bool,
    pub(super) status: KeyStatus,
    /// Set by a paste, so the change it makes is pinged at once.
    pasted: Rc<Cell<bool>>,
}

/// What the API key row says of the key typed, or of the one saved. A check's Task lives in its
/// state, so any other state drops it, and its result with it; closing the dialog doesn't.
pub(super) enum KeyStatus {
    /// Nothing beyond the row's own line: the field is empty, or the saved key is as it was.
    Idle,
    /// What is typed can't be one of the Provider's keys: why.
    Invalid(String),
    /// What is typed looks like a key; it is pinged once typing pauses, or, after an edit of
    /// the address, once the address is finished.
    Waiting { _task: Task<()> },
    /// The typed key is being pinged.
    Checking { _task: Task<()> },
    /// The typed key failed its ping or couldn't be kept, or the saved one couldn't be
    /// removed: why, for the user.
    Failed(String),
    /// The typed key passed its ping and is kept in the Keychain.
    Saved,
}

impl KeyField {
    /// The key field of `provider`, whose keys look as `format` says.
    pub(super) fn new(
        provider: Provider,
        format: &'static KeyFormat,
        window: &mut Window,
        cx: &mut Context<ProviderDialog>,
        subscriptions: &mut Vec<Subscription>,
    ) -> Self {
        let placeholder = content::key_placeholder(provider, format.required());
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .masked(true)
        });
        subscriptions.push(
            cx.subscribe(&input, |dialog, _, event: &InputEvent, cx| match event {
                InputEvent::Change => dialog.key_edited(cx),
                InputEvent::PressEnter { .. } => dialog.check_key(cx),
                InputEvent::Focus | InputEvent::Blur => {}
            }),
        );
        Self {
            input,
            format,
            replacing: false,
            status: KeyStatus::Idle,
            pasted: Rc::default(),
        }
    }
}

impl ProviderDialog {
    /// The key field while the Provider needs a key and has none saved, to focus as the dialog
    /// opens.
    pub(super) fn needed_key_field(&self) -> Option<&Entity<InputState>> {
        (self.key.as_ref())
            .filter(|key| key.format.required() && !self.saved.has_key)
            .map(|key| &key.input)
    }

    /// Follows an edit of the key field, dropping any check running: says why what is typed
    /// can't be a key, or pings it once typing pauses, or at once after a paste.
    fn key_edited(&mut self, cx: &mut Context<Self>) {
        let Some(key) = &self.key else {
            return;
        };
        let pasted = key.pasted.replace(false);
        let status = match key.format.check(&key.input.read(cx).value()) {
            KeyCheck::Empty => KeyStatus::Idle,
            KeyCheck::Invalid(why) => KeyStatus::Invalid(why),
            KeyCheck::Valid if pasted => return self.check_key(cx),
            KeyCheck::Valid => {
                // Held, not weak: a key typed just before Done is still checked and kept.
                let dialog = cx.entity();
                KeyStatus::Waiting {
                    _task: cx.spawn(async move |_, cx| {
                        cx.background_executor().timer(PAUSE).await;
                        dialog.update(cx, |dialog, cx| dialog.check_key(cx));
                    }),
                }
            }
        };
        self.set_key_status(status, cx);
    }

    /// Pings the key typed at once, if it looks like one: on Enter, Try again, a paste, a pause
    /// in typing, or a finished address.
    pub(super) fn check_key(&mut self, cx: &mut Context<Self>) {
        let Some(key) = &self.key else {
            return;
        };
        let typed = key.input.read(cx).value().trim().to_owned();
        if key.format.check(&typed) != KeyCheck::Valid {
            return;
        }
        let ping = ping(
            self.provider,
            self.address(cx),
            CheckKey::Typed(typed.clone()),
            cx,
        );
        // Held, not weak: closing the dialog mid-check still keeps a key that passes.
        let dialog = cx.entity();
        let task = cx.spawn(async move |_, cx| {
            let checked = ping.await;
            dialog.update(cx, |dialog, cx| dialog.key_checked(typed, checked, cx));
        });
        self.set_key_status(KeyStatus::Checking { _task: task }, cx);
    }

    /// Follows the ping of `typed`: a key that passed goes to the owner to keep, and a failure
    /// says why, keeping the text so it can be fixed.
    fn key_checked(
        &mut self,
        typed: String,
        checked: Result<Listed, String>,
        cx: &mut Context<Self>,
    ) {
        match checked {
            Ok(listed) => cx.emit(ProviderDialogEvent::SaveKey { key: typed, listed }),
            Err(why) => self.set_key_status(KeyStatus::Failed(why), cx),
        }
    }

    /// Follows the owner keeping the key that passed its ping, or failing to: a kept key empties
    /// the field, which goes, and the row shows it saved; what Check again found about the old
    /// key, or is finding, is dropped.
    pub(super) fn key_saved(
        &mut self,
        saved: Result<(), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = &mut self.key else {
            return;
        };
        match saved {
            Ok(()) => {
                // The field goes, so the dialog takes its focus, and Escape still closes it.
                if key.input.focus_handle(cx).is_focused(window) {
                    self.focus.focus(window, cx);
                }
                key.input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                key.replacing = false;
                key.status = KeyStatus::Saved;
                self.check = Check::Idle;
            }
            Err(why) => key.status = KeyStatus::Failed(why),
        }
        cx.notify();
    }

    /// Follows the owner removing the saved key, or failing to. Once it is removed, the empty
    /// field shows, focused, and a check of the removed key, done or running, is dropped.
    pub(super) fn key_removed(
        &mut self,
        removed: Result<(), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let status = match removed {
            Ok(()) => {
                self.check = Check::Idle;
                // Remove goes with the saved key's row, so the field takes its focus.
                if let Some(key) = &self.key {
                    key.input.update(cx, |input, cx| input.focus(window, cx));
                }
                KeyStatus::Idle
            }
            Err(why) => KeyStatus::Failed(why),
        };
        self.set_key_status(status, cx);
    }

    /// Shows the field again for a new key, focused.
    fn replace_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = &mut self.key else {
            return;
        };
        key.replacing = true;
        key.status = KeyStatus::Idle;
        key.input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    /// Follows the address the key goes to changing: a check of the key being typed is dropped,
    /// and held until the address is finished, so a key never goes to an address half typed;
    /// the saved key's check no longer holds.
    pub(super) fn drop_key_check(&mut self, cx: &mut Context<Self>) {
        let Some(key) = &self.key else {
            return;
        };
        let status = match key.status {
            KeyStatus::Waiting { .. } | KeyStatus::Checking { .. } | KeyStatus::Failed(_) => {
                match key.format.check(&key.input.read(cx).value()) {
                    KeyCheck::Valid => KeyStatus::Waiting {
                        _task: Task::ready(()),
                    },
                    _ => KeyStatus::Idle,
                }
            }
            KeyStatus::Saved => KeyStatus::Idle,
            KeyStatus::Idle | KeyStatus::Invalid(_) => return,
        };
        self.set_key_status(status, cx);
    }

    fn set_key_status(&mut self, status: KeyStatus, cx: &mut Context<Self>) {
        if let Some(key) = &mut self.key {
            key.status = status;
        }
        cx.notify();
    }

    /// The API key row: the field and what it says of the key typed, or the saved key with
    /// Replace and Remove; `None` for a Provider that takes no key.
    pub(super) fn key_row(&self, cx: &mut Context<Self>) -> Option<Setting> {
        let key = self.key.as_ref()?;
        let theme = cx.theme();
        let (danger, success) = (theme.danger, theme.success);
        let colored =
            |text: &str, color| div().text_color(color).child(key_status(text.to_owned()));
        if self.saved.has_key && !key.replacing {
            let saved = match key.status {
                KeyStatus::Saved => colored("Saved in Keychain · Checked", success),
                _ => div().child(key_status("Saved in Keychain")),
            };
            let failed = match &key.status {
                KeyStatus::Failed(why) => Some(div().text_color(danger).child(why.clone())),
                _ => None,
            };
            let buttons = h_flex()
                .gap_2()
                .child(
                    button("provider-key-replace")
                        .outline()
                        .small()
                        .label("Replace")
                        .on_click(
                            cx.listener(|dialog, _, window, cx| dialog.replace_key(window, cx)),
                        ),
                )
                .child(
                    button("provider-key-remove")
                        .outline()
                        .small()
                        .label("Remove")
                        .on_click(
                            cx.listener(|_, _, _, cx| cx.emit(ProviderDialogEvent::RemoveKey)),
                        ),
                );
            return Some(
                Setting::new("API key")
                    .description(saved)
                    .descriptions(failed)
                    .control(buttons),
            );
        }
        let line: Option<AnyElement> = match &key.status {
            // A key typed, waiting for typing to pause, says nothing yet.
            KeyStatus::Waiting { .. } => None,
            KeyStatus::Idle => self.remote.key_url().map(|url| {
                button("provider-key-get")
                    .link()
                    .small()
                    .label("Get an API key")
                    .on_click(move |_, _, cx| cx.open_url(url))
                    .into_any_element()
            }),
            KeyStatus::Invalid(why) => Some(colored(why, danger).into_any_element()),
            KeyStatus::Checking { .. } => Some(
                h_flex()
                    .gap_1p5()
                    .child(spinner())
                    .child(key_status("Checking…"))
                    .into_any_element(),
            ),
            KeyStatus::Failed(why) => Some(
                v_flex()
                    .items_start()
                    .gap_1()
                    .child(colored(why, danger))
                    .child(
                        button("provider-key-try-again")
                            .outline()
                            .xsmall()
                            .label("Try again")
                            .on_click(cx.listener(|dialog, _, window, cx| {
                                // Try again goes while the key is checked, so the field, which
                                // holds the key, takes its focus.
                                if let Some(key) = &dialog.key {
                                    key.input.update(cx, |input, cx| input.focus(window, cx));
                                }
                                dialog.check_key(cx);
                            })),
                    )
                    .into_any_element(),
            ),
            KeyStatus::Saved => {
                Some(colored("Saved in Keychain · Checked", success).into_any_element())
            }
        };
        let pasted = key.pasted.clone();
        let input = Input::new(&key.input)
            .id("provider-key")
            .small()
            .content_type(InputContentType::Password)
            .on_paste(move |_, _, _| {
                pasted.set(true);
                // The field inserts the text itself.
                false
            });
        Some(
            Setting::new("API key")
                .descriptions(line)
                .control(field(input)),
        )
    }
}

/// What the row says of the key, read out as a status.
fn key_status(text: impl Into<SharedString>) -> impl IntoElement {
    status("provider-key-status", text)
}
