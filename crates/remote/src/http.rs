//! The HTTP that reaches remote Providers, with `ureq`: blocking, as every caller already runs
//! off the main thread. Every status comes back to the caller, so failures become sentences.

use std::time::Duration;

use anyhow::anyhow;
use serde_json::Value;

use crate::Provider;

/// How long a JSON request may take in all, and the least an upload gets: a local Model can
/// take minutes on an hour's speech.
pub const TIMEOUT: Duration = Duration::from_secs(600);

/// POSTs `body` as JSON, within [`TIMEOUT`]. See [`post`].
pub fn post_json(
    url: &str,
    headers: &[(&str, &str)],
    body: &Value,
) -> Result<(u16, String), ureq::Error> {
    post(
        url,
        headers,
        ("application/json", body.to_string().as_bytes()),
        TIMEOUT,
    )
}

/// POSTs `body`, a content type and its bytes, and returns the status and the response's text,
/// whatever the status, within `timeout`. Only a request that got no response errors; its
/// error holds no header.
pub fn post(
    url: &str,
    headers: &[(&str, &str)],
    body: (&str, &[u8]),
    timeout: Duration,
) -> Result<(u16, String), ureq::Error> {
    read(send_post(url, headers, body, timeout)?)
}

/// POSTs like [`post`], and also returns the response's header `name`, if it has one: Google's
/// Files API answers the start of an upload with where to send the file, in
/// `x-goog-upload-url`.
pub fn post_reading_header(
    url: &str,
    headers: &[(&str, &str)],
    body: (&str, &[u8]),
    timeout: Duration,
    name: &str,
) -> Result<(u16, String, Option<String>), ureq::Error> {
    let response = send_post(url, headers, body, timeout)?;
    let value = (response.headers().get(name))
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let (status, text) = read(response)?;
    Ok((status, text, value))
}

/// GETs `url` and returns the status and the response's text, whatever the status, within
/// `timeout`. Only a request that got no response errors; its error holds no header.
pub fn get(
    url: &str,
    headers: &[(&str, &str)],
    timeout: Duration,
) -> Result<(u16, String), ureq::Error> {
    let mut request = agent(timeout).get(url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    read(request.call()?)
}

/// DELETEs `url` and returns the status and the response's text, whatever the status, within
/// `timeout`. Only a request that got no response errors; its error holds no header.
pub fn delete(
    url: &str,
    headers: &[(&str, &str)],
    timeout: Duration,
) -> Result<(u16, String), ureq::Error> {
    let mut request = agent(timeout).delete(url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    read(request.call()?)
}

/// POSTs `body`, a content type and its bytes, within `timeout`.
fn send_post(
    url: &str,
    headers: &[(&str, &str)],
    (content_type, body): (&str, &[u8]),
    timeout: Duration,
) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    let mut request = agent(timeout)
        .post(url)
        .header("content-type", content_type);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    request.send(body)
}

/// A response's status and text.
fn read(mut response: ureq::http::Response<ureq::Body>) -> Result<(u16, String), ureq::Error> {
    let status = response.status().as_u16();
    Ok((status, response.body_mut().read_to_string()?))
}

/// A client that gives up after `timeout`, hands every status back, and follows no redirect: a
/// key in a header of its own, such as `x-api-key`, would go along to wherever one points.
fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .max_redirects(0)
        .build()
        .into()
}

/// What to tell the user when a request to `who` got no response, with `hint` saying what to
/// check; the cause is logged. One that ran out of its `timeout` says so, as a slow Model is no
/// unreachable server.
pub fn no_response(who: &str, hint: &str, timeout: Duration, error: &ureq::Error) -> anyhow::Error {
    eprintln!("Could not reach {who}: {error}");
    match error {
        ureq::Error::Timeout(_) => anyhow!(
            "{who} didn't answer within {} minutes. Try again later.",
            timeout.as_secs() / 60
        ),
        _ => anyhow!("Couldn't reach {who}. {hint}"),
    }
}

/// Who answered, as a failure names it: the Provider's name, or "The server" for a server the
/// user runs.
pub(crate) fn who(provider: Provider) -> &'static str {
    match provider {
        Provider::OpenAiCompatible => "The server",
        _ => provider.name(),
    }
}

/// The message in an error response's `body`, or "" when it has none, with `key`, the one the
/// request sent, taken out wherever the server repeats it. OpenAI's, Anthropic's and Google's
/// APIs put the message under `error.message`; Ollama and xAI a bare string under `error`;
/// ElevenLabs under `detail`.
pub(crate) fn said(body: &str, key: Option<&str>) -> String {
    let body: Value = serde_json::from_str(body).unwrap_or_default();
    let said = [
        &body["error"]["message"],
        &body["error"],
        &body["detail"]["message"],
        &body["detail"],
    ]
    .into_iter()
    .find_map(Value::as_str)
    .unwrap_or_default();
    match key.map(str::trim) {
        Some(key) if !key.is_empty() => said.replace(key, "…"),
        _ => said.to_owned(),
    }
}

