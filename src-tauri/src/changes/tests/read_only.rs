//! Every command leaves the repository byte identical (CHC-FR-18).
//!
//! One part of `../tests/mod.rs`, which holds the fixture these run against.

use super::*;

// -----------------------------------------------------------------------
// CHC-FR-18 — strictly read-only
// -----------------------------------------------------------------------

#[test]
fn every_command_leaves_the_repository_byte_identical() {
    // CHC-FR-18: after running the whole contract surface, the index, HEAD,
    // the refs and every working-tree file must be unchanged.
    let f = Fixture::new();
    f.write("tracked.md", "t\n");
    f.commit("A");
    let current = f.current_branch();
    f.write("tracked.md", "t\nstaged\n");
    f.stage_all();
    f.write("unstaged.md", "u\n");
    f.stage_all();
    f.write("unstaged.md", "u\nmore\n");
    f.write("untracked.md", "n\n");

    let before = tests_support::snapshot(f.root());

    uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap();
    branch_change_set(&crate::fs::RootFs::for_root(f.root()), &current).unwrap();
    default_branch(&f.repo()).unwrap();
    comparison_branches(&f.root()).unwrap();
    // CHC-FR-21, CHC-FR-22, CHC-FR-18: the totals read is part of the same read-only surface.
    uncommitted_diff_totals(&crate::fs::RootFs::for_root(f.root())).unwrap();

    let after = tests_support::snapshot(f.root());
    assert_eq!(before, after, "the contract surface must not mutate anything");
}
