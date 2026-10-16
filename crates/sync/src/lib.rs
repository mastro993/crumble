//! What Crumble reads from EventKit while it runs, held in GPUI entities that views observe.
//!
//! Reads run off the main thread. Only this crate calls EventKit's read and watch functions.

mod agenda_events;
mod calendar_events;
mod calendar_sync;

pub use agenda_events::{AgendaEvents, OlderEventsRead};
pub use calendar_events::CalendarEvents;
pub use calendar_sync::CalendarSync;
