//! Suggestions: macOS notifications that offer to record an Event as it starts, with a
//! "Record" action, through `UNUserNotificationCenter` (ADR 0009).
//!
//! The caller decides what to suggest and what a click does; this crate only posts and hears
//! back. A notification's identifier is the caller's key, so a click that relaunches Crumble
//! still names its Event. Outside an app bundle, as in tests, and on other platforms, nothing
//! posts and authorization reads as denied.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod unsupported;

#[cfg(target_os = "macos")]
pub use macos::{authorization, authorize, post, set_handler};
#[cfg(not(target_os = "macos"))]
pub use unsupported::{authorization, authorize, post, set_handler};

/// Whether Crumble may show notifications.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The user has not been asked yet.
    NotDetermined,
    /// The user turned them off; only System Settings can change it.
    Denied,
    /// Notifications show.
    Allowed,
}

/// A notification to post: the Event's title, and the key a click hands back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    /// Identifies the notification and comes back in its [`Response`]; posting the same key
    /// again replaces the earlier notification.
    pub key: String,
    /// The notification's title.
    pub title: String,
}

/// What the user clicked on a notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// The notification itself.
    Open,
    /// Its "Record" button.
    Record,
}

/// A click on a posted notification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    /// The [`Suggestion::key`] it was posted with.
    pub key: String,
    /// What was clicked.
    pub action: Action,
}

/// The body every Suggestion shows under its title.
const BODY: &str = "Starting now. Record it in Crumble?";
/// The "Record" button's identifier, and its category's.
const RECORD: &str = "record";
