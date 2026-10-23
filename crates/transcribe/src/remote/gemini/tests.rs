use super::{File, file, interaction, offset, on_base, parse, transcribe};
use crate::remote::{Request, Timed};
use crumble_remote::{Provider, TIMEOUT};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::JoinHandle;
use std::time::Duration;

fn timed(start_ms: u64, end_ms: u64, text: &str) -> Timed {
    Timed {
        start: Duration::from_millis(start_ms),
        end: Duration::from_millis(end_ms),
        text: text.to_owned(),
    }
}

/// The example answer in https://ai.google.dev/gemini-api/docs/transcribe.
const EXAMPLE: &str = r#"{"id":"interactions/abc123xyz","status":"completed","steps":[
    {"id":"step_001","type":"model_output","content":[{"type":"text","text":"Hello world",
    "annotations":[
    {"type":"word_info","text":"Hello","speaker":"spk_1","start_offset":"0.100s","end_offset":"0.450s"},
    {"type":"word_info","text":"world","speaker":"spk_1","start_offset":"0.500s","end_offset":"0.850s"}
    ]}]}]}"#;

#[test]
fn the_interaction_asks_for_a_verbatim_transcript_with_word_times_kept_by_no_one() {
    assert_eq!(
        interaction(
            "gemini-3.5-transcribe",
            "https://g.example/v1beta/files/f1",
            "en-US"
        ),
        json!({
            "model": "gemini-3.5-transcribe",
            "input": [{
                "type": "audio",
                "uri": "https://g.example/v1beta/files/f1",
                "mime_type": "audio/m4a"
            }],
            "generation_config": {
                "transcription_config": {
                    "language_codes": ["en-US"],
                    "mode": {"type": "verbatim", "timestamp_granularities": ["word"]}
                }
            },
            "store": false
        })
    );
    let detect = interaction("gemini-3.5-transcribe", "u", "");
    assert_eq!(
        detect["generation_config"]["transcription_config"],
        json!({"mode": {"type": "verbatim", "timestamp_granularities": ["word"]}})
    );
}

