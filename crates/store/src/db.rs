//! The app state tables behind [`crate::Store`]: reading them into a [`State`], writing it back,
//! and the column encodings. A stored value that no longer decodes falls back to its default.

use std::collections::{BTreeMap, BTreeSet};

use chrono::Weekday;
use chrono::{SecondsFormat, Utc};
use crumble_calendar::CalendarView;
use crumble_db::schema::{accounts, app_state, calendars, remote_audio_consents};
use diesel::prelude::*;
use diesel::upsert::excluded;

use crate::AccountColor;

/// Everything the database holds, as the app reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct State {
    pub(crate) first_day_of_week: Weekday,
    pub(crate) calendar_view: CalendarView,
    pub(crate) disabled_accounts: BTreeSet<String>,
    /// Kept while their Account is Disabled.
    pub(crate) disabled_calendars: BTreeSet<String>,
    pub(crate) hidden_accounts: BTreeSet<String>,
    /// Account identifiers in the order the user dragged them into; Accounts not listed follow,
    /// in EventKit's name order.
    pub(crate) account_order: Vec<String>,
    /// Dot colors the user picked; the others use [`AccountColor::default_for`].
    pub(crate) account_colors: BTreeMap<String, AccountColor>,
    pub(crate) account_aliases: BTreeMap<String, String>,
    /// The id of the Summary Provider picked in Settings; `None` until one is.
    pub(crate) summary_provider: Option<String>,
    pub(crate) claude_model: String,
    /// Empty until set.
    pub(crate) openai_base_url: String,
    pub(crate) openai_model: String,
    /// The id of the Transcription Provider: `apple`, `local` or `openai`.
    pub(crate) transcription_provider: String,
    /// The Transcription Model on the OpenAI-compatible server.
    pub(crate) transcription_model: String,
    /// The locale picked for Transcripts; `None` follows the Mac's.
    pub(crate) transcription_locale: Option<String>,
    /// The slug of the Local Model picked for Transcripts; `None` until one is.
    pub(crate) local_model: Option<String>,
    /// Each (Provider id, Model, host) the user agreed to send Recordings' audio to.
    pub(crate) audio_consents: BTreeSet<AudioConsent>,
    /// Whether a meeting's start brings a notification that suggests recording it.
    pub(crate) suggest_recording: bool,
}

/// A Provider id, Model and lowercase host that may receive Recordings' audio (ADR 0008).
pub(crate) type AudioConsent = (String, String, String);

/// The Claude Model Settings starts with, as the migration's default.
pub(crate) const CLAUDE_MODEL: &str = "claude-sonnet-5-5";
/// The Transcription Model Settings starts with on the server, as the migration's default.
pub(crate) const TRANSCRIPTION_MODEL: &str = "whisper-1";

impl Default for State {
    fn default() -> Self {
        Self {
            first_day_of_week: Weekday::Mon,
            calendar_view: CalendarView::default(),
            disabled_accounts: BTreeSet::new(),
            disabled_calendars: BTreeSet::new(),
            hidden_accounts: BTreeSet::new(),
            account_order: Vec::new(),
            account_colors: BTreeMap::new(),
            account_aliases: BTreeMap::new(),
            summary_provider: None,
            claude_model: CLAUDE_MODEL.to_owned(),
            openai_base_url: String::new(),
            openai_model: String::new(),
            transcription_provider: "apple".to_owned(),
            transcription_model: TRANSCRIPTION_MODEL.to_owned(),
            transcription_locale: None,
            local_model: None,
            audio_consents: BTreeSet::new(),
            suggest_recording: true,
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
    type Row = (
        (i32, String),
        (Option<String>, String, String, String),
        (String, String, Option<String>, Option<String>),
        bool,
    );
    if let Some((
        (day, view),
        (provider, claude_model, openai_base_url, openai_model),
        (transcription_provider, transcription_model, transcription_locale, local_model),
        suggest_recording,
    )) = app_state::table
        .select((
            (app_state::first_day_of_week, app_state::calendar_view),
            (
                app_state::summary_provider,
                app_state::claude_model,
                app_state::openai_base_url,
                app_state::openai_model,
            ),
            (
                app_state::transcription_provider,
                app_state::transcription_model,
                app_state::transcription_locale,
                app_state::local_model,
            ),
            app_state::suggest_recording,
        ))
        .first::<Row>(connection)
        .optional()?
    {
        state.first_day_of_week = decode_weekday(day);
        state.calendar_view = decode_calendar_view(&view);
        state.summary_provider = provider;
        state.claude_model = claude_model;
        state.openai_base_url = openai_base_url;
        state.openai_model = openai_model;
        state.transcription_provider = transcription_provider;
        state.transcription_model = transcription_model;
        state.transcription_locale = transcription_locale;
        state.local_model = local_model;
        state.suggest_recording = suggest_recording;
    }
    state.audio_consents = remote_audio_consents::table
        .select((
            remote_audio_consents::provider,
            remote_audio_consents::model,
            remote_audio_consents::host,
        ))
        .load::<AudioConsent>(connection)?
        .into_iter()
        .collect();
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
    Ok(state)
}

pub(crate) fn write_app_state(connection: &mut SqliteConnection, state: &State) -> QueryResult<()> {
    diesel::update(app_state::table)
        .set((
            app_state::first_day_of_week.eq(encode_weekday(state.first_day_of_week)),
            app_state::calendar_view.eq(encode_calendar_view(state.calendar_view)),
            app_state::summary_provider.eq(&state.summary_provider),
            app_state::claude_model.eq(&state.claude_model),
            app_state::openai_base_url.eq(&state.openai_base_url),
            app_state::openai_model.eq(&state.openai_model),
            app_state::transcription_provider.eq(&state.transcription_provider),
            app_state::transcription_model.eq(&state.transcription_model),
            app_state::transcription_locale.eq(&state.transcription_locale),
            app_state::local_model.eq(&state.local_model),
            app_state::suggest_recording.eq(state.suggest_recording),
        ))
        .execute(connection)?;
    Ok(())
}

/// Records the consent to send Recordings' audio to `consent`, and when it was given.
pub(crate) fn write_audio_consent(
    connection: &mut SqliteConnection,
    consent: &AudioConsent,
) -> QueryResult<()> {
    let (provider, model, host) = consent;
    diesel::insert_or_ignore_into(remote_audio_consents::table)
        .values((
            remote_audio_consents::provider.eq(provider),
            remote_audio_consents::model.eq(model),
            remote_audio_consents::host.eq(host),
            remote_audio_consents::acknowledged_at
                .eq(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)),
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

#[cfg(test)]
mod tests {
    use super::{
        AccountColor, CalendarView, Weekday, decode_calendar_view, decode_color, decode_weekday,
        encode_calendar_view, encode_color, encode_weekday,
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
    }
}
