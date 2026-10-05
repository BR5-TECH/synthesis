//! The pull request list and detail operations (GTC-FR-GXUB, GTC-FR-ZIHE,
//! GTC-FR-DWVY), and the fixtures the timeline tests share.
//!
//! No network is reached: GitHub is a scripted fake that records every request
//! it receives, so "no request is made" is an assertion about that record.

use super::*;
use crate::git::pull_requests::client::{
    classify, GithubFailure, GithubPullRequests, HttpGithubPullRequests, API, MAX_PAGES, PAGE_SIZE,
};
use crate::git::pull_requests::{
    ERR_GITHUB_TOKEN_REJECTED, ERR_INVALID_PULL_REQUEST_STATE, ERR_NOT_A_GITHUB_REMOTE,
    ERR_PULL_REQUEST_NOT_FOUND,
};
use crate::github_tokens::{
    GithubTokenRecord, GithubVerifier, SecretStore, SecretUnavailable, VerifiedIdentity,
    VerifyError,
};
use serde_json::{json, Value};
use std::collections::HashMap;

/// The secret the fixture project resolves to. No record, payload, or error may
/// ever contain it.
pub(super) const SECRET: &str = "ghp_TOP_SECRET_VALUE_9f3a";

pub(super) const PLATFORM: &str = "https://github.com/acme/platform.git";

/// A GitHub that answers from a script and remembers what it was asked.
#[derive(Default)]
pub(super) struct FakeGithub {
    answers: Mutex<HashMap<String, Result<Value, GithubFailure>>>,
    requests: Mutex<Vec<(String, String)>>,
}

impl FakeGithub {
    pub(super) fn new() -> FakeGithub {
        FakeGithub::default()
    }

    /// Answer `path` (query included) with `answer`.
    pub(super) fn answer(&self, path: &str, answer: Result<Value, GithubFailure>) {
        self.answers.lock().unwrap().insert(path.to_string(), answer);
    }

    /// Answer the pages of the list at `base`: page `n` is `pages[n - 1]`.
    pub(super) fn pages(&self, base: &str, pages: Vec<Vec<Value>>) {
        for (index, rows) in pages.into_iter().enumerate() {
            self.answer(&page_path(base, index + 1), Ok(Value::Array(rows)));
        }
    }

    /// Every `(secret, path)` this fake was asked for, in order.
    pub(super) fn requests(&self) -> Vec<(String, String)> {
        self.requests.lock().unwrap().clone()
    }

    pub(super) fn paths(&self) -> Vec<String> {
        self.requests().into_iter().map(|(_, path)| path).collect()
    }
}

impl GithubPullRequests for FakeGithub {
    fn get_json(&self, secret: &str, path: &str) -> Result<Value, GithubFailure> {
        self.requests
            .lock()
            .unwrap()
            .push((secret.to_string(), path.to_string()));
        self.answers
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .unwrap_or(Err(GithubFailure::Unreachable))
    }
}

/// The path of page `page` of the list at `base`.
pub(super) fn page_path(base: &str, page: usize) -> String {
    let separator = if base.contains('?') { '&' } else { '?' };
    format!("{base}{separator}per_page=100&page={page}")
}

struct FixedSecret;

impl SecretStore for FixedSecret {
    fn set(&self, _id: &str, _secret: &str) -> Result<(), SecretUnavailable> {
        Ok(())
    }
    fn get(&self, _id: &str) -> Result<Option<String>, SecretUnavailable> {
        Ok(Some(SECRET.to_string()))
    }
    fn delete(&self, _id: &str) -> Result<(), SecretUnavailable> {
        Ok(())
    }
}

struct NoVerifier;

impl GithubVerifier for NoVerifier {
    fn verify(&self, _secret: &str) -> Result<VerifiedIdentity, VerifyError> {
        Err(VerifyError::Unreachable("not used".into()))
    }
}

fn record(id: &str) -> GithubTokenRecord {
    GithubTokenRecord { id: id.into(), label: id.into(), ..Default::default() }
}

/// A project that resolves one stored token to `SECRET`.
pub(super) fn token_ready() -> (GlobalSettingsStore, GithubTokens) {
    let store = GlobalSettingsStore::in_memory();
    store.save_github_token_registry(vec![record("a")]).unwrap();
    (store, GithubTokens::new(Box::new(FixedSecret), Box::new(NoVerifier)))
}

