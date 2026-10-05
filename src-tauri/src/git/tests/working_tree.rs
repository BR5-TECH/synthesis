//! The working-tree status (GTC-FR-29 .. GTC-FR-32) and the worktree
//! identity guard the status read and the commit both take.
//!
//! One part of `mod.rs`, which holds the fixtures these run against.

use super::*;

// -----------------------------------------------------------------------
// Working-tree status (GTC-FR-29 … GTC-FR-32)
// -----------------------------------------------------------------------

/// A worktree holding one of every shape GTC-FR-30 names, plus a file the
/// repository's ignore rules exclude.
fn dirty_fixture() -> Fixture {
    let f = Fixture::new();
    // Distinct content per file, so Git's rename detection pairs the one
    // rename this fixture makes and does not read the deletion and the
    // untracked file as a second one.
    f.write(".gitignore", "ignored.md\n");
    f.write("staged.md", "staged one\n");
    f.write("twice.md", "twice one\n");
    f.write("unstaged.md", "unstaged one\n");
    f.write("gone.md", "gone one\n");
    f.write("old.md", "old one\n");
    f.commit("base");

    // A staged modification, and nothing beside it.
    f.write("staged.md", "staged two\n");
    // Staged and then changed again — the two-letter state neither side
    // alone describes.
    f.write("twice.md", "twice two\n");
    f.stage_all();
    f.write("twice.md", "twice three\n");
    // An unstaged modification.
    f.write("unstaged.md", "unstaged two\n");
    // A tracked file deleted in the working tree.
    std::fs::remove_file(f.root().join("gone.md")).unwrap();
    // A rename, staged so Git pairs the two halves.
    std::fs::rename(f.root().join("old.md"), f.root().join("new.md")).unwrap();
    {
        let repo = f.repo();
        let mut index = repo.index().unwrap();
        index.remove_path(Path::new("old.md")).unwrap();
        index.add_path(Path::new("new.md")).unwrap();
        index.write().unwrap();
    }
    // Untracked, and ignored.
    f.write("untracked.md", "untracked one\n");
    f.write("ignored.md", "ignored one\n");
    f
}

fn entry_named<'a>(entries: &'a [WorkingTreeEntry], path: &str) -> &'a WorkingTreeEntry {
    entries
        .iter()
        .find(|e| e.path == path)
        .unwrap_or_else(|| panic!("no entry for {path}: {entries:?}"))
}

/// GTC-FR-29, GTC-FR-30, GTC-FR-32, GTC-FR-02, GTC-FR-06.
#[test]
fn working_tree_status_keeps_both_sides_of_the_index_apart() {
    let f = dirty_fixture();
    let root = crate::fs::RootFs::for_root(f.root());

    let before = f.repo().head().unwrap().target().unwrap();
    let index_before = std::fs::read(f.root().join(".git/index")).unwrap();
    let unstaged_before = std::fs::read(f.root().join("unstaged.md")).unwrap();

    let status = working_tree_status_reported(
        &NullSink,
        own_buffer(),
        &root,
        None,
    )
    .expect("the status reads");

    assert_eq!(
        changes::canonicalize_lenient(Path::new(&status.worktree_path)),
        changes::canonicalize_lenient(&f.root()),
        "the worktree the status describes travels with it (GTC-FR-29)"
    );
    let paths: Vec<&str> = status.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "gone.md",
            "new.md",
            "staged.md",
            "twice.md",
            "unstaged.md",
            "untracked.md"
        ],
        "exactly six entries, and the ignored file is absent"
    );

    let staged = entry_named(&status.entries, "staged.md");
    assert_eq!(staged.staged_status, Some(PathStatus::Modified));
    assert_eq!(staged.unstaged_status, None, "one side alone stays one side");

    let twice = entry_named(&status.entries, "twice.md");
    assert_eq!(twice.staged_status, Some(PathStatus::Modified));
    assert_eq!(
        twice.unstaged_status,
        Some(PathStatus::Modified),
        "a path staged and then changed again carries both (GTC-FR-30)"
    );

    let unstaged = entry_named(&status.entries, "unstaged.md");
    assert_eq!(unstaged.staged_status, None);
    assert_eq!(unstaged.unstaged_status, Some(PathStatus::Modified));

    let gone = entry_named(&status.entries, "gone.md");
    assert_eq!(gone.staged_status, None);
    assert_eq!(gone.unstaged_status, Some(PathStatus::Deleted));

    let renamed = entry_named(&status.entries, "new.md");
    assert_eq!(renamed.staged_status, Some(PathStatus::Renamed));
    assert_eq!(
        renamed.previous_path.as_deref(),
        Some("old.md"),
        "a rename carries its pre-rename path"
    );

    let untracked = entry_named(&status.entries, "untracked.md");
    assert_eq!(
        untracked.unstaged_status,
        Some(PathStatus::Untracked),
        "an untracked path carries `untracked` on the unstaged side alone"
    );
    assert_eq!(untracked.staged_status, None);

    // GTC-FR-29: complete and unclassified — the entry shape holds no
    // artifact type and no lens, so there is nothing a filter could withhold.
    let wire = serde_json::to_string(&status).unwrap();
    assert!(!wire.contains("artifactType"), "{wire}");
    assert!(!wire.contains("ignored.md"), "{wire}");

    // GTC-FR-06 / GTC-FR-32: read-only.
    assert_eq!(f.repo().head().unwrap().target().unwrap(), before);
    assert_eq!(
        std::fs::read(f.root().join(".git/index")).unwrap(),
        index_before
    );
    assert_eq!(
        std::fs::read(f.root().join("unstaged.md")).unwrap(),
        unstaged_before
    );
}

