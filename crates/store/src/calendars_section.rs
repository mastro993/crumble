//! Which state the sidebar's Calendars section shows.

use crumble_eventkit::Access;

/// What the Calendars section shows.
#[derive(Debug, PartialEq, Eq)]
pub enum CalendarsSection {
    /// No EventKit on this platform.
    Hidden,
    /// One row that asks for access.
    Connect,
    /// One row that opens Privacy settings, the only place a refusal can be undone.
    AllowAccess,
    /// A hint that opens Internet Accounts settings.
    AddAccounts,
    /// A hint that opens Settings, where every Account is Disabled.
    AllDisabled,
    /// One row per Account that is not Disabled; none until the first read finishes.
    Accounts,
}

/// `listed` counts the `accounts` that are not Disabled.
pub fn calendars_section(
    access: Access,
    loaded: bool,
    accounts: usize,
    listed: usize,
) -> CalendarsSection {
    match access {
        Access::Unsupported => CalendarsSection::Hidden,
        Access::NotDetermined => CalendarsSection::Connect,
        Access::Denied => CalendarsSection::AllowAccess,
        Access::Granted if loaded && accounts == 0 => CalendarsSection::AddAccounts,
        Access::Granted if listed == 0 && accounts > 0 => CalendarsSection::AllDisabled,
        Access::Granted => CalendarsSection::Accounts,
    }
}

#[cfg(test)]
mod tests {
    use super::{Access, CalendarsSection, calendars_section};

    #[test]
    fn each_access_state_picks_its_rows() {
        assert_eq!(
            calendars_section(Access::Unsupported, false, 0, 0),
            CalendarsSection::Hidden
        );
        assert_eq!(
            calendars_section(Access::NotDetermined, false, 0, 0),
            CalendarsSection::Connect
        );
        assert_eq!(
            calendars_section(Access::Denied, true, 2, 2),
            CalendarsSection::AllowAccess
        );
        assert_eq!(
            calendars_section(Access::Granted, true, 0, 0),
            CalendarsSection::AddAccounts
        );
        assert_eq!(
            calendars_section(Access::Granted, true, 2, 1),
            CalendarsSection::Accounts
        );
        assert_eq!(
            calendars_section(Access::Granted, true, 2, 0),
            CalendarsSection::AllDisabled
        );
        // No hint flashes before the first read finishes.
        assert_eq!(
            calendars_section(Access::Granted, false, 0, 0),
            CalendarsSection::Accounts
        );
    }
}
