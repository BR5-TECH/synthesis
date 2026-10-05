//! Activation of an existing worktree, and the path proposed for a new one.
//!
//! One part of `mod.rs`, which holds the fixture and the helpers these use.

use super::*;

// -- WTC-FR-07: the cheap active-entry call -----------------------------

#[test]
fn active_entry_names_the_content_root_without_enumerating_branches() {
    // WTC-FR-07. The entry stands alone: no worktree list, no branch list.
    let f = Fixture::new();
    f.branch("alpha");
    let a = f.add_worktree("wt-alpha", "alpha");

    let entry = active_entry_for(&a).unwrap();
    assert_eq!(entry.path, to_string_path(&a));
    assert_eq!(entry.branch.as_deref(), Some("alpha"));
    assert!(entry.is_active);
    assert!(!entry.is_primary);

    let primary = active_entry_for(&f.root()).unwrap();
    assert!(primary.is_primary);
    assert_eq!(primary.branch.as_deref(), Some(&*f.current_branch()));
}

// -- WTC-FR-09: activation validation -----------------------------------

#[test]
fn activating_a_path_that_is_not_a_worktree_is_the_typed_error() {
    // WTC-FR-09.
    let f = Fixture::new();
    let stranger = TempDir::new().unwrap();
    assert_eq!(
        validate_activation_target(&f.root(), &stranger.path().to_string_lossy()).unwrap_err(),
        ERR_NOT_A_WORKTREE
    );
}

#[test]
fn activating_a_worktree_whose_directory_is_gone_is_the_typed_error() {
    // WTC-FR-09, second half.
    let f = Fixture::new();
    f.branch("alpha");
    let gone = f.add_worktree("wt-gone", "alpha");
    fs::remove_dir_all(&gone).unwrap();
    assert_eq!(
        validate_activation_target(&f.root(), &gone.to_string_lossy()).unwrap_err(),
        ERR_WORKTREE_MISSING
    );
}

#[test]
fn activating_an_existing_worktree_resolves_to_its_canonical_path() {
    let f = Fixture::new();
    f.branch("alpha");
    let a = f.add_worktree("wt-alpha", "alpha");
    assert_eq!(
        validate_activation_target(&f.root(), &a.to_string_lossy()).unwrap(),
        a
    );
}

// -- WTC-FR-13: the proposed path ---------------------------------------

#[test]
fn the_proposed_path_is_named_after_the_branchs_final_segment() {
    // WTC-FR-13: the namespace a branch is filed under says nothing a
    // directory name needs, so only the leaf is carried across.
    let f = Fixture::new();
    let base = project::basename(&f.root().to_string_lossy());

    let proposed =
        propose_path_for(&f.root(), "feature/PROJ-12345/editor-scroll").unwrap();
    assert_eq!(
        proposed,
        to_string_path(&f.sibling(&format!("{base}-editor-scroll"))),
        "the namespace is dropped, not folded in"
    );
    assert!(
        !Path::new(&proposed).exists(),
        "proposing a path must create nothing"
    );

    // A branch with no separator is its own leaf.
    assert_eq!(
        propose_path_for(&f.root(), "spike").unwrap(),
        to_string_path(&f.sibling(&format!("{base}-spike"))),
    );
    // One separator is the ordinary case.
    assert_eq!(
        propose_path_for(&f.root(), "fix/editor-scroll").unwrap(),
        to_string_path(&f.sibling(&format!("{base}-editor-scroll"))),
    );
}

#[test]
fn a_branch_leaf_is_the_last_non_empty_segment() {
    assert_eq!(branch_leaf("feature/PROJ-12345/editor-scroll"), "editor-scroll");
    assert_eq!(branch_leaf("fix/editor-scroll"), "editor-scroll");
    assert_eq!(branch_leaf("spike"), "spike");
    // Trailing and repeated separators do not produce an empty leaf.
    assert_eq!(branch_leaf("feature/"), "feature");
    assert_eq!(branch_leaf("feature//name"), "name");
    // Nothing to name a directory after.
    assert_eq!(branch_leaf(""), "");
    assert_eq!(branch_leaf("   "), "");
    assert_eq!(branch_leaf("///"), "");
}

#[test]
fn a_leaf_is_still_sanitised_for_the_filesystem() {
    // Dropping the namespace does not mean trusting what is left.
    let f = Fixture::new();
    let base = project::basename(&f.root().to_string_lossy());
    assert_eq!(
        propose_path_for(&f.root(), "feature/a b:c").unwrap(),
        to_string_path(&f.sibling(&format!("{base}-a-b-c"))),
    );
}

#[test]
fn the_proposal_is_taken_from_the_primary_even_when_a_linked_worktree_is_active() {
    let f = Fixture::new();
    f.branch("alpha");
    let a = f.add_worktree("wt-alpha", "alpha");
    let base = project::basename(&f.root().to_string_lossy());
    assert_eq!(
        propose_path_for(&a, "spike").unwrap(),
        to_string_path(&f.sibling(&format!("{base}-spike")))
    );
}

#[test]
fn a_branch_with_no_usable_leaf_proposes_the_primary_basename_alone() {
    // The New worktree dialog opens with an empty Branch field (WTS-FR-17),
    // and a branch may be mid-typing, so the proposal must stay a
    // well-formed path rather than trailing a bare separator.
    let f = Fixture::new();
    let base = project::basename(&f.root().to_string_lossy());
    for branch in ["", "  ", "/", "///"] {
        assert_eq!(
            propose_path_for(&f.root(), branch).unwrap(),
            to_string_path(&f.sibling(&base)),
            "branch {branch:?}"
        );
    }
}
