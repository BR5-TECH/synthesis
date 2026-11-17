//! The tests of the pure parts of the `github` tracker adapter
//! (`GDP-github-delivery-plan.md` GDP-FR-ZGJB, GDP-FR-BGIT, GPP-FR-SUFH):
//! how it builds a request, how it maps a failed request to a code, and how it
//! reads the pages of an answer. No test sends a request.

use std::cell::RefCell;

use serde_json::{json, Value};

use super::*;
use crate::github_polling::records::{
    ERR_GITHUB_UNREACHABLE, ERR_PROJECT_UNAVAILABLE, ERR_REQUEST_FAILED,
};

mod writes;

const SECRET: &str = "ghp_test_secret";

/// GDP-FR-UPXR: a transport that records each request and gives the next
/// answer of a list, or `null` when the list is empty. No request leaves the
/// process.
#[derive(Default)]
pub(crate) struct Recorder {
    pub(crate) requests: Vec<crate::http::RequestView>,
    pub(crate) answers: std::collections::VecDeque<Value>,
}

impl Recorder {
    pub(crate) fn answer(
        &mut self,
        request: crate::http::RequestView,
    ) -> Result<String, crate::http::HttpError> {
        self.requests.push(request);
        Ok(self.answers.pop_front().unwrap_or(Value::Null).to_string())
    }
}

fn tracker(host: &str) -> GithubTracker {
    GithubTracker::new(
        TrackerRepository { host: host.into(), owner: "acme".into(), name: "widgets".into() },
        SECRET.into(),
    )
}

/// One raw issue value of a GraphQL answer.
fn raw_issue(number: u64) -> Value {
    json!({
        "__typename": "Issue",
        "id": format!("I_{number}"),
        "number": number,
        "url": format!("https://github.com/acme/widgets/issues/{number}"),
        "state": "OPEN",
        "stateReason": null,
        "title": format!("Issue {number}"),
        "body": "Body",
        "updatedAt": "2026-01-01T00:00:00Z",
        "repository": { "name": "widgets", "owner": { "login": "acme" } },
        "issueType": { "name": "Task" },
        "milestone": null,
        "parent": null,
        "blockedBy": { "nodes": [], "pageInfo": { "hasNextPage": false, "endCursor": null } }
    })
}

/// One answer of the Project query with the issues `numbers` and the cursor
/// of the next page.
fn page(numbers: &[u64], next: Option<&str>) -> Value {
    let nodes: Vec<Value> = numbers
        .iter()
        .map(|n| {
            json!({ "id": format!("ITEM_{n}"), "fieldValueByName": { "name": "Todo" },
                    "content": raw_issue(*n) })
        })
        .collect();
    json!({ "data": { "node": {
        "id": "PVT_1", "title": "Roadmap", "shortDescription": "The plan", "number": 7,
        "url": "https://github.com/orgs/acme/projects/7",
        "owner": { "login": "acme" },
        "field": { "id": "FIELD", "options": [ { "id": "O1", "name": "Todo" } ] },
        "items": { "pageInfo": { "hasNextPage": next.is_some(), "endCursor": next },
                   "nodes": nodes }
    } } })
}

fn numbers(project: &ProjectRead) -> Vec<u64> {
    project
        .items
        .iter()
        .filter_map(|i| match &i.content {
            ItemContent::Issue(issue) => Some(issue.number),
            _ => None,
        })
        .collect()
}

/// GDP-FR-BGIT: a read takes every page, in order, each with the cursor of
/// the page before, and each issue gets its blockers.
#[test]
fn a_read_takes_every_page_in_order() {
    let cursors = RefCell::new(Vec::new());
    let blocker_reads = RefCell::new(0);
    let project = read_project_pages(
        3,
        |cursor| {
            cursors.borrow_mut().push(cursor.map(str::to_string));
            Ok(match cursor {
                None => page(&[1, 2], Some("c1")),
                Some("c1") => page(&[3], Some("c2")),
                _ => page(&[4], None),
            })
        },
        |_| {
            *blocker_reads.borrow_mut() += 1;
            Ok(vec![IssueLink {
                owner: "acme".into(),
                repo: "widgets".into(),
                number: 9,
                url: String::new(),
                title: String::new(),
                state: "open".into(),
            }])
        },
    )
    .expect("a whole Project");
    assert_eq!(*cursors.borrow(), [None, Some("c1".to_string()), Some("c2".to_string())]);
    assert_eq!(numbers(&project), [1, 2, 3, 4]);
    assert_eq!(*blocker_reads.borrow(), 4, "each issue gets its blockers");
    let ItemContent::Issue(first) = &project.items[0].content else { panic!("an issue") };
    assert_eq!(first.blocked_by.len(), 1);
    assert_eq!((project.title.as_str(), project.number), ("Roadmap", 7));
    // DSR-FR-EDHP: the read takes the Project's URL.
    assert_eq!(project.url, "https://github.com/orgs/acme/projects/7");
    assert_eq!(project.items[0].status.as_deref(), Some("Todo"));
}

