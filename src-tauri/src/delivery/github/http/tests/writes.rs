//! The tests of the writes of the `github` tracker adapter
//! (`GDP-github-delivery-plan.md` GDP-FR-UPXR, GDP-FR-DTPB, GDP-FR-EJTS,
//! GDP-FR-NJJZ, GDP-FR-YGXA, GDP-FR-WAKG, GDP-FR-UDOL, GDP-FR-HULQ): the
//! method, the address, and the body of each request. A recording transport
//! gives each answer, so no request leaves the process.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use super::super::*;
use super::{Recorder, SECRET};

const API: &str = "https://api.github.com";
const GRAPHQL: &str = "https://api.github.com/graphql";
const REPO: &str = "https://api.github.com/repos/acme/widgets";

/// An adapter of `acme/widgets` on a recording transport with `answers`.
fn recorded(answers: Vec<Value>) -> (GithubTracker, Arc<Mutex<Recorder>>) {
    let recorder = Arc::new(Mutex::new(Recorder { answers: answers.into(), ..Default::default() }));
    let mut t = GithubTracker::new(
        TrackerRepository {
            host: "github.com".into(),
            owner: "acme".into(),
            name: "widgets".into(),
        },
        SECRET.into(),
    );
    t.transport = Some(recorder.clone());
    (t, recorder)
}

/// Each request as its method, its address, and its body as JSON (`None`
/// for no body).
fn sent(recorder: &Arc<Mutex<Recorder>>) -> Vec<(String, String, Option<Value>)> {
    let r = recorder.lock().unwrap();
    r.requests
        .iter()
        .map(|v| {
            let body = v.body.as_deref().map(|b| serde_json::from_str(b).unwrap());
            (v.method.clone(), v.url.clone(), body)
        })
        .collect()
}

fn rest(method: &str, path: &str, body: Option<Value>) -> (String, String, Option<Value>) {
    (method.into(), format!("{REPO}{path}"), body)
}

/// The answer of `GET /issues/<n>`, with GitHub's numeric id.
fn database_id(id: u64) -> Value {
    json!({ "id": id })
}

fn link(owner: &str, repo: &str, number: u64) -> IssueLink {
    IssueLink {
        owner: owner.into(),
        repo: repo.into(),
        number,
        url: String::new(),
        title: String::new(),
        state: "open".into(),
    }
}

/// GDP-FR-UDOL, GDP-FR-NJJZ: `create_issue` posts the title, the body, the
/// Type by name, and the milestone number, and reads the new issue from the
/// answer. With no Type and no milestone, neither key is sent.
#[test]
fn create_issue_posts_the_issue() {
    let answer = json!({ "number": 101, "id": 9001, "node_id": "I_101", "title": "T",
                         "body": "B", "state": "open", "html_url": "https://github.com/acme/widgets/issues/101",
                         "type": { "name": "Bug" }, "milestone": { "number": 2 },
                         "updated_at": "u" });
    let (t, r) = recorded(vec![answer]);
    let new = NewIssue {
        title: "T".into(),
        body: "B".into(),
        issue_type: Some("Bug".into()),
        milestone: Some(2),
    };
    let created = t.create_issue(&new).unwrap();
    assert_eq!(
        sent(&r),
        [rest(
            "POST",
            "/issues",
            Some(json!({ "title": "T", "body": "B", "type": "Bug", "milestone": 2 }))
        )]
    );
    assert_eq!((created.number, created.node_id.as_str()), (101, "I_101"));
    assert_eq!((created.issue_type.as_deref(), created.milestone), (Some("Bug"), Some(2)));
    // The numeric id of the answer is kept: a relation write needs no read.
    t.add_blocked_by(5, 101).unwrap();
    assert_eq!(
        sent(&r)[1],
        rest("POST", "/issues/5/dependencies/blocked_by", Some(json!({ "issue_id": 9001 })))
    );

    let (t, r) = recorded(vec![json!({ "number": 102 })]);
    let plain =
        NewIssue { title: "T".into(), body: String::new(), issue_type: None, milestone: None };
    t.create_issue(&plain).unwrap();
    assert_eq!(sent(&r), [rest("POST", "/issues", Some(json!({ "title": "T", "body": "" })))]);
    // An answer with no number is a failed request.
    let (t, _) = recorded(vec![json!({})]);
    assert_eq!(t.create_issue(&plain).unwrap_err(), ERR_REQUEST_FAILED);
}