/// GTC-FR-29, GTC-FR-30, GTC-FR-32, GTC-FR-02, GTC-FR-06: the status names how many paths it read and no path of the set.
#[test]
fn working_tree_status_records_a_count_and_no_path() {
    let f = dirty_fixture();
    let root = crate::fs::RootFs::for_root(f.root());
    let buffer = own_buffer();
    working_tree_status_reported(&NullSink, buffer, &root, None).unwrap();
    let record = record_for(buffer, "working tree status read");
    assert_eq!(
        record.fields.get("entryCount"),
        Some(&serde_json::json!(6))
    );
    let text = buffer_text(buffer);
    assert!(!text.contains("unstaged.md"), "{text}");
    assert!(!text.contains("untracked.md"), "{text}");
}

/// GTC-FR-29 / GSU-FR-MLEJ: an unresolved conflict is uncommitted state, so a
/// worktree stopped mid-merge is never read as clean.
///
/// libgit2 reports such a path as `CONFLICTED` alone, with no `WT_` bit
/// beside it — so without the arm for it the entry would be dropped, the
/// set would come back empty, and the graduation preflight would create a
/// run against a conflicted checkout.
#[test]
fn a_conflicted_path_is_reported_rather_than_dropped() {
    let f = Fixture::new();
    f.write("a.md", "base\n");
    f.commit("base");
    let base = {
        let repo = f.repo();
        let id = repo.head().unwrap().peel_to_commit().unwrap().id();
        id
    };

    // Two branches that change the same line, merged into a conflict.
    f.branch_and_checkout("theirs");
    f.write("a.md", "theirs\n");
    f.commit("theirs");
    let theirs = {
        let repo = f.repo();
        let id = repo.head().unwrap().peel_to_commit().unwrap().id();
        id
    };

    {
        let repo = f.repo();
        repo.set_head_detached(base).unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
    }
    f.write("a.md", "ours\n");
    f.commit("ours");
    {
        let repo = f.repo();
        let annotated = repo.find_annotated_commit(theirs).unwrap();
        let _ = repo.merge(&[&annotated], None, None);
    }

    let root = crate::fs::RootFs::for_root(f.root());
    let entries = working_tree_status(&root).expect("the status reads");
    let conflicted = entries
        .iter()
        .find(|e| e.path == "a.md")
        .unwrap_or_else(|| panic!("the conflicted path is absent: {entries:?}"));
    assert!(
        conflicted.staged_status.is_some() || conflicted.unstaged_status.is_some(),
        "at least one side is non-null (GTC-FR-30): {conflicted:?}",
    );
    assert!(
        !entries.is_empty(),
        "a worktree holding a conflict is never read as clean",
    );
}

/// GTC-FR-29: a project nested below the repository root reports its own
/// paths in its own coordinates, and a path outside it is not reported at
/// all — there being no project-relative spelling for one.
#[test]
fn a_nested_project_reports_its_own_paths_alone() {
    let f = Fixture::new();
    f.write("project/a.md", "one\n");
    f.write("outside.md", "one\n");
    f.commit("base");
    f.write("project/a.md", "two\n");
    f.write("outside.md", "two\n");

    let root = crate::fs::RootFs::for_root(f.root().join("project"));
    let entries = working_tree_status(&root).expect("the status reads");
    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        vec!["a.md"],
        "project-relative, and the sibling outside the project is not named",
    );
}

