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
    midnight_in(date, Local)
}

fn midnight_in<Tz: TimeZone>(date: NaiveDate, tz: Tz) -> Option<DateTime<Tz>> {
    let midnight = date.and_time(NaiveTime::MIN);
    tz.from_local_datetime(&midnight).earliest().or_else(|| {
        tz.from_local_datetime(&(midnight + TimeDelta::hours(1)))
            .earliest()
    })
}

/// Read on every use, so a window left open past midnight catches up on its next render.
pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

#[cfg(test)]
mod tests {
    use super::midnight_in;
    use chrono::{NaiveDate, NaiveTime, TimeZone};

    fn skipped_midnight() -> (chrono_tz::Tz, NaiveDate) {
        use chrono_tz::{Africa, America, Tz};
        const ZONES: [Tz; 4] = [
            America::Santiago,
            America::Asuncion,
            America::Sao_Paulo,
            Africa::Cairo,
        ];
        for tz in ZONES {
            for year in 2010..=2024 {
                let days = if NaiveDate::from_yo_opt(year, 366).is_some() {
                    366
                } else {
                    365
                };
                for ordinal in 1..=days {
                    let Some(date) = NaiveDate::from_yo_opt(year, ordinal) else {
                        continue;
                    };
                    let midnight = date.and_time(NaiveTime::MIN);
                    if tz.from_local_datetime(&midnight).earliest().is_none() {
                        return (tz, date);
                    }
                }
            }
        }
        panic!("no test timezone skipped local midnight in 2010–2024");
    }

    #[test]
    fn midnight_in_survives_a_skipped_local_midnight() {
        let (tz, date) = skipped_midnight();
        let start = midnight_in(date, tz).unwrap_or_else(|| {
            panic!("{tz} {date} skipped midnight and the fallback hour was also missing")
        });
        assert_eq!(start.date_naive(), date);
    }
}
