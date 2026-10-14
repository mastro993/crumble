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

/// Google and iCloud both arrive as CalDAV, so only the title tells them apart.
// ponytail: title heuristics; Google Workspace on a custom domain falls to Other. Checking for
// event identifiers ending in `@google.com` would catch it.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn provider(kind: SourceKind, title: &str) -> Provider {
    let title = title.to_lowercase();
    match kind {
        SourceKind::Exchange => Provider::Outlook,
        SourceKind::CalDav if title == "icloud" => Provider::Apple,
        SourceKind::CalDav if title.contains("gmail") || title.contains("google") => {
            Provider::Google
        }
        _ => Provider::Other,
    }
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
    use super::{Provider, SourceKind, join_link, provider};

    #[test]
    fn providers_follow_source_type_and_title() {
        assert_eq!(provider(SourceKind::Exchange, "Work"), Provider::Outlook);
        assert_eq!(provider(SourceKind::CalDav, "iCloud"), Provider::Apple);
        assert_eq!(
            provider(SourceKind::CalDav, "me@Gmail.com"),
            Provider::Google
        );
        assert_eq!(provider(SourceKind::CalDav, "Google"), Provider::Google);
        assert_eq!(
            provider(SourceKind::CalDav, "me@fastmail.com"),
            Provider::Other
        );
        assert_eq!(provider(SourceKind::Other, "Google"), Provider::Other);
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
