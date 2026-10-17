//! The app state tables behind [`crate::Store`]: reading them into a [`State`], writing it back,
//! and the column encodings. A stored value that no longer decodes falls back to its default.

use std::collections::{BTreeMap, BTreeSet};

use chrono::Weekday;
use crumble_calendar::CalendarView;
use crumble_db::schema::{accounts, app_state, calendars, folded_sections};
use diesel::prelude::*;
use diesel::upsert::excluded;

use crate::{AccountColor, SidebarSection};

/// Everything the database holds, as the app reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct State {
    pub(crate) first_day_of_week: Weekday,
    pub(crate) calendar_view: CalendarView,
    pub(crate) disabled_accounts: BTreeSet<String>,
    /// Kept while their Account is Disabled.
    pub(crate) disabled_calendars: BTreeSet<String>,
    pub(crate) hidden_accounts: BTreeSet<String>,
    pub(crate) folded_sections: BTreeSet<SidebarSection>,
    /// Account identifiers in the order the user dragged them into; Accounts not listed follow,
    /// in EventKit's name order.
    pub(crate) account_order: Vec<String>,
    /// Dot colors the user picked; the others use [`AccountColor::default_for`].
    pub(crate) account_colors: BTreeMap<String, AccountColor>,
    pub(crate) account_aliases: BTreeMap<String, String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            first_day_of_week: Weekday::Mon,
            calendar_view: CalendarView::default(),
            disabled_accounts: BTreeSet::new(),
            disabled_calendars: BTreeSet::new(),
            hidden_accounts: BTreeSet::new(),
            folded_sections: BTreeSet::new(),
            account_order: Vec::new(),
            account_colors: BTreeMap::new(),
            account_aliases: BTreeMap::new(),
        }
    }
}

#[derive(Queryable, Insertable, AsChangeset)]
#[diesel(table_name = accounts, treat_none_as_null = true)]
struct AccountRow {
    id: String,
    position: Option<i32>,
    color: Option<String>,
    alias: Option<String>,
    disabled: bool,
    hidden: bool,
}

/// Reads the whole database.
pub(crate) fn load(connection: &mut SqliteConnection) -> QueryResult<State> {
    let mut state = State::default();
    if let Some((day, view)) = app_state::table
        .select((app_state::first_day_of_week, app_state::calendar_view))
        .first::<(i32, String)>(connection)
        .optional()?
    {
        state.first_day_of_week = decode_weekday(day);
        state.calendar_view = decode_calendar_view(&view);
    }
    let mut rows = accounts::table.load::<AccountRow>(connection)?;
    rows.sort_by_key(|row| row.position);
    for row in rows {
        if row.position.is_some() {
            state.account_order.push(row.id.clone());
        }
        if let Some(color) = row.color.as_deref().and_then(decode_color) {
            state.account_colors.insert(row.id.clone(), color);
        }
        if let Some(alias) = row.alias {
            state.account_aliases.insert(row.id.clone(), alias);
        }
        if row.disabled {
            state.disabled_accounts.insert(row.id.clone());
        }
        if row.hidden {
            state.hidden_accounts.insert(row.id);
        }
    }
    state.disabled_calendars = calendars::table
        .filter(calendars::disabled)
        .select(calendars::id)
        .load::<String>(connection)?
        .into_iter()
        .collect();
    state.folded_sections = folded_sections::table
        .select(folded_sections::section)
        .load::<String>(connection)?
        .iter()
        .filter_map(|section| decode_section(section))
        .collect();
    Ok(state)
}

pub(crate) fn write_app_state(connection: &mut SqliteConnection, state: &State) -> QueryResult<()> {
    diesel::update(app_state::table)
        .set((
            app_state::first_day_of_week.eq(encode_weekday(state.first_day_of_week)),
            app_state::calendar_view.eq(encode_calendar_view(state.calendar_view)),
        ))
        .execute(connection)?;
    Ok(())
}

/// Writes the row of the Account `id`. Rows whose values are all back to their defaults are
/// kept: one per Account the user ever changed.
pub(crate) fn write_account(
    connection: &mut SqliteConnection,
    state: &State,
    id: &str,
) -> QueryResult<()> {
    let row = AccountRow {
        id: id.to_owned(),
        position: state
            .account_order
            .iter()
            .position(|ordered| ordered == id)
            .and_then(|position| i32::try_from(position).ok()),
        color: state
            .account_colors
            .get(id)
            .map(|color| encode_color(*color)),
        alias: state.account_aliases.get(id).cloned(),
        disabled: state.disabled_accounts.contains(id),
        hidden: state.hidden_accounts.contains(id),
    };
    diesel::insert_into(accounts::table)
        .values(&row)
        .on_conflict(accounts::id)
        .do_update()
        .set(&row)
        .execute(connection)?;
    Ok(())
}

