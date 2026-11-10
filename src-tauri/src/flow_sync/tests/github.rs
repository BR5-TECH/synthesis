//! FTS-FR-ARMD, FTS-FR-ZLFX: the GitHub adapter against a local server, and
//! the paging of a relation list, so a long list is never cut.
//!
//! The server of a test never blocks the test: [`Server::requests`] stops it
//! with a connection of its own, and each test then asserts the count of
//! requests it saw.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use serde_json::{json, Value};

use crate::flow_sync::adapter::{IssueKey, TrackerAdapter};
use crate::flow_sync::github::{all_nodes, next_cursor, GithubAdapter};
use crate::flow_sync::records::ERR_GITHUB_UNREACHABLE;

/// The first line of the connection that stops a [`Server`].
const STOP: &str = "STOP";
/// The longest wait for the bytes of one request. A request that does not
/// come in this time ends the read, so the server never blocks for ever.
const READ_LIMIT: Duration = Duration::from_secs(10);

/// One whole HTTP request: the head, then the body that `Content-Length` names.
fn read_request(stream: &mut TcpStream) -> String {
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
            let length = content_length(&text[..end]).unwrap_or(0);
            if received.len() >= end + 4 + length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&received).to_string()
}

fn content_length(head: &str) -> Option<usize> {
    header(head, "content-length").and_then(|v| v.parse::<usize>().ok())
}

/// The value of the header `name` in `head`, matched without case.
fn header(head: &str, name: &str) -> Option<String> {
    head.lines().find_map(|l| {
        let (key, value) = l.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim().to_string())
    })
}

/// The head of a request: the request line and the headers.
fn head_of(request: &str) -> &str {
    request.split("\r\n\r\n").next().unwrap_or_default()
}

/// The JSON body of a request.
fn body_of(request: &str) -> Value {
    let body = request.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or_default();
    serde_json::from_str(body).unwrap_or_else(|e| panic!("{e}: {request}"))
}

/// A local server that answers each request with the next body of its list,
/// with `200`, and a request past the end of the list with `500`.
struct Server {
    url: String,
    addr: SocketAddr,
    handle: std::thread::JoinHandle<Vec<String>>,
}

impl Server {
    /// Stop the server and return the requests it saw, in order. The stop is
    /// a connection of its own, so a test whose adapter made fewer requests
    /// than the list holds fails on the count of requests and does not wait.
    fn requests(self) -> Vec<String> {
        if let Ok(mut stop) = TcpStream::connect(self.addr) {
            let _ = write!(stop, "{STOP}\r\n\r\n");
        }
        self.handle.join().expect("the server thread")
    }
}

fn serve(bodies: Vec<Value>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let mut seen = Vec::new();
        let mut bodies = bodies.into_iter();
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let request = read_request(&mut stream);
            if request.starts_with(STOP) {
                break;
            }
            seen.push(request);
            let (status, text) = match bodies.next() {
                Some(body) => ("200 OK", body.to_string()),
                None => ("500 Internal Server Error", "{}".to_string()),
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}",
                text.len()
            );
        }
        seen
    });
    Server { url: format!("http://{addr}"), addr, handle }
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

/// [`issue_node`] of the repository `owner`/`repo`, with the numeric id
/// `database_id`.
fn issue_node_in(owner: &str, repo: &str, number: u64, database_id: u64) -> Value {
    let mut node = issue_node(number, page(vec![], None));
    node["databaseId"] = json!(database_id);
    node["url"] = json!(format!("https://github.com/{owner}/{repo}/issues/{number}"));
    node["repository"] = json!({ "name": repo, "owner": { "login": owner } });
    node
}

/// A sub-issue answer: parent issue 2 with these sub-issues on one page.
fn sub_issue_answer(children: Vec<Value>) -> Value {
    json!({ "data": { "repository": { "i0": { "id": "I_2", "subIssues": page(children, None) } } } })
}

