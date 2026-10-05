//! The remote part of a branch deletion (GTC-FR-FSRQ, GTC-FR-NPCT).
//!
//! The success path runs against a bare repository reached over libgit2's
//! `file://` transport, a real push rather than a mocked one. A GitHub remote is
//! named only by its URL and is never contacted: the credential is resolved, and
//! refused, before any transport is opened.

use super::branch_deletion::DeletionFixture;
use super::*;

/// A bare repository the fixture's `origin` points at, holding `branch` at the
/// commit the local branch has, with the remote-tracking ref and the upstream
/// wired as a fetch would leave them.
fn publish(fx: &DeletionFixture, branch: &str) -> TempDir {
    let bare = TempDir::new().unwrap();
    Repository::init_bare(bare.path()).unwrap();
    let repo = fx.f.repo();
    repo.remote("origin", bare.path().to_str().unwrap()).unwrap();
    publish_to(fx, branch);
    bare
}

fn publish_to(fx: &DeletionFixture, branch: &str) {
    let repo = fx.f.repo();
    if fx.f.repo().find_branch(branch, BranchType::Local).is_err() {
        fx.f.branch(branch);
    }
    let mut remote = repo.find_remote("origin").unwrap();
    remote
        .push(&[format!("refs/heads/{branch}:refs/heads/{branch}").as_str()], None)
        .unwrap();
    let id = repo
        .find_branch(branch, BranchType::Local)
        .unwrap()
        .get()
        .peel_to_commit()
        .unwrap()
        .id();
    repo.reference(&format!("refs/remotes/origin/{branch}"), id, true, "test")
        .unwrap();
    repo.find_branch(branch, BranchType::Local)
        .unwrap()
        .set_upstream(Some(&format!("origin/{branch}")))
        .unwrap();
}

fn remote_has(bare: &TempDir, branch: &str) -> bool {
    Repository::open_bare(bare.path())
        .unwrap()
        .find_branch(branch, BranchType::Local)
        .is_ok()
}

fn tracking_exists(fx: &DeletionFixture, tracking: &str) -> bool {
    fx.f.repo().find_branch(tracking, BranchType::Remote).is_ok()
}

// GTC-FR-FSRQ
#[test]
fn the_remote_is_not_contacted_unless_the_deletion_asks_for_it() {
    let fx = DeletionFixture::new();
    let bare = publish(&fx, "feature");

    let outcome = fx.delete("feature", false, false).unwrap();

    assert!(!fx.has_branch("feature"));
    assert!(remote_has(&bare, "feature"), "the remote branch stands");
    assert!(tracking_exists(&fx, "origin/feature"), "and so does its tracking ref");
    assert!(!outcome.remote.requested);
    assert_eq!(outcome.remote.state, "not_requested");
    assert!(outcome.remote.branch.is_none());
    assert!(
        records_for(fx.buffer, "deleting remote branch").is_empty(),
        "no transfer was recorded"
    );
}

// GTC-FR-FSRQ, GTC-FR-NPCT
#[test]
fn a_remote_deletion_removes_the_remote_branch_and_its_tracking_ref() {
    let fx = DeletionFixture::new();
    let bare = publish(&fx, "feature");
    fx.linked("wt-feature", "feature");
    assert!(remote_has(&bare, "feature"));

    let outcome = fx.delete("feature", true, false).unwrap();

    assert!(!fx.has_branch("feature"));
    assert!(!remote_has(&bare, "feature"), "the remote branch is deleted");
    assert!(!tracking_exists(&fx, "origin/feature"), "and its tracking ref");
    assert!(outcome.remote.requested);
    assert_eq!(outcome.remote.state, "deleted");
    assert_eq!(outcome.remote.branch.as_deref(), Some("origin/feature"));
    assert!(outcome.remote.error.is_none());
    assert!(outcome.removed_worktree_path.is_some());
    assert_eq!(fx.events(), 1);
    let done = record_for(fx.buffer, "remote branch deleted");
    assert_eq!(done.fields.get("remote"), Some(&serde_json::json!("origin")));
    assert!(!buffer_text(fx.buffer).contains(bare.path().to_str().unwrap()), "no remote URL in a record");
}

// GTC-FR-FSRQ
#[test]
fn the_other_branches_of_the_remote_are_left_alone() {
    let fx = DeletionFixture::new();
    let bare = publish(&fx, "feature");
    publish_to(&fx, "keeper");

    fx.delete("feature", true, false).unwrap();

    assert!(remote_has(&bare, "keeper"));
    assert!(tracking_exists(&fx, "origin/keeper"));
}

