//! Agent automation through the gpui-mcp server, only in `--features mcp` builds.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{App, Window};
use gpui_mcp::{AppId, BridgeConfig, BridgeHandle};

/// Matches the `--app-id` the server is started with in `.mcp.json`.
const APP_ID: &str = "crumble";

/// Keeps the window's bridge alive until the app quits; dropping it stops the bridge.
#[derive(Clone)]
pub(crate) struct Bridge(Rc<RefCell<Option<BridgeHandle>>>);

impl Bridge {
    pub(crate) fn new(cx: &mut App) -> Self {
        let bridge = Self(Rc::default());
        let slot = bridge.0.clone();
        cx.on_app_quit(move |_| {
            if let Some(handle) = slot.borrow_mut().take() {
                handle.shutdown();
            }
            async {}
        })
        .detach();
        bridge
    }

    /// A failure is logged and the app runs without automation.
    pub(crate) fn install(&self, window: &mut Window, cx: &mut App) {
        let installed = AppId::new(APP_ID)
            .map_err(|error| error.to_string())
            .and_then(|id| {
                BridgeHandle::install(window, cx, BridgeConfig::new(id, "Crumble"))
                    .map_err(|error| error.to_string())
            });
        match installed {
            Ok(handle) => *self.0.borrow_mut() = Some(handle),
            Err(error) => eprintln!("Could not start the gpui-mcp bridge: {error}"),
        }
    }
}
