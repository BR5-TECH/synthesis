//! Co-located projects, whose content root is below the worktree root.
//!
//! One part of `mod.rs`, which holds the fixture and the helpers these use.

use super::*;

// -- Co-located projects: content root below the worktree ---------------

#[test]
fn a_project_below_the_worktree_root_still_flags_its_checkout_active() {
    // GTC-FR-02 co-located mode: the content root is a *subdirectory* of the
    // checkout. It is the checkout that is enumerated and flagged, not the
    // subdirectory — otherwise no row is ever `current` and the chrome
    // labels itself with a folder name instead of a branch.
    let f = Fixture::new();
    f.write("docs/keep.md", "x\n");
    f.commit("docs");
    let nested = f.root().join("docs");

    let ctx = context_for(&nested).unwrap();
    assert_eq!(ctx.active_worktree_path, to_string_path(&f.root()));
    assert_eq!(
        ctx.worktrees.iter().filter(|w| w.is_active).count(),
        1,
        "exactly one worktree is active: {ctx:?}"
    );

    let entry = active_entry_for(&nested).unwrap();
    assert_eq!(entry.path, to_string_path(&f.root()));
    assert_eq!(entry.branch.as_deref(), Some(&*f.current_branch()));
    assert!(entry.is_primary);
}

#[test]
fn a_co_located_project_keeps_its_subdirectory_across_a_switch() {
    // WTC-FR-08: activating another worktree must land on the *corresponding*
    // subdirectory of that checkout, not relocate the project to its root.
    let f = Fixture::new();
    f.write("docs/keep.md", "x\n");
    f.commit("docs");
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    let nested = f.root().join("docs");

    let repo = open_repo(&nested).unwrap();
    let offset = root_offset(&repo, &nested);
    assert_eq!(offset, PathBuf::from("docs"));
    assert_eq!(content_root_in(&alpha, &offset), alpha.join("docs"));
}

#[test]
fn a_standalone_project_has_no_offset_to_carry() {
    let f = Fixture::new();
    let repo = open_repo(&f.root()).unwrap();
    let offset = root_offset(&repo, &f.root());
    assert_eq!(offset, PathBuf::new());
    assert_eq!(content_root_in(&f.root(), &offset), f.root());
}

#[test]
fn a_co_located_project_resumes_into_the_remembered_worktrees_subdirectory() {
    // WTC-FR-17 with a nested content root: what is remembered is the
    // worktree, and the subdirectory is re-derived from where the project
    // was opened.
    let f = Fixture::new();
    f.write("docs/keep.md", "x\n");
    f.commit("docs");
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    let store = GlobalSettingsStore::in_memory();
    store
        .save_active_worktree(&to_string_path(&f.root()), &to_string_path(&alpha))
        .unwrap();

    let (resumed, fell_back) =
        resume_active_worktree(&store, &f.root(), &f.root().join("docs"));
    assert_eq!(resumed, alpha.join("docs"));
    assert!(!fell_back);
}