/// What to tell the user when `provider` answered a request for `model` with `status` and
/// `body`: a sentence they can act on. `key`, the one the request sent, is taken out of what
/// the server said.
pub fn failure(
    provider: Provider,
    model: &str,
    status: u16,
    body: &str,
    key: Option<&str>,
) -> anyhow::Error {
    let who = who(provider);
    let said = said(body, key);
    match status {
        // No request follows a redirect, so a key goes only to the address the user gave.
        300..=399 => {
            anyhow!("{who} answered from another address. Check the address in Settings.")
        }
        401 => anyhow!("{who} didn't accept the API key. Check it in Settings."),
        // Such as an xAI team without credit.
        403 if !said.is_empty() => anyhow!("{who} turned the request down: {said}"),
        403 => anyhow!("{who} didn't allow this API key to use {model}."),
        // Blame the Model only when the server names it; a bare 404 is an address with nothing
        // behind it.
        404 if said.contains(model) => {
            anyhow!("{who} has no Model named {model}. Check the Model in Settings.")
        }
        404 if !said.is_empty() => anyhow!("{who} turned the request down: {said}"),
        404 => anyhow!("{who} has nothing at that address. Check the address in Settings."),
        413 => anyhow!("{who} turned the request down as too large."),
        // Such as an OpenAI account without credit, which OpenAI and Google answer with 429.
        429 if !said.is_empty() => anyhow!("{who} turned the request down: {said}"),
        429 => anyhow!("{who} is getting too many requests. Try again in a minute."),
        500.. => anyhow!("{who} had a problem ({status}). Try again later."),
        _ if !said.is_empty() => anyhow!("{who} turned the request down: {said}"),
        _ => anyhow!("{who} turned the request down ({status})."),
    }
}

#[cfg(test)]
mod tests {
    use super::{TIMEOUT, delete, failure, get, no_response, post_reading_header, said};
    use crate::Provider;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread::JoinHandle;
    use std::time::Duration;

