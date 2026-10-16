//! Calendar views, the Period each one shows, and stepping between Periods.

use chrono::{Datelike, Days, Months, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

/// How the Calendar page lays out time; it opens on Month view until the user picks another.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalendarView {
    /// Seven days by the hour.
    Week,
    /// The month's weeks, one row each.
    #[default]
    Month,
    /// The year's twelve months.
    Year,
}

impl CalendarView {
    /// Every Calendar view, in switcher order.
    pub const ALL: [Self; 3] = [Self::Week, Self::Month, Self::Year];

    /// The Calendar view's name, as its switcher tab shows it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Week => "Week",
            Self::Month => "Month",
            Self::Year => "Year",
        }
    }

    /// The Focused date moved one Period forward or back.
    pub fn step(self, focused: NaiveDate, forward: bool) -> NaiveDate {
        let shifted = match (self, forward) {
            (Self::Week, true) => focused.checked_add_days(Days::new(7)),
            (Self::Week, false) => focused.checked_sub_days(Days::new(7)),
            (Self::Month, true) => focused.checked_add_months(Months::new(1)),
            (Self::Month, false) => focused.checked_sub_months(Months::new(1)),
            (Self::Year, true) => focused.checked_add_months(Months::new(12)),
            (Self::Year, false) => focused.checked_sub_months(Months::new(12)),
        };
        shifted.unwrap_or(focused)
    }

    /// The dates this view draws Events on, from the first to the day after the last; `None`
    /// for Year view, which draws none.
    pub fn event_range(
        self,
        focused: NaiveDate,
        first_day: Weekday,
    ) -> Option<(NaiveDate, NaiveDate)> {
        let (first, last_week) = match self {
            Self::Week => {
                let start = week_start(focused, first_day);
                (start, start)
            }
            Self::Month => {
                let weeks = month_weeks(focused, first_day);
                (*weeks.first()?, *weeks.last()?)
            }
            Self::Year => return None,
        };
        Some((first, last_week + Days::new(7)))
    }

    /// Header title of the Period containing `focused`.
    pub fn title(self, focused: NaiveDate, first_day: Weekday) -> String {
        match self {
            Self::Year => focused.format("%Y").to_string(),
            Self::Month => focused.format("%B %Y").to_string(),
            Self::Week => {
                let start = week_start(focused, first_day);
                let end = start + Days::new(6);
                let (from, to) = if start.year() != end.year() {
                    ("%b %-d, %Y", "%b %-d, %Y")
                } else if start.month() != end.month() {
                    ("%b %-d", "%b %-d, %Y")
                } else {
                    ("%b %-d", "%-d, %Y")
                };
                format!("{} – {}", start.format(from), end.format(to))
            }
        }
    }
}

/// First date of the week containing `date`.
pub fn week_start(date: NaiveDate, first_day: Weekday) -> NaiveDate {
    date.week(first_day).first_day()
}

/// The seven dates of the week starting on `start`.
pub fn week_days(start: NaiveDate) -> impl Iterator<Item = NaiveDate> {
    start.iter_days().take(7)
}

/// Weekdays in column order.
pub fn weekdays(first_day: Weekday) -> impl Iterator<Item = Weekday> {
    std::iter::successors(Some(first_day), |day| Some(day.succ())).take(7)
}

/// Start of every week row that overlaps the month containing `date`.
pub fn month_weeks(date: NaiveDate, first_day: Weekday) -> Vec<NaiveDate> {
    let first = date.with_day(1).unwrap_or(date);
    let next_month = first + Months::new(1);
    std::iter::successors(Some(week_start(first, first_day)), |start| {
        Some(*start + Days::new(7))
    })
    .take_while(|start| *start < next_month)
    .collect()
}

/// First date of each month in the year containing `date`.
pub fn year_months(date: NaiveDate) -> impl Iterator<Item = NaiveDate> {
    let year = date.year();
    (1..=12).filter_map(move |month| NaiveDate::from_ymd_opt(year, month, 1))
}