/// A project that stores no token at all.
fn token_missing() -> (GlobalSettingsStore, GithubTokens) {
    (GlobalSettingsStore::in_memory(), GithubTokens::new(Box::new(FixedSecret), Box::new(NoVerifier)))
}

/// A project that stores two tokens and has chosen neither.
fn token_selection_required() -> (GlobalSettingsStore, GithubTokens) {
    let store = GlobalSettingsStore::in_memory();
    store.save_github_token_registry(vec![record("a"), record("b")]).unwrap();
    (store, GithubTokens::new(Box::new(FixedSecret), Box::new(NoVerifier)))
}

/// A repository whose `origin` is `url`.
pub(super) fn repo_with_origin(url: &str) -> WorktreeFixture {
    let f = WorktreeFixture::new();
    f.repo().remote("origin", url).unwrap();
    f
}

/// One pull request as GitHub lists it.
pub(super) fn pr_json(number: u64, state: &str, merged_at: Option<&str>, updated: &str) -> Value {
    json!({
        "number": number,
        "title": format!("Change {number}"),
        "state": state,
        "draft": false,
        "user": { "login": "alice" },
        "head": { "ref": "feature" },
        "base": { "ref": "main" },
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": updated,
        "merged_at": merged_at,
        "body": "private discussion body",
        "html_url": format!("https://github.com/acme/platform/pull/{number}"),
        "closed_at": null,
    })
}

pub(super) fn list_path(state: &str) -> String {
    format!("/repos/acme/platform/pulls?state={state}&sort=updated&direction=desc")
}

fn list(
    f: &WorktreeFixture,
    github: &FakeGithub,
    tokens: &(GlobalSettingsStore, GithubTokens),
    buffer: &'static LogBuffer,
    state: &str,
) -> Result<Vec<PullRequestSummary>, String> {
    list_pull_requests_reported(
        &NullSink, buffer, &f.root(), &tokens.0, &tokens.1, "/dev/acme", github, state,
    )
}

fn detail(
    f: &WorktreeFixture,
    github: &FakeGithub,
    tokens: &(GlobalSettingsStore, GithubTokens),
    buffer: &'static LogBuffer,
    id: u64,
) -> Result<PullRequestDetail, String> {
    get_pull_request_detail_reported(
        &NullSink, buffer, &f.root(), &tokens.0, &tokens.1, "/dev/acme", github, id,
    )
}

// -- GTC-FR-GXUB: the list -----------------------------------------------

// GTC-FR-GXUB, GTC-FR-DWVY
#[test]
fn listing_open_pull_requests_asks_for_the_open_state_newest_update_first() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    github.pages(
        &list_path("open"),
        vec![vec![
            pr_json(1, "open", None, "2024-03-01T00:00:00Z"),
            pr_json(2, "open", None, "2024-05-01T00:00:00Z"),
        ]],
    );

    let rows = list(&f, &github, &token_ready(), own_buffer(), "open").unwrap();

    assert_eq!(
        github.requests(),
        vec![(SECRET.to_string(), page_path(&list_path("open"), 1))],
        "one request, for the open state, sorted by update, carrying the project's token"
    );
    assert_eq!(
        rows.iter().map(|r| r.number).collect::<Vec<_>>(),
        vec![2, 1],
        "the most recently updated pull request is first"
    );
    assert_eq!(rows[0].state, "open");
    assert_eq!(rows[0].author, "alice");
    assert_eq!(rows[0].head_branch, "feature");
    assert_eq!(rows[0].base_branch, "main");
}

// GTC-FR-GXUB
#[test]
fn a_list_row_serialises_to_the_camel_case_contract() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    github.pages(&list_path("open"), vec![vec![pr_json(7, "open", None, "2024-03-01T00:00:00Z")]]);

    let rows = list(&f, &github, &token_ready(), own_buffer(), "open").unwrap();

    let wire = serde_json::to_value(&rows).unwrap();
    assert_eq!(
        wire,
        json!([{
            "number": 7,
            "title": "Change 7",
            "state": "open",
            "isDraft": false,
            "author": "alice",
            "headBranch": "feature",
            "baseBranch": "main",
            "createdAt": "2024-01-01T00:00:00Z",
            "updatedAt": "2024-03-01T00:00:00Z",
        }])
    );
    assert!(!wire.to_string().contains("private discussion body"), "a list row carries no body");
}

