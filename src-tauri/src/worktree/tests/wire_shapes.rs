//! An unborn HEAD, and the serialised shapes the frontend reads.
//!
//! One part of `mod.rs`, which holds the fixture and the helpers these use.

use super::*;

// -- An unborn HEAD -----------------------------------------------------

#[test]
fn a_repository_with_no_commits_still_names_the_branch_head_points_at() {
    let container = TempDir::new().unwrap();
    let root = container.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    Repository::init(&root).unwrap();

    let entry = active_entry_for(&root).unwrap();
    assert!(
        entry.branch.is_some(),
        "an unborn HEAD still names the branch the first commit will create"
    );
    assert_eq!(entry.head_short_hash, "", "and has no commit to abbreviate");
    assert!(!entry.is_detached);
}

// -- Wire shapes --------------------------------------------------------

#[test]
fn the_context_serialises_camel_case() {
    let json = serde_json::to_value(WorktreeContext {
        repository_root: "/r".into(),
        active_worktree_path: "/r-main".into(),
        worktrees: vec![WorktreeEntry {
            path: "/r-main".into(),
            name: "r-main".into(),
            branch: Some("main".into()),
            head_short_hash: "4f2a10c".into(),
            is_detached: false,
            is_active: true,
            is_primary: false,
            is_missing: false,
            stream: None,
        }],
        branches: vec![BranchEntry {
            name: "origin/experiment".into(),
            kind: BRANCH_KIND_REMOTE.into(),
            upstream: None,
            head_short_hash: "abc1234".into(),
        }],
    })
    .unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "repositoryRoot": "/r",
            "activeWorktreePath": "/r-main",
            "worktrees": [{
                "path": "/r-main",
                "name": "r-main",
                "branch": "main",
                "headShortHash": "4f2a10c",
                "isDetached": false,
                "isActive": true,
                "isPrimary": false,
                "isMissing": false,
            }],
            "branches": [{
                "name": "origin/experiment",
                "kind": "remote",
                "headShortHash": "abc1234",
            }],
        })
    );
}

#[test]
fn a_detached_entry_omits_the_branch_key_entirely() {
    let json = serde_json::to_value(WorktreeEntry {
        path: "/r".into(),
        name: "r".into(),
        branch: None,
        head_short_hash: "4f2a10c".into(),
        is_detached: true,
        ..Default::default()
    })
    .unwrap();
    assert!(json.get("branch").is_none());
    assert_eq!(json.get("isDetached").unwrap(), &serde_json::json!(true));
}

#[test]
fn the_event_payload_serialises_camel_case() {
    let json = serde_json::to_value(WorktreeContextChangedPayload {
        active_worktree_path: "/r-main".into(),
        branch: Some("main".into()),
        is_detached: false,
    })
    .unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "activeWorktreePath": "/r-main",
            "branch": "main",
            "isDetached": false,
        })
    );
}

#[test]
fn sanitise_replaces_every_awkward_segment_character() {
    assert_eq!(sanitize_segment("fix/editor-scroll"), "fix-editor-scroll");
    assert_eq!(sanitize_segment("release/2.1"), "release-2.1");
    assert_eq!(sanitize_segment("feat:a b"), "feat-a-b");
    assert_eq!(sanitize_segment("plain"), "plain");
}

#[test]
fn command_functions_are_in_scope() {
    // A rename here fails to compile rather than silently un-registering a
    // command at runtime (see `lib.rs`'s COMMAND_NAMES guard).
    let _list = list_worktrees_and_branches;
    let _active = get_active_worktree;
    let _propose = propose_worktree_path;
    let _activate = activate_worktree;
    let _checkout = check_out_branch_in_active_worktree;
    let _create = create_worktree;
    let _refresh = refresh_worktrees_and_branches;
}
