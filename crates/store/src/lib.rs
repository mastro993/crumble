//! State Crumble keeps across restarts, in a SQLite database under the platform data directory,
//! and the Account rules that read it. API keys are kept apart, in the Keychain, through
//! [`secrets`].

mod account_color;
mod accounts;
mod calendars_section;
mod db;
pub mod secrets;

pub use account_color::AccountColor;
pub use accounts::account_label;
pub use calendars_section::{CalendarsSection, calendars_section};

use chrono::Weekday;
use crumble_calendar::CalendarView;
use db::State;
use diesel::{Connection, QueryResult, SqliteConnection};

/// Everything Crumble keeps across restarts. Reads come from memory; every change is written to
/// the database as it happens.
pub struct Store {
    state: State,
    /// `None` only when not even an in-memory database opens; writes are then skipped.
    connection: Option<SqliteConnection>,
}

impl Store {
    /// Opens the app's database and reads it. When it cannot be opened or read, the app runs on
    /// the defaults in an in-memory database and leaves the file alone.
    pub fn open() -> Self {
        match crumble_db::open_file().and_then(Self::read) {
            Ok(store) => store,
            Err(error) => {
                eprintln!("Could not open the database, using defaults: {error}");
                Self::in_memory()
            }
        }
    }

    fn in_memory() -> Self {
        match crumble_db::open(":memory:").and_then(Self::read) {
            Ok(store) => store,
            Err(error) => {
                eprintln!("Could not open an in-memory database, changes are not kept: {error}");
                Self {
                    state: State::default(),
                    connection: None,
                }
            }
        }
    }

    /// Reads the whole database. A failed read returns the error rather than defaults, so a later
    /// write cannot overwrite stored values that were never loaded.
    fn read(mut connection: SqliteConnection) -> anyhow::Result<Self> {
        let state = db::load(&mut connection)?;
        Ok(Self {
            state,
            connection: Some(connection),
        })
    }

    /// Writes the in-memory state with `write` in one transaction; a failure is logged and the
    /// app keeps the in-memory value.
    // ponytail: writes are synchronous on the main thread; they are one-row updates. Move them to
    // a background writer if they ever show up in frame times.
    fn write(&mut self, write: impl FnOnce(&mut SqliteConnection, &State) -> QueryResult<()>) {
        let Some(connection) = self.connection.as_mut() else {
            return;
        };
        let state = &self.state;
        if let Err(error) = connection.transaction(|connection| write(connection, state)) {
            eprintln!("Could not save: {error}");
        }
    }

    /// Weekday that starts each Calendar week.
    pub fn first_day_of_week(&self) -> Weekday {
        self.state.first_day_of_week
    }

    /// Makes `day` start each Calendar week.
    pub fn set_first_day_of_week(&mut self, day: Weekday) {
        self.state.first_day_of_week = day;
        self.write(db::write_app_state);
    }

    /// The Calendar view last picked, which the Calendar page opens on.
    pub fn calendar_view(&self) -> CalendarView {
        self.state.calendar_view
    }

    /// Remembers `view` as the Calendar view the Calendar page opens on.
    pub fn set_calendar_view(&mut self, view: CalendarView) {
        if self.state.calendar_view != view {
            self.state.calendar_view = view;
            self.write(db::write_app_state);
        }
    }

    /// The id of the Summary Model's Provider, as `crumble_remote::Provider::id` gives it:
    /// the one picked in Settings, else `apple` when `apple_available`, else none.
    pub fn summary_provider(&self, apple_available: bool) -> Option<&str> {
        self.state
            .summary_provider
            .as_deref()
            .or(apple_available.then_some("apple"))
    }

    /// Makes the Provider with the id `provider` make Summaries.
    pub fn set_summary_provider(&mut self, provider: &str) {
        self.state.summary_provider = Some(provider.to_owned());
        self.write(db::write_app_state);
    }

