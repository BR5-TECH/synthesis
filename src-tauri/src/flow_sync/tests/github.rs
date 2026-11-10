//! FTS-FR-ARMD, FTS-FR-ZLFX: the GitHub adapter against a local server, and
//! the paging of a relation list, so a long list is never cut.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use serde_json::{json, Value};

use crate::flow_sync::adapter::{IssueKey, TrackerAdapter};
use crate::flow_sync::github::{all_nodes, next_cursor, GithubAdapter};
use crate::flow_sync::records::ERR_GITHUB_UNREACHABLE;

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
            let length = content_length(&text[..end]).unwrap_or(0);
            if received.len() >= end + 4 + length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&received).to_string()
}

fn content_length(head: &str) -> Option<usize> {
    head.lines().find_map(|l| {
        let (name, value) = l.split_once(':')?;
        name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().ok())?
    })
}

/// A server that answers one request for each body in `bodies`, in order,
/// with `200`, and returns the requests it saw.
fn serve(bodies: Vec<Value>) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for body in bodies {
            let (mut stream, _) = listener.accept().unwrap();
            seen.push(read_request(&mut stream));
            let text = body.to_string();
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}",
                text.len()
            );
        }
        seen
    });
    (url, handle)
}

fn adapter(url: &str) -> GithubAdapter {
    GithubAdapter::at(url, &format!("{url}/graphql"), "acme", "payments")
}

fn key_node(number: u64) -> Value {
    json!({ "number": number, "url": format!("https://github.com/acme/payments/issues/{number}"),
            "repository": { "name": "payments", "owner": { "login": "acme" } } })
}

fn page(nodes: Vec<Value>, next: Option<&str>) -> Value {
    json!({ "nodes": nodes,
            "pageInfo": { "hasNextPage": next.is_some(), "endCursor": next } })
}

fn issue_node(number: u64, blocked_by: Value) -> Value {
    json!({ "id": format!("I_{number}"), "databaseId": 9000 + number, "number": number,
            "url": format!("https://github.com/acme/payments/issues/{number}"),
            "state": "OPEN", "title": "t", "body": "",
            "repository": { "name": "payments", "owner": { "login": "acme" } },
            "issueType": null, "milestone": null, "parent": null, "blockedBy": blocked_by })
}

/// FTS-FR-ARMD: a removal of a blocked-by relation is a `DELETE` with no
/// body, which GitHub accepts; a body of `null` it refuses.
#[test]
fn remove_blocked_by_sends_a_delete_with_no_body() {
    let (url, server) = serve(vec![json!({})]);
    let github = adapter(&url);
    github.remember_id(4, 9004);
    github.remove_blocked_by(3, 4).unwrap();
    let requests = server.join().unwrap();
    let head = requests[0].split("\r\n\r\n").next().unwrap();
    assert!(
        head.starts_with("DELETE /repos/acme/payments/issues/3/dependencies/blocked_by/9004 "),
        "{head}"
    );
    assert_eq!(content_length(head).unwrap_or(0), 0, "{head}");
    assert!(!requests[0].contains("null"), "{}", requests[0]);
}

/// FTS-FR-ARMD: the removal of a parent in another repository is made on
/// the parent's own repository.
#[test]
fn a_parent_in_another_repository_is_removed_there() {
    let (url, server) = serve(vec![json!({})]);
    let github = adapter(&url);
    github.remember_id(3, 9003);
    let upstream =
        IssueKey { owner: "acme".into(), repo: "upstream".into(), number: 9, url: String::new() };
    github.set_parent(3, Some(&upstream), None).unwrap();
    let request = &server.join().unwrap()[0];
    assert!(request.starts_with("DELETE /repos/acme/upstream/issues/9/sub_issue "), "{request}");
    assert!(request.contains("\"sub_issue_id\":9003"), "{request}");
}

