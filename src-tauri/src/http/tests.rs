//! The rules of the one HTTP path against loopback servers
//! (`AAP-ai-api-integrations.md` AAP-FR-DTJA, AAP-FR-UXUI, AAP-FR-TIZO,
//! AAP-FR-GMVE, AAP-FR-JGYD).
//!
//! A server of a test never blocks the test: [`Loopback::requests`] stops it
//! with a connection of its own, and the test then asserts the requests it saw.

use super::*;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};

mod callers;
mod proxy;
mod redirects;

/// The first line of the connection that stops a [`Loopback`].
const STOP: &str = "STOP";
/// The longest wait for the bytes of one request, so a server never waits for
/// ever.
const READ_LIMIT: Duration = Duration::from_secs(10);
/// The time limit of a request of these tests. No test waits for it.
const LIMIT: Duration = Duration::from_secs(20);

/// One whole HTTP request: the head, then the body that `Content-Length` names.
pub(crate) fn read_request(stream: &mut TcpStream) -> String {
    let _ = stream.set_read_timeout(Some(READ_LIMIT));
    let mut received = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = stream.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            break;
        }
        received.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&received).to_string();
        if let Some(end) = text.find("\r\n\r\n") {
            let length = text[..end]
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.trim()
                        .eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            if received.len() >= end + 4 + length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&received).to_string()
}

/// The raw bytes of one answer, with `Content-Length` and `Connection: close`.
pub(crate) fn reply(status: u16, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut head = format!("HTTP/1.1 {status} X\r\n");
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!("content-length: {}\r\nconnection: close\r\n\r\n", body.len()));
    let mut bytes = head.into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

/// A JSON answer with `status`.
pub(crate) fn json_reply(status: u16, body: &str) -> Vec<u8> {
    reply(status, &[("content-type", "application/json")], body.as_bytes())
}

/// A local server that answers each request with the next answer of its list,
/// and a request past the end of the list with `500`.
pub(crate) struct Loopback {
    pub(crate) port: u16,
    addr: SocketAddr,
    handle: std::thread::JoinHandle<Vec<String>>,
}

impl Loopback {
    pub(crate) fn serve(replies: Vec<Vec<u8>>) -> Self {
        Self::serve_with(|_| replies)
    }

    /// [`Loopback::serve`] with answers that can name the port of the server.
    pub(crate) fn serve_with(replies: impl FnOnce(u16) -> Vec<Vec<u8>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("address");
        let replies = replies(addr.port());
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            let mut replies = replies.into_iter();
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let request = read_request(&mut stream);
                // A connection that closes with no byte is a stop as well, so
                // a stop that arrives cut short cannot leave the server waiting.
                if request.is_empty() || request.starts_with(STOP) {
                    break;
                }
                seen.push(request);
                let answer = replies.next().unwrap_or_else(|| json_reply(500, "{}"));
                let _ = stream.write_all(&answer);
                let _ = stream.flush();
            }
            seen
        });
        Self { port: addr.port(), addr, handle }
    }

    /// `http://127.0.0.1:<port><path>`.
    pub(crate) fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    /// Stop the server and give the requests it saw, in order.
    pub(crate) fn requests(self) -> Vec<String> {
        let mut stop = TcpStream::connect(self.addr).expect("the stop connection");
        let _ = write!(stop, "{STOP}\r\n\r\n");
        drop(stop);
        self.handle.join().expect("the server thread")
    }
}

/// The value of the header `name` in the head of `request`, matched without
/// case.
pub(crate) fn header_of(request: &str, name: &str) -> Option<String> {
    let head = request.split("\r\n\r\n").next().unwrap_or_default();
    head.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim().to_string())
    })
}

/// The port of a server that accepts each connection and closes it with no
/// answer. The listener lives until the test process ends, so no other
/// process can take the port while the test runs.
pub(crate) fn closing_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("address").port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            drop(stream);
        }
    });
    port
}

/// A server that reads one request, writes `head`, and then holds the
/// connection with no more bytes until the test sends on the channel.
pub(crate) struct Stalled {
    pub(crate) port: u16,
    release: std::sync::mpsc::Sender<()>,
    handle: std::thread::JoinHandle<()>,
}

