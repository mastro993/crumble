//! User settings kept across restarts in `settings.json` under the platform config directory.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use chrono::Weekday;
use crumble_eventkit::{Account, Calendar};
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
    /// Sidebar sections whose rows are folded away.
    pub(crate) folded_sections: BTreeSet<SidebarSection>,
    /// Account identifiers in the order the user dragged them into; Accounts not listed follow,
    /// in EventKit's name order.
    pub(crate) account_order: Vec<String>,
    /// Dot colors the user picked, by Account identifier; the others use [`AccountColor::default_for`].
    pub(crate) account_colors: BTreeMap<String, AccountColor>,
    /// Aliases the user gave, by Account identifier.
    pub(crate) account_aliases: BTreeMap<String, String>,
}

/// The dot that marks an Account in the sidebar and in Settings. The palette is SkillBinder's
/// folder colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum AccountColor {
    Gray,
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Pink,
}

impl AccountColor {
    pub(crate) const ALL: [Self; 8] = [
        Self::Gray,
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Blue,
        Self::Purple,
        Self::Pink,
    ];

    /// A color that looks random but stays the same for `id` across launches, so nothing has to
    /// be saved until the user picks one. Gray is left for the user to choose.
    pub(crate) fn default_for(id: &str) -> Self {
        // FNV-1a: tiny, and unlike std's hasher its output is fixed.
        let hash = id.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
        let colorful = &Self::ALL[1..];
        colorful[(hash % colorful.len() as u64) as usize]
    }
}

