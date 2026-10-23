//! Addresses requests go to: whether they can be sent to, their host, and how errors and logs
//! show them.

use anyhow::bail;
use ureq::http::Uri;

/// Whether `url` can be sent to and its host is `localhost`, `127.0.0.1` or `::1`.
pub(crate) fn is_loopback(url: &str) -> bool {
    host(url).is_some_and(|host| host == "localhost" || host == "127.0.0.1" || host == "::1")
}

/// `url`'s host in lowercase, without a port, credentials or IPv6 brackets: `api.openai.com`
/// for `https://api.openai.com/v1`. `None` when requests cannot go to `url`.
pub fn host(url: &str) -> Option<String> {
    let uri = parse(url)?;
    let host = uri.host()?;
    Some(
        host.trim_start_matches('[')
            .trim_end_matches(']')
            .to_ascii_lowercase(),
    )
}

/// `url` with its spaces and trailing `/` trimmed, for a path to follow; or, for the user, why
/// requests cannot go to it.
pub fn base_url(url: &str) -> anyhow::Result<&str> {
    let base = url.trim().trim_end_matches('/');
    if base.is_empty() {
        bail!("Enter the server's address in Settings.");
    }
    if !base.contains("://") {
        bail!("Start the server's address with http:// or https://.");
    }
    if parse(base).is_none() {
        bail!("Check the server's address in Settings.");
    }
    Ok(base)
}

/// `url` read as the HTTP client reads it, so what is judged local or asked about is where a
/// request goes: `None` unless it has an `http` or `https` scheme and a host.
fn parse(url: &str) -> Option<Uri> {
    let uri: Uri = url.trim().parse().ok()?;
    let sendable = matches!(uri.scheme_str(), Some("http" | "https")) && uri.host().is_some();
    sendable.then_some(uri)
}

/// `url` without the `user:password@` before its host, to show in errors and logs, even when
/// it cannot be sent to.
pub fn without_userinfo(url: &str) -> String {
    let [scheme, _, authority, rest] = url_parts(url);
    format!("{scheme}{authority}{rest}")
}

/// `url` in four parts: its scheme with `://` (empty without one), any `user:password@`, its host
/// and port, and the rest.
fn url_parts(url: &str) -> [&str; 4] {
    let start = url.find("://").map_or(0, |at| at + 3);
    let end = url[start..]
        .find(['/', '?', '#'])
        .map_or(url.len(), |at| start + at);
    // Credentials before an `@` are not the host.
    let host = url[start..end]
        .rfind('@')
        .map_or(start, |at| start + at + 1);
    [
        &url[..start],
        &url[start..host],
        &url[host..end],
        &url[end..],
    ]
}

#[cfg(test)]
mod tests {
    use super::{base_url, host, is_loopback, without_userinfo};

    #[test]
    fn only_a_sendable_loopback_address_is_loopback() {
        for local in [
            "http://localhost:11434/v1",
            "http://LOCALHOST:1234/v1/",
            "http://127.0.0.1:8765/v1",
            "https://[::1]:8080/v1",
            " http://user:pw@127.0.0.1/v1 ",
        ] {
            assert!(is_loopback(local), "{local}");
        }
        for remote in [
            "",
            "localhost:11434/v1",
            "http://192.168.1.20:8765/v1",
            "https://api.openai.com/v1",
            "http://localhost.example.com/v1",
            "http://127.0.0.1.example.com/v1",
            "http://localhost@evil.example.com/v1",
            "http://[::2]/v1",
            "http://10.0.0.1/localhost",
            "evil.example.com/p?u=http://localhost/v1",
            "http://evil.example.com\\@localhost/v1",
            "ftp://localhost/v1",
        ] {
            assert!(!is_loopback(remote), "{remote}");
        }
    }

    #[test]
    fn a_host_drops_scheme_port_path_and_credentials() {
        let host = |url| host(url);
        assert_eq!(
            host("https://API.openai.com/v1").as_deref(),
            Some("api.openai.com")
        );
        assert_eq!(
            host(" http://user:pw@whisper.lan:8000/v1/ ").as_deref(),
            Some("whisper.lan")
        );
        assert_eq!(host("https://[::1]:8080/v1").as_deref(), Some("::1"));
        assert_eq!(
            host("http://example.com?x=1").as_deref(),
            Some("example.com")
        );
        for unsendable in [
            "",
            "localhost:11434",
            "evil.example.com/p?u=http://localhost/v1",
        ] {
            assert_eq!(host(unsendable), None, "{unsendable}");
        }
    }

    #[test]
    fn a_base_url_must_be_sendable() {
        assert_eq!(
            base_url(" http://localhost:8000/v1/ ").ok(),
            Some("http://localhost:8000/v1")
        );
        let error = |url| base_url(url).map_err(|error| error.to_string()).err();
        assert_eq!(
            error(" ").as_deref(),
            Some("Enter the server's address in Settings.")
        );
        assert_eq!(
            error("whisper.lan:8000/v1").as_deref(),
            Some("Start the server's address with http:// or https://.")
        );
        assert_eq!(
            error("evil.example.com/p?u=http://localhost/v1").as_deref(),
            Some("Check the server's address in Settings.")
        );
    }

    #[test]
    fn an_address_loses_its_user_name_and_password() {
        assert_eq!(
            without_userinfo("http://me:pw@127.0.0.1:11434/v1"),
            "http://127.0.0.1:11434/v1"
        );
        assert_eq!(
            without_userinfo("http://localhost:11434/v1/a@b"),
            "http://localhost:11434/v1/a@b"
        );
        assert_eq!(without_userinfo("me@host/v1"), "host/v1");
    }
}
