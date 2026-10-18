//! Guesses EventKit leaves to Crumble: an Account's provider, an Event's join link, and the
//! parts of an Event's identity EventKit spells inconsistently.

use crate::{Join, Occurrence, Provider};
use chrono::{DateTime, Local, Utc};

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

/// The iCalendar UID in an external identifier. Exchange and some CalDAV servers append
/// `/RID=<n>` to it for a detached occurrence, which would split that occurrence from its series.
/// `<n>` is a whole number of seconds; any other suffix is part of the UID.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn series_uid(external: &str) -> Option<String> {
    let uid = match external.rsplit_once("/RID=") {
        Some((uid, seconds)) if seconds.parse::<i64>().is_ok() => uid,
        _ => external,
    };
    (!uid.is_empty()).then(|| uid.to_owned())
}

/// An occurrence that started at `original`: all-day ones by their local date, as
/// [`crate::When::AllDay`] reads it, timed ones by their instant. A floating Event, one with no
/// time zone, keeps its wall time whatever zone the Mac is in, so its key is that wall time read
/// as UTC.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn occurrence(all_day: bool, floating: bool, original: DateTime<Local>) -> Occurrence {
    if all_day {
        Occurrence::Day(original.date_naive())
    } else if floating {
        Occurrence::At(original.naive_local().and_utc())
    } else {
        Occurrence::At(original.with_timezone(&Utc))
    }
}

/// Hosts whose links open a video meeting, with the service's name. Subdomains match too, such
/// as `us02web.zoom.us`.
const MEETING_HOSTS: [(&str, &str); 10] = [
    ("meet.google.com", "Google Meet"),
    ("teams.microsoft.com", "Microsoft Teams"),
    ("teams.live.com", "Microsoft Teams"),
    ("zoom.us", "Zoom"),
    ("webex.com", "Webex"),
    ("facetime.apple.com", "FaceTime"),
    ("whereby.com", "Whereby"),
    ("meet.jit.si", "Jitsi Meet"),
    ("meet.goto.com", "GoTo Meeting"),
    ("chime.aws", "Amazon Chime"),
];

/// The words of `text` that could be links, without the brackets and punctuation around them.
fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| c.is_whitespace() || "<>\"'()[]".contains(c))
        .map(|word| word.trim_end_matches(['.', ',', ';']))
}

/// First meeting link in `fields`, searched in order. Unknown hosts never match.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn join_link<'a>(fields: impl IntoIterator<Item = Option<&'a str>>) -> Option<Join> {
    fields
        .into_iter()
        .flatten()
        .flat_map(words)
        .find_map(|word| {
            Some(Join {
                url: word.to_owned(),
                service: meeting_service(word)?,
            })
        })
}

/// The host of an `https://` URL: the authority after any `userinfo@`, without a numeric port.
/// Splitting on `:` before `@` would treat `https://zoom.us:443@evil.example/j/1` as Zoom.
/// Browsers end the authority at a `\` too, so `https://evil.example\@zoom.us` is not Zoom.
fn url_host(word: &str) -> Option<&str> {
    let rest = word.strip_prefix("https://")?;
    let authority = rest.split(['/', '\\', '?', '#']).next()?;
    if authority.is_empty() {
        return None;
    }
    let hostport = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = match hostport.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => host,
        _ => hostport,
    };
    (!host.is_empty()).then_some(host)
}

fn meeting_service(word: &str) -> Option<&'static str> {
    let host = url_host(word)?.to_ascii_lowercase();
    MEETING_HOSTS
        .iter()
        .find(|(known, _)| {
            host.strip_suffix(known)
                .is_some_and(|rest| rest.is_empty() || rest.ends_with('.'))
        })
        .map(|&(_, service)| service)
}

/// `notes` as plain text for the details, or `None` when nothing is left: HTML, as Google sends
/// a description edited on the web, becomes text; the meeting details Google and Teams add
/// between marker lines go; so does every meeting link, since the Join button stands for them.
// ponytail: Zoom and Webex invitations have no markers, so their dial-in text stays.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn description(notes: &str) -> Option<String> {
    let text = html_to_text(notes);
    let mut kept: Vec<&str> = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if !is_marker(line) {
            kept.push(line);
            continue;
        }
        // A block that holds a meeting link goes, up to its closing marker or the end.
        let mut block = vec![line];
        for line in lines.by_ref() {
            block.push(line);
            if is_marker(line) {
                break;
            }
        }
        if !block
            .iter()
            .any(|line| words(line).any(|w| meeting_service(w).is_some()))
        {
            kept.extend(block);
        }
    }
    let mut out = String::new();
    let mut blank = true;
    for line in kept {
        let line = without_meeting_links(line);
        let line = line.trim_end();
        if line.trim().is_empty() {
            blank = true;
            continue;
        }
        if blank && !out.is_empty() {
            out.push('\n');
        }
        blank = false;
        out.push_str(line);
        out.push('\n');
    }
    let out = out.trim_end();
    (!out.is_empty()).then(|| out.to_owned())
}