/// FTS-FR-ARMD / FTS-FR-ZLFX: the adapter of a test sends its requests
/// directly to the local server and never through a proxy that the
/// environment names, so the tests give the same result on every machine.
#[test]
fn the_test_adapter_uses_no_proxy() {
    let github = adapter("http://127.0.0.1:9");
    assert!(github.agent().config().proxy().is_none());
}

/// FTS-FR-ARMD: a removal of a blocked-by relation is a `DELETE` with no
/// body, which GitHub accepts; a body of `null` it refuses. It sends no
/// `Transfer-Encoding` header, so no chunked body goes with it either.
#[test]
fn remove_blocked_by_sends_a_delete_with_no_body() {
    let server = serve(vec![json!({})]);
    let github = adapter(&server.url);
    github.remember_id(4, 9004);
    github.remove_blocked_by(3, 4).unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    let head = head_of(&requests[0]);
    assert!(
        head.starts_with("DELETE /repos/acme/payments/issues/3/dependencies/blocked_by/9004 "),
        "{head}"
    );
    assert_eq!(content_length(head).unwrap_or(0), 0, "{head}");
    assert_eq!(header(head, "transfer-encoding"), None, "{head}");
    assert!(!requests[0].contains("null"), "{}", requests[0]);
}

/// FTS-FR-ARMD: the removal of a parent in another repository is made on
/// the parent's own repository.
#[test]
fn a_parent_in_another_repository_is_removed_there() {
    let server = serve(vec![json!({})]);
    let github = adapter(&server.url);
    github.remember_id(3, 9003);
    let upstream =
        IssueKey { owner: "acme".into(), repo: "upstream".into(), number: 9, url: String::new() };
    github.set_parent(3, Some(&upstream), None).unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert!(
        requests[0].starts_with("DELETE /repos/acme/upstream/issues/9/sub_issue "),
        "{}",
        requests[0]
    );
    assert_eq!(body_of(&requests[0]), json!({ "sub_issue_id": 9003 }));
}

/// FTS-FR-ARMD: the removal of a parent in the same repository is a
/// `DELETE` on the parent of this repository.
#[test]
fn a_parent_in_the_same_repository_is_removed_there() {
    let server = serve(vec![json!({})]);
    let github = adapter(&server.url);
    github.remember_id(3, 9003);
    let current =
        IssueKey { owner: "acme".into(), repo: "payments".into(), number: 2, url: String::new() };
    github.set_parent(3, Some(&current), None).unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert!(
        requests[0].starts_with("DELETE /repos/acme/payments/issues/2/sub_issue "),
        "{}",
        requests[0]
    );
    assert_eq!(body_of(&requests[0]), json!({ "sub_issue_id": 9003 }));
}

/// FTS-FR-ARMD: a new parent is one `POST` on the new parent that replaces
/// the current parent, so no `DELETE` comes before it.
#[test]
fn a_new_parent_replaces_the_current_one() {
    let server = serve(vec![json!({})]);
    let github = adapter(&server.url);
    github.remember_id(3, 9003);
    let current =
        IssueKey { owner: "acme".into(), repo: "payments".into(), number: 2, url: String::new() };
    github.set_parent(3, Some(&current), Some(6)).unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert!(
        requests[0].starts_with("POST /repos/acme/payments/issues/6/sub_issues "),
        "{}",
        requests[0]
    );
    assert_eq!(body_of(&requests[0]), json!({ "sub_issue_id": 9003, "replace_parent": true }));
}

/// FTS-FR-ARMD: where no read gave the numeric id of an issue, a relation
/// write reads it first with a `GET` on the issue, and then names it.
#[test]
fn a_numeric_id_that_no_read_gave_is_read_before_the_write() {
    let server = serve(vec![json!({ "id": 4444, "number": 4 }), json!({})]);
    let github = adapter(&server.url);
    github.add_blocked_by(3, 4).unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 2, "{requests:?}");
    assert!(requests[0].starts_with("GET /repos/acme/payments/issues/4 "), "{}", requests[0]);
    assert!(
        requests[1].starts_with("POST /repos/acme/payments/issues/3/dependencies/blocked_by "),
        "{}",
        requests[1]
    );
    assert_eq!(body_of(&requests[1]), json!({ "issue_id": 4444 }));
}

