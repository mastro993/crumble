//! Guesses EventKit leaves to Crumble: an Account's provider and an Event's join link.

use crate::Provider;

/// The EventKit source types that matter for guessing a [`Provider`].
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy)]
pub(crate) enum SourceKind {
    Exchange,
    CalDav,
    Other,
}

/// Google and iCloud both arrive as CalDAV, so the title tells them apart. A Google Workspace
/// account on its own domain has neither in its title, so `google_events` is asked last: whether
/// the Account's Events carry Google's external identifiers.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn provider(
    kind: SourceKind,
    title: &str,
    google_events: impl FnOnce() -> bool,
) -> Provider {
    let title = title.to_lowercase();
    match kind {
        SourceKind::Exchange => Provider::Outlook,
        SourceKind::CalDav if title == "icloud" => Provider::Apple,
        SourceKind::CalDav if title.contains("gmail") || title.contains("google") => {
            Provider::Google
        }
        SourceKind::CalDav if google_events() => Provider::Google,
        _ => Provider::Other,
    }
}

/// The Account's email: the user's own address on its Events, else its one Calendar named after
/// an address. Several such Calendars are shared ones too, so none of them is trusted.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn account_email<'a>(
    own_address: Option<String>,
    calendar_names: impl IntoIterator<Item = &'a str>,
) -> Option<String> {
    own_address.or_else(|| {
        let mut addresses = calendar_names.into_iter().filter(|name| is_email(name));
        match (addresses.next(), addresses.next()) {
            (Some(address), None) => Some(address.to_owned()),
            _ => None,
        }
    })
}

/// The address in a participant's `mailto:` URL.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn mailto_address(url: &str) -> Option<String> {
    let (scheme, address) = url.split_once(':')?;
    (scheme.eq_ignore_ascii_case("mailto") && is_email(address)).then(|| address.to_owned())
}

fn is_email(text: &str) -> bool {
    let Some((user, domain)) = text.split_once('@') else {
        return false;
    };
    !user.is_empty()
        && !domain.contains('@')
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !text.contains(char::is_whitespace)
}

/// Google Calendar gives every Event an external identifier ending in `@google.com`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn is_google_event_id(external_id: &str) -> bool {
    external_id.ends_with("@google.com")
}

/// Hosts whose links open a video meeting. Subdomains match too, such as `us02web.zoom.us`.
const MEETING_HOSTS: [&str; 5] = [
    "meet.google.com",
    "teams.microsoft.com",
    "teams.live.com",
    "zoom.us",
    "webex.com",
];

/// First meeting link in `fields`, searched in order. Unknown hosts never match.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn join_link<'a>(fields: impl IntoIterator<Item = Option<&'a str>>) -> Option<String> {
    fields
        .into_iter()
        .flatten()
        .flat_map(|text| text.split(|c: char| c.is_whitespace() || "<>\"'()[]".contains(c)))
        .map(|word| word.trim_end_matches(['.', ',', ';']))
        .find(|word| is_meeting_link(word))
        .map(str::to_owned)
}

fn is_meeting_link(word: &str) -> bool {
    let Some(rest) = word.strip_prefix("https://") else {
        return false;
    };
    let host = rest
        .split(['/', '?', '#', ':'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    MEETING_HOSTS
        .iter()
        .any(|known| host == *known || host.ends_with(&format!(".{known}")))
}

#[cfg(test)]
mod tests {
    use super::{
        Provider, SourceKind, account_email, is_google_event_id, join_link, mailto_address,
        provider,
    };

    #[test]
    fn the_own_address_wins_then_a_single_calendar_named_like_one() {
        let own = Some("me@work.com".to_owned());
        assert_eq!(account_email(own.clone(), ["boss@work.com"]), own);
        assert_eq!(
            account_email(None, ["me@work.com", "Holidays in Italy"]).as_deref(),
            Some("me@work.com")
        );
        // Shared Calendars are named after other people too, so two addresses are ambiguous.
        assert_eq!(account_email(None, ["me@work.com", "boss@work.com"]), None);
        assert_eq!(account_email(None, ["Home", "Birthdays"]), None);
    }

    #[test]
    fn mailto_urls_give_their_address() {
        assert_eq!(
            mailto_address("mailto:me@work.com").as_deref(),
            Some("me@work.com")
        );
        assert_eq!(
            mailto_address("MAILTO:me@work.com").as_deref(),
            Some("me@work.com")
        );
        assert_eq!(mailto_address("tel:+390000"), None);
        assert_eq!(mailto_address("mailto:not an address"), None);
        assert_eq!(mailto_address("mailto:me@localhost"), None);
    }

    #[test]
    fn providers_follow_source_type_and_title() {
        let no = || false;
        assert_eq!(
            provider(SourceKind::Exchange, "Work", no),
            Provider::Outlook
        );
        assert_eq!(provider(SourceKind::CalDav, "iCloud", no), Provider::Apple);
        assert_eq!(
            provider(SourceKind::CalDav, "me@Gmail.com", no),
            Provider::Google
        );
        assert_eq!(provider(SourceKind::CalDav, "Google", no), Provider::Google);
        assert_eq!(
            provider(SourceKind::CalDav, "me@fastmail.com", no),
            Provider::Other
        );
        assert_eq!(provider(SourceKind::Other, "Google", no), Provider::Other);
    }

    #[test]
    fn workspace_accounts_are_google_by_their_events() {
        let yes = || true;
        assert_eq!(provider(SourceKind::CalDav, "Acme", yes), Provider::Google);
        assert_eq!(provider(SourceKind::Other, "Acme", yes), Provider::Other);
        // A title match never pays for the Event lookup.
        let unreachable = || panic!("looked up Events for a known title");
        assert_eq!(
            provider(SourceKind::CalDav, "iCloud", unreachable),
            Provider::Apple
        );
        assert!(is_google_event_id("5f3k2@google.com"));
        assert!(!is_google_event_id("A1B2-C3D4"));
        assert!(!is_google_event_id("x@google.com.evil"));
    }

    #[test]
    fn join_link_takes_the_first_meeting_link_in_field_order() {
        let notes = "Agenda\n<https://teams.microsoft.com/l/meetup-join/abc>\nhttps://zoom.us/j/1";
        assert_eq!(
            join_link([None, Some("Room 4"), Some(notes)]).as_deref(),
            Some("https://teams.microsoft.com/l/meetup-join/abc")
        );
        assert_eq!(
            join_link([Some("https://meet.google.com/abc-defg-hij"), Some(notes)]).as_deref(),
            Some("https://meet.google.com/abc-defg-hij")
        );
    }

    #[test]
    fn join_link_matches_subdomains_and_drops_trailing_punctuation() {
        assert_eq!(
            join_link([Some("Join: https://us02web.zoom.us/j/123?pwd=x.")]).as_deref(),
            Some("https://us02web.zoom.us/j/123?pwd=x")
        );
    }

    #[test]
    fn join_link_ignores_unknown_hosts_and_lookalikes() {
        assert_eq!(join_link([Some("https://example.com/zoom.us")]), None);
        assert_eq!(join_link([Some("https://notzoom.us/j/1")]), None);
        assert_eq!(join_link([Some("http://zoom.us/j/1")]), None);
        assert_eq!(join_link([None, None]), None);
    }
}
