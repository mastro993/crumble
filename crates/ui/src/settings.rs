//! User settings kept across restarts in `settings.json` under the platform config directory.

use std::path::PathBuf;

use chrono::Weekday;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    /// Weekday that starts each Calendar week.
    pub(crate) first_day_of_week: Weekday,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            first_day_of_week: Weekday::Mon,
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
}

#[cfg(test)]
mod tests {
    use super::{Settings, Weekday};

    #[test]
    fn saved_settings_read_back() {
        let settings = Settings {
            first_day_of_week: Weekday::Sun,
        };
        let json = serde_json::to_string(&settings).unwrap_or_default();
        assert_eq!(Settings::parse(&json), settings);
    }

    #[test]
    fn corrupt_or_partial_files_fall_back_to_defaults() {
        assert_eq!(Settings::parse("not json"), Settings::default());
        assert_eq!(Settings::parse("{}"), Settings::default());
        assert_eq!(Settings::default().first_day_of_week, Weekday::Mon);
    }
}
