//! Checks of a Provider with the key in the Keychain, or with none for a server that takes none:
//! when its address is finished, and on Check again.

use super::super::listing::{CheckKey, ping};
use super::{ProviderDialog, ProviderDialogEvent, spinner, status};
use crumble_remote::{KeyFormat, Listed};
use gpui_kit::component::{ActiveTheme, h_flex};
use gpui_kit::{App, Context, IntoElement, ParentElement, Styled, Task, prelude::FluentBuilder};

/// How the last check with the saved key went.
pub(super) enum Check {
    /// None ran, or what it found no longer holds, as the address changed.
    Idle,
    /// Running; dropping it drops the result.
    Running {
        _task: Task<()>,
    },
    Passed,
    /// Failed, and why, for the user.
    Failed(String),
}

impl ProviderDialog {
    /// Whether the Provider has what every request needs: a key if it requires one, and an
    /// address, as typed, requests can go to.
    pub(super) fn is_set_up(&self, cx: &App) -> bool {
        (self.remote).is_set_up(self.saved.has_key, &self.typed_address(cx))
    }

    /// Whether Check again shows: a key is saved, or the Provider needs none.
    pub(super) fn checks_again(&self) -> bool {
        self.saved.has_key || !self.remote.api_key().is_some_and(KeyFormat::required)
    }

    /// Whether a check with the saved key is running.
    pub(super) fn is_checking(&self) -> bool {
        matches!(self.check, Check::Running { .. })
    }

    /// Checks the Provider at its address with the saved key, or with none when none is saved,
    /// dropping any check running.
    pub(super) fn start_check(&mut self, cx: &mut Context<Self>) {
        let key = if self.saved.has_key {
            CheckKey::Saved
        } else {
            CheckKey::NoKey
        };
        let ping = ping(self.provider, self.address(cx), key, cx);
        let task = cx.spawn(async move |dialog, cx| {
            let checked = ping.await;
            let _ = dialog.update(cx, |dialog, cx| dialog.checked(checked, cx));
        });
        self.check = Check::Running { _task: task };
        cx.notify();
    }

    /// Follows a check: the owner keeps the Models it listed.
    fn checked(&mut self, checked: Result<Listed, String>, cx: &mut Context<Self>) {
        self.check = match checked {
            Ok(listed) => {
                cx.emit(ProviderDialogEvent::Checked(listed));
                Check::Passed
            }
            Err(why) => Check::Failed(why),
        };
        cx.notify();
    }

    /// The footer's line on the last check: "Checking…", "Checked", or why it failed.
    pub(super) fn check_line(&self, cx: &App) -> Option<impl IntoElement + use<>> {
        let theme = cx.theme();
        let (text, color, running) = match &self.check {
            Check::Running { .. } => ("Checking…".to_owned(), theme.muted_foreground, true),
            Check::Passed => ("Checked".to_owned(), theme.success, false),
            Check::Failed(why) => (why.clone(), theme.danger, false),
            Check::Idle => return None,
        };
        Some(
            h_flex()
                .gap_1p5()
                .text_sm()
                .text_color(color)
                .when(running, |line| line.child(spinner()))
                .child(status("provider-check-status", text)),
        )
    }
}
