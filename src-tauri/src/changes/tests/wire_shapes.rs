//! The shapes the frontend types against, and a linked worktree.
//!
//! One part of `../tests/mod.rs`, which holds the fixture these run against.

use super::*;

// -----------------------------------------------------------------------
// Wire shape
// -----------------------------------------------------------------------

#[test]
fn comparison_serialises_as_a_tagged_union() {
    let uncommitted = serde_json::to_value(Comparison::Uncommitted).unwrap();
    assert_eq!(uncommitted, serde_json::json!({ "kind": "uncommitted" }));

    let branch = serde_json::to_value(Comparison::Branch {
        target_branch: "main".into(),
        merge_base: "abc123".into(),
    })
    .unwrap();
    assert_eq!(
        branch,
        serde_json::json!({
            "kind": "branch",
            "targetBranch": "main",
            "mergeBase": "abc123",
        })
    );
}

#[test]
fn change_status_serialises_lowercase() {
    for (status, expected) in [
        (ChangeStatus::Added, "added"),
        (ChangeStatus::Modified, "modified"),
        (ChangeStatus::Deleted, "deleted"),
        (ChangeStatus::Renamed, "renamed"),
        (ChangeStatus::Untracked, "untracked"),
    ] {
        assert_eq!(serde_json::to_string(&status).unwrap(), format!("\"{expected}\""));
    }
}

#[test]
fn map_status_drops_non_changes() {
    assert_eq!(map_status(Delta::Unmodified), None);
    assert_eq!(map_status(Delta::Ignored), None);
    assert_eq!(map_status(Delta::Unreadable), None);
    assert_eq!(map_status(Delta::Copied), Some(ChangeStatus::Renamed));
    assert_eq!(map_status(Delta::Typechange), Some(ChangeStatus::Modified));
    assert_eq!(map_status(Delta::Conflicted), Some(ChangeStatus::Modified));
}

#[test]
fn branch_option_serialises_camel_case() {
    let json = serde_json::to_value(BranchOption {
        name: "main".into(),
        is_current: false,
        is_default: true,
    })
    .unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "name": "main", "isCurrent": false, "isDefault": true })
    );
}

#[test]
fn changes_updated_payload_serialises_camel_case() {
    let json = serde_json::to_value(ChangesUpdatedPayload { change_count: 3 }).unwrap();
    assert_eq!(json, serde_json::json!({ "changeCount": 3 }));
}

#[test]
fn command_functions_are_in_scope() {
    // Renaming or dropping one of these fails to compile here, before it can
    // silently regress at `invoke` time.
    let _a = list_uncommitted_changes;
    let _b = list_branch_changes;
    let _c = get_default_branch;
    let _d = list_comparison_branches;
    let _e = get_uncommitted_diff_totals;
}

#[test]
fn uncommitted_changes_are_found_in_a_linked_worktree() {
    let fx = Fixture::new();
    fx.write("a.md", "one\n");
    fx.commit("init");
    let holder = TempDir::new().unwrap();
    let wt_path = holder.path().join("wt");
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();
    let reference = repo
        .find_branch("feature", BranchType::Local)
        .unwrap()
        .into_reference();
    let mut opts = git2::WorktreeAddOptions::new();
    opts.reference(Some(&reference));
    repo.worktree("feature", &wt_path, Some(&opts)).unwrap();
    std::fs::write(wt_path.join("a.md"), "two\n").unwrap();
    std::fs::write(wt_path.join("new.md"), "new\n").unwrap();
    let root = crate::fs::RootFs::for_root(&wt_path);
    let set = uncommitted_change_set(&root).unwrap();
    let mut paths: Vec<&str> = set.entries.iter().map(|e| e.path.as_str()).collect();
    paths.sort();
    assert_eq!(paths, vec!["a.md", "new.md"]);
}
