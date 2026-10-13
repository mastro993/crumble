//! Read-only access to the Accounts, Calendars, and Events macOS keeps in EventKit.
//!
//! Everything returned is plain Rust data, so callers never touch Objective-C. Other
//! platforms report [`Access::Unsupported`] and return nothing.

mod classify;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod unsupported;

use chrono::{DateTime, Local, NaiveDate};

#[cfg(target_os = "macos")]
pub use macos::{
    ChangeObserver, access, accounts, calendars, events, observe_changes, request_access,
};
#[cfg(not(target_os = "macos"))]
pub use unsupported::{
    ChangeObserver, access, accounts, calendars, events, observe_changes, request_access,
};

/// Whether Crumble may read the user's Calendars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// The user has not been asked yet.
    NotDetermined,
    /// The user refused, or a policy blocks access; only System Settings can change it.
    Denied,
    /// Crumble may read every Calendar.
    Granted,
    /// This platform has no EventKit.
    Unsupported,
}

/// The service behind an Account, guessed from what EventKit reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    /// A Google account.
    Google,
    /// An Exchange or Outlook account.
    Outlook,
    /// An iCloud account.
    Apple,
    /// Anything else, such as On My Mac, subscribed calendars, or Birthdays.
    Other,
}

/// One group of Calendars macOS keeps together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// Stable EventKit source identifier.
    pub id: String,
    /// The name shown in System Settings, often the email address.
    pub name: String,
    /// Service behind the Account.
    pub provider: Provider,
}

/// One list of Events inside an Account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Calendar {
    /// Stable EventKit calendar identifier.
    pub id: String,
    /// Identifier of the owning [`Account`].
    pub account_id: String,
    /// Display name.
    pub name: String,
    /// sRGB color as `0xRRGGBB`.
    pub color: u32,
}

/// When an Event happens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    /// Whole days with no time zone, from `first` to `last` inclusive.
    AllDay {
        /// First day.
        first: NaiveDate,
        /// Last day, the same as `first` for a one-day Event.
        last: NaiveDate,
    },
    /// A span of time in the system time zone; `end` is exclusive.
    Timed {
        /// Start time.
        start: DateTime<Local>,
        /// End time.
        end: DateTime<Local>,
    },
}

/// One occurrence of an entry in a Calendar. Declined and cancelled Events are never returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// Unique per occurrence, so each instance of a repeating Event has its own.
    pub id: String,
    /// Identifier of the owning [`Calendar`].
    pub calendar_id: String,
    /// Title, possibly empty.
    pub title: String,
    /// Date or time span.
    pub when: When,
    /// Free-text location.
    pub location: Option<String>,
    /// Link to a known meeting service, found in the URL, location, or notes.
    pub join_url: Option<String>,
}