/// GDP-FR-DTPB, GDP-FR-EJTS: `update_issue` patches only the fields it is
/// given, the Type by name among them.
#[test]
fn update_issue_patches_the_given_fields() {
    let (t, r) = recorded(vec![]);
    let all = IssueChange {
        title: Some("N".into()),
        body: Some("D".into()),
        issue_type: Some("Feature".into()),
    };
    t.update_issue(3, &all).unwrap();
    t.update_issue(3, &IssueChange { issue_type: Some("Task".into()), ..Default::default() })
        .unwrap();
    t.update_issue(4, &IssueChange { title: Some("Only".into()), ..Default::default() }).unwrap();
    assert_eq!(
        sent(&r),
        [
            rest(
                "PATCH",
                "/issues/3",
                Some(json!({ "title": "N", "body": "D", "type": "Feature" }))
            ),
            rest("PATCH", "/issues/3", Some(json!({ "type": "Task" }))),
            rest("PATCH", "/issues/4", Some(json!({ "title": "Only" }))),
        ]
    );
}

/// GDP-FR-WAKG: `set_parent` reads the numeric id of the issue once, posts it
/// to the new parent's `sub_issues` with `replace_parent`, and, to remove the
/// parent, deletes it from the `sub_issue` of the current parent's
/// repository. No parent before and after sends nothing but the id read.
#[test]
fn set_parent_posts_the_sub_issue_and_removes_it_from_the_old_parent() {
    let (t, r) = recorded(vec![database_id(7003)]);
    t.set_parent(3, Some(&link("acme", "widgets", 1)), Some(2)).unwrap();
    t.set_parent(3, Some(&link("other", "repo", 9)), None).unwrap();
    t.set_parent(3, None, None).unwrap();
    assert_eq!(
        sent(&r),
        [
            rest("GET", "/issues/3", None),
            rest(
                "POST",
                "/issues/2/sub_issues",
                Some(json!({ "sub_issue_id": 7003, "replace_parent": true }))
            ),
            (
                "DELETE".into(),
                format!("{API}/repos/other/repo/issues/9/sub_issue"),
                Some(json!({ "sub_issue_id": 7003 }))
            ),
        ],
        "the id is read once"
    );
    // An id answer with no `id` is a failed request, and nothing is written.
    let (t, r) = recorded(vec![json!({})]);
    assert_eq!(t.set_parent(3, None, Some(2)).unwrap_err(), ERR_REQUEST_FAILED);
    assert_eq!(sent(&r).len(), 1);
}

/// GDP-FR-WAKG: `set_milestone` patches the milestone number, and `null`
/// removes it.
#[test]
fn set_milestone_patches_the_number_and_null_removes_it() {
    let (t, r) = recorded(vec![]);
    t.set_milestone(3, Some(2)).unwrap();
    t.set_milestone(3, None).unwrap();
    assert_eq!(
        sent(&r),
        [
            rest("PATCH", "/issues/3", Some(json!({ "milestone": 2 }))),
            rest("PATCH", "/issues/3", Some(json!({ "milestone": null }))),
        ]
    );
}

/// GDP-FR-YGXA: a `blocks` edge is a blocked-by relation of the blocked
/// issue. The add posts the blocker's numeric id; the remove deletes it by
/// that id in the address, with no body.
#[test]
fn blocked_by_relations_are_added_and_removed() {
    let (t, r) = recorded(vec![database_id(8001)]);
    t.add_blocked_by(4, 1).unwrap();
    t.remove_blocked_by(4, 1).unwrap();
    assert_eq!(
        sent(&r),
        [
            rest("GET", "/issues/1", None),
            rest("POST", "/issues/4/dependencies/blocked_by", Some(json!({ "issue_id": 8001 }))),
            rest("DELETE", "/issues/4/dependencies/blocked_by/8001", None),
        ]
    );
}