/// GTC-FR-31: the guard compares **identities**, not spellings.
///
/// A caller carries the path back through the frontend and returns it as it
/// was given rather than as libgit2 canonicalises it — a symlinked temp root
/// and a trailing separator are the two everyday shapes — and refusing those
/// would break the loop GSU-FR-MLEJ describes for a worktree that never moved.
#[test]
fn the_guard_accepts_the_same_checkout_spelled_differently() {
    let f = dirty_fixture();
    let root = crate::fs::RootFs::for_root(f.root());
    let active = crate::worktree::active_entry_for(&f.root()).unwrap().path;

    for spelling in [
        format!("{active}/"),
        f.root().to_string_lossy().into_owned(),
    ] {
        let status =
            working_tree_status_reported(&NullSink, own_buffer(), &root, Some(&spelling))
                .unwrap_or_else(|e| panic!("{spelling:?} names the active worktree: {e}"));
        assert_eq!(status.entries.len(), 6);
    }
}

/// GTC-FR-29: a clean worktree reports an empty set, which is the whole of
/// what `GRD-graduation.md`'s preflight decides on (GSU-FR-MLEJ).
#[test]
fn a_clean_worktree_reports_no_entries() {
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    let root = crate::fs::RootFs::for_root(f.root());
    assert!(working_tree_status(&root).expect("the status reads").is_empty());
}

/// GTC-FR-20: a caller that supplies no expectation is served exactly as
/// before, the guard resolving nothing — so a project outside a repository
/// still reaches the message and path checks in that order.
#[test]
fn an_unbound_commit_resolves_no_worktree_and_keeps_its_check_order() {
    let outside = TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(outside.path());
    assert_eq!(
        commit_named_paths(&root, "", &["a.md".into()], None).unwrap_err(),
        ERR_EMPTY_COMMIT_MESSAGE,
        "the message check still comes first outside a repository",
    );
    // With an expectation the guard runs first, and outside a repository
    // there is no active worktree to compare against (GTC-FR-02).
    assert_eq!(
        commit_named_paths(&root, "", &["a.md".into()], Some("/somewhere"))
            .unwrap_err(),
        ERR_NOT_A_REPO,
    );
}

/// GTC-FR-31, GTC-FR-20, GTC-FR-19: the identity guard, on both operations that take it.
#[test]
fn a_changed_worktree_refuses_the_status_read_and_the_commit() {
    let f = dirty_fixture();
    let root = crate::fs::RootFs::for_root(f.root());
    let active = crate::worktree::active_entry_for(&f.root()).unwrap().path;

    // The identity the caller holds is the active one: served normally.
    let status =
        working_tree_status_reported(&NullSink, own_buffer(), &root, Some(&active))
            .expect("the read is bound to the checkout it describes");
    assert_eq!(status.entries.len(), 6);

    // A different checkout: refused, having read nothing.
    let elsewhere = f.root().join("somewhere-else");
    let error = working_tree_status_reported(
        &NullSink,
        own_buffer(),
        &root,
        Some(&elsewhere.to_string_lossy()),
    )
    .expect_err("a moved checkout is refused");
    assert!(is_worktree_identity_changed(&error), "{error}");
    assert!(error.contains("somewhere-else"), "{error}");
    assert!(error.contains(&active), "{error}");

    // GTC-FR-20: ahead of the message and path checks — the message here is
    // empty and the path list names nothing that differs from HEAD, and it is
    // still the identity that is reported.
    let index_before = std::fs::read(f.root().join(".git/index")).unwrap();
    let head_before = f.repo().head().unwrap().target().unwrap();
    let error = commit_named_paths(
        &root,
        "",
        &[],
        Some(&elsewhere.to_string_lossy()),
    )
    .expect_err("the commit is refused");
    assert!(is_worktree_identity_changed(&error), "{error}");
    assert_eq!(f.repo().head().unwrap().target().unwrap(), head_before);
    assert_eq!(
        std::fs::read(f.root().join(".git/index")).unwrap(),
        index_before
    );

    // The same commit with no expectation at all commits exactly as before.
    let outcome = commit_named_paths(&root, "msg", &["unstaged.md".into()], None)
        .expect("an unbound commit is served as it otherwise would be");
    assert_eq!(outcome.committed_paths, vec!["unstaged.md".to_string()]);
}
