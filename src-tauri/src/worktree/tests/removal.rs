//! Removing one linked worktree (WTC-FR-RDVK, WTC-FR-BHFE).
//!
//! One part of `mod.rs`, which holds the fixture and the helpers these use. A
//! work stream's working copy is refused in `streams/tests/deletion_guards.rs`,
//! where a stream can be made.

use super::*;

fn linked_count(f: &Fixture) -> usize {
    f.repo().worktrees().unwrap().len()
}

// WTC-FR-RDVK
#[test]
fn removing_an_inactive_linked_worktree_deletes_its_directory_and_its_registration() {
    let f = Fixture::new();
    f.branch("alpha");
    let path = f.add_worktree("wt-alpha", "alpha");
    let app = mock_app_on(&f.root(), &f.root());

    remove_linked_worktree(app.handle(), &f.root(), &path).unwrap();

    assert!(!path.exists(), "the directory is gone");
    assert_eq!(linked_count(&f), 0, "so is the registration");
    assert!(
        f.repo().find_branch("alpha", BranchType::Local).is_ok(),
        "the removal deletes no branch"
    );
}

// WTC-FR-RDVK
#[test]
fn a_worktree_whose_directory_is_gone_has_its_registration_pruned() {
    let f = Fixture::new();
    f.branch("alpha");
    let path = f.add_worktree("wt-alpha", "alpha");
    fs::remove_dir_all(&path).unwrap();
    assert_eq!(linked_count(&f), 1);
    let app = mock_app_on(&f.root(), &f.root());

    remove_linked_worktree(app.handle(), &f.root(), &path).unwrap();

    assert_eq!(linked_count(&f), 0);
}

// WTC-FR-RDVK
#[test]
fn the_primary_worktree_is_refused_and_nothing_changes() {
    let f = Fixture::new();
    f.branch("alpha");
    let linked = f.add_worktree("wt-alpha", "alpha");
    // Rooted in the linked worktree, so the primary is not the active one and
    // the refusal is the primary's own.
    let app = mock_app_on(&linked, &f.root());
    let before = f.container.path().to_path_buf();
    let snapshot = crate::changes::tests_support::snapshot(before.clone());

    let refused = remove_linked_worktree(app.handle(), &linked, &f.root()).unwrap_err();

    assert_eq!(refused, ERR_WORKTREE_IS_PRIMARY);
    assert_eq!(snapshot, crate::changes::tests_support::snapshot(before));
}

// WTC-FR-RDVK, WTC-FR-BHFE
#[test]
fn the_active_worktree_is_refused_whatever_an_earlier_listing_said() {
    let f = Fixture::new();
    f.branch("alpha");
    let path = f.add_worktree("wt-alpha", "alpha");
    // A listing read while the primary was active names the linked worktree as
    // removable.
    let earlier = context_for(&f.root()).unwrap();
    assert!(earlier.worktrees.iter().any(|w| w.path == to_string_path(&path) && !w.is_active));
    // The project is then rooted in it.
    let app = mock_app_on(&path, &f.root());
    let snapshot = crate::changes::tests_support::snapshot(f.container.path().to_path_buf());

    let refused = remove_linked_worktree(app.handle(), &path, &path).unwrap_err();

    assert_eq!(refused, ERR_WORKTREE_IS_ACTIVE);
    assert_eq!(
        snapshot,
        crate::changes::tests_support::snapshot(f.container.path().to_path_buf())
    );
}

// WTC-FR-RDVK
#[test]
fn a_path_that_is_no_worktree_of_the_repository_is_refused() {
    let f = Fixture::new();
    let stranger = TempDir::new().unwrap();
    let app = mock_app_on(&f.root(), &f.root());

    assert_eq!(
        remove_linked_worktree(app.handle(), &f.root(), stranger.path()).unwrap_err(),
        ERR_NOT_A_WORKTREE
    );
    assert!(stranger.path().exists(), "a directory outside the repository is never removed");
}

// WTC-FR-RDVK, FSA-FR-OWVT
#[cfg(unix)]
#[test]
fn a_worktree_holding_a_symbolic_link_is_removed_and_the_link_target_survives() {
    let f = Fixture::new();
    f.branch("alpha");
    let path = f.add_worktree("wt-alpha", "alpha");
    let outside = tempfile::TempDir::new().unwrap();
    fs::write(outside.path().join("store.js"), "module.exports = {};").unwrap();
    fs::create_dir_all(path.join("node_modules")).unwrap();
    std::os::unix::fs::symlink(outside.path(), path.join("node_modules/dep")).unwrap();
    std::os::unix::fs::symlink(f.root(), path.join("link-out")).unwrap();
    let app = mock_app_on(&f.root(), &f.root());

    remove_linked_worktree(app.handle(), &f.root(), &path).unwrap();

    assert!(!path.exists(), "the worktree directory is gone");
    assert_eq!(linked_count(&f), 0, "and its registration is pruned");
    assert!(
        outside.path().join("store.js").exists(),
        "a link was unlinked and never followed"
    );
    assert!(f.root().join("a.md").exists(), "the primary worktree is untouched");
}
