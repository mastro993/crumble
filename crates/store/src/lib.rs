//! State Crumble keeps across restarts, in a SQLite database under the platform data directory,
//! and the Account rules that read it.

mod account_color;
mod calendars_section;
mod db;
mod schema;

pub use account_color::AccountColor;
pub use calendars_section::{CalendarsSection, calendars_section};

use std::collections::BTreeSet;

use chrono::Weekday;
use crumble_calendar::CalendarView;
use crumble_eventkit::{Account, Calendar};
use db::State;
use diesel::{Connection, QueryResult, SqliteConnection};

/// Everything Crumble keeps across restarts. Reads come from memory; every change is written to
/// the database as it happens.
pub struct Store {
    state: State,
    /// `None` only when not even an in-memory database opens; writes are then skipped.
    connection: Option<SqliteConnection>,
}

/// A foldable sidebar section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SidebarSection {
    /// The Calendars section, listing Accounts.
    Calendars,
}

impl Store {
    /// Opens the app's database and reads it. When it cannot be opened, the app runs on the
    /// defaults in an in-memory database and leaves the file alone.
    pub fn open() -> Self {
        match db::open_file() {
            Ok(connection) => Self::read(connection),
            Err(error) => {
                eprintln!("Could not open the database, using defaults: {error}");
                Self::in_memory()
            }
        }
    }

    fn in_memory() -> Self {
        match db::open(":memory:") {
            Ok(connection) => Self::read(connection),
            Err(error) => {
                eprintln!("Could not open an in-memory database, changes are not kept: {error}");
                Self {
                    state: State::default(),
                    connection: None,
                }
            }
        }
    }

