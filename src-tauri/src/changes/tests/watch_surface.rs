//! The watch surface, which is pure (CHC-FR-16).
//!
//! One part of `../tests/mod.rs`, which holds the fixture these run against.

use super::*;

// -----------------------------------------------------------------------
// CHC-FR-16 — the watch surface (pure)
// -----------------------------------------------------------------------

#[test]
fn git_meta_rel_covers_head_refs_and_the_index_but_not_locks() {
    // Paths are relative to the Git directory itself, not the project root.
    assert!(is_git_meta_rel("HEAD"));
    assert!(is_git_meta_rel("ORIG_HEAD"));
    assert!(is_git_meta_rel("MERGE_HEAD"));
    assert!(is_git_meta_rel("index"));
    assert!(is_git_meta_rel("packed-refs"));
    assert!(is_git_meta_rel("refs/heads/main"));
    // Transient scaffolding of an in-flight operation, not its result.
    assert!(!is_git_meta_rel("index.lock"));
    assert!(!is_git_meta_rel("refs/heads/main.lock"));
    // Object writes are covered by the ref update that follows them.
    assert!(!is_git_meta_rel("objects/ab/cdef"));
    assert!(!is_git_meta_rel("COMMIT_EDITMSG"));
    assert!(!is_git_meta_rel("logs/HEAD"));
}

#[test]
fn changes_change_count_counts_worktree_and_git_directory_changes() {
    use notify_debouncer_mini::DebouncedEvent;

    let root = PathBuf::from("/tmp/proj");
    let git_dir = root.join(".git");
    let ev = |base: &Path, rel: &str| DebouncedEvent {
        path: base.join(rel),
        kind: notify_debouncer_mini::DebouncedEventKind::Any,
    };
    let count = |events: Vec<DebouncedEvent>| {
        changes_change_count(&Ok(events), &root, Some(&git_dir))
    };

    // A working-tree edit alone is enough — a content change alters the
    // diff even though the tree shape is untouched.
    assert_eq!(count(vec![ev(&root, "src/App.tsx")]), 1);

    // A commit moves HEAD and refs with no working-tree change at all.
    assert_eq!(
        count(vec![ev(&git_dir, "HEAD"), ev(&git_dir, "refs/heads/main")]),
        2
    );

    // Lock churn and object writes on their own are not a change.
    assert_eq!(
        count(vec![ev(&git_dir, "index.lock"), ev(&git_dir, "objects/ab/cd")]),
        0
    );

    // Nothing at all -> no event is due.
    assert_eq!(count(vec![]), 0);
    let errored: DebounceEventResult =
        Err(notify_debouncer_mini::notify::Error::generic("watcher failed"));
    assert_eq!(changes_change_count(&errored, &root, Some(&git_dir)), 0);

    // With no repository there is nothing to watch beyond the working tree.
    assert_eq!(
        changes_change_count(&Ok(vec![ev(&git_dir, "HEAD")]), &root, None),
        0
    );
}

#[test]
fn a_co_located_git_directory_still_counts_as_a_change() {
    // CHC-FR-16: in co-located mode the repository lives ABOVE
    // the project root, so a commit touches nothing inside the watched
    // project tree. Keying the Git-directory check off the project root
    // would silently drop every commit and checkout.
    use notify_debouncer_mini::DebouncedEvent;

    let repo_root = PathBuf::from("/tmp/host-repo");
    let root = repo_root.join("apps/web");
    let git_dir = repo_root.join(".git");
    let events = vec![DebouncedEvent {
        path: git_dir.join("refs/heads/feature"),
        kind: notify_debouncer_mini::DebouncedEventKind::Any,
    }];
    assert_eq!(changes_change_count(&Ok(events), &root, Some(&git_dir)), 1);
}

#[test]
fn git_dir_to_watch_only_asks_for_a_second_watch_when_it_is_outside_the_root() {
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::create_dir_all(root.join("apps/web")).unwrap();

    // Standalone: `.git` is already inside the recursive project watch.
    assert_eq!(git_dir_to_watch(&root.join(".git"), root), None);

    // Co-located: the host repository's Git directory is above the project
    // root, so it needs its own watch.
    let nested = root.join("apps/web");
    assert_eq!(
        git_dir_to_watch(&root.join(".git"), &nested),
        Some(canonicalize_lenient(&root.join(".git")))
    );
}
