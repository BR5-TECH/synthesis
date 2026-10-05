//! The path-scoped rollback (GTC-FR-23 .. GTC-FR-28): what it restores, what
//! it removes, and how one failed path leaves the others done.
//!
//! One part of `mod.rs`, which holds the fixtures these run against.

use super::*;

// -----------------------------------------------------------------------
// Path-scoped rollback (GTC-FR-23 – GTC-FR-28)
// -----------------------------------------------------------------------

/// The entry for `path`, so a test reads one outcome rather than an index.
fn rollback_entry<'a>(
    outcome: &'a RollbackOutcome,
    path: &str,
) -> &'a RollbackEntryOutcome {
    outcome
        .entries
        .iter()
        .find(|e| e.path == path)
        .unwrap_or_else(|| panic!("no rollback entry for {path}"))
}

fn rollback(f: &Fixture, paths: &[&str]) -> RollbackOutcome {
    let owned: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
    rollback_named_paths(&crate::fs::RootFs::for_root(f.root()), &owned)
        .unwrap()
        .0
}

#[test]
fn rollback_restores_tracked_paths_and_removes_ones_head_does_not_hold() {
    // GTC-FR-23.
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.write("b.md", "one\n");
    f.write("c.md", "one\n");
    f.write("gone.md", "one\n");
    f.commit("base");

    f.write("a.md", "two\n"); // modified, unstaged
    f.write("b.md", "two\n"); // modified, staged
    f.write("c.md", "two\n"); // staged…
    std::fs::remove_file(f.root().join("gone.md")).unwrap();
    f.write("new.md", "fresh\n");
    f.stage_all();
    f.write("c.md", "three\n"); // …and edited again, unstaged

    let outcome = rollback(
        &f,
        &["a.md", "b.md", "c.md", "gone.md", "new.md"],
    );

    // Every tracked path is back at HEAD, staged and unstaged edits alike.
    for name in ["a.md", "b.md", "c.md", "gone.md"] {
        assert_eq!(
            std::fs::read_to_string(f.root().join(name)).unwrap(),
            "one\n",
            "{name} should hold its HEAD content"
        );
        assert_eq!(rollback_entry(&outcome, name).outcome, ROLLBACK_RESTORED);
    }
    // The staged addition is gone from disk and from the index.
    assert!(!f.root().join("new.md").exists());
    assert_eq!(rollback_entry(&outcome, "new.md").outcome, ROLLBACK_REMOVED);

    // Nothing is left over: the working tree matches HEAD exactly.
    let set = changes::uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap();
    assert!(
        set.entries.is_empty(),
        "the change set should be empty, held {:?}",
        set.entries.iter().map(|e| &e.path).collect::<Vec<_>>()
    );
}

#[test]
fn rollback_removes_an_untracked_file_without_removing_its_parent() {
    // GTC-FR-24.
    let f = Fixture::new();
    f.write("keep.md", "one\n");
    f.commit("base");
    f.write("src/scratch/notes.txt", "draft\n");

    let outcome = rollback(&f, &["src/scratch/notes.txt"]);

    assert!(!f.root().join("src/scratch/notes.txt").exists());
    assert!(
        f.root().join("src/scratch").is_dir(),
        "an emptied parent directory is left where it is (GTC-FR-24)"
    );
    let entry = rollback_entry(&outcome, "src/scratch/notes.txt");
    assert_eq!(entry.outcome, ROLLBACK_REMOVED);
    assert_eq!(entry.removed_paths, vec!["src/scratch/notes.txt".to_string()]);
    assert!(entry.failures.is_empty());
}

#[test]
fn rollback_refuses_a_directory_and_leaves_everything_beneath_it() {
    // GTC-FR-24, second half.
    let f = Fixture::new();
    f.write("keep.md", "one\n");
    f.commit("base");
    f.write("src/scratch/notes.txt", "draft\n");

    let outcome = rollback(&f, &["src/scratch"]);

    let entry = rollback_entry(&outcome, "src/scratch");
    assert_eq!(entry.outcome, ROLLBACK_FAILED);
    assert_eq!(entry.failures[0].kind, ROLLBACK_IS_DIRECTORY);
    assert!(
        f.root().join("src/scratch/notes.txt").exists(),
        "nothing beneath a refused directory is touched"
    );
}

#[test]
fn rollback_is_per_path_so_one_failure_leaves_the_others_restored() {
    // GTC-FR-25: the property the caller relies on to decide whose
    // in-memory buffer it may discard (CHG-FR-63).
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    f.write("a.md", "two\n");

    // A path HEAD does not hold and that is not on disk either: nothing to
    // restore and nothing to remove, so it fails rather than being reported
    // as done.
    let outcome = rollback(&f, &["a.md", "absent.md"]);

    assert_eq!(
        std::fs::read_to_string(f.root().join("a.md")).unwrap(),
        "one\n",
        "the reachable path is restored despite the other failing"
    );
    let ok = rollback_entry(&outcome, "a.md");
    assert_eq!(ok.outcome, ROLLBACK_RESTORED);
    assert_eq!(ok.restored_paths, vec!["a.md".to_string()]);

    let bad = rollback_entry(&outcome, "absent.md");
    assert_eq!(bad.outcome, ROLLBACK_FAILED);
    assert!(
        bad.restored_paths.is_empty() && bad.removed_paths.is_empty(),
        "a failed path appears in neither success list (GTC-FR-25)"
    );
    assert_eq!(bad.failures[0].path, "absent.md");
}