/// Whether `date` falls on a Saturday or Sunday.
pub fn is_weekend(date: NaiveDate) -> bool {
    matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap_or_default()
    }

    #[test]
    fn weeks_start_on_the_chosen_weekday() {
        // Tuesday 13 October 2026.
        let tuesday = date(2026, 10, 13);
        assert_eq!(week_start(tuesday, Weekday::Mon), date(2026, 10, 12));
        assert_eq!(week_start(tuesday, Weekday::Sun), date(2026, 10, 11));
        assert_eq!(week_start(tuesday, Weekday::Tue), tuesday);
        assert_eq!(week_start(tuesday, Weekday::Wed), date(2026, 10, 7));
    }

    #[test]
    fn stepping_moves_one_period() {
        let focused = date(2026, 1, 31);
        assert_eq!(CalendarView::Week.step(focused, true), date(2026, 2, 7));
        assert_eq!(CalendarView::Week.step(focused, false), date(2026, 1, 24));
        assert_eq!(CalendarView::Month.step(focused, true), date(2026, 2, 28));
        assert_eq!(CalendarView::Month.step(focused, false), date(2025, 12, 31));
        assert_eq!(
            CalendarView::Year.step(date(2028, 2, 29), true),
            date(2029, 2, 28)
        );
    }

    #[test]
    fn titles_name_the_period() {
        let monday = Weekday::Mon;
        assert_eq!(CalendarView::Year.title(date(2026, 10, 13), monday), "2026");
        assert_eq!(
            CalendarView::Month.title(date(2026, 10, 13), monday),
            "October 2026"
        );
        assert_eq!(
            CalendarView::Week.title(date(2026, 10, 13), monday),
            "Oct 12 – 18, 2026"
        );
        assert_eq!(
            CalendarView::Week.title(date(2026, 10, 1), monday),
            "Sep 28 – Oct 4, 2026"
        );
        assert_eq!(
            CalendarView::Week.title(date(2026, 12, 31), monday),
            "Dec 28, 2026 – Jan 3, 2027"
        );
    }

    #[test]
    fn month_grid_covers_the_month_in_whole_weeks() {
        // February 2027 starts on a Monday and has exactly four weeks.
        assert_eq!(month_weeks(date(2027, 2, 14), Weekday::Mon).len(), 4);
        assert_eq!(month_weeks(date(2027, 2, 14), Weekday::Sun).len(), 5);
        // August 2026 starts on a Saturday and needs six Monday-first rows.
        let weeks = month_weeks(date(2026, 8, 20), Weekday::Mon);
        assert_eq!(weeks.len(), 6);
        assert_eq!(weeks.first(), Some(&date(2026, 7, 27)));
        assert_eq!(weeks.last(), Some(&date(2026, 8, 31)));
    }

    #[test]
    fn events_are_read_for_the_dates_on_screen() {
        let monday = Weekday::Mon;
        assert_eq!(
            CalendarView::Week.event_range(date(2026, 10, 13), monday),
            Some((date(2026, 10, 12), date(2026, 10, 19)))
        );
        // Month view includes the neighbouring months' days in its first and last rows.
        assert_eq!(
            CalendarView::Month.event_range(date(2026, 8, 20), monday),
            Some((date(2026, 7, 27), date(2026, 9, 7)))
        );
        assert_eq!(
            CalendarView::Year.event_range(date(2026, 8, 20), monday),
            None
        );
    }

    #[test]
    fn weekday_columns_wrap_around_the_week() {
        let columns: Vec<_> = weekdays(Weekday::Sat).collect();
        assert_eq!(columns.first(), Some(&Weekday::Sat));
        assert_eq!(columns.last(), Some(&Weekday::Fri));
        assert_eq!(columns.len(), 7);
    }

    #[test]
    fn weekends_are_saturday_and_sunday() {
        assert!(is_weekend(date(2026, 10, 17)));
        assert!(is_weekend(date(2026, 10, 18)));
        assert!(!is_weekend(date(2026, 10, 19)));
    }
}
