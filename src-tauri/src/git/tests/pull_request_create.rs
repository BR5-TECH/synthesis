//! Opening a pull request and reading what stands between a branch and one
//! (GTC-FR-MMFM, GTC-FR-YQAE, GTC-FR-NEIW, GTC-FR-YWCP).
//!
//! GitHub is a scripted fake that records every write it receives, so "no
//! request is made" is an assertion about that record.

use super::pull_requests::{
    repo_with_origin, token_missing, token_selection_required, FakeGithub, PLATFORM, SECRET,
};
use super::*;
use crate::git::pull_requests::client::{GithubWriteFailure, invalid_reason};
use crate::git::pull_requests::create::{
    checkout_holding, create_pull_request_reported as create_reported, head_state_in,
};
use crate::git::pull_requests::{
    ERR_GITHUB_TOKEN_REJECTED,
    ERR_PULL_REQUEST_EXISTS, ERR_PULL_REQUEST_REJECTED, ERR_PULL_REQUEST_TITLE_REQUIRED,
};
use serde_json::{json, Value};

fn created_json(number: u64) -> Value {
    json!({
        "number": number,
        "html_url": format!("https://github.com/acme/platform/pull/{number}"),
        "body": "private description",
    })
}

fn create(
    f: &WorktreeFixture,
    github: &FakeGithub,
    tokens: &(GlobalSettingsStore, GithubTokens),
    buffer: &'static LogBuffer,
    title: &str,
    draft: bool,
) -> Result<crate::git::CreatedPullRequest, String> {
    create_reported(
        &NullSink, buffer, &f.root(), &tokens.0, &tokens.1, "/dev/acme", github, title,
        "the description", "main", "feature", draft,
    )
}

// GTC-FR-MMFM
#[test]
fn creating_a_pull_request_posts_the_request_and_keeps_the_number_and_the_page_only() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    github.answer_post(Ok(created_json(42)));

    let created =
        create(&f, &github, &super::pull_requests::token_ready(), own_buffer(), "Add it", true)
            .unwrap();

    assert_eq!(
        github.posts(),
        vec![(
            SECRET.to_string(),
            "/repos/acme/platform/pulls".to_string(),
            json!({
                "title": "Add it",
                "body": "the description",
                "head": "feature",
                "base": "main",
                "draft": true,
            }),
        )]
    );
    let wire = serde_json::to_value(&created).unwrap();
    assert_eq!(
        wire,
        json!({ "number": 42, "url": "https://github.com/acme/platform/pull/42" })
    );
}

// GTC-FR-MMFM
#[test]
fn an_empty_title_head_or_base_makes_no_request_and_opens_no_repository() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    let tokens = super::pull_requests::token_ready();
    assert_eq!(
        create(&f, &github, &tokens, own_buffer(), "   ", false).unwrap_err(),
        ERR_PULL_REQUEST_TITLE_REQUIRED
    );
    let error = create_reported(
        &NullSink, own_buffer(), &f.root(), &tokens.0, &tokens.1, "/dev/acme", &github, "t",
        "", "main", " ", false,
    )
    .unwrap_err();
    assert_eq!(error, ERR_UNKNOWN_BRANCH);
    assert!(github.posts().is_empty());
}

// GTC-FR-MMFM, GTC-FR-DWVY
#[test]
fn a_project_without_a_token_makes_no_request_and_returns_the_typed_cause() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    assert_eq!(
        create(&f, &github, &token_missing(), own_buffer(), "t", false).unwrap_err(),
        github_tokens::ERR_TOKEN_MISSING
    );
    assert_eq!(
        create(&f, &github, &token_selection_required(), own_buffer(), "t", false).unwrap_err(),
        github_tokens::ERR_SELECTION_REQUIRED
    );
    assert!(github.posts().is_empty(), "no request is made without a credential");
}

// GTC-FR-MMFM
#[test]
fn a_remote_that_is_not_on_github_makes_no_request() {
    let f = repo_with_origin("https://example.com/acme/platform.git");
    let github = FakeGithub::new();
    let error =
        create(&f, &github, &super::pull_requests::token_ready(), own_buffer(), "t", false)
            .unwrap_err();
    assert_eq!(error, crate::git::ERR_NOT_A_GITHUB_REMOTE);
    assert!(github.posts().is_empty());
}

