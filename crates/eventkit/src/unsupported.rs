//! Platforms without EventKit: no access and no data.

use crate::{Access, Account, Calendar, Event};
use chrono::{DateTime, Local};

/// Does nothing on this platform.
pub struct ChangeObserver;

/// Always [`Access::Unsupported`].
pub fn access() -> Access {
    Access::Unsupported
}

/// Calls `done(false)` right away.
pub fn request_access(done: impl FnOnce(bool) + Send + 'static) {
    done(false);
}

/// Always empty.
pub fn accounts() -> anyhow::Result<Vec<Account>> {
    Ok(Vec::new())
}

/// Always empty.
pub fn calendars() -> anyhow::Result<Vec<Calendar>> {
    Ok(Vec::new())
}

/// Always empty.
pub fn events(
    _start: DateTime<Local>,
    _end: DateTime<Local>,
    _calendar_ids: &[String],
) -> anyhow::Result<Vec<Event>> {
    Ok(Vec::new())
}

/// Never calls `on_change`.
pub fn observe_changes(_on_change: impl Fn() + Send + Sync + 'static) -> ChangeObserver {
    ChangeObserver
}