/// Writes the whole Account order.
pub(crate) fn write_account_order(
    connection: &mut SqliteConnection,
    state: &State,
) -> QueryResult<()> {
    diesel::update(accounts::table)
        .set(accounts::position.eq(None::<i32>))
        .execute(connection)?;
    for id in &state.account_order {
        write_account(connection, state, id)?;
    }
    Ok(())
}

/// Writes whether the Calendar `id` is Disabled; like Account rows, its row is kept.
pub(crate) fn write_calendar(
    connection: &mut SqliteConnection,
    state: &State,
    id: &str,
) -> QueryResult<()> {
    diesel::insert_into(calendars::table)
        .values((
            calendars::id.eq(id),
            calendars::disabled.eq(state.disabled_calendars.contains(id)),
        ))
        .on_conflict(calendars::id)
        .do_update()
        .set(calendars::disabled.eq(excluded(calendars::disabled)))
        .execute(connection)?;
    Ok(())
}

pub(crate) fn write_folded(
    connection: &mut SqliteConnection,
    state: &State,
    section: SidebarSection,
) -> QueryResult<()> {
    let encoded = encode_section(section);
    if state.folded_sections.contains(&section) {
        diesel::insert_or_ignore_into(folded_sections::table)
            .values(folded_sections::section.eq(encoded))
            .execute(connection)?;
    } else {
        diesel::delete(folded_sections::table.find(encoded)).execute(connection)?;
    }
    Ok(())
}

fn encode_weekday(day: Weekday) -> i32 {
    day.num_days_from_monday() as i32
}

fn decode_weekday(day: i32) -> Weekday {
    u8::try_from(day)
        .ok()
        .and_then(|day| Weekday::try_from(day).ok())
        .unwrap_or(Weekday::Mon)
}

fn encode_calendar_view(view: CalendarView) -> &'static str {
    match view {
        CalendarView::Week => "week",
        CalendarView::Month => "month",
        CalendarView::Year => "year",
    }
}

fn decode_calendar_view(view: &str) -> CalendarView {
    CalendarView::ALL
        .into_iter()
        .find(|known| encode_calendar_view(*known) == view)
        .unwrap_or_default()
}

fn encode_color(color: AccountColor) -> String {
    match color {
        AccountColor::Custom(srgb) => format!("#{srgb:06x}"),
        palette => palette.name().to_lowercase(),
    }
}

fn decode_color(color: &str) -> Option<AccountColor> {
    match color.strip_prefix('#') {
        Some(hex) if hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()) => {
            u32::from_str_radix(hex, 16).ok().map(AccountColor::Custom)
        }
        Some(_) => None,
        None => AccountColor::PALETTE
            .into_iter()
            .find(|palette| encode_color(*palette) == color),
    }
}

fn encode_section(section: SidebarSection) -> &'static str {
    match section {
        SidebarSection::Calendars => "calendars",
    }
}

fn decode_section(section: &str) -> Option<SidebarSection> {
    (section == "calendars").then_some(SidebarSection::Calendars)
}

#[cfg(test)]
mod tests {
    use super::{
        AccountColor, CalendarView, SidebarSection, Weekday, decode_calendar_view, decode_color,
        decode_section, decode_weekday, encode_calendar_view, encode_color, encode_section,
        encode_weekday,
    };

    #[test]
    fn encodings_round_trip() {
        for day in [Weekday::Mon, Weekday::Wed, Weekday::Sun] {
            assert_eq!(decode_weekday(encode_weekday(day)), day);
        }
        for view in CalendarView::ALL {
            assert_eq!(decode_calendar_view(encode_calendar_view(view)), view);
        }
        for color in AccountColor::PALETTE
            .into_iter()
            .chain([AccountColor::Custom(0x3366ff), AccountColor::Custom(0)])
        {
            assert_eq!(decode_color(&encode_color(color)), Some(color));
        }
        assert_eq!(encode_color(AccountColor::Red), "red");
        assert_eq!(encode_color(AccountColor::Custom(0x0000ff)), "#0000ff");
        let section = SidebarSection::Calendars;
        assert_eq!(decode_section(encode_section(section)), Some(section));
    }

    #[test]
    fn bad_stored_values_fall_back() {
        assert_eq!(decode_weekday(7), Weekday::Mon);
        assert_eq!(decode_weekday(-1), Weekday::Mon);
        assert_eq!(decode_calendar_view("day"), CalendarView::Month);
        for color in [
            "",
            "chartreuse",
            "Red",
            "#12345",
            "#+12345",
            "#zzzzzz",
            "#1234567",
        ] {
            assert_eq!(decode_color(color), None, "{color}");
        }
        assert_eq!(decode_section("agenda"), None);
    }
}