// GTC-FR-GXUB
#[test]
fn listing_closed_pull_requests_flags_the_merged_ones() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    github.pages(
        &list_path("closed"),
        vec![vec![
            pr_json(5, "closed", Some("2024-04-01T00:00:00Z"), "2024-04-02T00:00:00Z"),
            pr_json(4, "closed", None, "2024-04-01T00:00:00Z"),
        ]],
    );

    let rows = list(&f, &github, &token_ready(), own_buffer(), "closed").unwrap();

    assert_eq!(
        rows.iter().map(|r| (r.number, r.state.as_str())).collect::<Vec<_>>(),
        vec![(5, "merged"), (4, "closed")],
        "a closed pull request with a merge time is merged; one without is closed"
    );
}

// GTC-FR-GXUB
#[test]
fn any_other_state_is_a_typed_error_and_makes_no_request() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    for state in ["merged", "all", "", "OPEN", "open&per_page=1"] {
        assert_eq!(
            list(&f, &github, &token_ready(), own_buffer(), state).unwrap_err(),
            ERR_INVALID_PULL_REQUEST_STATE,
            "state {state:?}"
        );
    }
    assert!(github.requests().is_empty(), "no request for a state GitHub would not take");
}

// GTC-FR-GXUB
#[test]
fn listing_reads_pages_until_a_short_page() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    let rows = |from: u64, count: u64| -> Vec<Value> {
        (from..from + count).map(|n| pr_json(n, "open", None, "2024-03-01T00:00:00Z")).collect()
    };
    github.pages(&list_path("open"), vec![rows(1, 100), rows(101, 50)]);

    let listed = list(&f, &github, &token_ready(), own_buffer(), "open").unwrap();

    assert_eq!(listed.len(), 150);
    assert_eq!(github.requests().len(), 2, "a short second page ends the read");
}

// GTC-FR-GXUB
#[test]
fn listing_stops_at_one_thousand_pull_requests() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    let page = |from: u64| -> Vec<Value> {
        (from..from + PAGE_SIZE as u64)
            .map(|n| pr_json(n, "open", None, "2024-03-01T00:00:00Z"))
            .collect()
    };
    // GitHub holds more than the limit: an eleventh full page exists.
    let mut pages: Vec<Vec<Value>> = (0..=MAX_PAGES as u64).map(|i| page(i * 100 + 1)).collect();
    pages.push(page(5000));
    github.pages(&list_path("open"), pages);

    let listed = list(&f, &github, &token_ready(), own_buffer(), "open").unwrap();

    assert_eq!(listed.len(), 1000, "no more than 1000 pull requests are returned");
    assert_eq!(MAX_PAGES * PAGE_SIZE, 1000);
    assert_eq!(
        github.requests().len(),
        MAX_PAGES + 1,
        "ten pages, and one probe page that tells whether GitHub held more"
    );
}

// GTC-FR-GXUB, GTC-FR-DWVY
#[test]
fn a_failing_later_page_fails_the_listing_instead_of_returning_a_part() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    let full: Vec<Value> =
        (1..=100).map(|n| pr_json(n, "open", None, "2024-03-01T00:00:00Z")).collect();
    github.pages(&list_path("open"), vec![full]);
    github.answer(&page_path(&list_path("open"), 2), Err(GithubFailure::Unreachable));

    let result = list(&f, &github, &token_ready(), own_buffer(), "open");

    assert_eq!(result.unwrap_err(), crate::github_tokens::ERR_GITHUB_UNREACHABLE);
}

// GTC-FR-GXUB
#[test]
fn a_remote_that_is_not_on_github_com_is_refused_without_a_request() {
    for url in [
        "https://gitlab.com/acme/platform.git",
        "https://github.com.evil.example/acme/platform.git",
        "file:///srv/repos/platform.git",
        "https://github.com/ac me/platform.git",
    ] {
        let f = repo_with_origin(url);
        let github = FakeGithub::new();
        assert_eq!(
            list(&f, &github, &token_ready(), own_buffer(), "open").unwrap_err(),
            ERR_NOT_A_GITHUB_REMOTE,
            "{url}"
        );
        assert!(github.requests().is_empty(), "{url}");
    }
}

// GTC-FR-GXUB, GTC-FR-05
#[test]
fn a_project_with_no_remote_gets_the_no_remote_error_without_a_request() {
    let f = WorktreeFixture::new();
    let github = FakeGithub::new();
    let buffer = own_buffer();

    assert_eq!(
        list(&f, &github, &token_ready(), buffer, "open").unwrap_err(),
        ERR_NO_REMOTE_CONFIGURED
    );
    assert_eq!(
        detail(&f, &github, &token_ready(), buffer, 1).unwrap_err(),
        ERR_NO_REMOTE_CONFIGURED
    );
    assert!(github.requests().is_empty());
}

