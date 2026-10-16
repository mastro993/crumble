//! Links in an Event's description: `[label](address)` links and bare web addresses.

use std::ops::Range;

/// `text` as shown, each `[label](address)` written as its label, with the byte ranges of its
/// links in what is shown and where each goes: those labels and the bare web links.
pub fn parse_links(text: &str) -> (String, Vec<(Range<usize>, String)>) {
    let mut shown = String::with_capacity(text.len());
    let mut links = Vec::new();
    let mut rest = text;
    while let Some(next) = rest.chars().next() {
        let (label, url, len) = if let Some((label, url, len)) = labelled_link(rest) {
            (label, url, len)
        } else if let Some(len) = bare_link(rest) {
            (&rest[..len], &rest[..len], len)
        } else {
            shown.push(next);
            rest = &rest[next.len_utf8()..];
            continue;
        };
        let start = shown.len();
        shown.push_str(label);
        links.push((start..shown.len(), url.to_owned()));
        rest = &rest[len..];
    }
    (shown, links)
}

/// The label, the address, and the length of the `[label](address)` link `text` starts with.
fn labelled_link(text: &str) -> Option<(&str, &str, usize)> {
    let inner = text.strip_prefix('[')?;
    let close = inner.find("](")?;
    let label = &inner[..close];
    let after = &inner[close + 2..];
    let url = &after[..after.find(')')?];
    let fits = !label.is_empty() && !label.contains(['[', ']', '\n']);
    (fits && is_web_link(url) && !url.contains(char::is_whitespace)).then_some((
        label,
        url,
        1 + close + 2 + url.len() + 1,
    ))
}

/// The length of the web link `text` starts with, without the punctuation after it.
fn bare_link(text: &str) -> Option<usize> {
    let len = text
        .find(|c: char| c.is_whitespace() || "<>\"'()[]".contains(c))
        .unwrap_or(text.len());
    let link = text[..len].trim_end_matches(['.', ',', ';', ':', '!', '?']);
    is_web_link(link).then_some(link.len())
}

/// An `https://` or `http://` address with something after the scheme.
fn is_web_link(text: &str) -> bool {
    ["https://", "http://"].into_iter().any(|scheme| {
        text.strip_prefix(scheme)
            .is_some_and(|rest| !rest.is_empty())
    })
}

/// `text` with its line breaks and runs of spaces turned into single spaces. GPUI clamps only
/// wrapped lines, so a description of many short lines would otherwise grow the card unclamped.
pub fn one_paragraph(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::parse_links;

    /// What `text` shows, and each link as its shown text and where it goes.
    fn links(text: &str) -> (String, Vec<(String, String)>) {
        let (shown, links) = parse_links(text);
        let links = links
            .into_iter()
            .map(|(range, url)| (shown[range].to_owned(), url))
            .collect();
        (shown, links)
    }

    #[test]
    fn bare_links_are_found_without_the_punctuation_around_them() {
        let text = "Notes: https://docs.example.com/a?b=1. Deck (http://x.io/d), <https://y.it>";
        let (shown, found) = links(text);
        assert_eq!(shown, text);
        let found: Vec<&str> = found.iter().map(|(text, _)| text.as_str()).collect();
        assert_eq!(
            found,
            [
                "https://docs.example.com/a?b=1",
                "http://x.io/d",
                "https://y.it"
            ]
        );
        // A bare scheme or a word that only starts like one is no link.
        assert!(links("see https:// or httpbin and ftp://x").1.is_empty());
    }

    #[test]
    fn labelled_links_show_their_label_and_go_to_their_address() {
        let (shown, found) =
            links("📆 [Notes on Fellow](https://fellow.link/Q8), è https://è.it/π");
        assert_eq!(shown, "📆 Notes on Fellow, è https://è.it/π");
        assert_eq!(
            found,
            [
                ("Notes on Fellow".into(), "https://fellow.link/Q8".into()),
                ("https://è.it/π".into(), "https://è.it/π".into()),
            ]
        );
        // Brackets that are not a link stay as they are.
        let plain = "[draft](see notes) and [x](ftp://y)";
        assert_eq!(links(plain), (plain.to_owned(), Vec::new()));
    }
}
