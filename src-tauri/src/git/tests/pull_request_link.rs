//! Linking a pull request to the issues it resolves (GTC-FR-ZHVO,
//! GTC-FR-QJFC, GTC-FR-WJUG, GTC-FR-GNKQ).
//!
//! GitHub is a scripted fake that records every read and every update, so "no
//! request is made" and "the description stays as it was" are assertions about
//! that record.

use super::pull_requests::{
    repo_with_origin, token_missing, token_ready, FakeGithub, PLATFORM, SECRET,
};
use super::*;
use crate::git::pull_requests::client::{GithubFailure, GithubWriteFailure};
use crate::git::pull_requests::link::{
    already_resolves, body_with_block, link_pull_request_issues_reported as link_reported,
    ERR_INVALID_ISSUE_REFERENCE, LINK_WAY_CLOSING_KEYWORD,
};
use crate::git::pull_requests::{
    PullRequestIssueRef, PullRequestLinkOutcome, ERR_GITHUB_TOKEN_REJECTED,
    ERR_PULL_REQUEST_NOT_FOUND,
};
use serde_json::{json, Value};

const PULL: &str = "/repos/acme/platform/pulls/42";
const REPO: &str = "/repos/acme/platform";

fn issue(owner: &str, repository: &str, number: u64) -> PullRequestIssueRef {
    PullRequestIssueRef { owner: owner.into(), repository: repository.into(), number }
}

/// A fake whose pull request 42 has `body` and goes into `base`, in a
/// repository whose default branch is `main`.
fn github_with(body: Option<&str>, base: &str) -> FakeGithub {
    let github = FakeGithub::new();
    github.answer(PULL, Ok(json!({ "number": 42, "body": body, "base": { "ref": base } })));
    github.answer(REPO, Ok(json!({ "default_branch": "main" })));
    github.answer_patch(Ok(json!({ "number": 42 })));
    github
}

fn link(
    github: &FakeGithub,
    tokens: &(GlobalSettingsStore, GithubTokens),
    buffer: &'static LogBuffer,
    issues: &[PullRequestIssueRef],
) -> Result<PullRequestLinkOutcome, String> {
    let f = repo_with_origin(PLATFORM);
    link_reported(
        &NullSink,
        buffer,
        &PullRequestAccess {
            project: GithubAccess {
                root: &f.root(),
                store: &tokens.0,
                tokens: &tokens.1,
                project_key: "/dev/acme",
            },
            client: github,
        },
        42,
        issues,
    )
}

fn patched_body(github: &FakeGithub) -> String {
    let patches = github.patches();
    assert_eq!(patches.len(), 1, "one update request");
    patches[0].2["body"].as_str().unwrap().to_string()
}

fn numbers(outcome: &PullRequestLinkOutcome) -> (Vec<u64>, Vec<u64>) {
    (
        outcome.linked.iter().map(|l| l.issue.number).collect(),
        outcome.failed.iter().map(|f| f.issue.number).collect(),
    )
}

// GTC-FR-ZHVO, GTC-FR-QJFC, GTC-FR-WJUG
#[test]
fn each_issue_gets_one_resolves_line_in_one_block_after_a_blank_line_and_one_update() {
    let github = github_with(Some("## Summary\nThe change.\n\n"), "main");
    let outcome = link(
        &github,
        &token_ready(),
        own_buffer(),
        &[
            issue("acme", "platform", 31),
            issue("other", "tracker", 12),
            issue("acme", "platform", 32),
        ],
    )
    .unwrap();

    assert_eq!(
        patched_body(&github),
        "## Summary\nThe change.\n\n* Resolves #31\n* Resolves other/tracker#12\n* Resolves #32"
    );
    let patches = github.patches();
    assert_eq!(patches[0].0, SECRET);
    assert_eq!(patches[0].1, PULL);
    // GTC-FR-GNKQ: the update carries the description alone.
    assert_eq!(patches[0].2.as_object().unwrap().keys().collect::<Vec<_>>(), vec!["body"]);
    assert_eq!(numbers(&outcome), (vec![31, 12, 32], vec![]));
    assert!(outcome.linked.iter().all(|l| l.way == LINK_WAY_CLOSING_KEYWORD));
}

// GTC-FR-QJFC
#[test]
fn an_issue_named_twice_occurs_once_and_the_order_of_the_request_is_kept() {
    let github = github_with(Some(""), "main");
    let outcome = link(
        &github,
        &token_ready(),
        own_buffer(),
        &[issue("acme", "platform", 7), issue("ACME", "Platform", 7), issue("acme", "platform", 3)],
    )
    .unwrap();
    // An empty description holds the block alone.
    assert_eq!(patched_body(&github), "* Resolves #7\n* Resolves #3");
    assert_eq!(numbers(&outcome), (vec![7, 3], vec![]));
}

