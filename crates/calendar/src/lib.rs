//! Calendar logic that needs no GPUI: Calendar views and their Periods, where Events fall on
//! the Calendar page, what the Agenda page lists and reads, which Events to suggest recording,
//! and the links in an Event's description. Everything here is plain data and pure functions;
//! EventKit reads stay with the caller.

pub mod agenda;
pub mod events;
pub mod links;
pub mod period;
pub mod suggestions;

use chrono::{DateTime, Local, NaiveDate, NaiveTime, TimeDelta, TimeZone};
pub use period::CalendarView;

/// The start of `date`. Where clocks skip midnight for daylight saving, as in Chile, the day
/// starts an hour later; the extra Events read are drawn only on their own dates.
pub fn local_midnight(date: NaiveDate) -> Option<DateTime<Local>> {
    let midnight = date.and_time(NaiveTime::MIN);
    Local.from_local_datetime(&midnight).earliest().or_else(|| {
        Local
            .from_local_datetime(&(midnight + TimeDelta::hours(1)))
            .earliest()
    })
}

/// Read on every use, so a window left open past midnight catches up on its next render.
pub fn today() -> NaiveDate {
    Local::now().date_naive()
}
