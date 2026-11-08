//! The API version header of the pull request client against a local server
//! (GTS-FR-NWVR) and the stacks answers 400 and 410 (GTC-FR-TBQW, GTC-FR-KQVA).

use crate::git::pull_requests::client::{
    get_at, write_at, GithubFailure, GithubWriteFailure, WriteMethod,
};
use serde_json::json;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

const SECRET: &str = "secret-token";

/// One whole HTTP request: the head, then the body that `Content-Length` names.
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

/// A server that answers one request with `status` and `{}`, and returns the
/// request it saw.
fn serve_once(status: u16) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        let _ = write!(
            stream,
            "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{{}}"
        );
        request
    });
    (url, handle)
}

fn version_header(request: &str) -> String {
    request
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("x-github-api-version").then(|| value.trim().to_string())
        })
        .unwrap_or_default()
}

// GTS-FR-NWVR
#[test]
fn a_stacks_read_sends_api_version_2026_03_10() {
    let (api, server) = serve_once(200);
    get_at(&api, SECRET, "/repos/acme/platform/stacks?pull_request=2").unwrap();
    assert_eq!(version_header(&server.join().unwrap()), "2026-03-10");
}

// GTS-FR-NWVR
#[test]
fn a_read_outside_stacks_sends_api_version_2022_11_28() {
    let (api, server) = serve_once(200);
    get_at(&api, SECRET, "/repos/acme/platform/pulls?state=open").unwrap();
    assert_eq!(version_header(&server.join().unwrap()), "2022-11-28");
}

// GTS-FR-NWVR
#[test]
fn a_stacks_write_sends_api_version_2026_03_10_and_a_pull_request_write_sends_2022_11_28() {
    let (api, server) = serve_once(200);
    write_at(&api, WriteMethod::Post, SECRET, "/repos/acme/platform/stacks", &json!({})).unwrap();
    assert_eq!(version_header(&server.join().unwrap()), "2026-03-10");

    let (api, server) = serve_once(200);
    write_at(&api, WriteMethod::Post, SECRET, "/repos/acme/platform/pulls", &json!({})).unwrap();
    assert_eq!(version_header(&server.join().unwrap()), "2022-11-28");
}

// GTS-FR-NWVR
#[test]
fn only_paths_under_the_stacks_endpoint_of_a_repository_are_stacks_paths() {
    use crate::github_tokens::{api_version_for_path, is_stacks_path};
    assert!(is_stacks_path("/repos/acme/platform/stacks"));
    assert!(is_stacks_path("/repos/acme/platform/stacks/7/add"));
    assert!(!is_stacks_path("/repos/acme/platform/pulls?head=stacks"));
    assert!(!is_stacks_path("/repos/acme/stacks"));
    assert!(!is_stacks_path("/user"));
    assert_eq!(api_version_for_path("/user"), "2022-11-28");
}

// GTC-FR-TBQW, GTC-FR-JMVC
#[test]
fn a_stacks_read_answered_400_or_410_is_unsupported_and_404_stays_not_found() {
    for (status, expected) in [
        (400, GithubFailure::Unsupported),
        (410, GithubFailure::Unsupported),
        (404, GithubFailure::NotFound),
    ] {
        let (api, server) = serve_once(status);
        let failure = get_at(&api, SECRET, "/repos/acme/platform/stacks?per_page=1").unwrap_err();
        server.join().unwrap();
        assert_eq!(failure, expected, "status {status}");
    }
    let (api, server) = serve_once(400);
    let failure = get_at(&api, SECRET, "/repos/acme/platform/pulls").unwrap_err();
    server.join().unwrap();
    assert_eq!(failure, GithubFailure::Unreachable);
}

// GTC-FR-KQVA
#[test]
fn a_stacks_write_answered_400_or_410_is_not_found() {
    for status in [400, 410] {
        let (api, server) = serve_once(status);
        let failure =
            write_at(&api, WriteMethod::Post, SECRET, "/repos/acme/platform/stacks", &json!({}))
                .unwrap_err();
        server.join().unwrap();
        assert_eq!(failure, GithubWriteFailure::NotFound, "status {status}");
    }
}