#[test]
fn annotated_words_become_segments_and_text_alone_spans_the_piece() {
    let length = Duration::from_secs(1);
    let read = |text: &str| serde_json::from_str::<Value>(text).unwrap_or_default();
    assert_eq!(
        parse(&read(EXAMPLE), length),
        Some(vec![timed(100, 850, "Hello world")])
    );
    let unannotated = r#"{"status":"completed","steps":[
        {"type":"thought","content":[{"type":"text","text":"ignored"}]},
        {"type":"model_output","content":[{"type":"text","text":"Hello world"}]}]}"#;
    assert_eq!(
        parse(&read(unannotated), length),
        Some(vec![timed(0, 1_000, "Hello world")])
    );
    assert_eq!(parse(&read(r#"{"error":{"message":"no"}}"#), length), None);
    let past_any_recording = EXAMPLE.replace("0.850s", "1e300s");
    assert_eq!(parse(&read(&past_any_recording), length), None);
}

#[test]
fn offsets_are_seconds_and_an_s() {
    assert_eq!(offset(&json!("0.100s")), Some(Duration::from_millis(100)));
    assert_eq!(offset(&json!("12s")), Some(Duration::from_secs(12)));
    assert_eq!(offset(&json!("-1s")), Some(Duration::ZERO));
    for bad in [json!("0.1"), json!("abc s"), json!(0.1), Value::Null] {
        assert_eq!(offset(&bad), None, "{bad}");
    }
}

#[test]
fn a_file_is_read_from_an_upload_or_by_itself_with_a_name_that_addresses_it() {
    let uploaded = File {
        name: "files/abc-123".to_owned(),
        uri: "https://g.example/v1beta/files/abc-123".to_owned(),
        state: "PROCESSING".to_owned(),
    };
    assert_eq!(
        file(
            r#"{"file":{"name":"files/abc-123","uri":"https://g.example/v1beta/files/abc-123",
            "mimeType":"audio/m4a","sizeBytes":"9","state":"PROCESSING"}}"#
        ),
        Some(uploaded)
    );
    assert_eq!(
        file(r#"{"name":"files/abc-123","uri":"u"}"#).map(|file| file.state),
        Some(String::new())
    );
    for bad in [
        r#"{"name":"files/../models","uri":"u"}"#,
        r#"{"name":"files/","uri":"u"}"#,
        r#"{"name":"models/abc","uri":"u"}"#,
        r#"{"name":"files/abc"}"#,
        "<html>",
    ] {
        assert_eq!(file(bad), None, "{bad}");
    }
}

#[test]
fn an_upload_goes_where_google_says_on_the_user_s_address() {
    let google = "https://generativelanguage.googleapis.com/upload/v1beta/files?upload_id=u1";
    assert_eq!(
        on_base(google, "https://generativelanguage.googleapis.com").as_deref(),
        Some(google)
    );
    // A proxy's path stays, as the start of the upload went through it.
    let proxied = "http://127.0.0.1:4000/gemini/upload/v1beta/files?upload_id=u1";
    assert_eq!(
        on_base(google, "http://127.0.0.1:4000/gemini").as_deref(),
        Some(proxied)
    );
    // Once, when the proxy already put its path in the upload's address.
    assert_eq!(
        on_base(proxied, "http://127.0.0.1:4000/gemini").as_deref(),
        Some(proxied)
    );
    assert_eq!(
        on_base(
            "https://gateway.example/v1/a1/g1/google-ai-studio/upload/v1beta/files?upload_id=u1",
            "https://gateway.example/v1/a1/g1/google-ai-studio"
        )
        .as_deref(),
        Some("https://gateway.example/v1/a1/g1/google-ai-studio/upload/v1beta/files?upload_id=u1")
    );
    assert_eq!(on_base("upload?id=1", "http://127.0.0.1:4000"), None);
}

/// An HTTP answer with `status`, `headers` and `body`, after which the connection closes.
fn answer(status: &str, headers: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\n{headers}content-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// A request the server got: its head and its body.
type Asked = (String, Vec<u8>);

/// Answers one request after another on loopback with `answers`, in order; the thread hands
/// back each request.
fn serve(answers: Vec<String>) -> (String, JoinHandle<Vec<Asked>>) {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) => panic!("could not listen: {error}"),
    };
    let address = match listener.local_addr() {
        Ok(address) => address,
        Err(error) => panic!("no address: {error}"),
    };
    let server = std::thread::spawn(move || {
        let mut asked = Vec::new();
        for answer in answers {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            asked.push(read_request(&mut stream));
            stream.write_all(answer.as_bytes()).ok();
        }
        asked
    });
    (format!("http://{address}"), server)
}

/// A request, once all of it has arrived.
fn read_request(stream: &mut TcpStream) -> Asked {
    let (mut data, mut buffer) = (Vec::new(), [0; 4096]);
    loop {
        if let Some(end) = data.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&data[..end]).into_owned();
            let length = (head.lines())
                .filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, value)| value.trim().parse().ok())
                .unwrap_or(0);
            if data.len() >= end + 4 + length {
                return (head, data[end + 4..end + 4 + length].to_vec());
            }
        }
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => return (String::from_utf8_lossy(&data).into_owned(), Vec::new()),
            Ok(read) => data.extend(&buffer[..read]),
        }
    }
}

/// The uploads' answers up to the interaction: the start, the bytes, and, while the file is
/// `PROCESSING`, a check.
fn uploaded(state: &str) -> Vec<String> {
    let start = answer(
        "200 OK",
        "X-Goog-Upload-URL: https://upload.example/upload/v1beta/files?upload_id=u1\r\n",
        "",
    );
    let described = |state: &str| json!({"name": "files/f1", "uri": "https://g.example/v1beta/files/f1", "state": state});
    let mut answers = vec![
        start,
        answer("200 OK", "", &json!({"file": described(state)}).to_string()),
    ];
    if state == "PROCESSING" {
        answers.push(answer("200 OK", "", &described("ACTIVE").to_string()));
    }
    answers
}

fn request(base: &str) -> Request<'_> {
    Request {
        provider: Provider::Gemini,
        base,
        headers: vec![("x-goog-api-key", "k1".to_owned())],
        key: Some("k1"),
        model: "gemini-3.5-transcribe",
        locale: "en_US",
    }
}

