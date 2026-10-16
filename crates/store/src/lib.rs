//! State Crumble keeps across restarts, in `settings.json` under the platform config directory,
//! and the Account rules that read it.

mod account_color;
mod calendars_section;

pub use account_color::AccountColor;
pub use calendars_section::{CalendarsSection, calendars_section};

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use chrono::Weekday;
use crumble_calendar::CalendarView;
use crumble_eventkit::{Account, Calendar};
use serde::{Deserialize, Serialize};

/// Everything Crumble keeps across restarts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Store {
    /// Weekday that starts each Calendar week.
    pub first_day_of_week: Weekday,
    /// The Calendar view last picked, which the Calendar page opens on.
    pub calendar_view: CalendarView,
    /// EventKit identifiers of the Disabled Accounts.
    pub disabled_accounts: BTreeSet<String>,
    /// EventKit identifiers of the Disabled Calendars, kept while their Account is Disabled.
    pub disabled_calendars: BTreeSet<String>,
    /// EventKit identifiers of the Hidden Accounts.
    pub hidden_accounts: BTreeSet<String>,
    /// Sidebar sections whose rows are folded away.
    pub folded_sections: BTreeSet<SidebarSection>,
    /// Account identifiers in the order the user dragged them into; Accounts not listed follow,
    /// in EventKit's name order.
    pub account_order: Vec<String>,
    /// Dot colors the user picked, by Account identifier; the others use [`AccountColor::default_for`].
    pub account_colors: BTreeMap<String, AccountColor>,
    /// Aliases the user gave, by Account identifier.
    pub account_aliases: BTreeMap<String, String>,
}

/// A foldable sidebar section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SidebarSection {
    /// The Calendars section, listing Accounts.
    Calendars,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            first_day_of_week: Weekday::Mon,
            calendar_view: CalendarView::default(),
            disabled_accounts: BTreeSet::new(),
            disabled_calendars: BTreeSet::new(),
            hidden_accounts: BTreeSet::new(),
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

impl Store {
    /// Reads the saved state; a missing or unreadable file yields the defaults.
    pub fn load() -> Self {
        path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|json| Self::parse(&json))
            .unwrap_or_default()
    }

    fn parse(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }

    /// Writes the state; a failure is logged and the app keeps the in-memory value.
    pub fn save(&self) {
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
    pub fn ordered_accounts<'a>(&self, accounts: &'a [Account]) -> Vec<&'a Account> {
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

    /// `accounts` that are not Disabled, in the user's order.
    pub fn listed_accounts<'a>(&self, accounts: &'a [Account]) -> Vec<&'a Account> {
        let mut listed = self.ordered_accounts(accounts);
        listed.retain(|account| !self.disabled_accounts.contains(&account.id));
        listed
    }

    /// The dot color of the Account `id`: the user's pick, else its default.
    pub fn account_color(&self, id: &str) -> AccountColor {
        self.account_colors
            .get(id)
            .copied()
            .unwrap_or_else(|| AccountColor::default_for(id))
    }

    /// The Alias of the Account `id`, if the user gave one.
    pub fn account_alias(&self, id: &str) -> Option<&str> {
        self.account_aliases.get(id).map(String::as_str)
    }

    /// The name an Account goes by: its Alias, else its own label.
    pub fn account_name<'a>(&'a self, account: &'a Account) -> &'a str {
        self.account_alias(&account.id)
            .unwrap_or_else(|| account_label(account))
    }

    /// Gives the Account `id` the Alias `alias`, trimmed; a blank one, or `own`, the Account's
    /// own name, removes it.
    pub fn set_account_alias(&mut self, id: &str, alias: &str, own: &str) {
        match alias.trim() {
            alias if alias.is_empty() || alias == own => self.account_aliases.remove(id),
            alias => self.account_aliases.insert(id.to_owned(), alias.to_owned()),
        };
    }

    /// Saves `listed`, the new order of the Accounts the sidebar lists, keeping each Disabled
    /// Account in its slot among `accounts`.
    pub fn set_listed_order(&mut self, accounts: &[Account], listed: Vec<String>) {
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
    /// Account is neither Disabled nor Hidden.
    pub fn shown_calendar_ids(&self, calendars: &[Calendar]) -> Vec<String> {
        calendars
            .iter()
            .filter(|calendar| {
                !self.disabled_accounts.contains(&calendar.account_id)
                    && !self.hidden_accounts.contains(&calendar.account_id)
                    && !self.disabled_calendars.contains(&calendar.id)
            })
            .map(|calendar| calendar.id.clone())
            .collect()
    }
}