// GTC-FR-FSRQ
#[test]
fn a_remote_that_cannot_be_reached_is_reported_failed_and_the_local_results_stay() {
    let fx = DeletionFixture::new();
    fx.f.branch("feature");
    fx.f.repo().remote("origin", "file:///nonexistent/synthesis-remote").unwrap();
    fx.f.remote_ref("origin/feature");
    let path = fx.linked("wt-feature", "feature");

    let outcome = fx.delete("feature", true, false).unwrap();

    assert!(!fx.has_branch("feature"), "the local branch is deleted");
    assert!(!path.exists(), "and the worktree");
    assert!(outcome.remote.requested);
    assert_eq!(outcome.remote.state, "failed");
    assert_eq!(outcome.remote.branch.as_deref(), Some("origin/feature"));
    assert_eq!(
        outcome.remote.error.as_deref(),
        Some(crate::github_tokens::ERR_GITHUB_UNREACHABLE)
    );
    assert!(tracking_exists(&fx, "origin/feature"), "a failed push leaves the tracking ref");
    assert_eq!(fx.events(), 1, "the branch was removed, so the event is emitted");
    let failed = record_for(fx.buffer, "remote branch deletion failed");
    assert_eq!(failed.level, crate::logging::LogLevel::Error);
}

// GTC-FR-FSRQ
#[test]
fn a_remote_deletion_with_no_associated_remote_branch_reports_nothing_to_delete() {
    let fx = DeletionFixture::new();
    fx.f.branch("local-only");

    let outcome = fx.delete("local-only", true, false).unwrap();

    assert!(!fx.has_branch("local-only"));
    assert!(outcome.remote.requested);
    assert_eq!(outcome.remote.state, "not_requested");
    assert!(outcome.remote.branch.is_none());
    assert_eq!(fx.events(), 1);
}

// GTC-FR-FSRQ
#[test]
fn a_github_remote_with_no_token_is_refused_before_any_change() {
    let fx = DeletionFixture::new();
    let path = fx.linked("wt-feature", "feature");
    fx.f.repo().remote("origin", "https://github.com/acme/platform.git").unwrap();
    fx.f.remote_ref("origin/feature");
    let before = fx.f.snapshot_all();

    let none_stored = fx.delete("feature", true, false).unwrap_err();
    assert_eq!(none_stored, crate::github_tokens::ERR_TOKEN_MISSING);

    fx.settings
        .save_github_token_registry(vec![
            crate::github_tokens::GithubTokenRecord {
                id: "a".into(),
                label: "one".into(),
                ..Default::default()
            },
            crate::github_tokens::GithubTokenRecord {
                id: "b".into(),
                label: "two".into(),
                ..Default::default()
            },
        ])
        .unwrap();
    let not_chosen = fx.delete("feature", true, false).unwrap_err();
    assert_eq!(not_chosen, crate::github_tokens::ERR_SELECTION_REQUIRED);

    assert!(fx.has_branch("feature"), "the local branch stands");
    assert!(path.exists(), "and the worktree");
    assert_eq!(before, fx.f.snapshot_all());
    assert_eq!(fx.events(), 0);
    let refusals = records_for(fx.buffer, "branch deletion refused");
    assert_eq!(refusals.len(), 2);
    assert!(refusals.iter().all(|r| r.level == crate::logging::LogLevel::Warn));
}

// GTC-FR-FSRQ
#[test]
fn a_github_remote_is_not_asked_for_a_token_when_the_remote_is_not_requested() {
    let fx = DeletionFixture::new();
    fx.f.branch("feature");
    fx.f.repo().remote("origin", "https://github.com/acme/platform.git").unwrap();
    fx.f.remote_ref("origin/feature");

    let outcome = fx.delete("feature", false, false).unwrap();

    assert_eq!(outcome.remote.state, "not_requested");
    assert!(!fx.has_branch("feature"));
}

// GTC-FR-FSRQ
#[test]
fn a_remote_that_is_not_github_needs_no_token() {
    let fx = DeletionFixture::new();
    let bare = publish(&fx, "feature");

    // No token is stored and none is asked for.
    let outcome = fx.delete("feature", true, false).unwrap();

    assert_eq!(outcome.remote.state, "deleted");
    assert!(!remote_has(&bare, "feature"));
}

// GTC-FR-FSRQ, GTC-FR-11
#[test]
fn a_credential_in_an_error_is_removed_before_it_is_reported() {
    let line = redact_credentials("rejected by https://user:s3cret@example.com/repo.git today");

    assert_eq!(line, "rejected by https://example.com/repo.git today");
}