/// FTS-FR-ARMD / FTS-FR-WANV: a sub-issue in another repository with the
/// same number as an issue of this repository does not give its numeric id
/// to that issue. A later relation write reads the id of the own issue.
#[test]
fn a_sub_issue_of_another_repository_keeps_the_own_id() {
    let server = serve(vec![
        sub_issue_answer(vec![issue_node_in("acme", "upstream", 4, 7777)]),
        json!({ "id": 9004, "number": 4 }),
        json!({}),
    ]);
    let github = adapter(&server.url);
    let found = github.read_sub_issues(&[2]).unwrap();
    assert_eq!(found[0].1[0].repo, "upstream");
    github.remove_blocked_by(3, 4).unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 3, "{requests:?}");
    assert!(requests[1].starts_with("GET /repos/acme/payments/issues/4 "), "{}", requests[1]);
    assert!(
        requests[2]
            .starts_with("DELETE /repos/acme/payments/issues/3/dependencies/blocked_by/9004 "),
        "{}",
        requests[2]
    );
}

/// FTS-FR-ARMD: a sub-issue of this repository gives its numeric id to the
/// relation writes after it, where the owner and the repository differ
/// from the bound ones only in case.
#[test]
fn a_sub_issue_of_this_repository_in_another_case_gives_its_id() {
    let server =
        serve(vec![sub_issue_answer(vec![issue_node_in("ACME", "Payments", 4, 8004)]), json!({})]);
    let github = adapter(&server.url);
    github.read_sub_issues(&[2]).unwrap();
    github.remove_blocked_by(3, 4).unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 2, "no GET of the id: {requests:?}");
    assert!(
        requests[1]
            .starts_with("DELETE /repos/acme/payments/issues/3/dependencies/blocked_by/8004 "),
        "{}",
        requests[1]
    );
}

/// FTS NFR / FTS-FR-VYOV: a blocked-by list longer than one page is read to
/// its end, page by page, so no blocker past the first page reads as
/// removed.
#[test]
fn a_long_blocked_by_list_is_read_page_by_page() {
    let first: Vec<Value> = (1..=50).map(|n| key_node(100 + n)).collect();
    let server = serve(vec![
        json!({ "data": { "repository": { "i0": issue_node(3, page(first, Some("c1"))) } } }),
        json!({ "data": { "node": { "blockedBy": page(vec![key_node(151), key_node(152)], Some("c2")) } } }),
        json!({ "data": { "node": { "blockedBy": page(vec![key_node(153)], None) } } }),
    ]);
    let issues = adapter(&server.url).read_issues(&[3]).unwrap();
    let issue = issues[0].1.clone().expect("issue 3");
    let numbers: Vec<u64> = issue.blocked_by.iter().map(|k| k.number).collect();
    assert_eq!(numbers.len(), 53);
    assert_eq!(&numbers[50..], &[151, 152, 153]);
    let requests = server.requests();
    assert_eq!(requests.len(), 3, "{requests:?}");
    assert_eq!(body_of(&requests[1])["variables"], json!({ "id": "I_3", "after": "c1" }));
    assert_eq!(body_of(&requests[2])["variables"], json!({ "id": "I_3", "after": "c2" }));
}

