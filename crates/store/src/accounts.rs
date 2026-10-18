//! Account rules: which Accounts and Calendars are Disabled or Hidden, and the Accounts' order,
//! colors, Aliases and labels.

use std::collections::BTreeSet;

use crate::{AccountColor, Store, db};
use crumble_eventkit::{Account, Calendar};

impl Store {
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
