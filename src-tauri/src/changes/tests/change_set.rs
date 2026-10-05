//! What the change set holds and how each entry is counted
//! (CHC-FR-02 to CHC-FR-08).
//!
//! One part of `../tests/mod.rs`, which holds the fixture these run against.

use super::*;

// -----------------------------------------------------------------------
// CHC-FR-02 — a non-repository is a typed error
// -----------------------------------------------------------------------

#[test]
fn commands_report_not_a_git_repository_outside_a_repo() {
    // CHC-FR-02: every command returns the same typed error, and none of
    // them panics or falls back to an empty change set.
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    assert_eq!(uncommitted_change_set(root).unwrap_err(), ERR_NOT_A_REPO);
    assert_eq!(
        branch_change_set(root, "main").unwrap_err(),
        ERR_NOT_A_REPO
    );
    assert_eq!(comparison_branches(root).unwrap_err(), ERR_NOT_A_REPO);
    assert!(open_repo(root).is_err());
}

// -----------------------------------------------------------------------
// CHC-FR-03 — staged and unstaged are both included, reported identically
// -----------------------------------------------------------------------

#[test]
fn uncommitted_includes_staged_and_unstaged_without_index_state() {
    let f = Fixture::new();
    // Deliberately neutral names: an assertion below reads the serialised
    // entry for any mention of index state, and a file called "staged.md"
    // would satisfy it for the wrong reason.
    f.write("alpha.md", "one\ntwo\n");
    f.write("beta.md", "one\ntwo\n");
    f.commit("base");

    f.write("alpha.md", "one\ntwo\nthree\n");
    f.stage_all();
    f.write("beta.md", "one\ntwo\nthree\n");

    let set = uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap();
    assert_eq!(set.comparison, Comparison::Uncommitted);
    // Both appear, and the shape carries nothing that distinguishes them.
    let staged = entry(&set, "alpha.md");
    let unstaged = entry(&set, "beta.md");
    assert_eq!(staged.change_status, ChangeStatus::Modified);
    assert_eq!(unstaged.change_status, ChangeStatus::Modified);
    assert_eq!(staged.added_lines, Some(1));
    assert_eq!(unstaged.added_lines, Some(1));
    // The wire shape has no index-state field at all (CHC-FR-03).
    let json = serde_json::to_string(staged).unwrap();
    for forbidden in ["staged", "unstaged", "index"] {
        assert!(
            !json.contains(forbidden),
            "no index state on the wire, found {forbidden:?} in {json}"
        );
    }
}

// -----------------------------------------------------------------------
// CHC-FR-04 — merge-base semantics
// -----------------------------------------------------------------------

#[test]
fn branch_changes_are_taken_against_the_merge_base() {
    // CHC-FR-04: `main` advancing after the fork point must not leak into
    // the feature branch's change set.
    let f = Fixture::new();
    f.write("base.md", "base\n");
    let base_oid = f.commit("A");
    // Whatever the default init branch is called, name it explicitly.
    let current = f.current_branch();
    f.branch("feature");
    f.checkout("feature");
    f.write("from-e.md", "e\n");
    f.commit("E");
    f.write("from-f.md", "f\n");
    f.commit("F");

    // The target branch advances past the merge base with a file the
    // feature branch has never seen.
    f.checkout(&current);
    f.write("only-on-main.md", "d\n");
    f.commit("D");
    f.checkout("feature");

    // The uncommitted edit is made last: `commit` stages everything, so an
    // edit left lying around during the target-branch commit above would be
    // swept into it rather than staying uncommitted.
    f.write("uncommitted.md", "u\n");

    let set = branch_change_set(&crate::fs::RootFs::for_root(f.root()), &current).unwrap();
    let got = paths(&set);
    assert!(got.contains(&"from-e.md".to_string()), "{got:?}");
    assert!(got.contains(&"from-f.md".to_string()), "{got:?}");
    assert!(
        got.contains(&"uncommitted.md".to_string()),
        "an uncommitted edit belongs to the branch's contribution: {got:?}"
    );
    assert!(
        !got.contains(&"only-on-main.md".to_string()),
        "commits that landed on the target after the merge base must not appear: {got:?}"
    );
    match set.comparison {
        Comparison::Branch {
            ref target_branch,
            ref merge_base,
        } => {
            assert_eq!(target_branch, &current);
            assert_eq!(
                merge_base, &base_oid.to_string(),
                "the reported merge base is commit A, the fork point — not \
                 HEAD and not the target's tip"
            );
        }
        ref other => panic!("expected a branch comparison, got {other:?}"),
    }
}