/// FTS NFR: the sub-issues of a parent are read to the end too, and the
/// blockers of each sub-issue with them. The next page names the parent's
/// node id and the cursor of the page before it.
#[test]
fn a_long_sub_issue_list_is_read_page_by_page() {
    let parent = json!({ "id": "I_2", "subIssues": page(vec![issue_node(10, page(vec![], None))], Some("s1")) });
    let server = serve(vec![
        json!({ "data": { "repository": { "i0": parent } } }),
        json!({ "data": { "node": { "subIssues": page(vec![issue_node(11, page(vec![key_node(10)], None))], None) } } }),
    ]);
    let found = adapter(&server.url).read_sub_issues(&[2]).unwrap();
    let numbers: Vec<u64> = found[0].1.iter().map(|i| i.number).collect();
    assert_eq!(numbers, vec![10, 11]);
    assert_eq!(found[0].1[1].blocked_by[0].number, 10);
    let requests = server.requests();
    assert_eq!(requests.len(), 2, "{requests:?}");
    let second = body_of(&requests[1]);
    assert_eq!(second["variables"], json!({ "id": "I_2", "after": "s1" }));
    assert!(second["query"].as_str().unwrap().contains("subIssues(first: 50, after: $after)"));
}

/// FTS NFR / FTS-FR-VYOV: the blockers of a sub-issue longer than one page
/// are read to their end, and the next page names the node id of the
/// sub-issue, not that of its parent.
#[test]
fn the_blockers_of_a_sub_issue_are_read_page_by_page() {
    let first: Vec<Value> = (1..=50).map(|n| key_node(100 + n)).collect();
    let server = serve(vec![
        sub_issue_answer(vec![issue_node(10, page(first, Some("b1")))]),
        json!({ "data": { "node": { "blockedBy": page(vec![key_node(151)], None) } } }),
    ]);
    let found = adapter(&server.url).read_sub_issues(&[2]).unwrap();
    let blockers: Vec<u64> = found[0].1[0].blocked_by.iter().map(|k| k.number).collect();
    assert_eq!(blockers.len(), 51);
    assert_eq!(blockers[50], 151);
    let requests = server.requests();
    assert_eq!(requests.len(), 2, "{requests:?}");
    let second = body_of(&requests[1]);
    assert_eq!(second["variables"], json!({ "id": "I_10", "after": "b1" }));
    assert!(second["query"].as_str().unwrap().contains("blockedBy(first: 50, after: $after)"));
}

/// FTS NFR: a page that cannot be read fails the read with a code, so a cut
/// list never reads as complete.
#[test]
fn a_page_that_cannot_be_read_fails_the_read() {
    let first: Vec<Value> = (1..=50).map(|n| key_node(100 + n)).collect();
    let server = serve(vec![
        json!({ "data": { "repository": { "i0": issue_node(3, page(first, Some("c1"))) } } }),
        json!({ "errors": [{ "type": "RATE_LIMITED", "message": "x" }] }),
    ]);
    let error = adapter(&server.url).read_issues(&[3]).unwrap_err();
    assert_eq!(error, ERR_GITHUB_UNREACHABLE);
    assert_eq!(server.requests().len(), 2);
}

/// FTS NFR: a next page whose node is `null` (the issue is gone between two
/// requests) fails the read with `github_unreachable`.
#[test]
fn a_next_page_with_no_node_fails_the_read() {
    let first: Vec<Value> = (1..=50).map(|n| key_node(100 + n)).collect();
    let server = serve(vec![
        json!({ "data": { "repository": { "i0": issue_node(3, page(first, Some("c1"))) } } }),
        json!({ "data": { "node": null } }),
    ]);
    let error = adapter(&server.url).read_issues(&[3]).unwrap_err();
    assert_eq!(error, ERR_GITHUB_UNREACHABLE);
    assert_eq!(server.requests().len(), 2);
}

/// FTS NFR: an issue whose list has a next page and that gives no node id
/// cannot be read to its end, so the read fails with `github_unreachable`
/// and asks for no next page.
#[test]
fn a_next_page_of_an_issue_with_no_id_fails_the_read() {
    let first: Vec<Value> = (1..=50).map(|n| key_node(100 + n)).collect();
    let mut node = issue_node(3, page(first, Some("c1")));
    node.as_object_mut().unwrap().remove("id");
    let server = serve(vec![json!({ "data": { "repository": { "i0": node } } })]);
    let error = adapter(&server.url).read_issues(&[3]).unwrap_err();
    assert_eq!(error, ERR_GITHUB_UNREACHABLE);
    assert_eq!(server.requests().len(), 1);
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