    fn read(mut connection: SqliteConnection) -> Self {
        let state = db::load(&mut connection).unwrap_or_else(|error| {
            eprintln!("Could not read the database, using defaults: {error}");
            State::default()
        });
        Self {
            state,
            connection: Some(connection),
        }
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

    /// Whether the Account `id` is Disabled.
    pub fn is_account_disabled(&self, id: &str) -> bool {
        self.state.disabled_accounts.contains(id)
    }

    /// Disables the Account `id`, or Enables it when it is Disabled.
    pub fn toggle_account_disabled(&mut self, id: &str) {
        toggle(&mut self.state.disabled_accounts, id);
        self.write(|connection, state| db::write_account(connection, state, id));
    }

    /// Whether the Account `id` is Hidden.
    pub fn is_account_hidden(&self, id: &str) -> bool {
        self.state.hidden_accounts.contains(id)
    }

    /// Hides the Account `id`, or shows it when it is Hidden.
    pub fn toggle_account_hidden(&mut self, id: &str) {
        toggle(&mut self.state.hidden_accounts, id);
        self.write(|connection, state| db::write_account(connection, state, id));
    }

    /// Whether the Calendar `id` is Disabled; kept while its Account is Disabled.
    pub fn is_calendar_disabled(&self, id: &str) -> bool {
        self.state.disabled_calendars.contains(id)
    }

    /// Disables the Calendar `id`, or Enables it when it is Disabled.
    pub fn toggle_calendar_disabled(&mut self, id: &str) {
        toggle(&mut self.state.disabled_calendars, id);
        self.write(|connection, state| db::write_calendar(connection, state, id));
    }

    /// Whether `section`'s rows are folded away.
    pub fn is_folded(&self, section: SidebarSection) -> bool {
        self.state.folded_sections.contains(&section)
    }

    /// Folds `section`'s rows away, or unfolds them.
    pub fn toggle_folded(&mut self, section: SidebarSection) {
        if !self.state.folded_sections.remove(&section) {
            self.state.folded_sections.insert(section);
        }
        self.write(|connection, state| db::write_folded(connection, state, section));
    }

    /// `accounts` in the user's order.
    pub fn ordered_accounts<'a>(&self, accounts: &'a [Account]) -> Vec<&'a Account> {
        let mut ordered: Vec<&Account> = accounts.iter().collect();
        // Stable, so unlisted Accounts keep EventKit's order after the listed ones.
        ordered.sort_by_key(|account| {
            self.state
                .account_order
                .iter()
                .position(|id| id == &account.id)
                .unwrap_or(usize::MAX)
        });
        ordered
    }

    /// `accounts` that are not Disabled, in the user's order.
    pub fn listed_accounts<'a>(&self, accounts: &'a [Account]) -> Vec<&'a Account> {
        let mut listed = self.ordered_accounts(accounts);
        listed.retain(|account| !self.is_account_disabled(&account.id));
        listed
    }

    /// The dot color of the Account `id`: the user's pick, else its default.
    pub fn account_color(&self, id: &str) -> AccountColor {
        self.state
            .account_colors
            .get(id)
            .copied()
            .unwrap_or_else(|| AccountColor::default_for(id))
    }

    /// Gives the Account `id` the dot color `color`.
    pub fn set_account_color(&mut self, id: &str, color: AccountColor) {
        self.state.account_colors.insert(id.to_owned(), color);
        self.write(|connection, state| db::write_account(connection, state, id));
    }

    /// The Alias of the Account `id`, if the user gave one.
    pub fn account_alias(&self, id: &str) -> Option<&str> {
        self.state.account_aliases.get(id).map(String::as_str)
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
            alias if alias.is_empty() || alias == own => self.state.account_aliases.remove(id),
            alias => self
                .state
                .account_aliases
                .insert(id.to_owned(), alias.to_owned()),
        };
        self.write(|connection, state| db::write_account(connection, state, id));
    }

    /// Saves `listed`, the new order of the Accounts the sidebar lists, keeping each Disabled
    /// Account in its slot among `accounts`.
    pub fn set_listed_order(&mut self, accounts: &[Account], listed: Vec<String>) {
        let mut listed = listed.into_iter();
        let order = self
            .ordered_accounts(accounts)
            .into_iter()
            .map(|account| {
                if self.is_account_disabled(&account.id) {
                    Some(account.id.clone())
                } else {
                    listed.next()
                }
            })
            .collect::<Option<_>>();
        // A list that no longer matches the Accounts leaves the order alone.
        if let Some(order) = order {
            self.state.account_order = order;
            self.write(db::write_account_order);
        }
    }

    /// Identifiers of the `calendars` whose Events show: the Calendar is not Disabled, and its
    /// Account is neither Disabled nor Hidden.
    pub fn shown_calendar_ids(&self, calendars: &[Calendar]) -> Vec<String> {
        calendars
            .iter()
            .filter(|calendar| {
                !self.is_account_disabled(&calendar.account_id)
                    && !self.is_account_hidden(&calendar.account_id)
                    && !self.is_calendar_disabled(&calendar.id)
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
fn toggle(set: &mut BTreeSet<String>, id: &str) {
    if !set.remove(id) {
        set.insert(id.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Account, AccountColor, Calendar, CalendarView, SidebarSection, Store, Weekday,
        account_label, db,
    };
    use crumble_eventkit::Provider;
    use diesel::{RunQueryDsl, sql_query};

    impl Store {
        /// What a reopened database reads.
        fn reloaded(&mut self) -> db::State {
            let connection = self.connection.as_mut().map(db::load);
            connection
                .and_then(Result::ok)
                .unwrap_or_else(|| panic!("no database"))
        }
    }

    #[test]
    fn a_new_database_holds_the_defaults() {
        let mut store = Store::in_memory();
        assert!(store.connection.is_some());
        assert_eq!(store.state, db::State::default());
        assert_eq!(store.first_day_of_week(), Weekday::Mon);
        assert_eq!(store.calendar_view(), CalendarView::Month);
        assert_eq!(store.reloaded(), db::State::default());
    }

    #[test]
    fn every_change_is_written_and_read_back() {
        let accounts = ["source-1", "source-2", "source-3"].map(account);
        let mut store = Store::in_memory();
        store.set_first_day_of_week(Weekday::Sun);
        store.set_calendar_view(CalendarView::Year);
        store.toggle_account_disabled("source-1");
        store.toggle_account_hidden("source-2");
        store.toggle_calendar_disabled("calendar-1");
        store.toggle_folded(SidebarSection::Calendars);
        store.set_account_color("source-1", AccountColor::Pink);
        store.set_account_color("source-2", AccountColor::Custom(0x3366ff));
        store.set_account_alias("source-3", "Work", "me@example.com");
        store.set_listed_order(&accounts, vec!["source-3".into(), "source-2".into()]);
        assert_eq!(
            store.state.account_order,
            ["source-1", "source-3", "source-2"]
        );
        assert_eq!(store.reloaded(), store.state);

        // Turning things back off is written too.
        store.toggle_account_disabled("source-1");
        store.toggle_account_hidden("source-2");
        store.toggle_calendar_disabled("calendar-1");
        store.toggle_folded(SidebarSection::Calendars);
        store.set_account_alias("source-3", "", "me@example.com");
        store.set_listed_order(
            &accounts,
            vec!["source-2".into(), "source-1".into(), "source-3".into()],
        );
        assert_eq!(store.reloaded(), store.state);
        assert!(store.state.disabled_accounts.is_empty());
        assert!(store.state.folded_sections.is_empty());
    }

    #[test]
    fn a_database_file_keeps_changes_across_opens() {
        let path = std::env::temp_dir().join(format!(
            "crumble-store-test-{}-{:?}.sqlite3",
            std::process::id(),
            std::time::SystemTime::now()
        ));
        let url = path.to_string_lossy().into_owned();
        let open = || Store::read(db::open(&url).unwrap_or_else(|error| panic!("{error}")));
        let mut store = open();
        store.set_first_day_of_week(Weekday::Wed);
        store.set_account_alias("source-1", "Work", "me@example.com");
        let written = store.state.clone();
        drop(store);
        let reopened = open().state;
        let _ = std::fs::remove_file(&path);
        assert_eq!(reopened, written);
    }

    #[test]
    fn a_bad_stored_color_falls_back_to_the_default() {
        let mut store = Store::in_memory();
        store.set_account_color("source-1", AccountColor::Red);
        if let Some(connection) = store.connection.as_mut() {
            let updated = sql_query("UPDATE accounts SET color = 'chartreuse'").execute(connection);
            assert_eq!(updated, Ok(1));
        }
        let reloaded = Store::read(
            store
                .connection
                .take()
                .unwrap_or_else(|| panic!("no database")),
        );
        assert_eq!(
            reloaded.account_color("source-1"),
            AccountColor::default_for("source-1")
        );
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
        let mut store = Store::in_memory();
        store.toggle_calendar_disabled("holidays");
        assert_eq!(store.shown_calendar_ids(&calendars), ["work", "family"]);

        store.toggle_account_disabled("google");
        assert_eq!(store.shown_calendar_ids(&calendars), ["family"]);

        store.toggle_account_disabled("google");
        assert_eq!(store.shown_calendar_ids(&calendars), ["work", "family"]);
    }

    #[test]
    fn a_hidden_account_hides_its_calendars_whether_or_not_it_is_disabled() {
        let calendars = [calendar("work", "google"), calendar("family", "icloud")];
        let mut store = Store::in_memory();
        store.toggle_account_hidden("google");
        assert_eq!(store.shown_calendar_ids(&calendars), ["family"]);

        // Disabling and Enabling it again leaves it Hidden.
        store.toggle_account_disabled("google");
        store.toggle_account_disabled("google");
        assert_eq!(store.shown_calendar_ids(&calendars), ["family"]);
    }

    #[test]
    fn reordering_the_listed_accounts_keeps_disabled_ones_in_their_slot() {
        let accounts = ["a", "b", "c", "d"].map(account);
        let mut store = Store::in_memory();
        store.toggle_account_disabled("b");
        store.set_listed_order(&accounts, vec!["d".into(), "a".into(), "c".into()]);
        assert_eq!(store.state.account_order, ["d", "b", "a", "c"]);

        // A list that no longer matches the Accounts leaves the order alone.
        store.set_listed_order(&accounts, vec!["a".into()]);
        assert_eq!(store.state.account_order, ["d", "b", "a", "c"]);
        assert_eq!(store.reloaded().account_order, ["d", "b", "a", "c"]);
    }

    #[test]
    fn an_account_keeps_its_default_color_until_the_user_picks_one() {
        let mut store = Store::in_memory();
        let default = store.account_color("source-1");
        assert_eq!(default, AccountColor::default_for("source-1"));
        assert_ne!(default, AccountColor::Gray);
        // Fixed across launches, and spread across the palette.
        assert_eq!(AccountColor::default_for("source-1"), default);
        let spread: std::collections::BTreeSet<_> = (0..40)
            .map(|n| format!("{:?}", AccountColor::default_for(&format!("source-{n}"))))
            .collect();
        assert!(spread.len() > 3);

        store.set_account_color("source-1", AccountColor::Gray);
        assert_eq!(store.account_color("source-1"), AccountColor::Gray);
    }

    #[test]
    fn an_alias_is_trimmed_and_a_blank_one_or_the_own_name_removes_it() {
        let own = "me@example.com";
        let mut store = Store::in_memory();
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
        let mut store = Store::in_memory();
        store.state.account_order = vec!["c".into(), "gone".into(), "a".into()];
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
        let mut store = Store::in_memory();
        assert_eq!(store.account_name(&account), "source-1");
        store.set_account_alias("source-1", "Work", "source-1");
        assert_eq!(store.account_name(&account), "Work");
    }

    #[test]
    fn listed_accounts_skip_disabled_ones_and_keep_the_order() {
        let accounts = ["a", "b", "c"].map(account);
        let mut store = Store::in_memory();
        store.state.account_order = vec!["c".into(), "b".into(), "a".into()];
        store.toggle_account_disabled("b");
        let ids: Vec<&str> = store
            .listed_accounts(&accounts)
            .iter()
            .map(|account| account.id.as_str())
            .collect();
        assert_eq!(ids, ["c", "a"]);
    }
}