// -----------------------------------------------------------------------
// CHC-FR-05 — detached HEAD
// -----------------------------------------------------------------------

#[test]
fn branch_changes_resolve_with_a_detached_head() {
    // CHC-FR-05: the checked-out commit stands in for the branch tip.
    let f = Fixture::new();
    f.write("base.md", "base\n");
    f.commit("A");
    let current = f.current_branch();
    f.branch("feature");
    f.checkout("feature");
    f.write("one.md", "1\n");
    f.commit("E");
    f.write("two.md", "2\n");
    f.commit("F");
    f.detach();
    f.write("dirty.md", "d\n");

    let set = branch_change_set(&crate::fs::RootFs::for_root(f.root()), &current).expect("a detached HEAD is not an error");
    let got = paths(&set);
    assert!(got.contains(&"one.md".to_string()), "{got:?}");
    assert!(got.contains(&"two.md".to_string()), "{got:?}");
    assert!(got.contains(&"dirty.md".to_string()), "{got:?}");
}

// -----------------------------------------------------------------------
// CHC-FR-06 — untracked in, ignored out
// -----------------------------------------------------------------------

#[test]
fn untracked_appears_and_ignored_never_does() {
    let f = Fixture::new();
    f.write("base.md", "base\n");
    f.write(".gitignore", "secret.md\n");
    f.commit("A");
    let current = f.current_branch();

    f.write("fresh.md", "new\n");
    f.write("secret.md", "hidden\n");

    let set = uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap();
    assert_eq!(entry(&set, "fresh.md").change_status, ChangeStatus::Untracked);
    assert!(
        !paths(&set).contains(&"secret.md".to_string()),
        "an ignored file must be absent from every change set: {:?}",
        paths(&set)
    );

    let branch = branch_change_set(&crate::fs::RootFs::for_root(f.root()), &current).unwrap();
    assert!(
        !paths(&branch).contains(&"secret.md".to_string()),
        "likewise in branch mode: {:?}",
        paths(&branch)
    );
}

// -----------------------------------------------------------------------
// CHC-FR-07 — the line-count rule
// -----------------------------------------------------------------------

#[test]
fn line_counts_follow_the_per_status_rule() {
    // CHC-FR-07: untracked counts every line as added; deleted counts every
    // line as removed; modified counts its unified diff.
    let f = Fixture::new();
    f.write("gone.md", "a\nb\nc\n");
    f.write("edited.md", "one\ntwo\nthree\nfour\n");
    f.commit("A");

    f.remove("gone.md");
    f.write("edited.md", "one\nTWO\nthree\nfour\nfive\n");
    let forty = (1..=40).map(|i| format!("line {i}\n")).collect::<String>();
    f.write("fresh.md", &forty);

    let set = uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap();

    let fresh = entry(&set, "fresh.md");
    assert_eq!((fresh.added_lines, fresh.removed_lines), (Some(40), Some(0)));

    let gone = entry(&set, "gone.md");
    assert_eq!(gone.change_status, ChangeStatus::Deleted);
    assert_eq!((gone.added_lines, gone.removed_lines), (Some(0), Some(3)));

    let edited = entry(&set, "edited.md");
    // One line rewritten (+1/-1) plus one appended (+1).
    assert_eq!((edited.added_lines, edited.removed_lines), (Some(2), Some(1)));
}

// -----------------------------------------------------------------------
// CHC-FR-08 — binary content
// -----------------------------------------------------------------------

#[test]
fn binary_entries_carry_no_line_counts() {
    let f = Fixture::new();
    f.write_bytes("logo.png", &[0u8, 1, 2, 0, 3, 4, 0, 5]);
    f.commit("A");
    f.write_bytes("logo.png", &[0u8, 9, 9, 0, 8, 8, 0, 7, 7]);

    let set = uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap();
    let e = entry(&set, "logo.png");
    assert!(e.is_binary, "Git treats NUL-bearing content as binary");
    assert_eq!(e.added_lines, None, "counts are never fabricated");
    assert_eq!(e.removed_lines, None);
    // `null`, not omitted — the panel renders a marker in their place.
    let json = serde_json::to_value(e).unwrap();
    assert!(json.get("addedLines").unwrap().is_null());
    assert!(json.get("removedLines").unwrap().is_null());
}
