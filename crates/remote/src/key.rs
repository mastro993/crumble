//! What a remote Provider's API keys look like, so Settings can tell a mistyped or wrong key
//! before it sends a request with it.

use regex::Regex;

/// What a remote Provider's API keys look like, and how they are sent.
#[derive(Debug)]
pub struct KeyFormat {
    /// A regex every whole key of this Provider matches; `None` takes any text.
    pub(crate) pattern: Option<&'static str>,
    /// How its keys start, for the hint under the field, such as `sk-ant-`; one of them
    /// before a key that fails `pattern` makes it look incomplete rather than another kind.
    pub(crate) starts: &'static [&'static str],
    /// Keys of another kind this Provider also issues, which its API refuses for this use: a
    /// regex each matches from its start, and what to tell the user.
    pub(crate) refused: &'static [(&'static str, &'static str)],
    /// Whether the Provider needs a key; a server the user runs may not.
    pub(crate) required: bool,
    /// The header the key goes in.
    pub(crate) header: KeyHeader,
}

/// The header an API key goes in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyHeader {
    /// `Authorization: Bearer <key>`.
    Bearer,
    /// A header of its own, such as Anthropic's `x-api-key`.
    Named(&'static str),
}

/// What [`KeyFormat::check`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyCheck {
    /// Nothing typed yet.
    Empty,
    /// It looks like this Provider's key: ping it.
    Valid,
    /// It can't be one, and why, for the user.
    Invalid(String),
}

impl KeyFormat {
    /// Whether `key`, as typed or pasted, can be one of this Provider's keys. Spaces around it
    /// are ignored, as a paste often carries them.
    pub fn check(&self, key: &str) -> KeyCheck {
        let key = key.trim();
        if key.is_empty() {
            return KeyCheck::Empty;
        }
        for (pattern, why) in self.refused {
            if matches(pattern, key) {
                return KeyCheck::Invalid((*why).to_owned());
            }
        }
        match self.pattern {
            Some(pattern) if !matches(pattern, key) => KeyCheck::Invalid(if key.contains(' ') {
                "A key has no spaces. Paste it again.".to_owned()
            } else if self.starts.iter().any(|start| key.starts_with(start)) {
                "This key looks incomplete. Paste it again.".to_owned()
            } else {
                format!("These keys start with {}.", self.starts.join(" or "))
            }),
            _ => KeyCheck::Valid,
        }
    }

    /// Whether the Provider needs a key before it can be used.
    pub fn required(&self) -> bool {
        self.required
    }

    /// The header and value that send `key`.
    pub(crate) fn header(&self, key: &str) -> (&'static str, String) {
        match self.header {
            KeyHeader::Bearer => ("authorization", format!("Bearer {}", key.trim())),
            KeyHeader::Named(name) => (name, key.trim().to_owned()),
        }
    }
}

/// Whether `key` matches `pattern`. Patterns are this crate's constants, which a test compiles,
/// so one that fails to compile matches nothing rather than panicking.
fn matches(pattern: &str, key: &str) -> bool {
    Regex::new(pattern).is_ok_and(|regex| regex.is_match(key))
}

#[cfg(test)]
mod tests {
    use super::{KeyCheck, KeyFormat, KeyHeader};

    const FORMAT: KeyFormat = KeyFormat {
        pattern: Some(r"^sk-ant-api\d{2}-[A-Za-z0-9_-]{40,}$"),
        starts: &["sk-ant-api"],
        refused: &[(r"^sk-ant-admin\d{2}-", "This is an Admin key.")],
        required: true,
        header: KeyHeader::Named("x-api-key"),
    };

    #[test]
    fn a_key_is_checked_against_its_provider_s_format() {
        let key = format!("sk-ant-api03-{}AA", "a".repeat(93));
        assert_eq!(FORMAT.check(""), KeyCheck::Empty);
        assert_eq!(FORMAT.check("  "), KeyCheck::Empty);
        assert_eq!(FORMAT.check(&format!(" {key}\n")), KeyCheck::Valid);
        assert_eq!(
            FORMAT.check("sk-ant-api03-short"),
            KeyCheck::Invalid("This key looks incomplete. Paste it again.".to_owned())
        );
        assert_eq!(
            FORMAT.check("sk-proj-abc"),
            KeyCheck::Invalid("These keys start with sk-ant-api.".to_owned())
        );
        assert_eq!(
            FORMAT.check(&format!(
                "sk-ant-api03-{} {}",
                "a".repeat(40),
                "b".repeat(40)
            )),
            KeyCheck::Invalid("A key has no spaces. Paste it again.".to_owned())
        );
        assert_eq!(
            FORMAT.check(&format!("sk-ant-admin01-{}", "a".repeat(93))),
            KeyCheck::Invalid("This is an Admin key.".to_owned())
        );
        // A key may start one of several ways.
        let two = KeyFormat {
            pattern: Some(r"^(?:AIza[0-9A-Za-z_-]{35}|AQ\.[0-9A-Za-z_.-]{20,})$"),
            starts: &["AQ.", "AIza"],
            ..FORMAT
        };
        assert_eq!(
            two.check("AIzaSyB12"),
            KeyCheck::Invalid("This key looks incomplete. Paste it again.".to_owned())
        );
        assert_eq!(
            two.check("sk-abc"),
            KeyCheck::Invalid("These keys start with AQ. or AIza.".to_owned())
        );
    }

    #[test]
    fn a_key_goes_in_its_provider_s_header() {
        assert_eq!(FORMAT.header(" k1 "), ("x-api-key", "k1".to_owned()));
        let bearer = KeyFormat {
            header: KeyHeader::Bearer,
            ..FORMAT
        };
        assert_eq!(
            bearer.header("k1"),
            ("authorization", "Bearer k1".to_owned())
        );
    }
}
