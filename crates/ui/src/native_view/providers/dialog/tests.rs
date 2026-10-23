//! UI integration tests of a Provider's dialog, opened over a stand-in for its owner: the key
//! and address rows in `key`.

mod key;

use super::key::KeyStatus;
use super::{ProviderDialog, ProviderDialogEvent, Saved, open};
use crumble_remote::Provider;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, App, AppContext, Bounds, Context, Entity, IntoElement, Render, Styled,
    Subscription, TestAppContext, WeakEntity, Window, WindowBounds, WindowOptions, div, px, size,
};

/// Where the stand-in's Provider is: nothing answers on port 9 of this Mac, so a ping fails at
/// once and nothing leaves the Mac.
const NOWHERE: &str = "http://127.0.0.1:9/v1";

/// Looks like an OpenAI project key.
const KEY: &str = "sk-proj-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// Stands in for `NativeView`, which needs the app's database, the Keychain and EventKit: holds
/// what the dialog shows as `Saved`, and counts the keys it is asked to keep and remove,
/// touching no Keychain.
struct Owner {
    /// Held weakly, as `NativeView` holds it: only the open dialog keeps it.
    dialog: WeakEntity<ProviderDialog>,
    /// The dialog until it opens.
    opening: Option<Entity<ProviderDialog>>,
    saved: Saved,
    saves: usize,
    removals: usize,
    _subscription: Subscription,
}

impl Owner {
    fn new(saved: Saved, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let provider = Provider::OpenAi;
        let remote = provider
            .remote()
            .unwrap_or_else(|| panic!("OpenAI isn't remote"));
        let dialog = cx.new(|cx| ProviderDialog::new(provider, remote, saved.clone(), window, cx));
        let subscription = cx.subscribe_in(
            &dialog,
            window,
            |owner: &mut Self, dialog, event, window, cx| match event {
                ProviderDialogEvent::SaveKey { .. } => owner.saves += 1,
                ProviderDialogEvent::RemoveKey => {
                    owner.removals += 1;
                    owner.saved.has_key = false;
                    let saved = owner.saved.clone();
                    dialog.update(cx, |dialog, cx| {
                        dialog.set_saved(saved, cx);
                        dialog.key_removed(Ok(()), window, cx);
                    });
                }
                _ => {}
            },
        );
        Self {
            dialog: dialog.downgrade(),
            opening: Some(dialog),
            saved,
            saves: 0,
            removals: 0,
            _subscription: subscription,
        }
    }
}

impl Render for Owner {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full()
    }
}

struct Harness {
    handle: AnyWindowHandle,
    owner: Entity<Owner>,
}

/// What is saved of OpenAI: its address, [`NOWHERE`], and a key when `has_key`.
fn saved(has_key: bool) -> Saved {
    Saved {
        has_key,
        base_url: NOWHERE.to_owned(),
    }
}

impl Harness {
    /// Opens OpenAI's dialog over the stand-in, as [`saved`] says.
    fn open(cx: &mut TestAppContext, has_key: bool) -> Self {
        Self::open_with(cx, saved(has_key))
    }

    /// Opens OpenAI's dialog over the stand-in, showing what is `saved`.
    fn open_with(cx: &mut TestAppContext, saved: Saved) -> Self {
        cx.update(|cx| {
            gpui_kit::init(cx);
            // The fade would otherwise stop at its first frame, as tests draw no frames on
            // their own.
            cx.set_reduce_motion(true);
        });
        let (handle, owner) = cx
            .update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            Default::default(),
                            size(px(900.), px(800.)),
                        ))),
                        ..Default::default()
                    },
                    cx,
                    |window, cx| cx.new(|cx| Owner::new(saved, window, cx)),
                )
            })
            .unwrap_or_else(|error| panic!("open test window: {error}"));
        let harness = Self { handle, owner };
        harness.step(cx, |window, cx| {
            // Focus events only reach an active window.
            window.activate_window();
            let dialog = harness.owner.update(cx, |owner, _| owner.opening.take());
            if let Some(dialog) = dialog {
                open(dialog, window, cx);
            }
        });
        harness
    }

    /// The dialog, while it is open or still checking a key.
    fn dialog(&self, cx: &App) -> Entity<ProviderDialog> {
        (self.owner.read(cx).dialog.upgrade()).unwrap_or_else(|| panic!("the dialog is gone"))
    }

    /// Draws a frame and runs `act`, whose events and the owner's answers follow as the update
    /// ends; then draws the result. Tasks, such as a ping, wait for `settle`.
    fn step(&self, cx: &mut TestAppContext, act: impl FnOnce(&mut Window, &mut App)) {
        cx.update_window(self.handle, |_, window, cx| {
            window.render_frame(cx);
            act(window, cx);
        })
        .unwrap_or_else(|error| panic!("update test window: {error}"));
        self.check(cx, |_| {});
    }

    /// Runs the Tasks waiting, such as a ping, then draws the result.
    fn settle(&self, cx: &mut TestAppContext) {
        cx.run_until_parked();
        self.check(cx, |_| {});
    }

    fn check(&self, cx: &mut TestAppContext, check: impl FnOnce(&mut Window)) {
        cx.update_window(self.handle, |_, window, cx| {
            window.render_frame(cx);
            check(window);
        })
        .unwrap_or_else(|error| panic!("update test window: {error}"));
    }

    /// What the key row says, as read out.
    fn key_status(&self, cx: &mut TestAppContext) -> Option<String> {
        let mut status = None;
        self.check(cx, |window| {
            status = (window.try_find("provider-key-status"))
                .and_then(|status| status.label().map(str::to_owned));
        });
        status
    }

    fn checking(&self, cx: &mut TestAppContext) -> bool {
        cx.update(|cx| {
            let dialog = self.dialog(cx);
            let key = dialog.read(cx).key.as_ref();
            key.is_some_and(|key| matches!(key.status, KeyStatus::Checking { .. }))
        })
    }

    fn type_key(&self, cx: &mut TestAppContext, key: &str) {
        self.step(cx, |window, cx| {
            window.click("provider-key", cx);
            window.input(key, cx);
        });
    }

    fn present(&self, cx: &mut TestAppContext, id: &'static str) -> bool {
        let mut present = false;
        self.check(cx, |window| present = window.try_find(id).is_some());
        present
    }
}
