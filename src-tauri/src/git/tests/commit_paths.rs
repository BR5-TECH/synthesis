//! The path-scoped commit (GTC-FR-19 / GTC-FR-20): which paths it commits,
//! which it leaves alone, and when it refuses.
//!
//! One part of `mod.rs`, which holds the fixtures these run against.

use super::*;

// -----------------------------------------------------------------------
// Path-scoped commit (GTC-FR-19 / GTC-FR-20)
// -----------------------------------------------------------------------

/// The repository-relative paths the index holds that differ from `HEAD` —
/// what `git status` calls the staged set.
fn staged_paths(f: &Fixture) -> Vec<String> {
    let repo = f.repo();
    let tree = head_tree(&repo);
    let diff = repo
        .diff_tree_to_index(tree.as_ref(), None, None)
        .unwrap();
    let mut paths: Vec<String> = diff
        .deltas()
        .filter_map(|d| {
            d.new_file()
                .path()
                .or_else(|| d.old_file().path())
                .map(|p| p.to_string_lossy().into_owned())
        })
        .collect();
    paths.sort();
    paths
}

/// The paths the commit at `HEAD` changed relative to its parent.
fn commit_touched(f: &Fixture) -> Vec<String> {
    let repo = f.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let parent = head.parent(0).ok().map(|c| c.tree().unwrap());
    let diff = repo
        .diff_tree_to_tree(parent.as_ref(), Some(&head.tree().unwrap()), None)
        .unwrap();
    let mut paths: Vec<String> = diff
        .deltas()
        .flat_map(|d| {
            [d.old_file().path(), d.new_file().path()]
                .into_iter()
                .flatten()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        })
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

/// The blob text at `path` in the commit at `HEAD`, or `None` when the
/// commit's tree has no such path.
fn head_text(f: &Fixture, path: &str) -> Option<String> {
    let repo = f.repo();
    let tree = repo.head().unwrap().peel_to_commit().unwrap().tree().unwrap();
    let entry = tree.get_path(Path::new(path)).ok()?;
    let blob = repo.find_blob(entry.id()).ok()?;
    Some(String::from_utf8_lossy(blob.content()).into_owned())
}

#[test]
fn commit_paths_commits_only_the_named_paths_and_adds_untracked_ones() {
    // GTC-FR-19.
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.write("b.md", "one\n");
    f.commit("base");
    f.write("a.md", "two\n");
    f.write("b.md", "two\n");
    f.write("new.md", "fresh\n");

    commit_named_paths(&crate::fs::RootFs::for_root(f.root()), "msg", &["new.md".into(), "a.md".into()], None).unwrap();

    assert_eq!(commit_touched(&f), vec!["a.md".to_string(), "new.md".into()]);
    assert_eq!(head_text(&f, "a.md").as_deref(), Some("two\n"));
    assert_eq!(head_text(&f, "new.md").as_deref(), Some("fresh\n"));

    // `new.md` is tracked afterwards, and `b.md` is still uncommitted.
    let set = changes::uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap();
    let names: Vec<&str> = set.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(names, vec!["b.md"], "only the unnamed change is left");
}

#[test]
fn commit_paths_leaves_an_unnamed_staged_path_staged() {
    // GTC-FR-19: the commit is built from HEAD's tree, not from the index,
    // which is what keeps a path the author did not tick out of it.
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.write("staged.md", "one\n");
    f.commit("base");
    f.write("a.md", "two\n");
    f.write("staged.md", "two\n");
    f.stage_all();

    commit_named_paths(&crate::fs::RootFs::for_root(f.root()), "msg", &["a.md".into()], None).unwrap();

    assert_eq!(commit_touched(&f), vec!["a.md".to_string()]);
    assert_eq!(
        staged_paths(&f),
        vec!["staged.md".to_string()],
        "the unnamed path is still staged and still uncommitted"
    );
    assert_eq!(head_text(&f, "staged.md").as_deref(), Some("one\n"));
}

#[test]
fn commit_paths_records_a_deletion_and_a_rename_at_both_locations() {
    // GTC-FR-19, GTC-FR-20.
    let f = Fixture::new();
    f.write("gone.md", "bye\n");
    f.write("old.md", "content that is long enough to pair as a rename\n");
    f.commit("base");
    std::fs::remove_file(f.root().join("gone.md")).unwrap();
    std::fs::rename(f.root().join("old.md"), f.root().join("new.md")).unwrap();

    let outcome = commit_named_paths(
        &crate::fs::RootFs::for_root(f.root()),
        "msg",
        &["gone.md".into(), "new.md".into()],
        None,
    )
    .unwrap();

    // GTC-FR-19: the outcome names every path the commit RECORDED, which is
    // the named set plus the rename's previous location — what the strip
    // closes Diff tabs from (TAB-FR-22), and not something it could have
    // re-derived from the two paths it submitted.
    assert!(!outcome.commit_id.is_empty());
    let mut recorded = outcome.committed_paths.clone();
    recorded.sort();
    assert_eq!(
        recorded,
        vec!["gone.md".to_string(), "new.md".to_string(), "old.md".to_string()]
    );

    assert_eq!(head_text(&f, "gone.md"), None, "recorded as a deletion");
    assert_eq!(head_text(&f, "old.md"), None, "the rename's old location");
    assert!(head_text(&f, "new.md").is_some(), "and its new one");
    assert!(
        changes::uncommitted_change_set(&crate::fs::RootFs::for_root(f.root()))
            .unwrap()
            .entries
            .is_empty(),
        "nothing is left uncommitted"
    );
}

#[test]
fn commit_paths_refuses_before_it_writes_anything() {
    // GTC-FR-18, GTC-FR-20: each refusal leaves the index, the refs, and the working
    // tree exactly as they were.
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.write("unchanged.md", "same\n");
    f.write("staged.md", "one\n");
    f.commit("base");
    f.write("a.md", "two\n");
    // A path staged beforehand, so "leaves the index exactly as it was" is
    // a claim about something rather than about an empty index.
    f.write("staged.md", "two\n");
    f.stage_all();
    let head_before = f.repo().head().unwrap().target().unwrap();
    let index_before = staged_paths(&f);
    assert_eq!(index_before, vec!["a.md".to_string(), "staged.md".into()]);

    // Each refusal is checked on its own, so one that disturbed the index
    // cannot hide behind a later one that restored it.
    let refusals: Vec<(&str, Vec<String>)> = vec![
        ("   \n", vec!["a.md".into()]),
        ("msg", vec![]),
        ("msg", vec!["unchanged.md".into()]),
        // GTC-FR-18: a path escaping the content root is refused, not read.
        ("msg", vec!["../outside.md".into()]),
    ];
    let mut errors = Vec::new();
    for (message, paths) in refusals {
        errors.push(commit_named_paths(&crate::fs::RootFs::for_root(f.root()), message, &paths, None).unwrap_err());
        assert_eq!(f.repo().head().unwrap().target().unwrap(), head_before);
        assert_eq!(staged_paths(&f), index_before, "the index was disturbed");
        assert_eq!(
            std::fs::read_to_string(f.root().join("a.md")).unwrap(),
            "two\n"
        );
    }
    assert_eq!(errors[0], ERR_EMPTY_COMMIT_MESSAGE);
    assert_eq!(errors[1], ERR_NO_PATHS_SELECTED);
    assert_eq!(errors[2], ERR_NOTHING_TO_COMMIT);
    // GTC-FR-18: the traversal is refused rather than read. The refusal
    // echoes the caller's own argument and nothing it learned by looking,
    // so it discloses nothing about what is outside the root.
    assert!(errors[3].contains("escapes"), "{}", errors[3]);
    assert!(
        !errors[3].contains("secret"),
        "no content of the refused path is read or reported"
    );
}

#[test]
fn commit_paths_survives_a_rename_rotation_without_losing_a_file() {
    // GTC-FR-19: the author moves `old.md` to `new.md` and puts `third.md`
    // in its place. Whatever Git makes of that — a rename pair, or a
    // modification plus an addition plus a deletion — every named path must
    // land in the commit with the content it has on disk, and no path may be
    // dropped on another entry's behalf.
    let f = Fixture::new();
    let long = "content long enough for libgit2 to pair it as a rename\n";
    f.write("old.md", &format!("first {long}"));
    f.write("third.md", &format!("third {long}"));
    f.commit("base");
    std::fs::remove_file(f.root().join("old.md")).unwrap();
    std::fs::rename(f.root().join("third.md"), f.root().join("old.md")).unwrap();
    std::fs::write(
        f.root().join("new.md"),
        format!("first {long}"),
    )
    .unwrap();

    commit_named_paths(
        &crate::fs::RootFs::for_root(f.root()),
        "rotate",
        &["new.md".into(), "old.md".into(), "third.md".into()],
        None,
    )
    .unwrap();

    assert_eq!(
        head_text(&f, "new.md").as_deref(),
        Some(format!("first {long}").as_str()),
        "the first rename landed"
    );
    assert_eq!(
        head_text(&f, "old.md").as_deref(),
        Some(format!("third {long}").as_str()),
        "and the file rotated INTO old.md is still in the commit"
    );
    assert_eq!(head_text(&f, "third.md"), None, "its previous location is gone");
    assert!(
        changes::uncommitted_change_set(&crate::fs::RootFs::for_root(f.root()))
            .unwrap()
            .entries
            .is_empty(),
        "nothing is left uncommitted"
    );
}

#[test]
fn commit_paths_commits_a_project_rooted_below_the_repository_root() {
    // GTC-FR-02: a co-located project sits below the repository root, so
    // every named path carries a prefix on its way to the index and the
    // tree. Nothing outside the project is touched.
    let f = Fixture::new();
    f.write("outside.md", "host\n");
    f.write("project/a.md", "one\n");
    f.commit("base");
    let root = f.root().join("project");
    f.write("project/a.md", "two\n");
    f.write("project/new.md", "fresh\n");
    f.write("outside.md", "host edited\n");

    let outcome =
        commit_named_paths(&crate::fs::RootFs::for_root(&root), "msg", &["a.md".into(), "new.md".into()], None).unwrap();

    // GTC-FR-19: reported project-relative, the way every path crossing this
    // boundary is — the frontend names files that way and could match a
    // repository-relative path against nothing it holds.
    let mut recorded = outcome.committed_paths.clone();
    recorded.sort();
    assert_eq!(recorded, vec!["a.md".to_string(), "new.md".to_string()]);

    assert_eq!(commit_touched(&f), vec!["project/a.md".to_string(), "project/new.md".into()]);
    assert_eq!(head_text(&f, "project/a.md").as_deref(), Some("two\n"));
    assert_eq!(head_text(&f, "project/new.md").as_deref(), Some("fresh\n"));
    // The host file was never named, so it is still modified and uncommitted.
    assert_eq!(head_text(&f, "outside.md").as_deref(), Some("host\n"));
    // And a path escaping the project root is still refused from here.
    assert!(commit_named_paths(&crate::fs::RootFs::for_root(&root), "msg", &["../outside.md".into()], None).is_err());
}

#[test]
fn commit_paths_is_nothing_to_commit_for_a_path_that_does_not_change_anything() {
    // GTC-FR-20: a path present in neither HEAD nor the working tree, and a
    // duplicate of a real one, are both handled without a spurious commit or
    // a duplicated tree entry.
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");

    assert_eq!(
        commit_named_paths(&crate::fs::RootFs::for_root(f.root()), "msg", &["ghost.md".into()], None).unwrap_err(),
        ERR_NOTHING_TO_COMMIT
    );

    f.write("a.md", "two\n");
    commit_named_paths(&crate::fs::RootFs::for_root(f.root()), "msg", &["a.md".into(), "a.md".into()], None).unwrap();
    assert_eq!(commit_touched(&f), vec!["a.md".to_string()]);
    assert_eq!(head_text(&f, "a.md").as_deref(), Some("two\n"));
}

#[test]
fn commit_paths_records_an_executable_bit_change() {
    // GTC-FR-19: the mode goes into the tree with the content, so a script
    // committed from here is executable when it is checked out again.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let f = Fixture::new();
        f.write("run.sh", "#!/bin/sh\necho hi\n");
        f.commit("base");
        let path = f.root().join("run.sh");
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();

        commit_named_paths(&crate::fs::RootFs::for_root(f.root()), "chmod", &["run.sh".into()], None).unwrap();

        let repo = f.repo();
        let tree = repo.head().unwrap().peel_to_commit().unwrap().tree().unwrap();
        let entry = tree.get_path(Path::new("run.sh")).unwrap();
        assert_eq!(entry.filemode(), i32::from(git2::FileMode::BlobExecutable));
    }
}

#[test]
fn commit_paths_takes_its_identity_from_the_git_configuration() {
    // GTC-FR-03.
    let f = Fixture::new();
    f.write("a.md", "one\n");
    commit_named_paths(&crate::fs::RootFs::for_root(f.root()), "first", &["a.md".into()], None).unwrap();
    let repo = f.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.author().name().unwrap(), "Test");
    assert_eq!(head.author().email().unwrap(), "test@example.com");
    assert_eq!(head.message().unwrap(), "first");
}

#[test]
fn commit_paths_outside_a_repository_is_the_typed_error() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("a.md"), "one\n").unwrap();
    assert_eq!(
        commit_named_paths(&crate::fs::RootFs::for_root(dir.path()), "msg", &["a.md".into()], None).unwrap_err(),
        ERR_NOT_A_REPO
    );
}