/// A foldable sidebar section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(crate) enum SidebarSection {
    /// The Calendars section, listing Accounts.
    Calendars,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            first_day_of_week: Weekday::Mon,
            disabled_accounts: BTreeSet::new(),
            disabled_calendars: BTreeSet::new(),
            folded_sections: BTreeSet::new(),
            account_colors: BTreeMap::new(),
            account_aliases: BTreeMap::new(),
            account_order: Vec::new(),
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

    /// `accounts` in the user's order.
    pub(crate) fn ordered_accounts<'a>(&self, accounts: &'a [Account]) -> Vec<&'a Account> {
        let mut ordered: Vec<&Account> = accounts.iter().collect();
        // Stable, so unlisted Accounts keep EventKit's order after the listed ones.
        ordered.sort_by_key(|account| {
            self.account_order
                .iter()
                .position(|id| id == &account.id)
                .unwrap_or(usize::MAX)
        });
        ordered
    }

    /// The dot color of the Account `id`: the user's pick, else its default.
    pub(crate) fn account_color(&self, id: &str) -> AccountColor {
        self.account_colors
            .get(id)
            .copied()
            .unwrap_or_else(|| AccountColor::default_for(id))
    }

    /// The Alias of the Account `id`, if the user gave one.
    pub(crate) fn account_alias(&self, id: &str) -> Option<&str> {
        self.account_aliases.get(id).map(String::as_str)
    }

    /// Gives the Account `id` the Alias `alias`, trimmed; a blank one, or `own`, the Account's
    /// own name, removes it.
    pub(crate) fn set_account_alias(&mut self, id: &str, alias: &str, own: &str) {
        match alias.trim() {
            alias if alias.is_empty() || alias == own => self.account_aliases.remove(id),
            alias => self.account_aliases.insert(id.to_owned(), alias.to_owned()),
        };
    }

    /// Saves `listed`, the new order of the Accounts the sidebar lists, keeping each Disabled
    /// Account in its slot among `accounts`.
    pub(crate) fn set_listed_order(&mut self, accounts: &[Account], listed: Vec<String>) {
        let mut listed = listed.into_iter();
        self.account_order = self
            .ordered_accounts(accounts)
            .into_iter()
            .map(|account| {
                if self.disabled_accounts.contains(&account.id) {
                    Some(account.id.clone())
                } else {
                    listed.next()
                }
            })
            .collect::<Option<_>>()
            .unwrap_or_else(|| self.account_order.clone());
    }

    /// Identifiers of the `calendars` whose Events show: the Calendar is not Disabled, and its
    /// Account is neither Disabled nor in `hidden_accounts`.
    pub(crate) fn shown_calendar_ids(
        &self,
        calendars: &[Calendar],
        hidden_accounts: &BTreeSet<String>,
    ) -> Vec<String> {
        calendars
            .iter()
            .filter(|calendar| {
                !self.disabled_accounts.contains(&calendar.account_id)
                    && !hidden_accounts.contains(&calendar.account_id)
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
    use super::{Account, AccountColor, Calendar, Settings, SidebarSection, Weekday, toggle};
    use crumble_eventkit::Provider;
    use std::collections::BTreeSet;

    #[test]
    fn saved_settings_read_back() {
        let mut settings = Settings {
            first_day_of_week: Weekday::Sun,
            ..Settings::default()
        };
        settings.disabled_accounts.insert("source-1".into());
        settings.disabled_calendars.insert("calendar-1".into());
        settings.folded_sections.insert(SidebarSection::Calendars);
        settings.account_order = vec!["source-2".into(), "source-1".into()];
        settings
            .account_colors
            .insert("source-1".into(), AccountColor::Pink);
        settings.set_account_alias("source-1", "Work", "me@example.com");
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
        assert!(old.folded_sections.is_empty() && old.account_colors.is_empty());
        assert!(old.account_order.is_empty() && old.account_aliases.is_empty());
    }

    fn account(id: &str) -> Account {
        Account {
            id: id.into(),
            name: id.into(),
            email: None,
            provider: Provider::Other,
        }
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
        let none = BTreeSet::new();
        let mut settings = Settings::default();
        toggle(&mut settings.disabled_calendars, "holidays");
        assert_eq!(
            settings.shown_calendar_ids(&calendars, &none),
            ["work", "family"]
        );

        toggle(&mut settings.disabled_accounts, "google");
        assert_eq!(settings.shown_calendar_ids(&calendars, &none), ["family"]);

        toggle(&mut settings.disabled_accounts, "google");
        assert_eq!(
            settings.shown_calendar_ids(&calendars, &none),
            ["work", "family"]
        );
    }

    #[test]
    fn a_hidden_account_hides_its_calendars_whether_or_not_it_is_disabled() {
        let calendars = [calendar("work", "google"), calendar("family", "icloud")];
        let hidden = BTreeSet::from(["google".to_owned()]);
        let mut settings = Settings::default();
        assert_eq!(settings.shown_calendar_ids(&calendars, &hidden), ["family"]);

        // Disabling and Enabling it again leaves it Hidden.
        toggle(&mut settings.disabled_accounts, "google");
        toggle(&mut settings.disabled_accounts, "google");
        assert_eq!(settings.shown_calendar_ids(&calendars, &hidden), ["family"]);
    }

    #[test]
    fn reordering_the_listed_accounts_keeps_disabled_ones_in_their_slot() {
        let accounts = ["a", "b", "c", "d"].map(account);
        let mut settings = Settings::default();
        settings.disabled_accounts.insert("b".into());
        settings.set_listed_order(&accounts, vec!["d".into(), "a".into(), "c".into()]);
        assert_eq!(settings.account_order, ["d", "b", "a", "c"]);

        // A list that no longer matches the Accounts leaves the order alone.
        settings.set_listed_order(&accounts, vec!["a".into()]);
        assert_eq!(settings.account_order, ["d", "b", "a", "c"]);
    }

    #[test]
    fn an_account_keeps_its_default_color_until_the_user_picks_one() {
        let mut settings = Settings::default();
        let default = settings.account_color("source-1");
        assert_eq!(default, AccountColor::default_for("source-1"));
        assert_ne!(default, AccountColor::Gray);
        // Fixed across launches, and spread across the palette.
        assert_eq!(AccountColor::default_for("source-1"), default);
        let spread: std::collections::BTreeSet<_> = (0..40)
            .map(|n| format!("{:?}", AccountColor::default_for(&format!("source-{n}"))))
            .collect();
        assert!(spread.len() > 3);

        settings
            .account_colors
            .insert("source-1".into(), AccountColor::Gray);
        assert_eq!(settings.account_color("source-1"), AccountColor::Gray);
    }

    #[test]
    fn an_alias_is_trimmed_and_a_blank_one_or_the_own_name_removes_it() {
        let own = "me@example.com";
        let mut settings = Settings::default();
        assert_eq!(settings.account_alias("source-1"), None);
        settings.set_account_alias("source-1", "  Work ", own);
        assert_eq!(settings.account_alias("source-1"), Some("Work"));
        settings.set_account_alias("source-1", "   ", own);
        assert_eq!(settings.account_alias("source-1"), None);

        settings.set_account_alias("source-1", "Work", own);
        settings.set_account_alias("source-1", " me@example.com", own);
        assert_eq!(settings.account_alias("source-1"), None);
    }

    #[test]
    fn accounts_follow_the_saved_order_and_new_ones_come_last() {
        let accounts = ["a", "b", "c", "d"].map(account);
        let settings = Settings {
            account_order: vec!["c".into(), "gone".into(), "a".into()],
            ..Settings::default()
        };
        let ids: Vec<&str> = settings
            .ordered_accounts(&accounts)
            .iter()
            .map(|account| account.id.as_str())
            .collect();
        assert_eq!(ids, ["c", "a", "b", "d"]);
    }
}