/// GDP-FR-BGIT: a Project that still has pages after the page limit is an
/// error, not a cut list that reads as whole.
#[test]
fn a_project_with_pages_after_the_limit_is_an_error() {
    let fetched = RefCell::new(0);
    let result = read_project_pages(
        3,
        |cursor| {
            *fetched.borrow_mut() += 1;
            let next = format!("{}x", cursor.unwrap_or("c"));
            Ok(page(&[*fetched.borrow() as u64], Some(next.as_str())))
        },
        |_| Ok(Vec::new()),
    );
    assert_eq!(result.unwrap_err(), ERR_REQUEST_FAILED);
    assert_eq!(*fetched.borrow(), 3, "no page after the limit is fetched");

    // The same number of pages with no page after the last one reads whole.
    let fetched = RefCell::new(0);
    let whole = read_project_pages(
        3,
        |_| {
            *fetched.borrow_mut() += 1;
            let n = *fetched.borrow();
            let next = format!("c{n}");
            Ok(page(&[n as u64], (n < 3).then_some(next.as_str())))
        },
        |_| Ok(Vec::new()),
    )
    .expect("three pages are whole");
    assert_eq!(numbers(&whole), [1, 2, 3]);
}

/// GDP-FR-BGIT: the adapter's Project read takes at most `MAX_ITEM_PAGES`
/// pages. That many pages with no page after the last one read whole; a
/// cursor after the last of them is an error, and no further page is
/// fetched. Each request names the Project and the cursor of the page before.
#[test]
fn the_project_read_takes_at_most_the_page_limit() {
    let t = tracker("github.com");
    for more in [false, true] {
        let cursors = RefCell::new(Vec::new());
        let result = t.read_project_with("PVT_1", |_, variables| {
            assert_eq!(variables["id"], "PVT_1");
            cursors.borrow_mut().push(variables["cursor"].clone());
            let n = cursors.borrow().len();
            let next = format!("c{n}");
            let last = n == MAX_ITEM_PAGES && !more;
            Ok(page(&[n as u64], (!last).then_some(next.as_str())))
        });
        let cursors = cursors.into_inner();
        assert_eq!(cursors.len(), MAX_ITEM_PAGES, "more pages: {more}");
        assert_eq!(cursors[0], Value::Null);
        assert_eq!(cursors[1], json!("c1"));
        match more {
            false => {
                let whole = result.expect("the page limit reads whole");
                assert_eq!(numbers(&whole), (1..=MAX_ITEM_PAGES as u64).collect::<Vec<_>>());
            }
            true => assert_eq!(result.unwrap_err(), ERR_REQUEST_FAILED),
        }
    }
}

/// GDP-FR-BGIT: a page that names its own cursor again is an error, so the
/// read always ends; the error of a page or of a blocker read is the read's.
#[test]
fn a_repeated_cursor_and_a_failed_page_are_errors() {
    let fetched = RefCell::new(0);
    let repeated = read_project_pages(
        40,
        |_| {
            *fetched.borrow_mut() += 1;
            Ok(page(&[1], Some("same")))
        },
        |_| Ok(Vec::new()),
    );
    assert_eq!(repeated.unwrap_err(), ERR_REQUEST_FAILED);
    assert_eq!(*fetched.borrow(), 2, "the read stops at the page that repeats its cursor");
    let fetched = RefCell::new(0);
    let failed = read_project_pages(
        40,
        |cursor| {
            *fetched.borrow_mut() += 1;
            match cursor {
                None => Ok(page(&[1], Some("c1"))),
                Some(_) => Err(ERR_GITHUB_UNREACHABLE.to_string()),
            }
        },
        |_| Ok(Vec::new()),
    );
    assert_eq!(failed.unwrap_err(), ERR_GITHUB_UNREACHABLE);
    assert_eq!(*fetched.borrow(), 2, "the read stops at the page that failed");
    let blockers =
        read_project_pages(40, |_| Ok(page(&[1], None)), |_| Err(ERR_REQUEST_FAILED.to_string()));
    assert_eq!(blockers.unwrap_err(), ERR_REQUEST_FAILED);
    assert_eq!(
        read_project_pages(0, |_| Ok(page(&[1], None)), |_| Ok(Vec::new())).unwrap_err(),
        ERR_REQUEST_FAILED,
        "no page at all is no Project"
    );
}