// GTC-FR-ZHVO
#[test]
fn a_null_description_is_read_as_empty() {
    let github = github_with(None, "main");
    link(&github, &token_ready(), own_buffer(), &[issue("acme", "platform", 5)]).unwrap();
    assert_eq!(patched_body(&github), "* Resolves #5");
}

// GTC-FR-ZHVO
#[test]
fn an_issue_the_description_already_resolves_gets_no_second_line() {
    let github = github_with(Some("Fixes #31.\n\nCloses: other/tracker#12"), "main");
    let outcome = link(
        &github,
        &token_ready(),
        own_buffer(),
        &[
            issue("acme", "platform", 31),
            issue("other", "tracker", 12),
            issue("acme", "platform", 3),
        ],
    )
    .unwrap();
    assert_eq!(patched_body(&github), "Fixes #31.\n\nCloses: other/tracker#12\n\n* Resolves #3");
    // Every issue is linked by its closing keyword, the old ones too.
    assert_eq!(numbers(&outcome), (vec![31, 12, 3], vec![]));
}

// GTC-FR-ZHVO
#[test]
fn a_description_that_resolves_every_issue_is_not_written() {
    let github = github_with(Some("Resolves #8"), "main");
    let outcome =
        link(&github, &token_ready(), own_buffer(), &[issue("acme", "platform", 8)]).unwrap();
    assert!(github.patches().is_empty());
    assert_eq!(numbers(&outcome), (vec![8], vec![]));
}

// GTC-FR-ZHVO, GTC-FR-QJFC
#[test]
fn the_resolves_test_knows_the_forms_github_reads_and_no_other() {
    let scope = ("github.com", "acme", "platform");
    let own = issue("acme", "platform", 12);
    let other = issue("other", "tracker", 12);
    assert!(already_resolves("closes #12", &own, scope));
    assert!(already_resolves("RESOLVED acme/platform#12", &own, scope));
    assert!(already_resolves("fixed https://github.com/acme/platform/issues/12", &own, scope));
    assert!(!already_resolves("closes #123", &own, scope), "another number");
    assert!(!already_resolves("see #12", &own, scope), "no closing keyword");
    assert!(!already_resolves("prefixes #12", &own, scope), "a keyword inside a word");
    assert!(!already_resolves("closes #12", &other, scope), "#N is the own repository alone");
    assert!(already_resolves("closes other/tracker#12", &other, scope));
}

// GTC-FR-ZHVO
#[test]
fn the_block_follows_one_blank_line_whatever_the_description_ends_with() {
    let lines = vec!["* Resolves #1".to_string()];
    assert_eq!(body_with_block("text", &lines), "text\n\n* Resolves #1");
    assert_eq!(body_with_block("text\n\n\n", &lines), "text\n\n* Resolves #1");
    assert_eq!(body_with_block("  \n", &lines), "* Resolves #1");
}

// GTC-FR-WJUG
#[test]
fn a_failed_update_puts_every_issue_in_failed_with_the_typed_cause() {
    let github = github_with(Some("body"), "main");
    github.answer_patch(Err(GithubWriteFailure::Rejected));
    let outcome = link(
        &github,
        &token_ready(),
        own_buffer(),
        &[issue("acme", "platform", 1), issue("acme", "platform", 2)],
    )
    .unwrap();
    assert!(outcome.linked.is_empty());
    assert_eq!(numbers(&outcome), (vec![], vec![1, 2]));
    // One update request, and no second write that could change the description.
    assert_eq!(github.patches().len(), 1);
    assert!(outcome.failed.iter().all(|f| f.error == ERR_GITHUB_TOKEN_REJECTED));
}

// GTC-FR-WJUG
#[test]
fn an_empty_list_makes_no_request_and_returns_empty_lists() {
    let github = github_with(Some("body"), "main");
    let outcome = link(&github, &token_ready(), own_buffer(), &[]).unwrap();
    assert!(github.requests().is_empty());
    assert!(github.patches().is_empty());
    assert_eq!(outcome, PullRequestLinkOutcome::default());
}

// GTC-FR-WJUG
#[test]
fn a_reference_that_is_not_a_github_name_is_failed_and_never_written() {
    let github = github_with(Some(""), "main");
    let outcome = link(
        &github,
        &token_ready(),
        own_buffer(),
        &[issue("bad owner\n", "platform", 4), issue("acme", "platform", 5)],
    )
    .unwrap();
    assert_eq!(patched_body(&github), "* Resolves #5");
    assert_eq!(numbers(&outcome), (vec![5], vec![4]));
    assert_eq!(outcome.failed[0].error, ERR_INVALID_ISSUE_REFERENCE);
}

// GTC-FR-ZHVO
#[test]
fn a_pull_request_that_cannot_be_read_is_the_typed_error_and_nothing_is_written() {
    let github = FakeGithub::new();
    github.answer(PULL, Err(GithubFailure::NotFound));
    let error =
        link(&github, &token_ready(), own_buffer(), &[issue("acme", "platform", 1)]).unwrap_err();
    assert_eq!(error, ERR_PULL_REQUEST_NOT_FOUND);
    assert!(github.patches().is_empty());
}

