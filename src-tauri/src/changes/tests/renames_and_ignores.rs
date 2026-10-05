//! Renames, binary markers and the ignore rules (CHC-FR-09).
//!
//! One part of `../tests/mod.rs`, which holds the fixture these run against.

use super::*;

// -----------------------------------------------------------------------
// CHC-FR-09 — renames
// -----------------------------------------------------------------------

#[test]
fn a_rename_reports_both_paths_and_a_plain_edit_reports_none() {
    let f = Fixture::new();
    let body: String = (1..=30).map(|i| format!("stable line {i}\n")).collect();
    f.write("src-tauri/main.rs", &body);
    f.write("plain.md", "one\n");
    f.commit("A");

    f.rename("src-tauri/main.rs", "src-tauri/lib.rs");
    f.write("plain.md", "one\ntwo\n");
    f.stage_all();

    let set = uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap();
    let renamed = entry(&set, "src-tauri/lib.rs");
    assert_eq!(renamed.change_status, ChangeStatus::Renamed);
    assert_eq!(
        renamed.previous_path.as_deref(),
        Some("src-tauri/main.rs"),
        "the pre-rename path travels with the entry"
    );
    assert_eq!(renamed.name, "lib.rs", "the entry is named at its new path");

    let plain = entry(&set, "plain.md");
    assert_eq!(plain.previous_path, None);
    let json = serde_json::to_value(plain).unwrap();
    assert!(
        json.get("previousPath").is_none(),
        "absent, not null, for every other status"
    );
}

#[test]
fn an_unstaged_rename_is_reported_as_a_rename() {
    // The common case for a panel that answers "what have I changed right
    // now": a file moved in the editor and not yet staged. Git sees
    // Deleted + Untracked, which libgit2 only pairs when rename detection
    // is told to consider untracked files.
    let f = Fixture::new();
    let body: String = (1..=30).map(|i| format!("stable line {i}\n")).collect();
    f.write("src-tauri/main.rs", &body);
    f.commit("A");
    f.rename("src-tauri/main.rs", "src-tauri/lib.rs");

    let set = uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap();
    let e = entry(&set, "src-tauri/lib.rs");
    assert_eq!(
        e.change_status,
        ChangeStatus::Renamed,
        "an unstaged move is still a rename, not a delete plus an untracked add"
    );
    assert_eq!(e.previous_path.as_deref(), Some("src-tauri/main.rs"));
    assert!(
        !paths(&set).contains(&"src-tauri/main.rs".to_string()),
        "the pre-rename path must not also appear as its own entry: {:?}",
        paths(&set)
    );
}

#[test]
fn an_untracked_binary_carries_the_binary_marker() {
    // The untracked path reaches libgit2 through `show_untracked_content`,
    // a different code path from a tracked modification.
    let f = Fixture::new();
    f.write("a.md", "a\n");
    f.commit("A");
    f.write_bytes("new-logo.png", &[0u8, 1, 2, 0, 3, 4]);

    let set = uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap();
    let e = entry(&set, "new-logo.png");
    assert_eq!(e.change_status, ChangeStatus::Untracked);
    assert!(e.is_binary, "a new binary file is still binary");
    assert_eq!(e.added_lines, None);
    assert_eq!(e.removed_lines, None);
}

#[test]
fn a_deleted_binary_carries_the_binary_marker() {
    let f = Fixture::new();
    f.write_bytes("logo.png", &[0u8, 1, 2, 0, 3]);
    f.commit("A");
    f.remove("logo.png");

    let set = uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap();
    let e = entry(&set, "logo.png");
    assert_eq!(e.change_status, ChangeStatus::Deleted);
    assert!(e.is_binary);
    assert_eq!(e.removed_lines, None, "counts are never fabricated");
}

#[test]
fn an_ignored_directory_and_a_nested_gitignore_are_both_honoured() {
    // `recurse_untracked_dirs(true)` descends into new folders, so an
    // ignored *directory* is a distinct case from an ignored file.
    let f = Fixture::new();
    f.write(".gitignore", "build/\n");
    f.write("src/.gitignore", "*.log\n");
    f.write("src/a.md", "a\n");
    f.commit("A");

    f.write("build/output.js", "generated\n");
    f.write("build/nested/more.js", "generated\n");
    f.write("src/debug.log", "noise\n");
    f.write("src/b.md", "b\n");

    let got = paths(&uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap());
    assert!(got.contains(&"src/b.md".to_string()), "{got:?}");
    assert!(
        !got.iter().any(|p| p.starts_with("build/")),
        "an ignored directory contributes nothing, at any depth: {got:?}"
    );
    assert!(
        !got.contains(&"src/debug.log".to_string()),
        "a nested .gitignore applies too: {got:?}"
    );
}