/// GDP-FR-DTPB, GDP-FR-UDOL: the milestone writes. A close patches the state,
/// a create posts the due date at noon UTC, and an update patches only the
/// given fields, where a due of `None` removes the due date.
#[test]
fn milestone_writes_send_their_fields() {
    let answer = json!({ "number": 5, "html_url": "https://github.com/acme/widgets/milestone/5",
                         "title": "M", "state": "open", "due_on": "2027-01-05T12:00:00Z",
                         "updated_at": "u" });
    let (t, r) = recorded(vec![answer]);
    let created = t
        .create_milestone(&NewMilestone {
            title: "M".into(),
            description: "D".into(),
            due: Some("2027-01-05".into()),
        })
        .unwrap();
    assert_eq!((created.number, created.due.as_deref()), (5, Some("2027-01-05")));
    t.update_milestone(5, &MilestoneChange { due: Some(None), ..Default::default() }).unwrap();
    t.update_milestone(
        5,
        &MilestoneChange {
            title: Some("N".into()),
            description: Some("E".into()),
            due: Some(Some("2027-02-01".into())),
        },
    )
    .unwrap();
    t.update_milestone(5, &MilestoneChange::default()).unwrap();
    t.close_milestone(5).unwrap();
    assert_eq!(
        sent(&r),
        [
            rest(
                "POST",
                "/milestones",
                Some(json!({ "title": "M", "description": "D", "due_on": "2027-01-05T12:00:00Z" }))
            ),
            rest("PATCH", "/milestones/5", Some(json!({ "due_on": null }))),
            rest(
                "PATCH",
                "/milestones/5",
                Some(json!({ "title": "N", "description": "E", "due_on": "2027-02-01T12:00:00Z" }))
            ),
            rest("PATCH", "/milestones/5", Some(json!({}))),
            rest("PATCH", "/milestones/5", Some(json!({ "state": "closed" }))),
        ]
    );
}

/// GDP-FR-DTPB: `close_issue` patches the state with the reason
/// `not_planned`.
#[test]
fn close_issue_patches_the_state_and_the_reason() {
    let (t, r) = recorded(vec![]);
    t.close_issue(3, CloseWith::NotPlanned).unwrap();
    assert_eq!(
        sent(&r),
        [rest(
            "PATCH",
            "/issues/3",
            Some(json!({ "state": "closed", "state_reason": "not_planned" }))
        )]
    );
}

/// GDP-FR-HULQ, GDP-FR-UDOL: the Project item writes are GraphQL mutations
/// with the Project and the content or item as variables. The add answers
/// the new item id; an answer with no item id is a failed request.
#[test]
fn project_item_writes_are_graphql_mutations() {
    let added = json!({ "data": { "addProjectV2ItemById": { "item": { "id": "PVTI_9" } } } });
    let removed = json!({ "data": { "deleteProjectV2Item": { "deletedItemId": "PVTI_9" } } });
    let (t, r) = recorded(vec![added, removed, json!({ "data": {} })]);
    assert_eq!(t.add_project_item("PVT_1", "I_101").unwrap(), "PVTI_9");
    t.remove_project_item("PVT_1", "PVTI_9").unwrap();
    assert_eq!(t.add_project_item("PVT_1", "I_101").unwrap_err(), ERR_REQUEST_FAILED);
    let requests = sent(&r);
    let mutation = |i: usize| {
        let (method, url, body) = &requests[i];
        assert_eq!((method.as_str(), url.as_str()), ("POST", GRAPHQL));
        body.clone().unwrap()
    };
    let add = mutation(0);
    assert!(add["query"].as_str().unwrap().contains(
        "addProjectV2ItemById(input: { projectId: $project, contentId: $content }) { item { id } }"
    ));
    assert_eq!(add["variables"], json!({ "project": "PVT_1", "content": "I_101" }));
    let remove = mutation(1);
    assert!(remove["query"]
        .as_str()
        .unwrap()
        .contains("deleteProjectV2Item(input: { projectId: $project, itemId: $item })"));
    assert_eq!(remove["variables"], json!({ "project": "PVT_1", "item": "PVTI_9" }));
    // Every write carries the token as a credential header.
    let r = r.lock().unwrap();
    assert!(r
        .requests
        .iter()
        .all(|v| v.headers["authorization"] == (format!("Bearer {SECRET}"), true)));
}
