//! The production detector against a loopback server (`AAP-ai-api-integrations.md`
//! AAP-FR-QRAX, AAP-FR-JQIJ, AAP-FR-KJSL).

use super::*;
use crate::ai_api::detection::{
    Detected, DetectionFailure, HttpModelDetector, ModelDetector, SHOW_PATH,
};
use crate::ai_shared::HttpEndpointProber;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

const SECRET: &str = "ollama-anything";

/// One whole HTTP request: the head, then the body that `Content-Length`
/// names. A single read can return the head alone on a slow runner.
fn read_request(stream: &mut TcpStream) -> String {
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
                .find_map(|l| {
                    let (name, value) = l.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
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

/// A server that answers each request with the status and the body of the
/// route it asked for, and gives back every request it read.
fn serve(
    routes: &'static [(&'static str, u16, &'static str)],
    requests: usize,
) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for _ in 0..requests {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            let (status, body) = routes
                .iter()
                .find(|(route, _, _)| request.contains(&format!(" {route} ")))
                .map(|(_, status, body)| (*status, *body))
                .unwrap_or((404, "{}"));
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            seen.push(request);
        }
        seen
    });
    (url, handle)
}

const SHOW_BODY: &str = r#"{"model_info":{"general.architecture":"qwen3","qwen3.context_length":40960},"capabilities":["completion","tools"]}"#;

// AAP-FR-QRAX, AAP-FR-JQIJ: the request is `POST <base>/api/show` with the
// bearer secret and the model id, and the answer gives the values.
#[test]
fn the_production_detector_posts_the_model_with_the_bearer_secret() {
    let (url, server) = serve(&[(SHOW_PATH, 200, SHOW_BODY)], 1);
    let detected = HttpModelDetector::new().show(&url, Some(SECRET), "qwen3:32b");
    let seen = server.join().unwrap();

    assert_eq!(
        detected,
        Ok(Detected {
            context_size: Some(40960),
            capabilities: Some(vec!["completion".into(), "tools".into()]),
        })
    );
    assert!(seen[0].starts_with("POST /api/show "), "{}", seen[0]);
    assert!(
        seen[0].to_lowercase().contains(&format!("authorization: bearer {SECRET}")),
        "{}",
        seen[0]
    );
    assert!(seen[0].ends_with(r#"{"model":"qwen3:32b"}"#), "{}", seen[0]);
}

// AAP-FR-KJSL: a status that is not a success and a body that is not the
// expected shape are failures of their own kind.
#[test]
fn the_production_detector_reports_a_status_and_a_bad_body() {
    let (url, server) = serve(&[(SHOW_PATH, 404, "{}")], 1);
    assert_eq!(
        HttpModelDetector::new().show(&url, Some(SECRET), "m"),
        Err(DetectionFailure::Status(404))
    );
    server.join().unwrap();

    let (url, server) = serve(&[(SHOW_PATH, 500, SHOW_BODY)], 1);
    assert_eq!(
        HttpModelDetector::new().show(&url, Some(SECRET), "m"),
        Err(DetectionFailure::Status(500))
    );
    server.join().unwrap();

    let (url, server) = serve(&[(SHOW_PATH, 200, "not json")], 1);
    assert_eq!(
        HttpModelDetector::new().show(&url, Some(SECRET), "m"),
        Err(DetectionFailure::BadBody)
    );
    server.join().unwrap();
}

// AAP-FR-KJSL: a host that cannot be reached is a network failure.
#[test]
fn the_production_detector_reports_a_host_that_cannot_be_reached() {
    // A port that was free a moment ago and has no listener now.
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let url = format!("http://127.0.0.1:{port}");
    assert_eq!(
        HttpModelDetector::new().show(&url, Some(SECRET), "m"),
        Err(DetectionFailure::Network)
    );
}

// AAP-FR-KJSL, AAP-FR-QRAX: a host that serves the model list and is not
// Ollama verifies as a generic host, with its list unchanged and no values.
#[test]
fn a_host_that_is_not_ollama_still_verifies() {
    let (url, server) = serve(
        &[("/v1/models", 200, r#"{"object":"list","data":[{"id":"a","object":"model"}]}"#)],
        2,
    );
    let keys = FakeKeychain::new();
    let ai = AiApiIntegrations::new(Box::new(HttpEndpointProber), Box::new(keys))
        .with_detector(Box::new(HttpModelDetector::new()));
    let store = GlobalSettingsStore::in_memory();
    let rec = verify_integration_impl(&store, &ai, "custom", &url, Some(SECRET)).unwrap();
    let seen = server.join().unwrap();

    assert_eq!(rec.state, IntegrationState::Verified);
    assert_eq!(rec.models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["a"]);
    assert_eq!(rec.models[0].detected_context_size, None);
    assert_eq!(rec.models[0].detected_capabilities, None);
    assert!(seen[1].starts_with("POST /api/show "), "{}", seen[1]);
}