// GTC-FR-YQAE
#[test]
fn the_refusals_of_github_map_to_typed_errors() {
    let f = repo_with_origin(PLATFORM);
    let tokens = super::pull_requests::token_ready();
    let cases = [
        (
            GithubWriteFailure::Invalid("Validation Failed; A pull request already exists for acme:feature.".into()),
            ERR_PULL_REQUEST_EXISTS.to_string(),
        ),
        (
            GithubWriteFailure::Invalid("No commits between main and feature".into()),
            format!("{ERR_PULL_REQUEST_REJECTED}: No commits between main and feature"),
        ),
        (GithubWriteFailure::Invalid(String::new()), ERR_PULL_REQUEST_REJECTED.to_string()),
        (GithubWriteFailure::Rejected, ERR_GITHUB_TOKEN_REJECTED.to_string()),
        (GithubWriteFailure::NotFound, ERR_GITHUB_TOKEN_REJECTED.to_string()),
        (GithubWriteFailure::Unreachable, github_tokens::ERR_GITHUB_UNREACHABLE.to_string()),
    ];
    for (failure, expected) in cases {
        let github = FakeGithub::new();
        github.answer_post(Err(failure));
        assert_eq!(create(&f, &github, &tokens, own_buffer(), "t", false).unwrap_err(), expected);
    }
}

// GTC-FR-YQAE
#[test]
fn an_answer_without_a_github_page_is_unreachable() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    github.answer_post(Ok(json!({ "number": 3, "html_url": "https://evil.example/x" })));
    assert_eq!(
        create(&f, &github, &super::pull_requests::token_ready(), own_buffer(), "t", false)
            .unwrap_err(),
        github_tokens::ERR_GITHUB_UNREACHABLE
    );
}

// GTC-FR-YQAE, GTC-FR-DWVY
#[test]
fn the_log_holds_neither_the_token_the_title_nor_the_description() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    github.answer_post(Ok(created_json(9)));
    let buffer = own_buffer();
    create(&f, &github, &super::pull_requests::token_ready(), buffer, "secret title", false)
        .unwrap();
    let text = buffer_text(buffer);
    assert!(!text.contains(SECRET));
    assert!(!text.contains("secret title"));
    assert!(!text.contains("the description"));
    assert!(text.contains("create_pull_request"));
}

// GTC-FR-YQAE
#[test]
fn the_reason_of_a_422_joins_the_message_and_each_error() {
    let body = r#"{"message":"Validation Failed","errors":[{"message":"No commits between a and b"}]}"#;
    assert_eq!(invalid_reason(body), "Validation Failed; No commits between a and b");
    assert_eq!(invalid_reason("not json"), "");
    // A branch GitHub does not hold is named by field and code only.
    let branch = r#"{"message":"Validation Failed","errors":[{"resource":"PullRequest","field":"head","code":"invalid"}]}"#;
    assert_eq!(invalid_reason(branch), "Validation Failed; head invalid");
}

// -- the head state ----------------------------------------------------------

fn fixture_with_feature() -> Fixture {
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    f.repo().remote("origin", PLATFORM).unwrap();
    f
}

fn point(f: &Fixture, name: &str, oid: Oid) {
    f.repo().reference(name, oid, true, "t").unwrap();
}

// GTC-FR-NEIW, GTC-FR-YWCP
#[test]
fn a_branch_with_commits_and_no_remote_branch_reports_both() {
    let f = fixture_with_feature();
    let default = f.current_branch();
    f.branch_and_checkout("feature");
    f.write("b.md", "two\n");
    f.commit("one");
    f.write("b.md", "three\n");
    f.commit("two");

    let state = head_state_in(&f.repo(), "feature", Some(&default), vec![]).unwrap();

    assert_eq!(state.head, "feature");
    assert_eq!(state.base, default);
    assert!(state.has_remote);
    assert!(!state.remote_branch_exists);
    assert_eq!(state.unpushed, None);
    assert_eq!(state.ahead_of_base, 2);
}

// GTC-FR-YWCP
#[test]
fn unpushed_counts_the_commits_the_remote_branch_lacks() {
    let f = fixture_with_feature();
    let default = f.current_branch();
    f.branch_and_checkout("feature");
    f.write("b.md", "two\n");
    let pushed = f.commit("pushed");
    point(&f, "refs/remotes/origin/feature", pushed);
    f.write("b.md", "three\n");
    f.commit("not pushed");

    let state = head_state_in(&f.repo(), "feature", Some(&default), vec![]).unwrap();

    assert!(state.remote_branch_exists);
    assert_eq!(state.unpushed, Some(1));
    assert_eq!(state.ahead_of_base, 2);
}

// GTC-FR-NEIW
#[test]
fn a_repository_without_a_remote_reports_no_remote() {
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    let default = f.current_branch();
    f.branch_and_checkout("feature");
    let state = head_state_in(&f.repo(), "feature", Some(&default), vec![]).unwrap();
    assert!(!state.has_remote);
    assert!(!state.remote_branch_exists);
    assert_eq!(state.ahead_of_base, 0);
}

// GTC-FR-NEIW
#[test]
fn an_absent_base_is_the_default_branch_and_the_default_branch_is_not_ahead_of_itself() {
    let f = fixture_with_feature();
    let default = f.current_branch();
    let state = head_state_in(&f.repo(), &default, None, vec![]).unwrap();
    assert_eq!(state.base, crate::changes::default_branch(&f.repo()).unwrap());
    assert_eq!(state.ahead_of_base, 0);
}

