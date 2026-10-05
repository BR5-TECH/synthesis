//! The wire shapes and the pure helpers: how a `DiffScope` deserialises,
//! how a `DiffPayload` serialises, and the small functions around them.
//!
//! One part of `mod.rs`, which holds the fixtures these run against.

use super::*;

// -----------------------------------------------------------------------
// Wire shape / pure helpers
// -----------------------------------------------------------------------

#[test]
fn diff_scope_deserialises_from_the_frontend_shape() {
    let path: DiffScope =
        serde_json::from_str(r#"{"kind":"path","path":"a/b.md"}"#).unwrap();
    assert_eq!(
        path,
        DiffScope::Path {
            path: "a/b.md".into(),
            previous_path: None,
        }
    );
    let staged: DiffScope = serde_json::from_str(r#"{"kind":"staged"}"#).unwrap();
    assert_eq!(staged, DiffScope::Staged);
    let branch: DiffScope = serde_json::from_str(
        r#"{"kind":"branch","path":"a/b.md","targetBranch":"main"}"#,
    )
    .unwrap();
    assert_eq!(
        branch,
        DiffScope::Branch {
            path: "a/b.md".into(),
            target_branch: "main".into(),
            previous_path: None,
        }
    );
}

#[test]
fn diff_scope_rejects_an_unknown_kind() {
    assert!(serde_json::from_str::<DiffScope>(r#"{"kind":"everything"}"#).is_err());
}

#[test]
fn diff_payload_serialises_camel_case() {
    let json = serde_json::to_value(DiffPayload {
        is_binary: false,
        hunks: vec![DiffHunk {
            header: "@@ -1 +1 @@".into(),
            lines: vec![DiffLine {
                kind: "add".into(),
                old_lineno: None,
                new_lineno: Some(1),
                content: "hi".into(),
            }],
        }],
    })
    .unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "isBinary": false,
            "hunks": [{
                "header": "@@ -1 +1 @@",
                "lines": [{ "kind": "add", "newLineno": 1, "content": "hi" }],
            }],
        })
    );
}

#[test]
fn line_kind_maps_the_unified_diff_origins() {
    assert_eq!(line_kind('+'), Some("add"));
    assert_eq!(line_kind('-'), Some("del"));
    assert_eq!(line_kind(' '), Some("context"));
    assert_eq!(line_kind('H'), None);
    assert_eq!(line_kind('F'), None);
    assert_eq!(line_kind('>'), None);
}

#[test]
fn repo_pathspec_joins_only_when_the_project_is_nested() {
    assert_eq!(repo_pathspec("", "a/b.md"), "a/b.md");
    assert_eq!(repo_pathspec("proj", "a/b.md"), "proj/a/b.md");
}

#[test]
fn command_function_is_in_scope() {
    let _get_diff = get_diff;
    let _get_file_revisions = get_file_revisions;
    let _list_branches = list_branches;
}