/// The `href` of the tag whose body, between `<` and `>`, is `tag`.
fn href(tag: &str) -> Option<String> {
    let value = &tag[tag.find("href=")? + "href=".len()..];
    let quote = value.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let value = &value[1..];
    Some(value[..value.find(quote)?].to_owned())
}

/// Ends the link whose text starts at `start` in `text`: a web link whose text is not its
/// address becomes `[text](address)`, and one whose text is its address stays the address.
fn close_anchor(text: &mut String, start: usize, href: &str) {
    let label = text[start..].trim();
    let web = href.starts_with("https://") || href.starts_with("http://");
    if !web || label == href {
        return;
    }
    if label.is_empty() || label.contains(['[', ']']) {
        text.truncate(start);
        text.push_str(href);
    } else {
        let label = label.to_owned();
        text.truncate(start);
        text.push_str(&format!("[{label}]({href})"));
    }
}

/// Google's `-::~:~::~…::-` and Teams' `____…` lines around the meeting details they add.
fn is_marker(line: &str) -> bool {
    let line = line.trim();
    line.starts_with("-::~") || (line.len() >= 20 && line.chars().all(|c| c == '_'))
}

/// `line` with its meeting links removed, along with the angle brackets Teams puts around them;
/// a `[text](meeting link)` keeps its text.
fn without_meeting_links(line: &str) -> String {
    words(line)
        .filter(|word| meeting_service(word).is_some())
        .fold(line.to_owned(), |line, link| {
            unwrap_labelled(line, link)
                .replace(&format!("<{link}>"), "")
                .replace(link, "")
        })
}

/// `line` with each `[text](link)` written as its text.
fn unwrap_labelled(mut line: String, link: &str) -> String {
    let tail = format!("]({link})");
    while let Some(close) = line.find(&tail) {
        let Some(open) = line[..close].rfind('[') else {
            break;
        };
        let label = line[open + 1..close].to_owned();
        line.replace_range(open..close + tail.len(), &label);
    }
    line
}

/// HTML tags dropped, line-breaking ones turned into newlines, and common entities decoded. A
/// link whose text is not its address is kept as `[text](address)`, so its address is not lost.
/// Text without tags, such as Teams' `<https://…>` links, is returned as it is.
fn html_to_text(html: &str) -> String {
    const BREAKS: [&str; 6] = ["br", "p", "div", "li", "ul", "ol"];
    let mut text = String::with_capacity(html.len());
    let mut rest = html;
    let mut tagged = false;
    // Where the open link's text starts in `text`, and its address.
    let mut anchor: Option<(usize, String)> = None;
    while let Some(open) = rest.find('<') {
        text.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let name: String = after
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        let tag_end = after.find('>');
        let is_tag = !name.is_empty()
            && name.starts_with(|c: char| c.is_ascii_alphabetic())
            && after
                .trim_start_matches('/')
                .chars()
                .nth(name.len())
                .is_some_and(|c| c == '>' || c == '/' || c.is_whitespace());
        match tag_end {
            Some(end) if is_tag => {
                tagged = true;
                let name = name.to_ascii_lowercase();
                if BREAKS.contains(&name.as_str()) {
                    text.push('\n');
                } else if name == "a" && after.starts_with('/') {
                    if let Some((start, href)) = anchor.take() {
                        close_anchor(&mut text, start, &href);
                    }
                } else if name == "a" {
                    anchor = href(&after[..end]).map(|href| (text.len(), href));
                }
                rest = &after[end + 1..];
            }
            _ => {
                text.push('<');
                rest = after;
            }
        }
    }
    text.push_str(rest);
    if !tagged {
        return html.to_owned();
    }
    [
        ("&nbsp;", " "),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&amp;", "&"),
    ]
    .into_iter()
    .fold(text, |text, (entity, char)| text.replace(entity, char))
}

#[cfg(test)]
mod tests;
