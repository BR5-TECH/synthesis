//! The uncommitted diff totals (CHC-FR-21, CHC-FR-22, CHC-FR-18).
//!
//! One part of `../tests/mod.rs`, which holds the fixture these run against.

use super::*;

// -----------------------------------------------------------------------
// CHC-FR-21 / CHC-FR-22, CHC-FR-18 — the uncommitted diff totals
// -----------------------------------------------------------------------

#[test]
fn the_totals_sum_the_change_set_and_exclude_binary_lines() {
    // CHC-FR-21: an untracked 40-line file, a modification of +18/-4, and a
    // modified binary -> added 58, removed 4, file_count 3.
    let f = Fixture::new();
    let original: String = (1..=20).map(|i| format!("line {i}\n")).collect();
    f.write("edited.md", &original);
    f.write_bytes("logo.png", &[0u8, 1, 2, 0, 3, 4]);
    f.commit("A");

    // 4 lines replaced by 18 fresh ones: +18 / -4.
    let mut edited: String = (1..=16).map(|i| format!("line {i}\n")).collect();
    edited.push_str(&(1..=18).map(|i| format!("new {i}\n")).collect::<String>());
    f.write("edited.md", &edited);
    f.write_bytes("logo.png", &[0u8, 9, 9, 0, 8, 8, 0, 7]);
    let forty: String = (1..=40).map(|i| format!("fresh {i}\n")).collect();
    f.write("fresh.md", &forty);

    let totals = uncommitted_diff_totals(&crate::fs::RootFs::for_root(f.root())).unwrap();
    assert_eq!(totals.added_lines, 58, "40 untracked + 18 inserted");
    assert_eq!(totals.removed_lines, 4);
    assert_eq!(
        totals.file_count, 3,
        "the binary counts toward the file count, just not the line totals"
    );

    // And they agree with the list the panel would render — summed here
    // independently rather than by re-invoking `totals_of`, which would only
    // compare the function against itself.
    let set = uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap();
    let listed_added: u32 = set.entries.iter().filter_map(|e| e.added_lines).sum();
    let listed_removed: u32 = set.entries.iter().filter_map(|e| e.removed_lines).sum();
    assert_eq!(listed_added, totals.added_lines);
    assert_eq!(listed_removed, totals.removed_lines);
    assert_eq!(set.entries.len(), totals.file_count);
}

#[test]
fn the_totals_are_a_standalone_read_and_report_a_non_repository() {
    // CHC-FR-21, CHC-FR-18 / CHC-FR-22: no prior `list_uncommitted_changes` is needed,
    // and outside a repository the same typed error is returned as by every
    // other command in the surface.
    let f = Fixture::new();
    f.write("a.md", "one\ntwo\n");
    f.commit("A");
    f.write("a.md", "one\ntwo\nthree\n");

    // The very first call this process makes against the fixture.
    let totals = uncommitted_diff_totals(&crate::fs::RootFs::for_root(f.root())).unwrap();
    assert_eq!((totals.added_lines, totals.removed_lines), (1, 0));
    assert_eq!(totals.file_count, 1);

    let outside = TempDir::new().unwrap();
    assert_eq!(
        uncommitted_diff_totals(&crate::fs::RootFs::for_root(outside.path())).unwrap_err(),
        ERR_NOT_A_REPO
    );
}

#[test]
fn a_clean_working_tree_totals_to_zero() {
    // What STB-FR-30 renders after switching to a clean worktree: zeros, not
    // an error and not the previous worktree's counts.
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("A");
    assert_eq!(
        uncommitted_diff_totals(&crate::fs::RootFs::for_root(f.root())).unwrap(),
        DiffTotals::default()
    );
}

#[test]
fn diff_totals_serialise_camel_case() {
    let json = serde_json::to_value(DiffTotals {
        added_lines: 412,
        removed_lines: 87,
        file_count: 5,
    })
    .unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "addedLines": 412, "removedLines": 87, "fileCount": 5 })
    );
}