/// An Account's label: its email when EventKit gives it away, else its System Settings name.
pub fn account_label(account: &Account) -> &str {
    account.email.as_deref().unwrap_or(&account.name)
}

/// Adds `id` to `set`, or removes it when it is already there.
pub fn toggle(set: &mut BTreeSet<String>, id: &str) {
    if !set.remove(id) {
        set.insert(id.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Account, AccountColor, Calendar, CalendarView, SidebarSection, Store, Weekday,
        account_label, toggle,
    };
    use crumble_eventkit::Provider;

    #[test]
    fn saved_settings_read_back() {
        let mut store = Store {
            first_day_of_week: Weekday::Sun,
            calendar_view: CalendarView::Year,
            ..Store::default()
        };
        store.disabled_accounts.insert("source-1".into());
        store.disabled_calendars.insert("calendar-1".into());
        store.hidden_accounts.insert("source-2".into());
        store.folded_sections.insert(SidebarSection::Calendars);
        store.account_order = vec!["source-2".into(), "source-1".into()];
        store
            .account_colors
            .insert("source-1".into(), AccountColor::Pink);
        store
            .account_colors
            .insert("source-2".into(), AccountColor::Custom(0x3366ff));
        store.set_account_alias("source-1", "Work", "me@example.com");
        let json = serde_json::to_string(&store).unwrap_or_default();
        assert_eq!(Store::parse(&json), store);
    }

    #[test]
    fn corrupt_or_partial_files_fall_back_to_defaults() {
        assert_eq!(Store::parse("not json"), Store::default());
        assert_eq!(Store::parse("{}"), Store::default());
        assert_eq!(Store::default().first_day_of_week, Weekday::Mon);
        // Files saved before Accounts could be Disabled keep their First day of week.
        let old = Store::parse(r#"{"first_day_of_week":"Sun"}"#);
        assert_eq!(old.first_day_of_week, Weekday::Sun);
        assert!(old.disabled_accounts.is_empty() && old.disabled_calendars.is_empty());
        assert!(old.hidden_accounts.is_empty());
        assert!(old.folded_sections.is_empty() && old.account_colors.is_empty());
        assert!(old.account_order.is_empty() && old.account_aliases.is_empty());
        // Files saved before the Calendar view was remembered open on Month view.
        assert_eq!(old.calendar_view, CalendarView::Month);
        // Files saved before custom colors keep their palette colors.
        let old = Store::parse(r#"{"account_colors":{"source-1":"Pink"}}"#);
        assert_eq!(old.account_color("source-1"), AccountColor::Pink);
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
        let mut store = Store::default();
        toggle(&mut store.disabled_calendars, "holidays");
        assert_eq!(store.shown_calendar_ids(&calendars), ["work", "family"]);

        toggle(&mut store.disabled_accounts, "google");
        assert_eq!(store.shown_calendar_ids(&calendars), ["family"]);

        toggle(&mut store.disabled_accounts, "google");
        assert_eq!(store.shown_calendar_ids(&calendars), ["work", "family"]);
    }

    #[test]
    fn a_hidden_account_hides_its_calendars_whether_or_not_it_is_disabled() {
        let calendars = [calendar("work", "google"), calendar("family", "icloud")];
        let mut store = Store::default();
        toggle(&mut store.hidden_accounts, "google");
        assert_eq!(store.shown_calendar_ids(&calendars), ["family"]);

        // Disabling and Enabling it again leaves it Hidden.
        toggle(&mut store.disabled_accounts, "google");
        toggle(&mut store.disabled_accounts, "google");
        assert_eq!(store.shown_calendar_ids(&calendars), ["family"]);
    }

    #[test]
    fn reordering_the_listed_accounts_keeps_disabled_ones_in_their_slot() {
        let accounts = ["a", "b", "c", "d"].map(account);
        let mut store = Store::default();
        store.disabled_accounts.insert("b".into());
        store.set_listed_order(&accounts, vec!["d".into(), "a".into(), "c".into()]);
        assert_eq!(store.account_order, ["d", "b", "a", "c"]);

        // A list that no longer matches the Accounts leaves the order alone.
        store.set_listed_order(&accounts, vec!["a".into()]);
        assert_eq!(store.account_order, ["d", "b", "a", "c"]);
    }

    #[test]
    fn an_account_keeps_its_default_color_until_the_user_picks_one() {
        let mut store = Store::default();
        let default = store.account_color("source-1");
        assert_eq!(default, AccountColor::default_for("source-1"));
        assert_ne!(default, AccountColor::Gray);
        // Fixed across launches, and spread across the palette.
        assert_eq!(AccountColor::default_for("source-1"), default);
        let spread: std::collections::BTreeSet<_> = (0..40)
            .map(|n| format!("{:?}", AccountColor::default_for(&format!("source-{n}"))))
            .collect();
        assert!(spread.len() > 3);

        store
            .account_colors
            .insert("source-1".into(), AccountColor::Gray);
        assert_eq!(store.account_color("source-1"), AccountColor::Gray);
    }

    #[test]
    fn an_alias_is_trimmed_and_a_blank_one_or_the_own_name_removes_it() {
        let own = "me@example.com";
        let mut store = Store::default();
        assert_eq!(store.account_alias("source-1"), None);
        store.set_account_alias("source-1", "  Work ", own);
        assert_eq!(store.account_alias("source-1"), Some("Work"));
        store.set_account_alias("source-1", "   ", own);
        assert_eq!(store.account_alias("source-1"), None);

        store.set_account_alias("source-1", "Work", own);
        store.set_account_alias("source-1", " me@example.com", own);
        assert_eq!(store.account_alias("source-1"), None);
    }

    #[test]
    fn accounts_follow_the_saved_order_and_new_ones_come_last() {
        let accounts = ["a", "b", "c", "d"].map(account);
        let store = Store {
            account_order: vec!["c".into(), "gone".into(), "a".into()],
            ..Store::default()
        };
        let ids: Vec<&str> = store
            .ordered_accounts(&accounts)
            .iter()
            .map(|account| account.id.as_str())
            .collect();
        assert_eq!(ids, ["c", "a", "b", "d"]);
    }

    #[test]
    fn an_account_is_labelled_with_its_email_else_its_name() {
        let mut account = account("source-1");
        assert_eq!(account_label(&account), "source-1");
        account.email = Some("me@example.com".into());
        assert_eq!(account_label(&account), "me@example.com");
    }

    #[test]
    fn an_account_goes_by_its_alias_else_its_label() {
        let account = account("source-1");
        let mut store = Store::default();
        assert_eq!(store.account_name(&account), "source-1");
        store.set_account_alias("source-1", "Work", "source-1");
        assert_eq!(store.account_name(&account), "Work");
    }

    #[test]
    fn listed_accounts_skip_disabled_ones_and_keep_the_order() {
        let accounts = ["a", "b", "c"].map(account);
        let mut store = Store {
            account_order: vec!["c".into(), "b".into(), "a".into()],
            ..Store::default()
        };
        store.disabled_accounts.insert("b".into());
        let ids: Vec<&str> = store
            .listed_accounts(&accounts)
            .iter()
            .map(|account| account.id.as_str())
            .collect();
        assert_eq!(ids, ["c", "a"]);
    }
}