// GTC-FR-GXUB, GTC-FR-05
#[test]
fn an_ssh_remote_on_github_com_names_the_same_repository() {
    let f = repo_with_origin("git@github.com:acme/platform.git");
    let github = FakeGithub::new();
    github.pages(&list_path("open"), vec![vec![]]);

    let listed = list(&f, &github, &token_ready(), own_buffer(), "open").unwrap();

    assert!(listed.is_empty());
    assert_eq!(github.paths(), vec![page_path(&list_path("open"), 1)]);
}

// GTC-FR-GXUB, GTC-FR-ZIHE, GTC-FR-CKTM, GTC-FR-09, GTC-FR-10
#[test]
fn a_project_with_no_resolvable_token_makes_no_request() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    let cases: [((GlobalSettingsStore, GithubTokens), &str); 2] = [
        (token_missing(), github_tokens::ERR_TOKEN_MISSING),
        (token_selection_required(), github_tokens::ERR_SELECTION_REQUIRED),
    ];
    for (tokens, expected) in &cases {
        let buffer = own_buffer();
        assert_eq!(list(&f, &github, tokens, buffer, "open").unwrap_err(), *expected);
        assert_eq!(detail(&f, &github, tokens, buffer, 3).unwrap_err(), *expected);
        assert_eq!(
            list_pull_request_timeline_reported(
                &NullSink, buffer, &f.root(), &tokens.0, &tokens.1, "/dev/acme", &github, 3,
            )
            .unwrap_err(),
            *expected
        );
    }
    assert!(github.requests().is_empty(), "the credential is resolved before any request");
}

// GTC-FR-GXUB, GTC-FR-DWVY
#[test]
fn a_repository_the_token_cannot_see_is_a_rejected_token_for_a_list() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    github.answer(&page_path(&list_path("open"), 1), Err(GithubFailure::NotFound));

    assert_eq!(
        list(&f, &github, &token_ready(), own_buffer(), "open").unwrap_err(),
        ERR_GITHUB_TOKEN_REJECTED
    );
}

// -- GTC-FR-ZIHE: the detail ----------------------------------------------

// GTC-FR-ZIHE
#[test]
fn the_detail_carries_the_header_fields_and_the_description_as_written() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    let mut body = pr_json(42, "closed", Some("2024-04-01T10:00:00Z"), "2024-04-02T00:00:00Z");
    body["body"] = json!("## Heading\n\nAs **written**.\n");
    body["closed_at"] = json!("2024-04-01T10:00:00Z");
    body["draft"] = json!(true);
    github.answer("/repos/acme/platform/pulls/42", Ok(body));

    let got = detail(&f, &github, &token_ready(), own_buffer(), 42).unwrap();

    assert_eq!(github.paths(), vec!["/repos/acme/platform/pulls/42".to_string()]);
    assert_eq!(got.body, "## Heading\n\nAs **written**.\n");
    assert_eq!(got.state, "merged");
    assert!(got.is_draft);
    assert_eq!(got.url, "https://github.com/acme/platform/pull/42");
    assert_eq!(got.closed_at.as_deref(), Some("2024-04-01T10:00:00Z"));
    assert_eq!(got.merged_at.as_deref(), Some("2024-04-01T10:00:00Z"));
    let wire = serde_json::to_value(&got).unwrap();
    for key in ["isDraft", "headBranch", "baseBranch", "createdAt", "updatedAt", "closedAt", "mergedAt"] {
        assert!(wire.get(key).is_some(), "the contract field {key} is present");
    }
}

// GTC-FR-ZIHE
#[test]
fn an_empty_description_is_returned_as_empty_text_and_open_has_no_close_time() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    let mut body = pr_json(8, "open", None, "2024-04-02T00:00:00Z");
    body["body"] = Value::Null;
    github.answer("/repos/acme/platform/pulls/8", Ok(body));

    let got = detail(&f, &github, &token_ready(), own_buffer(), 8).unwrap();

    assert_eq!(got.body, "");
    assert_eq!(got.state, "open");
    let wire = serde_json::to_value(&got).unwrap();
    assert!(wire.get("closedAt").is_none() && wire.get("mergedAt").is_none());
}

