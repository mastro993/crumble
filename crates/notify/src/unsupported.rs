//! Platforms without `UNUserNotificationCenter`: nothing posts.

use crate::{Response, Status, Suggestion};

/// Ignores the handler, as no notification is ever clicked.
pub fn set_handler(_handler: impl Fn(Response) + Send + Sync + 'static) {}

/// Always [`Status::Denied`].
pub fn authorization(done: impl FnOnce(Status) + Send + 'static) {
    done(Status::Denied);
}

/// Never asks, and answers `false`.
pub fn authorize(done: impl FnOnce(bool) + Send + 'static) {
    done(false);
}

/// Posts nothing.
pub fn post(_suggestion: &Suggestion) {}
