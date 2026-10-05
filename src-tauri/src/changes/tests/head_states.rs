//! An unborn HEAD, a detached HEAD, and what counts as a branch
//! (CHC-FR-05, CHC-FR-15).
//!
//! One part of `../tests/mod.rs`, which holds the fixture these run against.

use super::*;

// -----------------------------------------------------------------------
// Unborn HEAD — a repository with no commits yet
// -----------------------------------------------------------------------

#[test]
fn an_unborn_head_reports_the_whole_working_tree_as_new() {
    let f = Fixture::new();
    f.write("first.md", "one\ntwo\n");

    let set = uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).expect("no commits is not an error");
    let e = entry(&set, "first.md");
    assert_eq!(e.change_status, ChangeStatus::Untracked);
    assert_eq!((e.added_lines, e.removed_lines), (Some(2), Some(0)));
}

#[test]
fn an_unborn_head_has_no_branches_to_compare_against() {
    let f = Fixture::new();
    f.write("first.md", "one\n");

    // No branch exists yet, so any target is unknown rather than a crash.
    assert_eq!(
        branch_change_set(&crate::fs::RootFs::for_root(f.root()), "main").unwrap_err(),
        ERR_UNKNOWN_BRANCH
    );
    assert_eq!(comparison_branches(&f.root()).unwrap(), vec![]);
    // The resolver still answers rather than erroring, so the panel renders
    // its picker instead of failing outright. The *value* it answers with is
    // pinned by `the_current_branch_fallback_never_reports_the_literal_head`;
    // asserting it here would only be re-reading the same literal.
    assert!(default_branch(&f.repo()).is_ok());
}

// -----------------------------------------------------------------------
// Detached HEAD, beyond the change set
// -----------------------------------------------------------------------

#[test]
fn a_detached_head_flags_no_branch_as_current() {
    // The branch is deliberately named `HEAD`-free but the fixture detaches,
    // so `repo.head().shorthand()` is the literal "HEAD". A picker that did
    // not filter on `is_branch()` would still flag nothing current here —
    // so the real bite is the assertion that the branches ARE offered and
    // that the current-branch lookup did not simply fail.
    let f = Fixture::new();
    f.write("a.md", "a\n");
    f.commit("A");
    f.branch("main");
    f.branch("HEAD-ish");
    f.detach();

    let options = comparison_branches(&f.root()).unwrap();
    assert!(!options.is_empty(), "the branches are still offered");
    assert!(
        options.iter().all(|b| !b.is_current),
        "a detached HEAD is on no branch: {options:?}"
    );
    assert!(
        options.iter().any(|b| b.name == "main" && b.is_default),
        "and the default is still flagged: {options:?}"
    );
}

#[test]
fn the_current_branch_fallback_never_reports_the_literal_head() {
    // CHC-FR-13 step (4) in isolation: no remote, no `main`, no `master`, so
    // the resolver reaches the current branch. Attached, it is that branch's
    // name; detached, `shorthand()` yields the literal "HEAD", which is not
    // a branch anyone can compare against — the filter is the only thing
    // standing between the picker and a target that cannot resolve.
    let f = Fixture::new();
    f.write("a.md", "a\n");
    f.commit("A");
    f.branch("work");
    f.checkout("work");
    // Remove whatever branch `git init` created, so only `work` remains.
    let initial = {
        let repo = f.repo();
        let names: Vec<String> = repo
            .branches(Some(BranchType::Local))
            .unwrap()
            .filter_map(|b| b.ok())
            .filter_map(|(b, _)| b.name().ok().flatten().map(|n| n.to_string()))
            .filter(|n| n != "work")
            .collect();
        names
    };
    for name in initial {
        let repo = f.repo();
        let mut branch = repo.find_branch(&name, BranchType::Local).unwrap();
        branch.delete().unwrap();
    }

    assert_eq!(
        default_branch(&f.repo()).unwrap(),
        "work",
        "with no remote and no main/master, the current branch stands in"
    );

    f.detach();
    let detached = default_branch(&f.repo()).unwrap();
    assert_ne!(detached, "HEAD", "\"HEAD\" is not a comparison target");
    assert_eq!(detached, "main", "the conventional default stands in instead");
}

// -----------------------------------------------------------------------
// CHC-FR-15 — only a branch is a comparison target
// -----------------------------------------------------------------------

#[test]
fn a_tag_or_a_raw_commit_id_is_not_a_branch() {
    let f = Fixture::new();
    f.write("a.md", "a\n");
    let oid = f.commit("A");
    {
        let repo = f.repo();
        let obj = repo.find_object(oid, None).unwrap();
        repo.tag_lightweight("v1", &obj, false).unwrap();
    }

    assert_eq!(
        branch_change_set(&crate::fs::RootFs::for_root(f.root()), "v1").unwrap_err(),
        ERR_UNKNOWN_BRANCH,
        "a tag does not resolve to a branch in the repository (CHC-FR-15)"
    );
    assert_eq!(
        branch_change_set(&crate::fs::RootFs::for_root(f.root()), &oid.to_string()).unwrap_err(),
        ERR_UNKNOWN_BRANCH,
        "nor does a raw commit id"
    );
}