/// GDP-FR-BGIT: a pull request and a draft item are items of the Project with
/// no issue; only an issue gets its blockers.
#[test]
fn pull_requests_and_drafts_are_items_without_an_issue() {
    let mut answer = page(&[1], None);
    let nodes = answer["data"]["node"]["items"]["nodes"].as_array_mut().unwrap();
    nodes.push(json!({ "id": "PR", "content": { "__typename": "PullRequest" } }));
    nodes.push(json!({ "id": "DRAFT", "content": { "__typename": "DraftIssue" } }));
    nodes.push(json!({ "id": "NONE", "content": null }));
    let blocker_reads = RefCell::new(0);
    let project = read_project_pages(
        1,
        |_| Ok(answer.clone()),
        |_| {
            *blocker_reads.borrow_mut() += 1;
            Ok(Vec::new())
        },
    )
    .unwrap();
    let kinds: Vec<&ItemContent> = project.items.iter().map(|i| &i.content).collect();
    assert!(matches!(kinds[0], ItemContent::Issue(_)));
    assert_eq!(kinds[1], &ItemContent::PullRequest);
    assert_eq!(kinds[2], &ItemContent::Draft);
    assert_eq!(kinds[3], &ItemContent::Draft);
    assert_eq!(*blocker_reads.borrow(), 1);
}

/// GPP-FR-SUFH: a Project node that is absent is `project_unavailable` where
/// GitHub says not found or forbidden, and `github_request_failed` for any
/// other error; an answer with no items connection is an error.
#[test]
fn an_absent_project_maps_to_its_code() {
    let absent = |errors: Value| json!({ "data": { "node": null }, "errors": errors });
    let code = |v: Value| project_of(&v).err().unwrap();
    assert_eq!(code(absent(json!([{ "type": "NOT_FOUND" }]))), ERR_PROJECT_UNAVAILABLE);
    assert_eq!(code(absent(json!([{ "type": "FORBIDDEN" }]))), ERR_PROJECT_UNAVAILABLE);
    assert_eq!(code(json!({ "data": { "node": null } })), ERR_PROJECT_UNAVAILABLE);
    assert_eq!(code(absent(json!([{ "type": "RATE_LIMITED" }]))), ERR_REQUEST_FAILED);
    let mut no_items = page(&[], None);
    no_items["data"]["node"]["items"] = Value::Null;
    assert_eq!(code(no_items), ERR_REQUEST_FAILED);
}

/// GDP-FR-BGIT: the cursor of the next page: none on the last page, and an error where a
/// next page has no cursor.
#[test]
fn the_next_cursor_is_read_from_the_page_info() {
    let info =
        |has: Value, end: Value| json!({ "pageInfo": { "hasNextPage": has, "endCursor": end } });
    assert_eq!(next_cursor(&info(json!(false), json!("x"))), Ok(None));
    assert_eq!(next_cursor(&json!({})), Ok(None), "no page info is the last page");
    assert_eq!(next_cursor(&info(json!(true), json!("c9"))), Ok(Some("c9".into())));
    assert_eq!(next_cursor(&info(json!(true), json!(""))).unwrap_err(), ERR_REQUEST_FAILED);
    assert_eq!(next_cursor(&info(json!(true), Value::Null)).unwrap_err(), ERR_REQUEST_FAILED);
}

/// GDP-FR-BGIT: the nodes of a connection come from each page in order; a repeated cursor
/// and a failed page are errors.
#[test]
fn every_node_of_a_connection_is_read() {
    let first = json!({ "nodes": [1, 2], "pageInfo": { "hasNextPage": true, "endCursor": "a" } });
    let nodes = all_nodes(&first, |after| {
        assert_eq!(after, "a");
        Ok(json!({ "nodes": [3], "pageInfo": { "hasNextPage": false } }))
    })
    .unwrap();
    assert_eq!(nodes, [json!(1), json!(2), json!(3)]);
    let looping = all_nodes(&first, |_| {
        Ok(json!({ "nodes": [], "pageInfo": { "hasNextPage": true, "endCursor": "a" } }))
    });
    assert_eq!(looping.unwrap_err(), ERR_REQUEST_FAILED);
    let failed = all_nodes(&first, |_| Err(ERR_GITHUB_UNREACHABLE.to_string()));
    assert_eq!(failed.unwrap_err(), ERR_GITHUB_UNREACHABLE);
}