impl Stalled {
    pub(crate) fn serve(head: &'static [u8]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("address").port();
        let (release, wait) = std::sync::mpsc::channel::<()>();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("the connection");
            read_request(&mut stream);
            let _ = stream.write_all(head);
            let _ = stream.flush();
            let _ = wait.recv();
            drop(stream);
        });
        Self { port, release, handle }
    }

    /// Close the connection and end the server.
    pub(crate) fn release(self) {
        let _ = self.release.send(());
        self.handle.join().expect("the server thread");
    }
}

/// The head of an answer that names a body of 100 bytes and sends none.
pub(crate) const STALLED_HEAD: &[u8] = b"HTTP/1.1 200 OK\r\ncontent-length: 100\r\n\r\n";

// ---------------------------------------------------------------------------
// AAP-FR-UXUI: a status
// ---------------------------------------------------------------------------

// AAP-FR-UXUI
#[test]
fn a_4xx_or_5xx_answer_is_the_status_failure() {
    for status in [400, 401, 404, 429, 500, 503] {
        let server = Loopback::serve(vec![json_reply(status, r#"{"ok":true}"#)]);
        let result = Request::get(&server.url("/x"), LIMIT).direct(true).send();
        assert_eq!(result.unwrap_err(), HttpError::Status(status));
        assert_eq!(server.requests().len(), 1);
    }
}

// AAP-FR-UXUI
#[test]
fn a_caller_that_reads_every_answer_gets_the_status_and_the_body() {
    let server = Loopback::serve(vec![json_reply(422, r#"{"message":"bad"}"#)]);
    let response = Request::post(&server.url("/x"), LIMIT)
        .json(&serde_json::json!({ "a": 1 }))
        .statuses_as_responses()
        .direct(true)
        .send()
        .expect("the answer is a response");
    assert_eq!(response.status, 422);
    assert_eq!(response.text().unwrap(), r#"{"message":"bad"}"#);
    let requests = server.requests();
    assert!(requests[0].ends_with(r#"{"a":1}"#), "{}", requests[0]);
    assert_eq!(header_of(&requests[0], "content-type").as_deref(), Some("application/json"));
}

// AAP-FR-UXUI
#[test]
fn a_2xx_answer_is_a_response_with_its_headers() {
    let server =
        Loopback::serve(vec![reply(200, &[("x-oauth-scopes", "repo, gist")], b"{\"id\":1}")]);
    let response = Request::get(&server.url("/x"), LIMIT).direct(true).send().unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.header("x-oauth-scopes"), Some("repo, gist"));
    assert_eq!(response.text().unwrap(), "{\"id\":1}");
    server.requests();
}

// ---------------------------------------------------------------------------
// AAP-FR-TIZO: the body limit
// ---------------------------------------------------------------------------

// AAP-FR-TIZO
#[test]
fn a_body_of_exactly_ten_megabytes_is_read() {
    let body = vec![b'a'; MAX_BODY_BYTES];
    let server = Loopback::serve(vec![reply(200, &[], &body)]);
    let response = Request::get(&server.url("/x"), LIMIT).direct(true).send().unwrap();
    assert_eq!(response.body.len(), MAX_BODY_BYTES);
    server.requests();
}

// AAP-FR-TIZO
#[test]
fn a_body_longer_than_ten_megabytes_is_a_failure() {
    let body = vec![b'a'; MAX_BODY_BYTES + 1];
    let server = Loopback::serve(vec![reply(200, &[], &body)]);
    let result = Request::get(&server.url("/x"), LIMIT).direct(true).send();
    assert_eq!(result.unwrap_err(), HttpError::TooLarge);
    server.requests();
}

/// A chunked answer of `chunks` chunks of 1 MB, and then `extra` bytes.
fn chunked(chunks: usize, extra: usize) -> Vec<u8> {
    let mut answer =
        b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n".to_vec();
    let mut add = |chunk: &[u8]| {
        answer.extend_from_slice(format!("{:x}\r\n", chunk.len()).as_bytes());
        answer.extend_from_slice(chunk);
        answer.extend_from_slice(b"\r\n");
    };
    let megabyte = vec![b'a'; 1024 * 1024];
    for _ in 0..chunks {
        add(&megabyte);
    }
    if extra > 0 {
        add(&vec![b'a'; extra]);
    }
    answer.extend_from_slice(b"0\r\n\r\n");
    answer
}

// AAP-FR-TIZO: a body with no length is counted as it arrives.
#[test]
fn a_chunked_body_longer_than_ten_megabytes_is_a_failure() {
    let server = Loopback::serve(vec![chunked(10, 1)]);
    let result = Request::get(&server.url("/x"), LIMIT).direct(true).send();
    assert_eq!(result.unwrap_err(), HttpError::TooLarge);
    server.requests();
}

// AAP-FR-TIZO
#[test]
fn a_chunked_body_of_exactly_ten_megabytes_is_read() {
    let server = Loopback::serve(vec![chunked(10, 0)]);
    let response = Request::get(&server.url("/x"), LIMIT).direct(true).send().unwrap();
    assert_eq!(response.body.len(), MAX_BODY_BYTES);
    server.requests();
}

// ---------------------------------------------------------------------------
// Time limits and transport failures
// ---------------------------------------------------------------------------

// A server that never answers ends the request at its time limit. The result
// does not depend on the speed of the machine: the server never answers.
#[test]
fn a_server_that_never_answers_is_a_timeout() {
    let server = Stalled::serve(b"");
    let result =
        Request::get(&format!("http://127.0.0.1:{}/", server.port), Duration::from_millis(300))
            .direct(true)
            .send();
    assert_eq!(result.unwrap_err(), HttpError::Timeout);
    server.release();
}

// The time limit covers the body: a server that sends the head and then
// stops ends the request at its limit.
#[test]
fn a_body_that_stops_is_a_timeout() {
    let server = Stalled::serve(STALLED_HEAD);
    let result =
        Request::get(&format!("http://127.0.0.1:{}/", server.port), Duration::from_millis(300))
            .direct(true)
            .send();
    assert_eq!(result.unwrap_err(), HttpError::Timeout);
    server.release();
}

// A connection that closes before an answer is a transport failure.
#[test]
fn a_connection_closed_with_no_answer_is_a_transport_failure() {
    let port = closing_server();
    let result = Request::get(&format!("http://127.0.0.1:{port}/"), LIMIT).direct(true).send();
    assert_eq!(result.unwrap_err(), HttpError::Transport);
}

// A body shorter than its `Content-Length` is a body that broke off.
#[test]
fn a_body_cut_short_is_a_body_failure() {
    let server = Loopback::serve(vec![
        b"HTTP/1.1 200 OK\r\ncontent-length: 100\r\nconnection: close\r\n\r\n0123456789".to_vec(),
    ]);
    let result = Request::get(&server.url("/x"), LIMIT).direct(true).send();
    assert_eq!(result.unwrap_err(), HttpError::Body);
    server.requests();
}

// GHP-FR-DHXK: the text of an error names its kind and holds nothing of the
// request.
#[test]
fn the_text_of_an_error_holds_nothing_of_the_request() {
    let port = closing_server();
    let url = format!("http://127.0.0.1:{port}/path-secret?token=query-secret");
    let error = Request::get(&url, LIMIT).bearer("bearer-secret").direct(true).send().unwrap_err();
    assert_eq!(error, HttpError::Transport);
    let failure = TlsFailure::new("api.example.com", crate::tls::TlsCause::Expired);
    let every = [
        HttpError::Status(401),
        HttpError::Timeout,
        HttpError::Tls(failure.clone()),
        HttpError::Transport,
        HttpError::Redirect,
        HttpError::TooLarge,
        HttpError::Body,
        HttpError::Client,
    ];
    let texts: Vec<String> = every.iter().map(ToString::to_string).collect();
    for (error, text) in every.iter().zip(&texts) {
        for secret in ["path-secret", "query-secret", "bearer-secret", "127.0.0.1", "http"] {
            assert!(!text.contains(secret), "{error:?}: {text}");
        }
        assert!(!text.is_empty(), "{error:?}");
        assert_eq!(texts.iter().filter(|other| *other == text).count(), 1, "{text}");
    }
    assert_eq!(texts[2], failure.describe());
    assert!(texts[0].contains("401"), "{}", texts[0]);
}

// A header with a line break, as a pasted token can have, is not valid in
// HTTP. The request is not sent at all, so it never goes without the
// credential it was given.
#[test]
fn a_request_with_a_header_that_is_not_valid_is_not_sent() {
    let server = Loopback::serve(vec![json_reply(200, "{}")]);
    let with_credential =
        Request::get(&server.url("/x"), LIMIT).bearer("secret\nInjected: yes").direct(true).send();
    assert_eq!(with_credential.unwrap_err(), HttpError::Client);
    let with_header =
        Request::get(&server.url("/x"), LIMIT).header("bad name", "v").direct(true).send();
    assert_eq!(with_header.unwrap_err(), HttpError::Client);
    assert!(server.requests().is_empty(), "no request reaches the server");
}

// ---------------------------------------------------------------------------
// AAP-FR-GMVE: the origin of a redirect
// ---------------------------------------------------------------------------

// AAP-FR-GMVE
#[test]
fn the_origin_is_the_scheme_the_host_and_the_port() {
    let url = |text: &str| Url::parse(text).unwrap();
    for (a, b, same) in [
        ("https://h.test/a", "https://h.test/b", true),
        ("https://h.test/", "https://h.test:443/", true),
        ("http://h.test/", "http://h.test:80/", true),
        ("https://H.Test/", "https://h.test/", true),
        ("http://h.test/", "https://h.test/", false),
        ("https://h.test/", "http://h.test/", false),
        ("https://h.test/", "https://h.test:8443/", false),
        ("https://h.test/", "https://other.test/", false),
        ("http://localhost:8080/", "http://127.0.0.1:8080/", false),
    ] {
        assert_eq!(same_origin(&url(a), &url(b)), same, "{a} and {b}");
    }
}

// ---------------------------------------------------------------------------
// AAP-FR-DTJA: one client library
// ---------------------------------------------------------------------------

/// The lines of the `[dependencies]` table of the manifest.
fn dependencies_table(manifest: &str) -> Vec<&str> {
    let start = manifest.find("\n[dependencies]").expect("a [dependencies] table");
    let rest = &manifest[start + 1..];
    let rest = &rest["[dependencies]".len()..];
    let end = rest.find("\n[").unwrap_or(rest.len());
    rest[..end].lines().filter(|line| !line.trim_start().starts_with('#')).collect()
}

// AAP-FR-DTJA
#[test]
fn the_application_depends_on_no_http_client_library_but_reqwest_013() {
    let manifest = include_str!("../../Cargo.toml");
    for line in dependencies_table(manifest) {
        let name = line.split('=').next().unwrap_or_default().trim();
        assert_ne!(name, "ureq", "the application names ureq: {line}");
        assert_ne!(name, "reqwest012", "the application names reqwest 0.12: {line}");
        assert_ne!(name, "reqwest", "reqwest is named under its version alias only: {line}");
        if line.contains("package = \"reqwest\"") {
            assert!(line.contains("version = \"0.13\""), "{line}");
        }
    }

    // The lock file holds what the build really resolved for the application
    // crate, whatever name the manifest gives it.
    let lock = include_str!("../../Cargo.lock");
    let block = lock
        .split("[[package]]")
        .find(|block| block.trim_start().starts_with("name = \"synthesis\"\n"))
        .expect("the package of the application in the lock file");
    let dependencies: Vec<&str> =
        block.lines().map(|line| line.trim().trim_end_matches(',').trim_matches('"')).collect();
    for dependency in &dependencies {
        assert!(!dependency.starts_with("ureq"), "the application depends on {dependency}");
        assert!(!dependency.starts_with("reqwest 0.12"), "the application depends on {dependency}");
        assert!(!dependency.starts_with("isahc") && !dependency.starts_with("attohttpc"));
    }
    assert!(
        dependencies.iter().any(|d| d.starts_with("reqwest 0.13") || *d == "reqwest"),
        "the application depends on reqwest 0.13"
    );
}
