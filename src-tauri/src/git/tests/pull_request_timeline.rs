//! The pull request timeline operation (GTC-FR-CKTM, GTC-FR-DWVY).
//!
//! The fixtures are the ones `pull_requests.rs` holds: a scripted GitHub that
//! records its requests, and a project that resolves a token.

use super::pull_requests::{
    page_path, repo_with_origin, token_ready, FakeGithub, PLATFORM, SECRET,
};
use super::*;
use crate::git::pull_requests::client::{GithubFailure, MAX_PAGES, PAGE_SIZE};
use crate::git::pull_requests::{ERR_PULL_REQUEST_NOT_FOUND, ERR_GITHUB_TOKEN_REJECTED};
use serde_json::{json, Value};

const TIMELINE: &str = "/repos/acme/platform/issues/9/timeline";
const REVIEW_COMMENTS: &str = "/repos/acme/platform/pulls/9/comments";

fn timeline(
    f: &WorktreeFixture,
    github: &FakeGithub,
    buffer: &'static LogBuffer,
) -> Result<PullRequestTimeline, String> {
    let (store, tokens) = token_ready();
    list_pull_request_timeline_reported(
        &NullSink, buffer, &f.root(), &store, &tokens, "/dev/acme", github, 9,
    )
}

/// A GitHub holding `events` as the whole timeline and `comments` as the whole
/// list of review comments.
fn github_with(events: Vec<Value>, comments: Vec<Value>) -> FakeGithub {
    let github = FakeGithub::new();
    github.pages(TIMELINE, vec![events]);
    github.pages(REVIEW_COMMENTS, vec![comments]);
    github
}

fn comment_event(id: u64, at: &str) -> Value {
    json!({ "event": "commented", "id": id, "actor": { "login": "alice" },
            "created_at": at, "body": format!("Comment {id}") })
}

fn review_comment_json(id: u64, at: &str) -> Value {
    json!({ "id": id, "user": { "login": "bob" }, "path": "src/a.rs",
            "body": format!("Review comment {id}"), "created_at": at })
}

fn kinds(items: &[PullRequestTimelineItem]) -> Vec<&str> {
    items.iter().map(|item| item.kind.as_str()).collect()
}

// GTC-FR-CKTM
#[test]
fn every_kind_of_activity_maps_to_its_item() {
    let f = repo_with_origin(PLATFORM);
    let github = github_with(
        vec![
            json!({ "event": "committed", "sha": "abc123", "node_id": "C_x",
                    "message": "Add the thing\n\nWith a longer explanation.",
                    "author": { "name": "Carol", "email": "carol@example.com",
                                "date": "2024-01-01T08:00:00Z" },
                    "committer": { "name": "Carol", "email": "carol@example.com",
                                   "date": "2024-01-01T08:00:00Z" } }),
            comment_event(101, "2024-01-02T10:00:00Z"),
            json!({ "event": "reviewed", "id": 201, "user": { "login": "bob" },
                    "submitted_at": "2024-01-03T10:00:00Z", "state": "changes_requested",
                    "body": "Please fix", "html_url": "https://example.test/x" }),
            json!({ "event": "line-commented", "comments": [
                    review_comment_json(301, "2024-01-03T10:00:05Z") ] }),
            json!({ "event": "labeled", "id": 401, "actor": { "login": "alice" },
                    "created_at": "2024-01-04T10:00:00Z", "label": { "name": "bug" } }),
            json!({ "event": "review_requested", "id": 402, "actor": { "login": "alice" },
                    "created_at": "2024-01-04T11:00:00Z" }),
            json!({ "event": "head_ref_force_pushed", "id": 403, "actor": { "login": "alice" },
                    "created_at": "2024-01-05T10:00:00Z" }),
            json!({ "event": "merged", "id": 404, "actor": { "login": "dave" },
                    "created_at": "2024-01-06T10:00:00Z", "commit_id": "abc123" }),
        ],
        vec![],
    );

    let got = timeline(&f, &github, own_buffer()).unwrap();

    assert!(!got.truncated);
    assert_eq!(
        kinds(&got.items),
        vec!["commit", "comment", "review", "review_comment", "event", "event", "event", "event"]
    );
    let commit = &got.items[0];
    assert_eq!(commit.id, "commit:abc123");
    assert_eq!(commit.commit_id.as_deref(), Some("abc123"));
    assert_eq!(commit.subject.as_deref(), Some("Add the thing"));
    assert_eq!(commit.body.as_deref(), Some("With a longer explanation."));
    assert_eq!(commit.actor.as_deref(), Some("Carol"));
    assert_eq!(commit.created_at, "2024-01-01T08:00:00Z");

    let comment = &got.items[1];
    assert_eq!(comment.id, "comment:101");
    assert_eq!(comment.actor.as_deref(), Some("alice"));
    assert_eq!(comment.body.as_deref(), Some("Comment 101"));

    let review = &got.items[2];
    assert_eq!(review.id, "review:201");
    assert_eq!(review.review_state.as_deref(), Some("changes_requested"));
    assert_eq!(review.body.as_deref(), Some("Please fix"));
    assert_eq!(review.actor.as_deref(), Some("bob"));
    assert_eq!(review.created_at, "2024-01-03T10:00:00Z");

    let line = &got.items[3];
    assert_eq!(line.id, "review_comment:301");
    assert_eq!(line.path.as_deref(), Some("src/a.rs"));
    assert_eq!(line.body.as_deref(), Some("Review comment 301"));
    assert_eq!(line.actor.as_deref(), Some("bob"));

    let names: Vec<_> = got.items[4..].iter().map(|i| i.event.as_deref().unwrap()).collect();
    assert_eq!(names, vec!["labeled", "review_requested", "head_ref_force_pushed", "merged"]);
    assert_eq!(got.items[4].id, "event:401");
    assert_eq!(got.items[7].actor.as_deref(), Some("dave"));
    assert!(got.items[4].body.is_none(), "an event with no text has no body");
}

