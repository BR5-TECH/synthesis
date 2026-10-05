//! The `get_diff` scenarios: what each scope compares, and what it reports
//! for a path that is added, deleted, renamed, binary, or unchanged.
//!
//! One part of `mod.rs`, which holds the fixtures these run against.

use super::*;

#[test]
fn path_scope_diffs_the_working_tree_against_head() {
    let f = Fixture::new();
    f.write("a.md", "one\ntwo\nthree\n");
    f.commit("A");
    f.write("a.md", "one\nTWO\nthree\n");

    let payload = diff_for_scope(
        &f.root(),
        &DiffScope::Path {
            path: "a.md".into(),
            previous_path: None,
        },
    )
    .unwrap();
    assert!(!payload.is_binary);
    assert_eq!(payload.hunks.len(), 1);
    assert!(payload.hunks[0].header.starts_with("@@"));
    let lines = all_lines(&payload);
    assert!(lines.contains(&("del".to_string(), "two".to_string())), "{lines:?}");
    assert!(lines.contains(&("add".to_string(), "TWO".to_string())), "{lines:?}");
    assert!(
        lines.contains(&("context".to_string(), "one".to_string())),
        "surrounding context travels with the hunk: {lines:?}"
    );
}

#[test]
fn path_scope_covers_staged_and_unstaged_alike() {
    // The Changes panel does not distinguish the two (CHC-FR-03), so the
    // Diff tab it opens must show both.
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("A");
    f.write("a.md", "one\nstaged\n");
    f.stage_all();
    f.write("a.md", "one\nstaged\nunstaged\n");

    let payload = diff_for_scope(
        &f.root(),
        &DiffScope::Path {
            path: "a.md".into(),
            previous_path: None,
        },
    )
    .unwrap();
    let lines = all_lines(&payload);
    assert!(lines.contains(&("add".to_string(), "staged".to_string())), "{lines:?}");
    assert!(
        lines.contains(&("add".to_string(), "unstaged".to_string())),
        "{lines:?}"
    );
}

#[test]
fn path_scope_shows_only_the_requested_file() {
    let f = Fixture::new();
    f.write("a.md", "a\n");
    f.write("b.md", "b\n");
    f.commit("A");
    f.write("a.md", "a\nchanged\n");
    f.write("b.md", "b\nalso changed\n");

    let payload = diff_for_scope(
        &f.root(),
        &DiffScope::Path {
            path: "a.md".into(),
            previous_path: None,
        },
    )
    .unwrap();
    let lines = all_lines(&payload);
    assert!(lines.contains(&("add".to_string(), "changed".to_string())));
    assert!(
        !lines.iter().any(|(_, c)| c == "also changed"),
        "the pathspec must scope the diff to one file: {lines:?}"
    );
}

#[test]
fn branch_scope_diffs_against_the_merge_base() {
    let f = Fixture::new();
    f.write("a.md", "base\n");
    f.commit("A");
    let main = f.current_branch();
    f.branch_and_checkout("feature");
    f.write("a.md", "base\ncommitted on feature\n");
    f.commit("E");
    f.write("a.md", "base\ncommitted on feature\nuncommitted\n");

    let payload = diff_for_scope(
        &f.root(),
        &DiffScope::Branch {
            path: "a.md".into(),
            target_branch: main,
            previous_path: None,
        },
    )
    .unwrap();
    let lines = all_lines(&payload);
    assert!(
        lines.contains(&("add".to_string(), "committed on feature".to_string())),
        "a committed change belongs to the branch comparison: {lines:?}"
    );
    assert!(
        lines.contains(&("add".to_string(), "uncommitted".to_string())),
        "so does an uncommitted one: {lines:?}"
    );
}

#[test]
fn branch_scope_reports_typed_branch_errors() {
    let f = Fixture::new();
    f.write("a.md", "a\n");
    f.commit("A");
    let err = diff_for_scope(
        &f.root(),
        &DiffScope::Branch {
            path: "a.md".into(),
            target_branch: "nope".into(),
            previous_path: None,
        },
    )
    .unwrap_err();
    assert_eq!(err, crate::changes::ERR_UNKNOWN_BRANCH);
}

#[test]
fn staged_scope_shows_the_index_and_not_the_unstaged_remainder() {
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("A");
    f.write("a.md", "one\nstaged\n");
    f.stage_all();
    f.write("a.md", "one\nstaged\nunstaged\n");

    let payload = diff_for_scope(&f.root(), &DiffScope::Staged).unwrap();
    let lines = all_lines(&payload);
    assert!(lines.contains(&("add".to_string(), "staged".to_string())), "{lines:?}");
    assert!(
        !lines.iter().any(|(_, c)| c == "unstaged"),
        "the staged scope stops at the index: {lines:?}"
    );
}