/// Each request's first line.
fn lines(asked: &[Asked]) -> Vec<&str> {
    (asked.iter())
        .map(|(head, _)| head.lines().next().unwrap_or_default())
        .collect()
}

#[test]
fn a_piece_is_uploaded_waited_for_transcribed_and_deleted() {
    let mut answers = uploaded("PROCESSING");
    answers.push(answer("200 OK", "", EXAMPLE));
    answers.push(answer("200 OK", "", "{}"));
    let (base, server) = serve(answers);
    let heard = transcribe(
        &request(&base),
        b"m4a bytes",
        Duration::from_secs(1),
        TIMEOUT,
    );
    assert_eq!(heard.ok(), Some(vec![timed(100, 850, "Hello world")]));
    let asked = server.join().unwrap_or_default();
    assert_eq!(
        lines(&asked),
        [
            "POST /upload/v1beta/files HTTP/1.1",
            "POST /upload/v1beta/files?upload_id=u1 HTTP/1.1",
            "GET /v1beta/files/f1 HTTP/1.1",
            "POST /v1beta/interactions HTTP/1.1",
            "DELETE /v1beta/files/f1 HTTP/1.1",
        ]
    );
    for (head, _) in &asked {
        assert!(
            head.to_ascii_lowercase().contains("x-goog-api-key: k1"),
            "{head}"
        );
    }
    let start = asked[0].0.to_ascii_lowercase();
    for header in [
        "x-goog-upload-protocol: resumable",
        "x-goog-upload-command: start",
        "x-goog-upload-header-content-length: 9",
        "x-goog-upload-header-content-type: audio/m4a",
    ] {
        assert!(start.contains(header), "{header} in {start}");
    }
    assert!(
        asked[1]
            .0
            .to_ascii_lowercase()
            .contains("x-goog-upload-command: upload, finalize")
    );
    assert_eq!(asked[1].1, b"m4a bytes");
    let sent: Value = serde_json::from_slice(&asked[3].1).unwrap_or_default();
    assert_eq!(
        sent,
        interaction(
            "gemini-3.5-transcribe",
            "https://g.example/v1beta/files/f1",
            "en-US"
        )
    );
}

#[test]
fn a_piece_is_deleted_even_when_it_is_not_transcribed() {
    let failed = r#"{"id":"interactions/1","status":"failed","steps":[]}"#;
    for (interaction, said) in [
        (
            answer("500 Internal Server Error", "", ""),
            "Google Gemini had a problem (500). Try again later.",
        ),
        (
            answer("200 OK", "", failed),
            "Google Gemini couldn't transcribe the audio. Try again later.",
        ),
    ] {
        let mut answers = uploaded("ACTIVE");
        answers.push(interaction);
        answers.push(answer("404 Not Found", "", ""));
        let (base, server) = serve(answers);
        let heard = transcribe(&request(&base), b"m4a", Duration::from_secs(1), TIMEOUT);
        let error = heard.map_err(|error| error.to_string()).err();
        assert_eq!(error.as_deref(), Some(said));
        let asked = server.join().unwrap_or_default();
        assert_eq!(
            lines(&asked).last().copied(),
            Some("DELETE /v1beta/files/f1 HTTP/1.1")
        );
        assert_eq!(asked.len(), 4);
    }
}

#[test]
fn a_file_google_could_not_read_fails_and_is_deleted() {
    let mut answers = uploaded("FAILED");
    answers.push(answer("200 OK", "", "{}"));
    let (base, server) = serve(answers);
    let heard = transcribe(&request(&base), b"m4a", Duration::from_secs(1), TIMEOUT);
    let error = heard.map_err(|error| error.to_string()).err();
    assert_eq!(
        error.as_deref(),
        Some("Google Gemini couldn't read the audio.")
    );
    let asked = server.join().unwrap_or_default();
    assert_eq!(
        lines(&asked),
        [
            "POST /upload/v1beta/files HTTP/1.1",
            "POST /upload/v1beta/files?upload_id=u1 HTTP/1.1",
            "DELETE /v1beta/files/f1 HTTP/1.1",
        ]
    );
}