// GTC-FR-CKTM
#[test]
fn every_review_state_of_the_contract_is_kept_and_others_are_dropped() {
    let f = repo_with_origin(PLATFORM);
    let states = ["approved", "changes_requested", "commented", "dismissed", "pending", "CHANGES_REQUESTED", "odd"];
    let events: Vec<Value> = states
        .iter()
        .enumerate()
        .map(|(i, state)| {
            json!({ "event": "reviewed", "id": 500 + i, "user": { "login": "bob" },
                    "submitted_at": format!("2024-01-0{}T10:00:00Z", i + 1), "state": state })
        })
        .collect();
    let github = github_with(events, vec![]);

    let got = timeline(&f, &github, own_buffer()).unwrap();

    let seen: Vec<_> = got.items.iter().map(|i| i.review_state.clone()).collect();
    assert_eq!(
        seen,
        vec![
            Some("approved".into()),
            Some("changes_requested".into()),
            Some("commented".into()),
            Some("dismissed".into()),
            Some("pending".into()),
            Some("changes_requested".into()),
            None,
        ]
    );
    assert!(got.items.iter().all(|i| i.body.is_none()), "an empty review has no body");
}

// GTC-FR-CKTM
#[test]
fn a_review_comment_appears_once_whether_the_timeline_or_the_comment_list_holds_it() {
    let f = repo_with_origin(PLATFORM);
    let github = github_with(
        vec![json!({ "event": "line-commented", "comments": [
                review_comment_json(301, "2024-01-03T10:00:05Z"),
                review_comment_json(302, "2024-01-03T10:00:06Z") ] })],
        vec![
            // 301 is in both; 303 only in the comment list.
            review_comment_json(301, "2024-01-03T10:00:05Z"),
            review_comment_json(303, "2024-01-03T10:00:07Z"),
        ],
    );

    let got = timeline(&f, &github, own_buffer()).unwrap();

    let ids: Vec<_> = got.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, vec!["review_comment:301", "review_comment:302", "review_comment:303"]);
    assert_eq!(
        github.paths(),
        vec![page_path(TIMELINE, 1), page_path(REVIEW_COMMENTS, 1)],
        "the comment list is read as well, to find the comments the timeline left out"
    );
}

