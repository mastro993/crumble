//! User settings kept across restarts in `settings.json` under the platform config directory.

use std::collections::BTreeSet;
use std::path::PathBuf;

use chrono::Weekday;
use crumble_eventkit::Calendar;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    /// Weekday that starts each Calendar week.
    pub(crate) first_day_of_week: Weekday,
    /// EventKit identifiers of the Disabled Accounts.
    pub(crate) disabled_accounts: BTreeSet<String>,
    /// EventKit identifiers of the Disabled Calendars, kept while their Account is Disabled.
    pub(crate) disabled_calendars: BTreeSet<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            first_day_of_week: Weekday::Mon,
            disabled_accounts: BTreeSet::new(),
            disabled_calendars: BTreeSet::new(),
        }
    }
}

fn path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("Crumble").join("settings.json"))
}

impl Settings {
    /// Reads the saved settings; a missing or unreadable file yields the defaults.
    pub(crate) fn load() -> Self {
        path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|json| Self::parse(&json))
            .unwrap_or_default()
    }

    fn parse(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }

    /// Writes the settings; a failure is logged and the app keeps the in-memory value.
    pub(crate) fn save(&self) {
        let write = || -> anyhow::Result<()> {
            let path = path().ok_or_else(|| anyhow::anyhow!("no config directory"))?;
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(path, serde_json::to_string_pretty(self)?)?;
            Ok(())
        };
        if let Err(error) = write() {
            eprintln!("Could not save settings: {error}");
        }
    }

    /// Identifiers of the `calendars` whose Events show: neither they nor their Account is
    /// Disabled.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the Calendar page reads it once it shows Events")
    )]
    pub(crate) fn enabled_calendar_ids(&self, calendars: &[Calendar]) -> Vec<String> {
        calendars
            .iter()
            .filter(|calendar| {
                !self.disabled_accounts.contains(&calendar.account_id)
                    && !self.disabled_calendars.contains(&calendar.id)
            })
            .map(|calendar| calendar.id.clone())
            .collect()
    }
}

/// Disables `id` if it is enabled and enables it otherwise.
pub(crate) fn toggle(disabled: &mut BTreeSet<String>, id: &str) {
    if !disabled.remove(id) {
        disabled.insert(id.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::{Calendar, Settings, Weekday, toggle};

    #[test]
    fn saved_settings_read_back() {
        let mut settings = Settings {
            first_day_of_week: Weekday::Sun,
            ..Settings::default()
        };
        settings.disabled_accounts.insert("source-1".into());
        settings.disabled_calendars.insert("calendar-1".into());
        let json = serde_json::to_string(&settings).unwrap_or_default();
        assert_eq!(Settings::parse(&json), settings);
    }

    #[test]
    fn corrupt_or_partial_files_fall_back_to_defaults() {
        assert_eq!(Settings::parse("not json"), Settings::default());
        assert_eq!(Settings::parse("{}"), Settings::default());
        assert_eq!(Settings::default().first_day_of_week, Weekday::Mon);
        // Files saved before Accounts could be Disabled keep their First day of week.
        let old = Settings::parse(r#"{"first_day_of_week":"Sun"}"#);
        assert_eq!(old.first_day_of_week, Weekday::Sun);
        assert!(old.disabled_accounts.is_empty() && old.disabled_calendars.is_empty());
    }

    fn calendar(id: &str, account_id: &str) -> Calendar {
        Calendar {
            id: id.into(),
            account_id: account_id.into(),
            name: id.into(),
            color: 0,
        }
    }

    #[test]
    fn a_disabled_account_hides_its_calendars_but_keeps_their_own_setting() {
        let calendars = [
            calendar("work", "google"),
            calendar("holidays", "google"),
            calendar("family", "icloud"),
        ];
        let mut settings = Settings::default();
        toggle(&mut settings.disabled_calendars, "holidays");
        assert_eq!(
            settings.enabled_calendar_ids(&calendars),
            ["work", "family"]
        );

        toggle(&mut settings.disabled_accounts, "google");
        assert_eq!(settings.enabled_calendar_ids(&calendars), ["family"]);

        toggle(&mut settings.disabled_accounts, "google");
        assert_eq!(
            settings.enabled_calendar_ids(&calendars),
            ["work", "family"]
        );
    }
}