/// FTS NFR / FTS-FR-VYOV: a blocked-by list longer than one page is read to
/// its end, page by page, so no blocker past the first page reads as
/// removed.
#[test]
fn a_long_blocked_by_list_is_read_page_by_page() {
    let first: Vec<Value> = (1..=50).map(|n| key_node(100 + n)).collect();
    let (url, server) = serve(vec![
        json!({ "data": { "repository": { "i0": issue_node(3, page(first, Some("c1"))) } } }),
        json!({ "data": { "node": { "blockedBy": page(vec![key_node(151), key_node(152)], Some("c2")) } } }),
        json!({ "data": { "node": { "blockedBy": page(vec![key_node(153)], None) } } }),
    ]);
    let issues = adapter(&url).read_issues(&[3]).unwrap();
    let issue = issues[0].1.clone().expect("issue 3");
    let numbers: Vec<u64> = issue.blocked_by.iter().map(|k| k.number).collect();
    assert_eq!(numbers.len(), 53);
    assert_eq!(&numbers[50..], &[151, 152, 153]);
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(
        requests[1].contains("\"id\":\"I_3\"") && requests[1].contains("\"after\":\"c1\""),
        "{}",
        requests[1]
    );
    assert!(requests[2].contains("\"after\":\"c2\""), "{}", requests[2]);
}

/// FTS NFR: the sub-issues of a parent are read to the end too, and the
/// blockers of each sub-issue with them.
#[test]
fn a_long_sub_issue_list_is_read_page_by_page() {
    let parent = json!({ "id": "I_2", "subIssues": page(vec![issue_node(10, page(vec![], None))], Some("s1")) });
    let (url, server) = serve(vec![
        json!({ "data": { "repository": { "i0": parent } } }),
        json!({ "data": { "node": { "subIssues": page(vec![issue_node(11, page(vec![key_node(10)], None))], None) } } }),
    ]);
    let found = adapter(&url).read_sub_issues(&[2]).unwrap();
    let numbers: Vec<u64> = found[0].1.iter().map(|i| i.number).collect();
    assert_eq!(numbers, vec![10, 11]);
    assert_eq!(found[0].1[1].blocked_by[0].number, 10);
    assert_eq!(server.join().unwrap().len(), 2);
}

/// FTS NFR: a page that cannot be read fails the read with a code, so a cut
/// list never reads as complete.
#[test]
fn a_page_that_cannot_be_read_fails_the_read() {
    let first: Vec<Value> = (1..=50).map(|n| key_node(100 + n)).collect();
    let (url, server) = serve(vec![
        json!({ "data": { "repository": { "i0": issue_node(3, page(first, Some("c1"))) } } }),
        json!({ "errors": [{ "type": "RATE_LIMITED", "message": "x" }] }),
    ]);
    let error = adapter(&url).read_issues(&[3]).unwrap_err();
    assert_eq!(error, ERR_GITHUB_UNREACHABLE);
    server.join().unwrap();
}

/// FTS NFR: the cursor of the next page, none on the last page, and an
/// error where the answer says there is a next page and gives no cursor or
/// no page information at all.
#[test]
fn the_next_cursor_is_read_from_the_page_information() {
    assert_eq!(next_cursor(&page(vec![], None)), Ok(None));
    assert_eq!(next_cursor(&page(vec![], Some("abc"))), Ok(Some("abc".into())));
    let no_cursor = json!({ "nodes": [], "pageInfo": { "hasNextPage": true, "endCursor": null } });
    assert_eq!(next_cursor(&no_cursor), Err(ERR_GITHUB_UNREACHABLE.into()));
    assert_eq!(next_cursor(&json!({ "nodes": [] })), Err(ERR_GITHUB_UNREACHABLE.into()));
}

/// FTS NFR: every node of every page, in order; a page that gives the same
/// cursor again ends the read with an error rather than running for ever.
#[test]
fn all_nodes_reads_every_page_and_stops_on_a_repeated_cursor() {
    let mut asked = Vec::new();
    let nodes = all_nodes(&page(vec![json!(1)], Some("a")), |after| {
        asked.push(after.to_string());
        Ok(match after {
            "a" => page(vec![json!(2)], Some("b")),
            _ => page(vec![json!(3)], None),
        })
    })
    .unwrap();
    assert_eq!(nodes, vec![json!(1), json!(2), json!(3)]);
    assert_eq!(asked, vec!["a", "b"]);

    let looping = all_nodes(&page(vec![], Some("a")), |_| Ok(page(vec![json!(1)], Some("a"))));
    assert_eq!(looping, Err(ERR_GITHUB_UNREACHABLE.into()));
    let failing = all_nodes(&page(vec![], Some("a")), |_| Err("rate_limited".into()));
    assert_eq!(failing, Err("rate_limited".into()));
}