    /// The Claude Model id Summaries use.
    pub fn claude_model(&self) -> &str {
        &self.state.claude_model
    }

    /// Makes Summaries use the Claude Model `model`.
    pub fn set_claude_model(&mut self, model: &str) {
        self.state.claude_model = model.trim().to_owned();
        self.write(db::write_app_state);
    }

    /// The OpenAI-compatible server's address up to `/chat/completions`; empty until set.
    pub fn openai_base_url(&self) -> &str {
        &self.state.openai_base_url
    }

    /// Sets the OpenAI-compatible server's address.
    pub fn set_openai_base_url(&mut self, url: &str) {
        self.state.openai_base_url = url.trim().to_owned();
        self.write(db::write_app_state);
    }

    /// The Model Summaries use on the OpenAI-compatible server; empty until set.
    pub fn openai_model(&self) -> &str {
        &self.state.openai_model
    }

    /// Makes Summaries use the Model `model` on the OpenAI-compatible server.
    pub fn set_openai_model(&mut self, model: &str) {
        self.state.openai_model = model.trim().to_owned();
        self.write(db::write_app_state);
    }

    /// The id of the Transcription Model's Provider, as `crumble_remote::Provider::id` gives
    /// it: `apple` until another is picked.
    pub fn transcription_provider(&self) -> &str {
        &self.state.transcription_provider
    }

    /// Makes the Provider with the id `provider` make Transcripts.
    pub fn set_transcription_provider(&mut self, provider: &str) {
        self.state.transcription_provider = provider.to_owned();
        self.write(db::write_app_state);
    }

    /// The Model Transcripts use on the OpenAI-compatible server, `whisper-1` until changed.
    pub fn transcription_model(&self) -> &str {
        &self.state.transcription_model
    }

    /// Makes Transcripts use the Model `model` on the OpenAI-compatible server.
    pub fn set_transcription_model(&mut self, model: &str) {
        self.state.transcription_model = model.trim().to_owned();
        self.write(db::write_app_state);
    }

    /// The locale picked for Transcripts, such as `it_IT`; `None` follows the Mac's.
    pub fn transcription_locale(&self) -> Option<&str> {
        self.state.transcription_locale.as_deref()
    }

    /// Makes Transcripts use `locale`, or the Mac's locale for `None`.
    pub fn set_transcription_locale(&mut self, locale: Option<&str>) {
        self.state.transcription_locale = locale.map(str::to_owned);
        self.write(db::write_app_state);
    }

    /// Whether a meeting's start brings a notification that suggests recording it; on until
    /// turned off.
    pub fn suggests_recording(&self) -> bool {
        self.state.suggest_recording
    }

    /// Turns the notification that suggests recording a starting meeting on or off.
    pub fn set_suggests_recording(&mut self, on: bool) {
        self.state.suggest_recording = on;
        self.write(db::write_app_state);
    }

    /// Whether the user agreed to send Recordings' audio to `model` of the Provider with the id
    /// `provider` at `host` (ADR 0008). The host is compared without case.
    pub fn has_audio_consent(&self, provider: &str, model: &str, host: &str) -> bool {
        self.state
            .audio_consents
            .contains(&consent(provider, model, host))
    }

    /// Records that the user agreed to send Recordings' audio to `model` of the Provider with
    /// the id `provider` at `host`, and when.
    pub fn add_audio_consent(&mut self, provider: &str, model: &str, host: &str) {
        let consent = consent(provider, model, host);
        if self.state.audio_consents.insert(consent.clone()) {
            self.write(|connection, _| db::write_audio_consent(connection, &consent));
        }
    }
}

/// The key of a consent to send audio, its host in lowercase.
fn consent(provider: &str, model: &str, host: &str) -> db::AudioConsent {
    (
        provider.to_owned(),
        model.to_owned(),
        host.to_ascii_lowercase(),
    )
}

#[cfg(test)]
mod tests;
