//! What one branch is and holds (GTC-FR-TGOI).
//!
//! One part of `mod.rs`, which holds the fixtures these run against.

use super::history::commit_at;
use super::*;

fn information(
    f: &WorktreeFixture,
    name: &str,
    kind: &str,
    stream: Option<BranchStreamRef>,
) -> Result<BranchInformation, String> {
    let worktrees = crate::worktree::context_for(&f.root()).unwrap().worktrees;
    branch_information_from(&f.root(), name, kind, &worktrees, stream)
}

// GTC-FR-TGOI
#[test]
fn a_local_branch_with_an_upstream_names_it_and_holds_its_tip_and_commits() {
    let f = WorktreeFixture::new();
    f.branch("feature");
    f.remote_ref("origin/feature");
    f.repo()
        .find_branch("feature", BranchType::Local)
        .unwrap()
        .set_upstream(Some("origin/feature"))
        .unwrap();
    let repo = f.repo();
    let tip = repo.head().unwrap().peel_to_commit().unwrap().id();

    let info = information(&f, "feature", "local", None).unwrap();

    assert_eq!(info.name, "feature");
    assert_eq!(info.kind, "local");
    assert_eq!(info.upstream.as_deref(), Some("origin/feature"));
    assert!(!info.is_current);
    assert_eq!(info.tip.id, tip.to_string());
    assert!(info.tip.refs.contains(&"feature".to_string()));
    assert!(info.tip.refs.contains(&"origin/feature".to_string()));
    assert_eq!(info.commits.len(), 1);
    assert_eq!(info.commits[0].id, info.tip.id);
    assert!(info.stream.is_none());
}

// GTC-FR-TGOI
#[test]
fn the_current_branch_is_current_and_names_the_primary_worktree() {
    let f = WorktreeFixture::new();

    let info = information(&f, &f.current(), "local", None).unwrap();

    assert!(info.is_current);
    let worktree = info.worktree.expect("the primary worktree has it checked out");
    assert!(worktree.is_primary);
    assert!(worktree.is_active);
    assert_eq!(std::path::Path::new(&worktree.path), f.root().as_path());
}

// GTC-FR-TGOI
#[test]
fn a_branch_in_a_linked_worktree_names_that_worktree() {
    let f = WorktreeFixture::new();
    f.branch("feature");
    let path = crate::worktree::create_worktree_at(&f.root(), "feature", &f.sibling("wt-feature").to_string_lossy())
        .unwrap();

    let info = information(&f, "feature", "local", None).unwrap();

    assert!(!info.is_current);
    let worktree = info.worktree.expect("the linked worktree has it checked out");
    assert_eq!(std::path::Path::new(&worktree.path), path.as_path());
    assert_eq!(worktree.name, "wt-feature");
    assert!(!worktree.is_primary);
    assert!(!worktree.is_active);
    let json = serde_json::to_value(&worktree).unwrap();
    assert_eq!(json["isActive"], false);
    assert_eq!(json["isPrimary"], false);
}

// GTC-FR-TGOI
#[test]
fn a_remote_branch_has_no_upstream_no_worktree_and_is_never_current() {
    let f = WorktreeFixture::new();
    f.remote_ref("origin/experiment");

    let info = information(&f, "origin/experiment", "remote", None).unwrap();

    assert_eq!(info.name, "origin/experiment");
    assert_eq!(info.kind, "remote");
    assert!(info.upstream.is_none());
    assert!(info.worktree.is_none());
    assert!(!info.is_current);
    assert!(info.tip.refs.contains(&"origin/experiment".to_string()));
}

// GTC-FR-TGOI
#[test]
fn the_stream_that_owns_the_branch_is_carried_through_in_camel_case() {
    let f = WorktreeFixture::new();
    f.branch("synthesis/stream/editor");

    let info = information(
        &f,
        "synthesis/stream/editor",
        "local",
        Some(BranchStreamRef {
            stream_id: "w1".into(),
            stream_name: "Editor".into(),
        }),
    )
    .unwrap();

    let json = serde_json::to_value(&info).unwrap();
    assert_eq!(json["stream"]["streamId"], "w1");
    assert_eq!(json["stream"]["streamName"], "Editor");
    assert!(json.get("worktree").is_none(), "a branch with no worktree omits it");
    assert!(json.get("upstream").is_none());
    assert_eq!(json["isCurrent"], false);
    assert!(json["tip"]["shortId"].is_string());
}

// GTC-FR-TGOI
#[test]
fn a_branch_lists_at_most_a_hundred_commits_newest_first() {
    let f = Fixture::new();
    let repo = f.repo();
    for n in 0..103 {
        commit_at(&repo, &format!("c{n}"), 1_700_000_000 + n * 60, &[("a.md", Some(format!("{n}\n").as_bytes()))]);
    }
    let branch = f.current_branch();

    let info = branch_information_from(&f.root(), &branch, "local", &[], None).unwrap();

    assert_eq!(info.commits.len(), 100);
    assert_eq!(info.commits[0].subject, "c102");
    assert_eq!(info.tip.subject, "c102");
}

// GTC-FR-TGOI
#[test]
fn a_name_that_is_no_branch_of_that_kind_is_unknown_branch() {
    let f = WorktreeFixture::new();
    f.remote_ref("origin/experiment");

    assert_eq!(information(&f, "missing", "local", None).unwrap_err(), ERR_UNKNOWN_BRANCH);
    assert_eq!(information(&f, "origin/experiment", "local", None).unwrap_err(), ERR_UNKNOWN_BRANCH);
    assert_eq!(information(&f, &f.current(), "remote", None).unwrap_err(), ERR_UNKNOWN_BRANCH);
    assert_eq!(information(&f, "origin/HEAD", "remote", None).unwrap_err(), ERR_UNKNOWN_BRANCH);
    assert_eq!(information(&f, "", "local", None).unwrap_err(), ERR_UNKNOWN_BRANCH);
    assert_eq!(information(&f, &f.current(), "other", None).unwrap_err(), ERR_UNKNOWN_BRANCH);
}

// GTC-FR-TGOI
#[test]
fn reading_branch_information_writes_nothing() {
    let f = WorktreeFixture::new();
    f.branch("feature");
    let before = f.snapshot_all();

    information(&f, "feature", "local", None).unwrap();

    assert_eq!(before, f.snapshot_all());
}
