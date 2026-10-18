use super::{
    Join, Occurrence, Provider, SourceKind, account_email, description, is_google_event_id,
    join_link, mailto_address, occurrence, provider, series_uid,
};
use chrono::{Local, NaiveDate, TimeZone, Utc};

fn join_url<'a>(fields: impl IntoIterator<Item = Option<&'a str>>) -> Option<String> {
    join_link(fields).map(|join| join.url)
}

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
        join_url([None, Some("Room 4"), Some(notes)]),
        Some("https://teams.microsoft.com/l/meetup-join/abc".to_owned())
    );
    assert_eq!(
        join_url([Some("https://meet.google.com/abc-defg-hij"), Some(notes)]),
        Some("https://meet.google.com/abc-defg-hij".to_owned())
    );
}

#[test]
fn join_link_matches_subdomains_and_drops_trailing_punctuation() {
    assert_eq!(
        join_url([Some("Join: https://us02web.zoom.us/j/123?pwd=x.")]),
        Some("https://us02web.zoom.us/j/123?pwd=x".to_owned())
    );
}

#[test]
fn join_link_ignores_unknown_hosts_and_lookalikes() {
    assert_eq!(join_url([Some("https://example.com/zoom.us")]), None);
    assert_eq!(join_url([Some("https://notzoom.us/j/1")]), None);
    assert_eq!(join_url([Some("http://zoom.us/j/1")]), None);
    assert_eq!(join_url([None, None]), None);
}

#[test]
fn join_links_name_their_service() {
    assert_eq!(
        join_link([Some("https://meet.google.com/abc-defg-hij")]),
        Some(Join {
            url: "https://meet.google.com/abc-defg-hij".into(),
            service: "Google Meet",
        })
    );
    assert_eq!(
        join_link([Some("https://facetime.apple.com/join#v=1")]).map(|join| join.service),
        Some("FaceTime")
    );
}

#[test]
fn descriptions_drop_google_meeting_details_and_html() {
    let notes = "<b>Agenda</b><br>Review &amp; plan<br><br>\
                 <a href=\"https://docs.example.com/x\">https://docs.example.com/x</a>\n\n\
                 -::~:~::~:~:~:~::-\n\
                 Join with Google Meet: https://meet.google.com/abc-defg-hij\n\
                 Please do not edit this section.\n\
                 -::~:~::~:~:~:~::-";
    assert_eq!(
        description(notes).as_deref(),
        Some("Agenda\nReview & plan\n\nhttps://docs.example.com/x")
    );
}

#[test]
fn descriptions_drop_teams_blocks_and_bare_meeting_links() {
    let underline = "_".repeat(30);
    let notes = format!(
        "Weekly sync\n{underline}\nMicrosoft Teams meeting\n\
         Click here to join<https://teams.microsoft.com/l/meetup-join/abc>\n{underline}"
    );
    assert_eq!(description(&notes).as_deref(), Some("Weekly sync"));
    assert_eq!(
        description("Call at https://zoom.us/j/1 then lunch").as_deref(),
        Some("Call at  then lunch")
    );
    assert_eq!(description("https://zoom.us/j/1"), None);
    // Underlines without a meeting link and comparisons that only look like tags stay.
    let kept = format!("{underline}\nif a < b and c > d\n{underline}");
    assert_eq!(description(&kept).as_deref(), Some(kept.as_str()));
}

#[test]
fn html_links_keep_their_address() {
    let notes = "📆 <a href=\"https://fellow.link/Q8\">Notes on Fellow</a><br/>\
                 <a href='https://x.io/a'>https://x.io/a</a><br/>\
                 <a href=\"https://meet.google.com/abc-defg-hij\">Join the call</a>";
    assert_eq!(
        description(notes).as_deref(),
        Some("📆 [Notes on Fellow](https://fellow.link/Q8)\nhttps://x.io/a\nJoin the call")
    );
}

#[test]
fn html_meeting_blocks_go_when_their_link_is_only_in_an_href() {
    let notes = "Agenda<br>________________________________________<br>\
                 Microsoft Teams meeting<br>\
                 <a href=\"https://teams.microsoft.com/l/meetup-join/abc\">Join the meeting now</a><br>\
                 ________________________________________";
    assert_eq!(description(notes).as_deref(), Some("Agenda"));
}

#[test]
fn series_uids_drop_the_detached_occurrence_suffix() {
    assert_eq!(
        series_uid("040000008200E000/RID=811159200").as_deref(),
        Some("040000008200E000")
    );
    assert_eq!(
        series_uid("abc123@google.com").as_deref(),
        Some("abc123@google.com")
    );
    // A slash elsewhere is part of the UID.
    assert_eq!(
        series_uid("team/standup-1").as_deref(),
        Some("team/standup-1")
    );
    // Only a number of seconds is a detached occurrence's suffix.
    assert_eq!(
        series_uid("team/RID=draft").as_deref(),
        Some("team/RID=draft")
    );
    assert_eq!(series_uid(""), None);
    assert_eq!(series_uid("/RID=811159200"), None);
}

#[test]
fn all_day_occurrences_are_dates_and_timed_ones_instants() {
    let (Some(start), Some(wall)) = (
        Local.with_ymd_and_hms(2026, 10, 13, 9, 30, 0).earliest(),
        Utc.with_ymd_and_hms(2026, 10, 13, 9, 30, 0).earliest(),
    ) else {
        panic!("09:30 exists");
    };
    assert_eq!(
        occurrence(true, true, start),
        Occurrence::Day(NaiveDate::from_ymd_opt(2026, 10, 13).unwrap_or_default())
    );
    assert_eq!(
        occurrence(false, false, start),
        Occurrence::At(start.with_timezone(&Utc))
    );
    // A floating Event keeps its 09:30 in any zone.
    assert_eq!(occurrence(false, true, start), Occurrence::At(wall));
}