// GTC-FR-ZIHE
#[test]
fn the_detail_of_an_unknown_number_is_pull_request_not_found() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    github.answer("/repos/acme/platform/pulls/999", Err(GithubFailure::NotFound));

    assert_eq!(
        detail(&f, &github, &token_ready(), own_buffer(), 999).unwrap_err(),
        ERR_PULL_REQUEST_NOT_FOUND
    );
}

// GTC-FR-ZIHE
#[test]
fn a_token_github_refuses_is_github_token_rejected() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    github.answer("/repos/acme/platform/pulls/1", Err(GithubFailure::Rejected));

    assert_eq!(
        detail(&f, &github, &token_ready(), own_buffer(), 1).unwrap_err(),
        ERR_GITHUB_TOKEN_REJECTED
    );
}

// GTC-FR-ZIHE
#[test]
fn an_unreachable_github_is_github_unreachable() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    github.answer("/repos/acme/platform/pulls/1", Err(GithubFailure::Unreachable));
    // A body that is not a pull request is no better an answer.
    github.answer("/repos/acme/platform/pulls/2", Ok(json!({ "message": "odd" })));

    assert_eq!(
        detail(&f, &github, &token_ready(), own_buffer(), 1).unwrap_err(),
        github_tokens::ERR_GITHUB_UNREACHABLE
    );
    assert_eq!(
        detail(&f, &github, &token_ready(), own_buffer(), 2).unwrap_err(),
        github_tokens::ERR_GITHUB_UNREACHABLE
    );
}

// -- GTC-FR-DWVY: the transport and the secret ------------------------------

// GTC-FR-DWVY
#[test]
fn the_production_client_reads_only_api_github_com_and_maps_statuses_to_fixed_failures() {
    assert_eq!(API, "https://api.github.com", "the one host a token is sent to");
    // A path that could name another host is refused before any request.
    assert_eq!(
        HttpGithubPullRequests.get_json(SECRET, "@evil.example/x").unwrap_err(),
        GithubFailure::Unreachable
    );
    assert_eq!(
        classify(&ureq::Error::StatusCode(401)),
        GithubFailure::Rejected
    );
    assert_eq!(
        classify(&ureq::Error::StatusCode(403)),
        GithubFailure::Rejected
    );
    assert_eq!(
        classify(&ureq::Error::StatusCode(404)),
        GithubFailure::NotFound
    );
    assert_eq!(
        classify(&ureq::Error::StatusCode(500)),
        GithubFailure::Unreachable
    );
}

// GTC-FR-DWVY, GTC-FR-11, GTC-FR-09
#[test]
fn no_secret_reaches_a_returned_error_a_payload_or_the_log_buffer() {
    let f = repo_with_origin(PLATFORM);
    let buffer = own_buffer();
    let github = FakeGithub::new();
    github.pages(&list_path("open"), vec![vec![pr_json(1, "open", None, "2024-03-01T00:00:00Z")]]);
    github.answer("/repos/acme/platform/pulls/1", Err(GithubFailure::Rejected));
    github.answer("/repos/acme/platform/pulls/2", Err(GithubFailure::Unreachable));
    github.answer("/repos/acme/platform/pulls/3", Err(GithubFailure::NotFound));

    let mut returned = vec![
        serde_json::to_string(&list(&f, &github, &token_ready(), buffer, "open").unwrap()).unwrap(),
    ];
    for id in 1..=3 {
        returned.push(detail(&f, &github, &token_ready(), buffer, id).unwrap_err());
    }
    returned.push(list(&f, &github, &token_ready(), buffer, "bogus").unwrap_err());

    for text in returned {
        assert!(!text.contains(SECRET), "a return value holds the token: {text}");
    }
    let logged = buffer_text(buffer);
    assert!(!logged.contains(SECRET), "the log buffer holds the token");
    assert!(
        !logged.contains("private discussion body") && !logged.contains("Bearer"),
        "the log buffer holds neither pull request text nor an authorization value"
    );
    let finished = records_for(buffer, "pull request read finished");
    assert_eq!(finished.len(), 1, "the one list that succeeded is reported once");
    assert_eq!(finished[0].fields.get("count"), Some(&json!(1)));
    assert_eq!(finished[0].fields.get("owner"), Some(&json!("acme")));
    let failed = records_for(buffer, "pull request read failed");
    assert_eq!(failed.len(), 4, "every error path is reported, handled ones included");
}
