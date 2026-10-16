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
    /// The name shown in System Settings, such as "iCloud" or the email address.
    pub name: String,
    /// The user's email address in this Account, when EventKit gives it away. EventKit has no
    /// email field: it is the user's own address on a recent Event, or else the one Calendar named
    /// after an address, as Google names an Account's main Calendar.
    pub email: Option<String>,
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
    /// Unique per occurrence, so each instance of a repeating Event has its own: the calendar
    /// item's identifier, `@`, then the start as a Unix timestamp.
    pub id: String,
    /// Identifier of the owning [`Calendar`].
    pub calendar_id: String,
    /// Title, possibly empty.
    pub title: String,
    /// Date or time span.
    pub when: When,
    /// Free-text location.
    pub location: Option<String>,
    /// The notes as plain text, without meeting links or the meeting details Google and Teams
    /// add, since [`Event::join`] stands for them. A link whose text is not its address, as
    /// Google sends one from the web, is written `[text](address)`.
    pub description: Option<String>,
    /// Meeting link of a known service, found in the URL, location, or notes.
    pub join: Option<Join>,
    /// The organizer first, then the other attendees; empty when the Event has no attendees.
    pub participants: Vec<Participant>,
}

impl Event {
    /// Unique per occurrence in one run: the calendar item's identifier, `@`, then the start
    /// as a Unix timestamp. [`Event::item_id`] reads the item back; do not persist this, and
    /// do not use it to follow an Event after it moves.
    pub fn occurrence_id(item_id: &str, start: DateTime<Local>) -> String {
        format!("{item_id}@{}", start.timestamp())
    }

    /// The calendar item this occurrence belongs to: every occurrence of a repeating Event
    /// shares it, and it stays when the Event moves, while [`Event::id`] also holds the start.
    pub fn item_id(&self) -> &str {
        item_id(&self.id)
    }
}

fn item_id(occurrence_id: &str) -> &str {
    occurrence_id
        .rsplit_once('@')
        .map_or(occurrence_id, |(item, _)| item)
}

/// A link that opens a video meeting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Join {
    /// The link, starting with `https://`.
    pub url: String,
    /// The service's name, such as "Google Meet".
    pub service: &'static str,
}

/// Someone invited to an Event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Participant {
    /// Their name, else their email address.
    pub name: String,
    /// Their answer to the invitation.
    pub response: Response,
    /// Whether they organize the Event.
    pub organizer: bool,
    /// Whether they are the user.
    pub you: bool,
}

/// A participant's answer to an invitation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Response {
    /// Going.
    Accepted,
    /// Maybe going.
    Tentative,
    /// Not going.
    Declined,
    /// Passed the invitation on to someone else.
    Delegated,
    /// No answer yet.
    Pending,
}

#[cfg(test)]
mod tests {
    use super::{Event, When, item_id};
    use chrono::{Local, TimeDelta, TimeZone};

    fn start() -> chrono::DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 10, 16, 9, 0, 0)
            .earliest()
            .unwrap_or_default()
    }

    fn event_with_id(id: String) -> Event {
        Event {
            id,
            calendar_id: "calendar".into(),
            title: "Standup".into(),
            when: When::Timed {
                start: start(),
                end: start() + TimeDelta::minutes(15),
            },
            location: None,
            description: None,
            join: None,
            participants: Vec::new(),
        }
    }

    #[test]
    fn occurrence_id_embeds_the_start_and_item_id_strips_it() {
        let morning = start();
        let later = morning + TimeDelta::hours(1);
        let first = Event::occurrence_id("item", morning);
        let moved = Event::occurrence_id("item", later);
        assert_ne!(first, moved);
        assert_eq!(item_id(&first), "item");
        assert_eq!(item_id(&moved), "item");
        assert_eq!(event_with_id(first).item_id(), "item");
        // An item identifier that itself contains `@` still round-trips: the start is the last
        // `@` segment.
        let nested = Event::occurrence_id("foo@bar", morning);
        assert_eq!(item_id(&nested), "foo@bar");
    }
}