#[test]
fn rollback_undoes_a_rename_at_both_of_its_locations() {
    // GTC-FR-26, GTC-FR-25.
    let f = Fixture::new();
    // Long enough for libgit2 to pair the two halves as a rename, and edited
    // only slightly so the similarity score still clears its threshold.
    let original = "fn main() {\n    // the entry point, long enough to pair as a rename\n    run();\n}\n";
    f.write("src-tauri/main.rs", original);
    f.commit("base");
    std::fs::rename(
        f.root().join("src-tauri/main.rs"),
        f.root().join("src-tauri/lib.rs"),
    )
    .unwrap();
    f.write(
        "src-tauri/lib.rs",
        "fn main() {\n    // the entry point, long enough to pair as a rename\n    run_edited();\n}\n",
    );
    f.stage_all();

    let outcome = rollback(&f, &["src-tauri/lib.rs"]);

    let entry = rollback_entry(&outcome, "src-tauri/lib.rs");
    assert_eq!(
        entry.previous_path.as_deref(),
        Some("src-tauri/main.rs"),
        "both identities are reported (GTC-FR-26)"
    );
    assert_eq!(entry.restored_paths, vec!["src-tauri/main.rs".to_string()]);
    assert_eq!(entry.removed_paths, vec!["src-tauri/lib.rs".to_string()]);
    assert_eq!(entry.outcome, ROLLBACK_RESTORED);

    assert_eq!(
        std::fs::read_to_string(f.root().join("src-tauri/main.rs")).unwrap(),
        original,
        "the pre-rename location holds its HEAD content, edit discarded"
    );
    assert!(!f.root().join("src-tauri/lib.rs").exists());
    let set = changes::uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap();
    assert!(set.entries.is_empty(), "the rename is fully undone");
}

#[test]
fn rollback_validates_before_writing_anything() {
    // GTC-FR-27, GTC-FR-02, GTC-FR-18.
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    f.write("a.md", "two\n");

    assert_eq!(
        rollback_named_paths(&crate::fs::RootFs::for_root(f.root()), &[]).unwrap_err(),
        ERR_NO_PATHS_SELECTED
    );
    assert_eq!(
        std::fs::read_to_string(f.root().join("a.md")).unwrap(),
        "two\n",
        "a refusal writes nothing"
    );

    let outside = TempDir::new().unwrap();
    std::fs::write(outside.path().join("a.md"), "one\n").unwrap();
    assert_eq!(
        rollback_named_paths(
            &crate::fs::RootFs::for_root(outside.path()),
            &["a.md".into()]
        )
        .unwrap_err(),
        ERR_NOT_A_REPO
    );
}

#[test]
fn rollback_reports_an_escaping_path_without_refusing_the_call() {
    // GTC-FR-27, GTC-FR-02, GTC-FR-18: a refusal binding one path alone is that path's failure,
    // so the valid paths beside it are still rolled back (GTC-FR-27).
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    f.write("a.md", "two\n");

    let outcome = rollback(&f, &["../outside.md", "a.md"]);

    let escaped = rollback_entry(&outcome, "../outside.md");
    assert_eq!(escaped.outcome, ROLLBACK_FAILED);
    assert_eq!(escaped.failures[0].kind, ROLLBACK_OUTSIDE_ROOT);
    assert_eq!(
        std::fs::read_to_string(f.root().join("a.md")).unwrap(),
        "one\n",
        "the valid path beside it is still rolled back"
    );
}

#[test]
fn rollback_touches_nothing_it_was_not_given() {
    // GTC-FR-28, CHC-FR-16.
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.write("staged.md", "one\n");
    f.commit("base");
    let head_before = f.repo().head().unwrap().peel_to_commit().unwrap().id();

    f.write("a.md", "two\n");
    f.write("staged.md", "two\n");
    f.stage_all();

    rollback(&f, &["a.md"]);

    // The unnamed path keeps its modification AND its index state.
    assert_eq!(
        std::fs::read_to_string(f.root().join("staged.md")).unwrap(),
        "two\n"
    );
    let repo = f.repo();
    let statuses = repo.statuses(None).unwrap();
    let staged = statuses
        .iter()
        .find(|s| s.path() == Ok("staged.md"))
        .expect("staged.md is still in the status list");
    assert!(
        staged.status().contains(git2::Status::INDEX_MODIFIED),
        "an unnamed staged path stays staged (GTC-FR-28)"
    );
    assert_eq!(
        repo.head().unwrap().peel_to_commit().unwrap().id(),
        head_before,
        "no commit was created and HEAD did not move"
    );
}

#[test]
fn rollback_of_a_path_already_at_head_is_not_a_failure() {
    // GTC-FR-23: a path the working tree and HEAD agree on needs nothing
    // done to it, and reporting it as failed would cost the caller a buffer
    // it was right to discard.
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");

    let outcome = rollback(&f, &["a.md"]);

    let entry = rollback_entry(&outcome, "a.md");
    assert_eq!(entry.outcome, ROLLBACK_RESTORED);
    assert!(entry.failures.is_empty());
}
