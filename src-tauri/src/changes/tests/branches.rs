//! The default branch, the picker, and the typed branch errors
//! (CHC-FR-13, CHC-FR-14, CHC-FR-15).
//!
//! One part of `../tests/mod.rs`, which holds the fixture these run against.

use super::*;

// -----------------------------------------------------------------------
// CHC-FR-13 / CHC-FR-14 — default branch and the picker
// -----------------------------------------------------------------------

#[test]
fn default_branch_prefers_the_remote_head_then_main_then_master() {
    // (2)/(3): with no remote, a local `main` wins, and `master` stands in
    // when there is no `main`.
    let f = Fixture::new();
    f.write("a.md", "a\n");
    f.commit("A");
    // `git init` names the initial branch from the ambient configuration,
    // so it may already be `master` (or `main`) here.
    let initial = f.current_branch();
    if initial != "master" {
        f.branch("master");
    }
    assert_eq!(default_branch(&f.repo()).unwrap(), "master");
    if initial != "main" {
        f.branch("main");
    }
    assert_eq!(
        default_branch(&f.repo()).unwrap(),
        "main",
        "`main` outranks `master`"
    );

    // (1): a remote publishing HEAD outranks both.
    {
        let repo = f.repo();
        repo.remote("origin", "https://example.invalid/repo.git")
            .unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.reference("refs/remotes/origin/trunk", head.id(), true, "test")
            .unwrap();
        repo.reference_symbolic(
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/trunk",
            true,
            "test",
        )
        .unwrap();
    }
    assert_eq!(default_branch(&f.repo()).unwrap(), "trunk");
}

#[test]
fn comparison_branches_flag_current_and_default() {
    let f = Fixture::new();
    f.write("a.md", "a\n");
    f.commit("A");
    f.branch("main");
    f.branch("feature/x");
    f.checkout("feature/x");

    let options = comparison_branches(&f.root()).unwrap();
    let feature = options
        .iter()
        .find(|b| b.name == "feature/x")
        .expect("the current branch is offered");
    assert!(feature.is_current);
    assert!(!feature.is_default);
    let main = options
        .iter()
        .find(|b| b.name == "main")
        .expect("the default branch is offered");
    assert!(main.is_default);
    assert!(!main.is_current);
}

#[test]
fn comparison_branches_include_remote_tracking_branches() {
    // CHC-FR-14: the picker offers local AND remote-tracking branches. The
    // symbolic `origin/HEAD` pointer is not a target and must be excluded.
    let f = Fixture::new();
    f.write("a.md", "a\n");
    f.commit("A");
    {
        let repo = f.repo();
        repo.remote("origin", "https://example.invalid/repo.git").unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.reference("refs/remotes/origin/main", head.id(), true, "test")
            .unwrap();
        repo.reference("refs/remotes/origin/release", head.id(), true, "test")
            .unwrap();
        repo.reference_symbolic(
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
            true,
            "test",
        )
        .unwrap();
    }

    let options = comparison_branches(&f.root()).unwrap();
    let names: Vec<&str> = options.iter().map(|b| b.name.as_str()).collect();
    assert!(names.contains(&"origin/main"), "{names:?}");
    assert!(names.contains(&"origin/release"), "{names:?}");
    assert!(
        !names.iter().any(|n| n.ends_with("/HEAD")),
        "the symbolic remote HEAD pointer is not a comparison target: {names:?}"
    );
    // A remote-tracking branch is never the checked-out one.
    assert!(
        options
            .iter()
            .filter(|b| b.name.starts_with("origin/"))
            .all(|b| !b.is_current),
        "{options:?}"
    );
    // Local branches sort ahead of remote-tracking ones.
    let first_remote = names.iter().position(|n| n.starts_with("origin/")).unwrap();
    assert!(
        names[..first_remote].iter().all(|n| !n.starts_with("origin/")),
        "local branches lead: {names:?}"
    );
}

#[test]
fn branch_sort_key_puts_local_branches_before_remote_ones() {
    assert!(branch_sort_key("zeta", false) < branch_sort_key("alpha", true));
    assert!(branch_sort_key("alpha", false) < branch_sort_key("beta", false));
}

// -----------------------------------------------------------------------
// CHC-FR-15 — typed branch errors
// -----------------------------------------------------------------------

#[test]
fn unknown_branch_and_no_merge_base_are_typed_errors() {
    let f = Fixture::new();
    f.write("a.md", "a\n");
    f.commit("A");
    assert_eq!(
        branch_change_set(&crate::fs::RootFs::for_root(f.root()), "does-not-exist").unwrap_err(),
        ERR_UNKNOWN_BRANCH
    );

    // An orphan branch shares no ancestor with the current one.
    {
        let repo = f.repo();
        repo.set_head("refs/heads/orphan").unwrap();
        let mut index = repo.index().unwrap();
        index.clear().unwrap();
        std::fs::write(f.root().join("orphan.md"), "o\n").unwrap();
        index.add_path(Path::new("orphan.md")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "orphan", &tree, &[])
            .unwrap();
    }
    // Back on the original branch, compare against the unrelated orphan.
    f.checkout("orphan");
    let err = branch_change_set(&crate::fs::RootFs::for_root(f.root()), "main")
        .or_else(|_| branch_change_set(&crate::fs::RootFs::for_root(f.root()), "master"))
        .unwrap_err();
    assert_eq!(err, ERR_NO_MERGE_BASE);
}