/// GPP-FR-SUFH: a GraphQL `NOT_FOUND` error leaves its field `null`; any other error, and
/// an answer with no `data`, fail the request.
#[test]
fn graphql_errors_map_to_codes() {
    let found =
        data_of(json!({ "data": { "repository": null }, "errors": [{ "type": "NOT_FOUND" }] }));
    assert_eq!(found.unwrap(), json!({ "repository": null }));
    let other = data_of(json!({ "data": { "x": 1 }, "errors": [{ "type": "FORBIDDEN" }] }));
    assert_eq!(other.unwrap_err(), ERR_REQUEST_FAILED);
    assert_eq!(data_of(json!({ "data": null })).unwrap_err(), ERR_REQUEST_FAILED);
    assert_eq!(data_of(json!({})).unwrap_err(), ERR_REQUEST_FAILED);
}

/// GPP-FR-SUFH: a failed request is a fixed code that carries no request
/// text: a status is `github_request_failed`, a refused certificate is its
/// TLS code, and every other failure is `github_unreachable`.
#[test]
fn a_failed_request_maps_to_a_fixed_code() {
    use crate::http::HttpError;
    assert_eq!(GithubTracker::fail(&HttpError::Status(404)), ERR_REQUEST_FAILED);
    assert_eq!(GithubTracker::fail(&HttpError::Status(502)), ERR_REQUEST_FAILED);
    for other in [HttpError::Timeout, HttpError::Transport, HttpError::Redirect, HttpError::Body] {
        assert_eq!(GithubTracker::fail(&other), ERR_GITHUB_UNREACHABLE, "{other:?}");
    }
    let tls = crate::tls::TlsFailure::new("ghe.example.com", crate::tls::TlsCause::Expired);
    let wire = tls.wire();
    assert_eq!(GithubTracker::fail(&HttpError::Tls(tls)), wire);
}

/// GDP-FR-ZGJB: every request goes to the GraphQL or REST base of the
/// polling repository's host, with the secret as a bearer credential.
#[test]
fn requests_go_to_the_bases_of_the_host_with_the_secret() {
    for (host, graphql, api) in [
        ("github.com", "https://api.github.com/graphql", "https://api.github.com"),
        ("acme.ghe.com", "https://api.acme.ghe.com/graphql", "https://api.acme.ghe.com"),
        (
            "git.example.com",
            "https://git.example.com/api/graphql",
            "https://git.example.com/api/v3",
        ),
    ] {
        let t = tracker(host);
        let query = t.graphql_request("query { viewer { login } }", json!({ "a": 1 })).test_view();
        assert_eq!((query.method.as_str(), query.url.as_str()), ("POST", graphql), "{host}");
        assert_eq!(query.headers["authorization"], (format!("Bearer {SECRET}"), true));
        assert_eq!(query.headers["graphql-features"].0, "issue_types,sub_issues");
        assert_eq!(query.headers["accept"].0, "application/vnd.github+json");
        let body: Value = serde_json::from_str(query.body.as_deref().unwrap()).unwrap();
        assert_eq!(body, json!({ "query": "query { viewer { login } }", "variables": { "a": 1 } }));

        let path = "/repos/acme/widgets/issues/3";
        let patch = t.rest_request("PATCH", path, Some(json!({ "title": "x" }))).test_view();
        assert_eq!((patch.method.as_str(), patch.url), ("PATCH", format!("{api}{path}")));
        assert_eq!(patch.headers["authorization"], (format!("Bearer {SECRET}"), true));
        assert_eq!(
            patch.headers["x-github-api-version"].0,
            crate::github_tokens::GITHUB_API_VERSION
        );
        assert_eq!(patch.headers["content-type"].0, "application/json");
        assert_eq!(patch.body.as_deref(), Some("{\"title\":\"x\"}"));
        assert!(!patch.invalid_header);
    }
    let t = tracker("github.com");
    assert_eq!(t.rest_request("GET", "/x", Some(json!({ "a": 1 }))).test_view().body, None);
    assert_eq!(t.rest_request("DELETE", "/x", None).test_view().body, None);
    let delete = t.rest_request("DELETE", "/x", Some(json!({ "sub_issue_id": 5 }))).test_view();
    assert_eq!(delete.body.as_deref(), Some("{\"sub_issue_id\":5}"));
    assert_eq!(t.rest_request("POST", "/x", None).test_view().body.as_deref(), Some("{}"));
    assert_eq!(t.repo_path(), "/repos/acme/widgets");

    // The production connector binds the adapter to the repository it names.
    let bound =
        TrackerRepository { host: "acme.ghe.com".into(), owner: "o".into(), name: "n".into() };
    assert_eq!(*HttpConnector.connect(&bound, SECRET).repository(), bound);
}