// GTC-FR-CKTM
#[test]
fn items_are_oldest_first_by_instant_and_keep_github_order_for_ties() {
    let f = repo_with_origin(PLATFORM);
    // The commit's date has an offset: 09:00+02:00 is 07:00 UTC, earlier than
    // 08:00Z although its text sorts later.
    let github = github_with(
        vec![
            comment_event(1, "2024-01-01T08:00:00Z"),
            json!({ "event": "committed", "sha": "aaa", "message": "m",
                    "author": { "name": "C", "date": "2024-01-01T09:00:00+02:00" },
                    "committer": { "name": "C", "date": "2024-01-01T09:00:00+02:00" } }),
            comment_event(2, "2024-01-01T12:00:00Z"),
            comment_event(3, "2024-01-01T12:00:00Z"),
            comment_event(4, "2023-12-31T23:59:59Z"),
        ],
        vec![review_comment_json(77, "2024-01-01T10:30:00Z")],
    );

    let got = timeline(&f, &github, own_buffer()).unwrap();

    let ids: Vec<_> = got.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["comment:4", "commit:aaa", "comment:1", "review_comment:77", "comment:2", "comment:3"]
    );
}

// GTC-FR-CKTM, GTC-FR-DWVY
#[test]
fn the_timeline_serialises_to_the_camel_case_contract() {
    let f = repo_with_origin(PLATFORM);
    let github = github_with(
        vec![json!({ "event": "reviewed", "id": 9, "user": { "login": "bob" },
                     "submitted_at": "2024-01-03T10:00:00Z", "state": "approved", "body": "ok" })],
        vec![review_comment_json(5, "2024-01-04T10:00:00Z")],
    );

    let got = timeline(&f, &github, own_buffer()).unwrap();

    assert_eq!(
        serde_json::to_value(&got).unwrap(),
        json!({
            "items": [
                { "id": "review:9", "kind": "review", "actor": "bob",
                  "createdAt": "2024-01-03T10:00:00Z", "body": "ok", "reviewState": "approved" },
                { "id": "review_comment:5", "kind": "review_comment", "actor": "bob",
                  "createdAt": "2024-01-04T10:00:00Z", "body": "Review comment 5",
                  "path": "src/a.rs" },
            ],
            "truncated": false,
        })
    );
}

// GTC-FR-CKTM
#[test]
fn an_event_without_an_id_still_gets_a_stable_id() {
    let f = repo_with_origin(PLATFORM);
    let github = github_with(
        vec![json!({ "event": "closed", "actor": { "login": "dave" },
                     "created_at": "2024-02-01T00:00:00Z" })],
        vec![],
    );

    let first = timeline(&f, &github, own_buffer()).unwrap();
    let second = timeline(&f, &github, own_buffer()).unwrap();

    assert_eq!(first.items[0].id, second.items[0].id);
    assert!(!first.items[0].id.is_empty());
}

// GTC-FR-CKTM
#[test]
fn the_timeline_reads_every_page_up_to_a_thousand_items_and_flags_the_rest() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    let page = |first: u64| -> Vec<Value> {
        (first..first + PAGE_SIZE as u64)
            .map(|n| comment_event(n, "2024-01-01T00:00:00Z"))
            .collect()
    };
    // An eleventh page holds a row: GitHub has more than the limit.
    let mut pages: Vec<Vec<Value>> = (0..MAX_PAGES as u64).map(|i| page(i * 100 + 1)).collect();
    pages.push(vec![comment_event(5000, "2024-01-01T00:00:00Z")]);
    github.pages(TIMELINE, pages);
    github.pages(REVIEW_COMMENTS, vec![vec![]]);

    let got = timeline(&f, &github, own_buffer()).unwrap();

    assert_eq!(got.items.len(), 1000);
    assert!(got.truncated, "GitHub held more than was read");
    assert!(!got.items.iter().any(|i| i.id == "comment:5000"));
}

// GTC-FR-CKTM
#[test]
fn a_timeline_that_exactly_fills_the_limit_is_not_flagged() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    let page = |first: u64| -> Vec<Value> {
        (first..first + PAGE_SIZE as u64)
            .map(|n| comment_event(n, "2024-01-01T00:00:00Z"))
            .collect()
    };
    let mut pages: Vec<Vec<Value>> = (0..MAX_PAGES as u64).map(|i| page(i * 100 + 1)).collect();
    pages.push(vec![]);
    github.pages(TIMELINE, pages);
    github.pages(REVIEW_COMMENTS, vec![vec![]]);

    let got = timeline(&f, &github, own_buffer()).unwrap();

    assert_eq!(got.items.len(), 1000);
    assert!(!got.truncated);
}