// GTC-FR-NEIW
#[test]
fn a_base_may_be_a_remote_tracking_branch_and_an_unknown_name_is_refused() {
    let f = fixture_with_feature();
    let tip = f.repo().head().unwrap().peel_to_commit().unwrap().id();
    point(&f, "refs/remotes/origin/release", tip);
    f.branch_and_checkout("feature");
    f.write("b.md", "two\n");
    f.commit("one");

    let state = head_state_in(&f.repo(), "feature", Some("release"), vec![]).unwrap();
    assert_eq!(state.ahead_of_base, 1);

    assert_eq!(
        head_state_in(&f.repo(), "feature", Some("nope"), vec![]).unwrap_err(),
        ERR_UNKNOWN_BRANCH
    );
    assert_eq!(
        head_state_in(&f.repo(), "ghost", None, vec![]).unwrap_err(),
        ERR_UNKNOWN_BRANCH
    );
}

// GTC-FR-YWCP
#[test]
fn the_uncommitted_paths_given_are_reported_as_they_are() {
    let f = fixture_with_feature();
    let default = f.current_branch();
    f.branch_and_checkout("feature");
    let state =
        head_state_in(&f.repo(), "feature", Some(&default), vec!["a.md".into()]).unwrap();
    assert_eq!(state.uncommitted_paths, vec!["a.md".to_string()]);
}

// GTC-FR-NEIW
#[test]
fn the_head_state_serialises_to_the_camel_case_contract() {
    let f = fixture_with_feature();
    let default = f.current_branch();
    f.branch_and_checkout("feature");
    let state = head_state_in(&f.repo(), "feature", Some(&default), vec![]).unwrap();
    let wire = serde_json::to_value(&state).unwrap();
    assert_eq!(
        wire,
        json!({
            "head": "feature",
            "base": default,
            "hasRemote": true,
            "remoteBranchExists": false,
            "unpushed": null,
            "uncommittedPaths": [],
            "aheadOfBase": 0,
        })
    );
}

// GTC-FR-YWCP
#[test]
fn a_remote_branch_level_with_the_head_leaves_nothing_unpushed() {
    let f = fixture_with_feature();
    let default = f.current_branch();
    f.branch_and_checkout("feature");
    f.write("b.md", "two\n");
    let tip = f.commit("pushed");
    point(&f, "refs/remotes/origin/feature", tip);

    let state = head_state_in(&f.repo(), "feature", Some(&default), vec![]).unwrap();

    assert!(state.remote_branch_exists);
    assert_eq!(state.unpushed, Some(0));
}

// GTC-FR-YWCP
#[test]
fn only_the_primary_remote_counts_as_the_remote_of_the_branch() {
    let f = fixture_with_feature();
    let default = f.current_branch();
    f.repo().remote("fork", "https://github.com/someone/platform.git").unwrap();
    f.branch_and_checkout("feature");
    let tip = f.repo().head().unwrap().peel_to_commit().unwrap().id();
    point(&f, "refs/remotes/fork/feature", tip);

    let state = head_state_in(&f.repo(), "feature", Some(&default), vec![]).unwrap();

    assert!(
        !state.remote_branch_exists,
        "a branch on another remote is not on the primary remote"
    );
}

// GTC-FR-NEIW
#[test]
fn a_head_that_exists_only_on_the_remote_is_unknown() {
    let f = fixture_with_feature();
    let default = f.current_branch();
    let tip = f.repo().head().unwrap().peel_to_commit().unwrap().id();
    point(&f, "refs/remotes/origin/elsewhere", tip);
    assert_eq!(
        head_state_in(&f.repo(), "elsewhere", Some(&default), vec![]).unwrap_err(),
        ERR_UNKNOWN_BRANCH
    );
}

// GTC-FR-NEIW
#[test]
fn an_empty_base_is_the_default_branch_and_a_remote_tracking_name_is_taken_as_written() {
    let f = fixture_with_feature();
    let default = f.current_branch();
    let tip = f.repo().head().unwrap().peel_to_commit().unwrap().id();
    point(&f, "refs/remotes/origin/release", tip);
    f.branch_and_checkout("feature");
    f.write("b.md", "two\n");
    f.commit("one");

    let empty = head_state_in(&f.repo(), "feature", Some("  "), vec![]).unwrap();
    assert_eq!(empty.base, crate::changes::default_branch(&f.repo()).unwrap());
    assert_eq!(empty.base, default);

    let written = head_state_in(&f.repo(), "feature", Some("origin/release"), vec![]).unwrap();
    assert_eq!(written.ahead_of_base, 1);
}