/// GDP-FR-QGFO, GDP-FR-MYLM: an issue and a milestone of an answer, with the
/// state in lower case, the close reason, and the due date as a calendar date.
#[test]
fn issues_and_milestones_are_read_from_an_answer() {
    let mut raw = raw_issue(5);
    raw["state"] = json!("CLOSED");
    raw["stateReason"] = json!("NOT_PLANNED");
    raw["milestone"] = json!({ "number": 2 });
    raw["parent"] = json!({ "number": 1, "url": "u", "title": "P", "state": "OPEN",
                            "repository": { "name": "widgets", "owner": { "login": "acme" } } });
    let issue = issue_of(&raw).unwrap();
    assert_eq!(
        (issue.state.as_str(), issue.state_reason),
        ("closed", Some(StateReason::NotPlanned))
    );
    assert_eq!((issue.milestone, issue.issue_type.as_deref()), (Some(2), Some("Task")));
    assert_eq!(issue.parent.as_ref().map(|p| (p.number, p.state.as_str())), Some((1, "open")));
    assert_eq!(issue_of(&Value::Null), None);
    assert_eq!(issue_of(&json!({ "__typename": "Issue" })), None);

    let graphql = json!({ "number": 3, "url": "https://x/m/3", "title": "M", "state": "OPEN",
                          "dueOn": "2027-01-05T08:00:00Z", "updatedAt": "t" });
    let m = milestone_of(&graphql).unwrap();
    assert_eq!(
        (m.state.as_str(), m.due.as_deref(), m.url.as_str()),
        ("open", Some("2027-01-05"), "https://x/m/3")
    );
    let rest = json!({ "number": 4, "html_url": "https://x/m/4", "title": "R", "state": "closed",
                       "due_on": null, "updated_at": "u" });
    let m = milestone_of(&rest).unwrap();
    assert_eq!((m.url.as_str(), m.due, m.updated_at.as_str()), ("https://x/m/4", None, "u"));
    assert_eq!(due_on(Some("2027-01-05")), json!("2027-01-05T12:00:00Z"));
    assert_eq!(due_on(None), Value::Null);
}

/// GDP-FR-QGFO: the issue query asks for the assignees, and an issue value
/// gives their logins in order: from `assignees.nodes` of GraphQL, and from
/// `assignees` of REST. An issue with no assignee value has none.
#[test]
fn an_issue_gives_its_assignee_logins() {
    assert!(issue_fields().contains("assignees(first: 10) { nodes { login } }"));
    let mut raw = raw_issue(1);
    raw["assignees"] = json!({ "nodes": [ { "login": "mia" }, { "login": "jo" } ] });
    assert_eq!(issue_of(&raw).unwrap().assignees, vec!["mia", "jo"]);
    assert!(issue_of(&raw_issue(2)).unwrap().assignees.is_empty());
    let rest = json!({ "assignees": [ { "login": "kai" }, { "id": 5 } ] });
    assert_eq!(assignees_of(&rest), vec!["kai"]);
    assert!(assignees_of(&json!({ "assignees": null })).is_empty());
}

/// DSR-FR-EDHP: the Project query asks for the Project's `url`.
#[test]
fn the_project_query_asks_for_the_url() {
    let t = tracker("github.com");
    let queries = RefCell::new(Vec::new());
    let project = t
        .read_project_with("PVT_1", |query, _| {
            queries.borrow_mut().push(query.to_string());
            Ok(page(&[], None))
        })
        .unwrap();
    assert!(queries.borrow()[0].contains("number url owner"));
    assert_eq!(project.url, "https://github.com/orgs/acme/projects/7");
}