    /// Answers one request on loopback with `response`; the thread hands back the request's
    /// head.
    fn serve_once(response: impl Into<String>) -> (String, JoinHandle<String>) {
        let response = response.into();
        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(error) => panic!("could not listen: {error}"),
        };
        let address = match listener.local_addr() {
            Ok(address) => address,
            Err(error) => panic!("no address: {error}"),
        };
        let server = std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return String::new();
            };
            let head = read_request(&mut stream);
            stream.write_all(response.as_bytes()).ok();
            head
        });
        (format!("http://{address}"), server)
    }

    /// A request's head, once its body has been read too.
    fn read_request(stream: &mut TcpStream) -> String {
        let (mut data, mut buffer) = (Vec::new(), [0; 4096]);
        loop {
            let text = String::from_utf8_lossy(&data).into_owned();
            if let Some((head, body)) = text.split_once("\r\n\r\n") {
                let length = (head.lines())
                    .filter_map(|line| line.split_once(':'))
                    .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, value)| value.trim().parse().ok())
                    .unwrap_or(0);
                if body.len() >= length {
                    return head.to_owned();
                }
            }
            match stream.read(&mut buffer) {
                Ok(0) | Err(_) => return text,
                Ok(read) => data.extend(&buffer[..read]),
            }
        }
    }

    #[test]
    fn a_post_hands_back_the_header_asked_for_and_a_delete_goes_out() {
        let (url, server) = serve_once(
            "HTTP/1.1 200 OK\r\nX-Goog-Upload-URL: http://127.0.0.1/upload?id=1\r\n\
             content-length: 2\r\nconnection: close\r\n\r\n{}",
        );
        let answer = post_reading_header(
            &format!("{url}/upload/v1beta/files"),
            &[("x-goog-upload-command", "start")],
            ("application/json", b"{}"),
            TIMEOUT,
            "x-goog-upload-url",
        );
        assert_eq!(
            answer.ok(),
            Some((
                200,
                "{}".to_owned(),
                Some("http://127.0.0.1/upload?id=1".to_owned())
            ))
        );
        let head = server.join().unwrap_or_default().to_ascii_lowercase();
        assert!(head.starts_with("post /upload/v1beta/files "), "{head}");
        assert!(head.contains("x-goog-upload-command: start"), "{head}");

        let (url, server) =
            serve_once("HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
        let answer = delete(&format!("{url}/v1beta/files/a1"), &[], TIMEOUT);
        assert_eq!(answer.ok(), Some((404, String::new())));
        let head = server.join().unwrap_or_default();
        assert!(head.starts_with("DELETE /v1beta/files/a1 "), "{head}");
    }

    #[test]
    fn a_redirect_comes_back_without_being_followed() {
        let elsewhere = match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(error) => panic!("could not listen: {error}"),
        };
        let other = match elsewhere.local_addr() {
            Ok(address) => address,
            Err(error) => panic!("no address: {error}"),
        };
        if let Err(error) = elsewhere.set_nonblocking(true) {
            panic!("could not stop blocking: {error}");
        }
        let redirect = format!(
            "HTTP/1.1 302 Found\r\nLocation: http://{other}/login\r\ncontent-length: 0\r\n\
             connection: close\r\n\r\n"
        );
        let (url, server) = serve_once(redirect);
        // A redirect followed would wait on `elsewhere` for this long, then fail.
        let quick = Duration::from_secs(5);
        let answer = get(&format!("{url}/v1/models"), &[("x-api-key", "k1")], quick);
        assert_eq!(answer.ok(), Some((302, String::new())));
        server.join().ok();
        // The key went nowhere else.
        assert!(elsewhere.accept().is_err());
    }

    #[test]
    fn no_response_tells_a_timeout_apart() {
        let timeout = ureq::Error::Timeout(ureq::Timeout::Global);
        assert_eq!(
            no_response(
                "http://whisper.lan/v1",
                "Check that the server is running.",
                Duration::from_secs(80 * 60),
                &timeout
            )
            .to_string(),
            "http://whisper.lan/v1 didn't answer within 80 minutes. Try again later."
        );
        let refused = ureq::Error::ConnectionFailed;
        assert_eq!(
            no_response(
                "Anthropic",
                "Check the internet connection.",
                TIMEOUT,
                &refused
            )
            .to_string(),
            "Couldn't reach Anthropic. Check the internet connection."
        );
    }

    #[test]
    fn an_error_s_message_is_found_wherever_the_api_puts_it() {
        assert_eq!(said(r#"{"error":{"message":"bad key"}}"#, None), "bad key");
        assert_eq!(said(r#"{"error":"bad input"}"#, None), "bad input");
        assert_eq!(
            said(
                r#"{"detail":{"status":"invalid_api_key","message":"Invalid API key"}}"#,
                None
            ),
            "Invalid API key"
        );
        assert_eq!(
            said(r#"{"detail":"Invalid API key"}"#, None),
            "Invalid API key"
        );
        assert_eq!(said("404 page not found", None), "");
    }

    #[test]
    fn a_key_a_server_repeats_is_taken_out_of_its_message() {
        let body = r#"{"error":"Invalid key sk-123 here"}"#;
        assert_eq!(said(body, Some(" sk-123 ")), "Invalid key … here");
        assert_eq!(said(body, None), "Invalid key sk-123 here");
        assert_eq!(said(body, Some("")), "Invalid key sk-123 here");
    }

    #[test]
    fn failures_say_what_to_do() {
        let message =
            |provider, status, body| failure(provider, "m1", status, body, None).to_string();
        let anthropic_error = r#"{"type":"error","error":{"type":"invalid_request_error",
            "message":"max_tokens: too large"}}"#;
        assert_eq!(
            message(Provider::Anthropic, 401, anthropic_error),
            "Anthropic didn't accept the API key. Check it in Settings."
        );
        assert_eq!(
            message(
                Provider::OpenAiCompatible,
                404,
                r#"{"error":"model 'm1' not found"}"#
            ),
            "The server has no Model named m1. Check the Model in Settings."
        );
        assert_eq!(
            message(
                Provider::OpenAiCompatible,
                404,
                r#"{"error":{"message":"Invalid URL (POST /chat/completions)"}}"#
            ),
            "The server turned the request down: Invalid URL (POST /chat/completions)"
        );
        assert_eq!(
            message(Provider::OpenAiCompatible, 404, "404 page not found"),
            "The server has nothing at that address. Check the address in Settings."
        );
        assert_eq!(
            message(Provider::OpenAiCompatible, 413, ""),
            "The server turned the request down as too large."
        );
        assert_eq!(
            message(Provider::Anthropic, 429, ""),
            "Anthropic is getting too many requests. Try again in a minute."
        );
        assert_eq!(
            message(Provider::Anthropic, 529, ""),
            "Anthropic had a problem (529). Try again later."
        );
        assert_eq!(
            message(Provider::Anthropic, 400, anthropic_error),
            "Anthropic turned the request down: max_tokens: too large"
        );
        assert_eq!(
            message(
                Provider::ElevenLabs,
                400,
                r#"{"detail":{"message":"bad audio"}}"#
            ),
            "ElevenLabs turned the request down: bad audio"
        );
        assert_eq!(
            message(Provider::OpenAiCompatible, 400, "nope"),
            "The server turned the request down (400)."
        );
        // An account without credit is told so in the Provider's words.
        assert_eq!(
            message(
                Provider::OpenAi,
                429,
                r#"{"error":{"type":"insufficient_quota","message":"You exceeded your current quota"}}"#
            ),
            "OpenAI turned the request down: You exceeded your current quota"
        );
        assert_eq!(
            message(
                Provider::Xai,
                403,
                r#"{"error":"Your team has no credits."}"#
            ),
            "xAI turned the request down: Your team has no credits."
        );
        assert_eq!(
            message(Provider::Xai, 403, ""),
            "xAI didn't allow this API key to use m1."
        );
        assert_eq!(
            message(Provider::Gemini, 302, ""),
            "Google Gemini answered from another address. Check the address in Settings."
        );
        assert_eq!(
            failure(
                Provider::OpenAiCompatible,
                "m1",
                400,
                r#"{"error":"invalid token sk-123"}"#,
                Some("sk-123")
            )
            .to_string(),
            "The server turned the request down: invalid token …"
        );
    }
}