// GTC-FR-NEIW
#[test]
fn reading_the_head_state_writes_no_ref_and_no_file() {
    let f = fixture_with_feature();
    let default = f.current_branch();
    f.branch_and_checkout("feature");
    let before = crate::changes::tests_support::snapshot(f.dir.path().to_path_buf());
    head_state_in(&f.repo(), "feature", Some(&default), vec![]).unwrap();
    let after = crate::changes::tests_support::snapshot(f.dir.path().to_path_buf());
    assert_eq!(before, after);
}

// GTC-FR-MMFM
#[test]
fn an_empty_base_makes_no_request_and_a_blank_title_wins_over_a_missing_token() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    let ready = super::pull_requests::token_ready();
    let error = create_reported(
        &NullSink, own_buffer(), &f.root(), &ready.0, &ready.1, "/dev/acme", &github, "t", "",
        "", "feature", false,
    )
    .unwrap_err();
    assert_eq!(error, ERR_UNKNOWN_BRANCH);

    // The title is checked before the token, so no keychain is opened for it.
    let missing = token_missing();
    assert_eq!(
        create(&f, &github, &missing, own_buffer(), " ", false).unwrap_err(),
        ERR_PULL_REQUEST_TITLE_REQUIRED
    );
    assert!(github.posts().is_empty());
}

// GTC-FR-MMFM
#[test]
fn the_title_and_description_are_sent_as_given_and_draft_false_is_sent_as_false() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    github.answer_post(Ok(created_json(3)));
    create_reported(
        &NullSink, own_buffer(), &f.root(), &super::pull_requests::token_ready().0,
        &super::pull_requests::token_ready().1, "/dev/acme", &github, "  spaced title  ",
        "  body\n", "main", "feature", false,
    )
    .unwrap();
    let body = &github.posts()[0].2;
    assert_eq!(body["title"], "  spaced title  ");
    assert_eq!(body["body"], "  body\n");
    assert_eq!(body["draft"], false);
}

// GTC-FR-MMFM, GTC-FR-YQAE
#[test]
fn creating_changes_no_ref_and_no_file() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    github.answer_post(Ok(created_json(3)));
    let before = f.snapshot_all();
    create(&f, &github, &super::pull_requests::token_ready(), own_buffer(), "t", false).unwrap();
    assert_eq!(before, f.snapshot_all());
}

// GTC-FR-YQAE
#[test]
fn the_log_names_the_branches_the_draft_flag_and_the_number() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    github.answer_post(Ok(created_json(9)));
    let buffer = own_buffer();
    create(&f, &github, &super::pull_requests::token_ready(), buffer, "t", true).unwrap();
    let text = buffer_text(buffer);
    for expected in ["feature", "main", "acme", "platform", "9", "true"] {
        assert!(text.contains(expected), "the record names {expected}: {text}");
    }
}

// GTC-FR-YQAE, GTC-FR-DWVY
#[test]
fn a_failed_creation_logs_the_error_code_and_still_no_secret() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    github.answer_post(Err(GithubWriteFailure::Invalid("No commits between main and feature".into())));
    let buffer = own_buffer();
    create(&f, &github, &super::pull_requests::token_ready(), buffer, "secret title", false)
        .unwrap_err();
    let text = buffer_text(buffer);
    assert!(text.contains("pull_request_rejected"));
    assert!(!text.contains(SECRET));
    assert!(!text.contains("secret title"));
}

// GTC-FR-YQAE
#[test]
fn a_reason_that_does_not_name_an_existing_pull_request_is_a_rejection() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    github.answer_post(Err(GithubWriteFailure::Invalid("Validation Failed".into())));
    let error =
        create(&f, &github, &super::pull_requests::token_ready(), own_buffer(), "t", false)
            .unwrap_err();
    assert_eq!(error, format!("{ERR_PULL_REQUEST_REJECTED}: Validation Failed"));
}

// GTC-FR-YWCP
#[test]
fn the_checkout_that_holds_the_head_is_found_in_any_worktree_and_a_missing_one_holds_nothing() {
    use crate::worktree::WorktreeEntry;
    let entry = |path: &str, branch: Option<&str>, missing: bool| WorktreeEntry {
        path: path.to_string(),
        branch: branch.map(str::to_string),
        is_missing: missing,
        ..Default::default()
    };
    let worktrees = vec![
        entry("/repo", Some("main"), false),
        entry("/linked", Some("feature"), false),
        entry("/gone", Some("old"), true),
    ];
    assert_eq!(checkout_holding(&worktrees, "feature"), Some("/linked".into()));
    assert_eq!(checkout_holding(&worktrees, "main"), Some("/repo".into()));
    assert_eq!(checkout_holding(&worktrees, "old"), None);
    assert_eq!(checkout_holding(&worktrees, "nowhere"), None);
}
