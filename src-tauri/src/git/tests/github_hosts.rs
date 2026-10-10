//! GitHub Enterprise hosts in the pull request operations and the credential
//! chain (GTC-FR-09, GTC-FR-FSLC, GTC-FR-XUAC, GTC-FR-GXUB, GTC-FR-MMFM).
//!
//! GitHub is a scripted fake that records every request, so "no request is
//! made" is an assertion about that record.

use super::pull_requests::{
    pr_json, repo_with_origin, token_on, token_ready, FakeGithub, SECRET,
};
use super::*;
use crate::git::pull_requests::client::GithubWriteFailure;
use crate::git::pull_requests::create::create_pull_request_reported as create_reported;
use crate::git::pull_requests::ERR_NOT_A_GITHUB_REMOTE;
use serde_json::json;

const GHE_REMOTE: &str = "https://company.ghe.com/acme/platform.git";
const OPEN_PATH: &str = "/repos/acme/platform/pulls?state=open&sort=updated&direction=desc";

fn list_open(
    f: &WorktreeFixture,
    github: &FakeGithub,
    tokens: &(GlobalSettingsStore, GithubTokens),
) -> Result<Vec<PullRequestSummary>, String> {
    list_pull_requests_reported(
        &NullSink, own_buffer(), &f.root(), &tokens.0, &tokens.1, "/dev/acme", github, "open",
    )
}

fn open_answer(github: &FakeGithub) {
    github.pages(
        OPEN_PATH,
        vec![vec![pr_json(3, "open", None, "2024-03-01T00:00:00Z")]],
    );
}

// GTC-FR-GXUB, GTC-FR-09
#[test]
fn a_pull_request_list_is_read_for_a_remote_on_the_host_of_the_token() {
    let f = repo_with_origin(GHE_REMOTE);
    let github = FakeGithub::new();
    open_answer(&github);

    let rows = list_open(&f, &github, &token_on("company.ghe.com")).unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(github.requests().len(), 1);
    assert_eq!(github.requests()[0].0, SECRET);
}

// GTC-FR-FSLC, GTS-FR-OBAS
#[test]
fn a_token_of_another_host_is_refused_with_no_request() {
    let f = repo_with_origin(GHE_REMOTE);
    let github = FakeGithub::new();
    open_answer(&github);

    let error = list_open(&f, &github, &token_ready()).unwrap_err();

    assert_eq!(error, "github_host_mismatch");
    assert!(github.requests().is_empty(), "no request is made on a host mismatch");
}

// GTC-FR-FSLC, GTC-FR-GXUB
#[test]
fn a_github_com_remote_with_an_enterprise_token_is_a_host_mismatch() {
    let f = repo_with_origin("https://github.com/acme/platform.git");
    let github = FakeGithub::new();

    let error = list_open(&f, &github, &token_on("company.ghe.com")).unwrap_err();

    assert_eq!(error, "github_host_mismatch");
    assert!(github.requests().is_empty());
}

// GTC-FR-GXUB, GTC-FR-FSLC
#[test]
fn a_remote_on_another_provider_is_not_a_github_remote() {
    let f = repo_with_origin("https://gitlab.com/acme/platform.git");
    let github = FakeGithub::new();

    let error = list_open(&f, &github, &token_on("company.ghe.com")).unwrap_err();

    assert_eq!(error, ERR_NOT_A_GITHUB_REMOTE);
    assert!(github.requests().is_empty());
}

// GTC-FR-FSLC
#[test]
fn a_host_that_only_a_stored_token_names_is_read_as_github() {
    let github = FakeGithub::new();
    open_answer(&github);
    let custom = repo_with_origin("https://git.corp.example/acme/platform.git");

    assert!(list_open(&custom, &github, &token_on("git.corp.example")).is_ok());
    assert_eq!(
        list_open(&custom, &github, &token_ready()).unwrap_err(),
        ERR_NOT_A_GITHUB_REMOTE,
        "no stored token names the host, so it is not a GitHub remote"
    );
}

fn create(
    f: &WorktreeFixture,
    github: &FakeGithub,
    tokens: &(GlobalSettingsStore, GithubTokens),
) -> Result<crate::git::CreatedPullRequest, String> {
    create_reported(
        &NullSink, own_buffer(), &f.root(), &tokens.0, &tokens.1, "/dev/acme", github, "title",
        "body", "main", "feature", false,
    )
}

// GTC-FR-XUAC, GTC-FR-MMFM
#[test]
fn a_created_pull_request_page_must_be_on_the_host_of_the_remote() {
    let f = repo_with_origin(GHE_REMOTE);
    let github = FakeGithub::new();
    github.answer_post(Ok(json!({
        "number": 9,
        "html_url": "https://company.ghe.com/acme/platform/pull/9",
    })));
    let created = create(&f, &github, &token_on("company.ghe.com")).unwrap();
    assert_eq!(created.number, 9);
    assert_eq!(created.url, "https://company.ghe.com/acme/platform/pull/9");

    let github = FakeGithub::new();
    github.answer_post(Ok(json!({
        "number": 9,
        "html_url": "https://github.com/acme/platform/pull/9",
    })));
    assert_eq!(
        create(&f, &github, &token_on("company.ghe.com")).unwrap_err(),
        "github_unreachable",
        "a page on another host is not accepted"
    );
}

// GTC-FR-FSLC, GTC-FR-MMFM
#[test]
fn creating_a_pull_request_with_a_token_of_another_host_writes_nothing() {
    let f = repo_with_origin(GHE_REMOTE);
    let github = FakeGithub::new();
    github.answer_post(Err(GithubWriteFailure::Unreachable));

    let error = create(&f, &github, &token_ready()).unwrap_err();

    assert_eq!(error, "github_host_mismatch");
    assert!(github.posts().is_empty());
}

// GTC-FR-09
#[test]
fn a_push_to_an_enterprise_remote_presents_no_token_of_another_host() {
    let tokens = token_ready();
    assert_eq!(
        crate::github_tokens::resolve_remote_token(&tokens.0, &tokens.1, "/dev/acme", GHE_REMOTE)
            .unwrap_err(),
        "github_host_mismatch"
    );
    let ghe = token_on("company.ghe.com");
    assert_eq!(
        crate::github_tokens::resolve_remote_token(&ghe.0, &ghe.1, "/dev/acme", GHE_REMOTE)
            .unwrap()
            .as_deref(),
        Some(SECRET)
    );
    assert!(is_github_https_remote(GHE_REMOTE));
    assert!(!is_github_https_remote("https://ghe.com/acme/platform.git"));
}