// GTC-FR-CKTM
#[test]
fn a_review_comment_list_longer_than_the_limit_flags_the_timeline_truncated() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    github.pages(TIMELINE, vec![vec![]]);
    let page = |first: u64| -> Vec<Value> {
        (first..first + PAGE_SIZE as u64)
            .map(|n| review_comment_json(n, "2024-01-01T00:00:00Z"))
            .collect()
    };
    let mut pages: Vec<Vec<Value>> = (0..MAX_PAGES as u64).map(|i| page(i * 100 + 1)).collect();
    pages.push(vec![review_comment_json(9000, "2024-01-01T00:00:00Z")]);
    github.pages(REVIEW_COMMENTS, pages);

    let got = timeline(&f, &github, own_buffer()).unwrap();

    assert_eq!(got.items.len(), 1000);
    assert!(got.truncated);
}

// GTC-FR-CKTM, GTC-FR-DWVY
#[test]
fn a_failing_later_page_fails_the_timeline_instead_of_returning_a_part() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    let full: Vec<Value> = (1..=100).map(|n| comment_event(n, "2024-01-01T00:00:00Z")).collect();
    github.pages(TIMELINE, vec![full]);
    github.answer(&page_path(TIMELINE, 2), Err(GithubFailure::Unreachable));
    github.pages(REVIEW_COMMENTS, vec![vec![]]);

    assert_eq!(
        timeline(&f, &github, own_buffer()).unwrap_err(),
        github_tokens::ERR_GITHUB_UNREACHABLE
    );
}

// GTC-FR-CKTM, GTC-FR-DWVY
#[test]
fn a_failing_review_comment_read_fails_the_timeline() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    github.pages(TIMELINE, vec![vec![comment_event(1, "2024-01-01T00:00:00Z")]]);
    github.answer(&page_path(REVIEW_COMMENTS, 1), Err(GithubFailure::Unreachable));

    assert_eq!(
        timeline(&f, &github, own_buffer()).unwrap_err(),
        github_tokens::ERR_GITHUB_UNREACHABLE
    );
}

// GTC-FR-CKTM
#[test]
fn an_unknown_number_and_a_refused_token_have_their_own_typed_errors() {
    let f = repo_with_origin(PLATFORM);
    let missing = FakeGithub::new();
    missing.answer(&page_path(TIMELINE, 1), Err(GithubFailure::NotFound));
    let refused = FakeGithub::new();
    refused.answer(&page_path(TIMELINE, 1), Err(GithubFailure::Rejected));

    assert_eq!(timeline(&f, &missing, own_buffer()).unwrap_err(), ERR_PULL_REQUEST_NOT_FOUND);
    assert_eq!(timeline(&f, &refused, own_buffer()).unwrap_err(), ERR_GITHUB_TOKEN_REJECTED);
}

// GTC-FR-CKTM, GTC-FR-DWVY, GTC-FR-11
#[test]
fn the_token_is_presented_to_github_and_never_appears_in_an_item_or_a_record() {
    let f = repo_with_origin(PLATFORM);
    let buffer = own_buffer();
    let github = github_with(
        vec![
            // Fields the contract does not name must not be carried over.
            json!({ "event": "labeled", "id": 1, "actor": { "login": "alice" },
                    "created_at": "2024-01-01T00:00:00Z",
                    "url": "https://api.github.com/x?access_token=leaky",
                    "node_id": "LE_leaky" }),
            comment_event(2, "2024-01-02T00:00:00Z"),
        ],
        vec![],
    );

    let got = timeline(&f, &github, buffer).unwrap();

    assert!(github.requests().iter().all(|(secret, _)| secret == SECRET));
    let wire = serde_json::to_string(&got).unwrap();
    assert!(!wire.contains(SECRET) && !wire.contains("leaky"), "{wire}");
    let logged = buffer_text(buffer);
    assert!(!logged.contains(SECRET), "the log buffer holds the token");
    assert!(!logged.contains("Comment 2"), "the log buffer holds no comment text");
    let finished = records_for(buffer, "pull request read finished");
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].fields.get("itemCount"), Some(&json!(2)));
    assert_eq!(finished[0].fields.get("number"), Some(&json!(9)));
}