#[test]
fn an_untracked_file_diffs_as_all_additions() {
    let f = Fixture::new();
    f.write("a.md", "a\n");
    f.commit("A");
    f.write("fresh.md", "new one\nnew two\n");

    let payload = diff_for_scope(
        &f.root(),
        &DiffScope::Path {
            path: "fresh.md".into(),
            previous_path: None,
        },
    )
    .unwrap();
    let lines = all_lines(&payload);
    assert_eq!(
        lines,
        vec![
            ("add".to_string(), "new one".to_string()),
            ("add".to_string(), "new two".to_string()),
        ]
    );
}

#[test]
fn binary_content_yields_a_marker_and_no_hunks() {
    let f = Fixture::new();
    f.write_bytes("logo.png", &[0u8, 1, 2, 0, 3]);
    f.commit("A");
    f.write_bytes("logo.png", &[0u8, 9, 9, 0, 8]);

    let payload = diff_for_scope(
        &f.root(),
        &DiffScope::Path {
            path: "logo.png".into(),
            previous_path: None,
        },
    )
    .unwrap();
    assert!(payload.is_binary);
    assert!(payload.hunks.is_empty(), "no hunks are invented for binary content");
}

#[test]
fn an_unchanged_path_yields_an_empty_payload_rather_than_an_error() {
    // CHG-FR-22: a Diff tab whose file was reverted re-renders as "no
    // changes" instead of failing.
    let f = Fixture::new();
    f.write("a.md", "a\n");
    f.commit("A");

    let payload = diff_for_scope(
        &f.root(),
        &DiffScope::Path {
            path: "a.md".into(),
            previous_path: None,
        },
    )
    .unwrap();
    assert_eq!(payload, DiffPayload::empty());
}

#[test]
fn a_non_repository_is_the_typed_error() {
    let dir = TempDir::new().unwrap();
    let err = diff_for_scope(
        dir.path(),
        &DiffScope::Path {
            path: "a.md".into(),
            previous_path: None,
        },
    )
    .unwrap_err();
    assert_eq!(err, ERR_NOT_A_REPO);
}

#[test]
fn a_nested_project_still_diffs_only_the_requested_file() {
    // Regression: the project prefix and the file path are BOTH pathspecs,
    // and libgit2 matches a path against any entry — so reusing
    // prefix-scoped options here widened the diff back to the whole project
    // and a Diff tab rendered every changed file at once, with no file
    // headers to separate them.
    let f = Fixture::new();
    f.write("proj/a.md", "a\n");
    f.write("proj/b.md", "b\n");
    f.commit("A");
    f.write("proj/a.md", "a\nfrom a\n");
    f.write("proj/b.md", "b\nfrom b\n");

    let payload = diff_for_scope(
        &f.root().join("proj"),
        &DiffScope::Path {
            path: "a.md".into(),
            previous_path: None,
        },
    )
    .unwrap();
    let lines = all_lines(&payload);
    assert!(lines.contains(&("add".to_string(), "from a".to_string())), "{lines:?}");
    assert!(
        !lines.iter().any(|(_, c)| c == "from b"),
        "a sibling file's changes must not leak into this file's diff: {lines:?}"
    );
}

#[test]
fn a_nested_projects_binary_sibling_does_not_blank_a_text_diff() {
    // The same widening had a second, louder symptom: `format_diff` carries
    // one `is_binary` flag for the whole payload, so any binary file caught
    // by the over-broad pathspec collapsed the requested text diff to
    // "Binary file — no textual diff".
    let f = Fixture::new();
    f.write("proj/a.md", "a\n");
    f.write_bytes("proj/logo.png", &[0u8, 1, 2, 0, 3]);
    f.commit("A");
    f.write("proj/a.md", "a\nedited\n");
    f.write_bytes("proj/logo.png", &[0u8, 9, 9, 0, 8]);

    let payload = diff_for_scope(
        &f.root().join("proj"),
        &DiffScope::Path {
            path: "a.md".into(),
            previous_path: None,
        },
    )
    .unwrap();
    assert!(!payload.is_binary, "the requested file is text");
    assert!(all_lines(&payload).contains(&("add".to_string(), "edited".to_string())));
}