// GTC-FR-ZHVO
#[test]
fn a_project_without_a_token_makes_no_request() {
    let github = github_with(Some(""), "main");
    let error =
        link(&github, &token_missing(), own_buffer(), &[issue("acme", "platform", 1)]).unwrap_err();
    assert_eq!(error, github_tokens::ERR_TOKEN_MISSING);
    assert!(github.requests().is_empty());
    assert!(github.patches().is_empty());
}

// GTC-FR-GNKQ
#[test]
fn closes_on_merge_is_true_only_for_the_default_branch_github_reports() {
    let into_main = github_with(Some(""), "main");
    let outcome =
        link(&into_main, &token_ready(), own_buffer(), &[issue("acme", "platform", 1)]).unwrap();
    assert!(outcome.closes_on_merge);

    let stacked = github_with(Some(""), "feature/pr-tools");
    let outcome =
        link(&stacked, &token_ready(), own_buffer(), &[issue("acme", "platform", 1)]).unwrap();
    assert!(!outcome.closes_on_merge);

    // A repository read that fails makes it false, and the link still runs.
    let unread = FakeGithub::new();
    unread.answer(PULL, Ok(json!({ "body": "", "base": { "ref": "main" } })));
    unread.answer(REPO, Err(GithubFailure::Unreachable));
    unread.answer_patch(Ok(json!({})));
    let outcome =
        link(&unread, &token_ready(), own_buffer(), &[issue("acme", "platform", 1)]).unwrap();
    assert!(!outcome.closes_on_merge);
    assert_eq!(numbers(&outcome), (vec![1], vec![]));
}

// GTC-FR-GNKQ
#[test]
fn the_operation_reads_and_writes_the_pull_request_and_its_repository_alone() {
    let github = github_with(Some(""), "main");
    link(&github, &token_ready(), own_buffer(), &[issue("acme", "platform", 1)]).unwrap();
    assert_eq!(github.paths(), vec![PULL.to_string(), REPO.to_string()]);
    assert!(github.posts().is_empty(), "no issue is changed or closed");
    assert_eq!(github.patches()[0].1, PULL);
}

// GTC-FR-GNKQ
#[test]
fn the_log_holds_the_number_and_the_count_and_no_description_text() {
    let github = github_with(Some("private description text"), "main");
    let buffer = own_buffer();
    link(
        &github,
        &token_ready(),
        buffer,
        &[issue("acme", "platform", 1), issue("acme", "platform", 2)],
    )
    .unwrap();
    let text = buffer_text(buffer);
    assert!(!text.contains(SECRET));
    assert!(!text.contains("private description text"));
    assert!(!text.contains("Resolves"));
    let done = record_for(buffer, "pull request read finished");
    let fields: Value = serde_json::to_value(&done.fields).unwrap();
    assert_eq!(fields["operation"], "link_pull_request_issues");
    assert_eq!(fields["number"], 42);
    assert_eq!(fields["issueCount"], 2);
    assert_eq!(fields["linkedCount"], 2);
}

// GTC-FR-ZHVO
#[test]
fn the_wire_shape_is_the_contract_shape() {
    let github = github_with(Some(""), "feature");
    let outcome =
        link(&github, &token_ready(), own_buffer(), &[issue("acme", "platform", 1)]).unwrap();
    assert_eq!(
        serde_json::to_value(&outcome).unwrap(),
        json!({
            "linked": [{ "issue": { "owner": "acme", "repository": "platform", "number": 1 }, "way": "closing_keyword" }],
            "failed": [],
            "closesOnMerge": false,
        })
    );
    let parsed: PullRequestIssueRef =
        serde_json::from_value(json!({ "owner": "o", "repository": "r", "number": 3 })).unwrap();
    assert_eq!(parsed, issue("o", "r", 3));
}

// GTC-FR-WJUG, GTC-FR-GNKQ
#[test]
fn a_failed_update_is_logged_by_its_code_alone() {
    let github = github_with(Some("private description text"), "main");
    github.answer_patch(Err(GithubWriteFailure::Invalid(
        "body quotes private description text".into(),
    )));
    let buffer = own_buffer();
    let outcome = link(&github, &token_ready(), buffer, &[issue("acme", "platform", 1)]).unwrap();
    assert!(outcome.failed[0].error.starts_with("pull_request_rejected"));
    let record = record_for(buffer, "pull request description update failed");
    let fields: Value = serde_json::to_value(&record.fields).unwrap();
    assert_eq!(fields["error"], "pull_request_rejected");
    assert_eq!(fields["number"], 42);
    assert!(!buffer_text(buffer).contains("private description text"));
}