#[test]
fn a_rename_diffs_as_a_rename_rather_than_a_whole_file_addition() {
    // CHC-FR-09 / CHG-FR-12: the panel row says `lib.rs (was main.rs) +1 −1`,
    // so the Diff tab it opens must show that edit — not the whole file in
    // green, which is what an unpaired `Added` delta produces.
    let f = Fixture::new();
    let body: String = (1..=30).map(|i| format!("stable line {i}\n")).collect();
    f.write("src-tauri/main.rs", &body);
    f.commit("A");
    std::fs::rename(
        f.root().join("src-tauri/main.rs"),
        f.root().join("src-tauri/lib.rs"),
    )
    .unwrap();
    f.write("src-tauri/lib.rs", &body.replace("stable line 1\n", "edited line 1\n"));

    let payload = diff_for_scope(
        &f.root(),
        &DiffScope::Path {
            path: "src-tauri/lib.rs".into(),
            previous_path: Some("src-tauri/main.rs".into()),
        },
    )
    .unwrap();
    let lines = all_lines(&payload);
    let added = lines.iter().filter(|(k, _)| k == "add").count();
    assert!(
        added < 5,
        "a rename with one edited line must not read as 30 additions: {lines:?}"
    );
    assert!(lines.contains(&("add".to_string(), "edited line 1".to_string())), "{lines:?}");
    assert!(lines.contains(&("del".to_string(), "stable line 1".to_string())), "{lines:?}");
}

#[test]
fn get_diff_leaves_the_repository_byte_identical() {
    // CHG-FR-24: the Diff tab mutates nothing either. Every scope is
    // exercised, since each builds its diff differently.
    let f = Fixture::new();
    f.write("a.md", "a\n");
    f.commit("A");
    let main = f.current_branch();
    f.write("a.md", "a\nstaged\n");
    f.stage_all();
    f.write("a.md", "a\nstaged\nunstaged\n");
    f.write("untracked.md", "u\n");

    let before = crate::changes::tests_support::snapshot(f.root());
    for scope in [
        DiffScope::Path {
            path: "a.md".into(),
            previous_path: None,
        },
        DiffScope::Staged,
        DiffScope::Branch {
            path: "a.md".into(),
            target_branch: main,
            previous_path: None,
        },
    ] {
        diff_for_scope(&f.root(), &scope).unwrap();
    }
    assert_eq!(
        before,
        crate::changes::tests_support::snapshot(f.root()),
        "get_diff must not write to the index, refs, or the working tree"
    );
}

#[test]
fn a_branch_scope_in_a_nested_project_diffs_only_its_own_file() {
    let f = Fixture::new();
    f.write("proj/a.md", "base\n");
    f.write("proj/b.md", "b\n");
    f.commit("A");
    let main = f.current_branch();
    f.branch_and_checkout("feature");
    f.write("proj/a.md", "base\nfrom a\n");
    f.write("proj/b.md", "b\nfrom b\n");
    f.commit("E");

    let payload = diff_for_scope(
        &f.root().join("proj"),
        &DiffScope::Branch {
            path: "a.md".into(),
            target_branch: main,
            previous_path: None,
        },
    )
    .unwrap();
    let lines = all_lines(&payload);
    assert!(lines.contains(&("add".to_string(), "from a".to_string())), "{lines:?}");
    assert!(!lines.iter().any(|(_, c)| c == "from b"), "{lines:?}");
}

#[test]
fn an_unborn_head_diffs_a_brand_new_file_as_additions() {
    // A repository with no commits at all: `head_tree` is None, so the whole
    // working tree is new.
    let f = Fixture::new();
    f.write("first.md", "one\ntwo\n");
    let payload = diff_for_scope(
        &f.root(),
        &DiffScope::Path {
            path: "first.md".into(),
            previous_path: None,
        },
    )
    .unwrap();
    assert_eq!(
        all_lines(&payload),
        vec![
            ("add".to_string(), "one".to_string()),
            ("add".to_string(), "two".to_string()),
        ]
    );
}

#[test]
fn a_project_below_the_repo_root_diffs_its_own_path() {
    let f = Fixture::new();
    f.write("proj/a.md", "a\n");
    f.commit("A");
    f.write("proj/a.md", "a\nb\n");

    let payload = diff_for_scope(
        &f.root().join("proj"),
        &DiffScope::Path {
            path: "a.md".into(),
            previous_path: None,
        },
    )
    .unwrap();
    let lines = all_lines(&payload);
    assert!(lines.contains(&("add".to_string(), "b".to_string())), "{lines:?}");
}
