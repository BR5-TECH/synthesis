//! Tests for the `fs` module.

use super::*;
use tempfile::TempDir;

// -----------------------------------------------------------------
// FSA-FR-02: discover_project_root walks upward and returns the
// first ancestor containing `.synthesis/project.toml`.
// -----------------------------------------------------------------
#[test]
fn ts1_discover_project_root_walks_upward() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    // /a/b/c/.synthesis/project.toml
    let project = root.join("a").join("b").join("c");
    let syn = project.join(".synthesis");
    fs::create_dir_all(&syn).unwrap();
    fs::write(syn.join("project.toml"), b"name = \"x\"\n").unwrap();

    let start = project.join("some").join("deeper").join("path");
    fs::create_dir_all(&start).unwrap();
    let found = discover_project_root(&start).unwrap();
    assert_eq!(found, project);
}

#[test]
fn discover_project_root_returns_self_when_start_is_root() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path();
    let syn = project.join(".synthesis");
    fs::create_dir_all(&syn).unwrap();
    fs::write(syn.join("project.toml"), b"").unwrap();

    let found = discover_project_root(project).unwrap();
    assert_eq!(found, project);
}

// -----------------------------------------------------------------
// FSA-FR-03: NotAProject sentinel.
// -----------------------------------------------------------------
#[test]
fn ts2_discover_project_root_returns_not_a_project_when_none_exists() {
    let tmp = TempDir::new().unwrap();
    let deeper = tmp.path().join("a").join("b").join("c");
    fs::create_dir_all(&deeper).unwrap();
    let err = discover_project_root(&deeper).unwrap_err();
    assert!(
        matches!(err, FsError::NotAProject),
        "expected NotAProject, got {err:?}"
    );
}

#[test]
fn merge_gitignore_appends_in_required_order() {
    let merged = merge_gitignore("", &["cache/", "local.toml", "proposals/"]);
    // Should append all three, one per line, in the listed order.
    let lines: Vec<&str> = merged.lines().collect();
    assert_eq!(lines, vec!["cache/", "local.toml", "proposals/"]);
}

/// FSA-FR-08 / DRS-FR-ISPI: the entries this module supersedes go — the ones
/// that ignore `drafts/`, and the unanchored spellings of the required
/// entries — and every other byte of the file stays where it was: its order,
/// its comments, and the line ending it carried.
#[test]
fn fr8_merge_gitignore_removes_the_superseded_entries_byte_for_byte() {
    // Each drafts spelling on its own, against the required set the product
    // uses. The unanchored `cache/`, `local.toml`, and `proposals/` beside
    // it are superseded in the same pass.
    for spelling in DRAFT_IGNORE_ENTRIES {
        let merged = merge_gitignore(
            &format!("# mine\ncache/\n{spelling}\nlocal.toml\nproposals/\nnode_modules/\n"),
            REQUIRED_GITIGNORE_ENTRIES,
        );
        assert_eq!(
            merged, "# mine\nnode_modules/\n/cache/\n/local.toml\n/proposals/\n",
            "{spelling:?} goes, the author's lines keep their order, and the \
             required entries arrive anchored",
        );
    }
    // A CRLF file keeps CRLF on the lines it keeps, and a file with no
    // trailing newline is not given one: a rebuild from `lines()` would
    // silently rewrite both.
    assert_eq!(
        merge_gitignore(
            "# mine\r\n/cache/\r\ndrafts/\r\n/local.toml\r\n/proposals/\r\n",
            REQUIRED_GITIGNORE_ENTRIES,
        ),
        "# mine\r\n/cache/\r\n/local.toml\r\n/proposals/\r\n",
    );
    // A file with no trailing newline keeps none: the removed line took its
    // own bytes and no other line was rewritten.
    assert_eq!(
        merge_gitignore(
            "drafts/\n/cache/\n/local.toml\n/proposals/\nnode_modules/",
            REQUIRED_GITIGNORE_ENTRIES,
        ),
        "/cache/\n/local.toml\n/proposals/\nnode_modules/",
    );
    // A file whose only line ignored the drafts gains the required entries
    // and nothing else.
    assert_eq!(
        merge_gitignore("drafts/\n", REQUIRED_GITIGNORE_ENTRIES),
        "/cache/\n/local.toml\n/proposals/\n",
    );
    // A negation ignores nothing, so it is the author's own line and stays;
    // so does a leading-whitespace line, which Git reads as a pattern for a
    // differently named folder.
    assert_eq!(
        merge_gitignore(
            "/cache/\n/local.toml\n/proposals/\n!drafts/\n  drafts/\n",
            REQUIRED_GITIGNORE_ENTRIES,
        ),
        "/cache/\n/local.toml\n/proposals/\n!drafts/\n  drafts/\n",
    );
}

#[test]
fn merge_gitignore_preserves_trailing_newline_handling() {
    // No trailing newline -> we insert one before appending.
    let merged = merge_gitignore("foo", &["cache/"]);
    assert_eq!(merged, "foo\ncache/\n");
    // Already has trailing newline -> we don't add a blank line.
    let merged = merge_gitignore("foo\n", &["cache/"]);
    assert_eq!(merged, "foo\ncache/\n");
}

#[test]
fn merge_gitignore_comments_do_not_satisfy_entries() {
    // A line `# cache/` is a comment, not an entry — `cache/` must still
    // be appended.
    let merged = merge_gitignore("# cache/\n", &["cache/"]);
    assert!(
        merged.lines().any(|l| l.trim() == "cache/" && !l.trim_start().starts_with('#')),
        "expected literal cache/ entry after comment-only existing, got: {merged:?}"
    );
}

// -----------------------------------------------------------------
// FSA-FR-09: app_data_dir is platform-correct, stable, and
// self-creating.
//
// We cannot easily simulate "fresh user account" without isolating the
// user data dir. Instead we verify:
// - the path is non-empty,
// - it ends with `synthesis`,
// - on first call (with the dir possibly already present from prior
//   runs) it returns OK and the dir exists afterward.
// -----------------------------------------------------------------
#[test]
fn ts11_app_data_dir_returns_platform_path_and_creates_directory() {
    let p = app_data_dir().unwrap();
    assert!(
        p.file_name().and_then(|s| s.to_str()) == Some("synthesis"),
        "expected path ending with `synthesis`, got {p:?}"
    );
    assert!(p.exists(), "app_data_dir must exist after call: {p:?}");
    assert!(p.is_dir(), "app_data_dir must be a directory: {p:?}");
}

#[test]
fn ts11_app_data_dir_is_stable_across_calls() {
    let a = app_data_dir().unwrap();
    let b = app_data_dir().unwrap();
    assert_eq!(a, b);
}

// -----------------------------------------------------------------
// FSA-FR-10: resolve_under rejects path-escape with a typed error
// and performs no filesystem operation.
// -----------------------------------------------------------------
#[test]
fn ts12_resolve_under_rejects_escape_with_dotdot() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    let err = resolve_under(&root, "../escape.txt").unwrap_err();
    assert!(
        matches!(err, FsError::PathEscape { .. }),
        "expected PathEscape, got {err:?}"
    );
}

#[test]
fn ts12_resolve_under_rejects_compound_escape() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    // `a/b/../../../escape` — three `..`, only two normal components
    // below root.
    let err = resolve_under(&root, "a/b/../../../escape.txt").unwrap_err();
    assert!(matches!(err, FsError::PathEscape { .. }), "got {err:?}");
}

#[test]
fn fr10_resolve_under_allows_inner_dotdot_that_stays_under_root() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    // `a/b/../c` resolves to `<root>/a/c` — never leaves the root.
    let resolved = resolve_under(root, "a/b/../c").unwrap();
    assert_eq!(resolved, root.join("a").join("c"));
}

#[test]
fn fr10_resolve_under_allows_plain_relative() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let resolved = resolve_under(root, "a/b/c.txt").unwrap();
    assert_eq!(resolved, root.join("a").join("b").join("c.txt"));
}

#[test]
fn fr10_resolve_under_rejects_absolute_path_outside_root() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    // An absolute path that is definitely outside `root`.
    let other = if cfg!(windows) { r"C:\Windows\System32" } else { "/etc/passwd" };
    let err = resolve_under(root, other).unwrap_err();
    assert!(matches!(err, FsError::PathEscape { .. }), "got {err:?}");
}

#[test]
fn fr10_resolve_under_no_filesystem_side_effects_on_escape() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    let before: Vec<_> = fs::read_dir(&root).unwrap().count().to_string().into_bytes();
    let _ = resolve_under(&root, "../../../tmp/nope");
    let after: Vec<_> = fs::read_dir(&root).unwrap().count().to_string().into_bytes();
    assert_eq!(before, after, "resolve_under must not touch the filesystem");
}

// -----------------------------------------------------------------
// FSA-FR-01: browse_for_folder cancel/select semantics (mapped via
// map_dialog_result).
// -----------------------------------------------------------------
#[test]
fn fr1_dialog_result_selected_maps_to_selected_with_absolute_path() {
    let pb = PathBuf::from("/Users/x/dev/proj");
    let r = map_dialog_result(Some(pb.clone()));
    match r {
        BrowseResult::Selected { path } => assert_eq!(path, pb.to_string_lossy()),
        BrowseResult::Cancelled => panic!("expected Selected"),
    }
}

#[test]
fn fr1_dialog_result_none_maps_to_cancelled_not_error() {
    // FSA-FR-01: cancellation MUST NOT surface as an error — it's a normal
    // outcome, distinguishable in the return type.
    let r = map_dialog_result(None);
    assert!(matches!(r, BrowseResult::Cancelled));
}

#[test]
fn fr1_browse_result_serializes_cancelled_as_externally_tagged() {
    // The frontend speaks JSON; pin the wire shape so a refactor that
    // breaks the picker UI's parse is caught here.
    let r = BrowseResult::Cancelled;
    let json = serde_json::to_string(&r).unwrap();
    assert_eq!(json, "\"cancelled\"");
}

#[test]
fn fr1_browse_result_serializes_selected_with_path_field() {
    let r = BrowseResult::Selected {
        path: "/Users/x/dev/proj".into(),
    };
    let v: serde_json::Value = serde_json::to_value(&r).unwrap();
    // Externally tagged: { "selected": { "path": "..." } }
    let inner = v.get("selected").expect("expected `selected` tag");
    assert_eq!(
        inner.get("path").and_then(|p| p.as_str()),
        Some("/Users/x/dev/proj")
    );
}

// PST-FR-26's decision half, testable without a Tauri runtime: which paths
// the command layer refuses before invoking any primitive, and why. The
// reason is what the ERROR record carries, so both arms matter.
#[cfg(unix)]
#[test]
fn resolve_inside_separates_escape_from_symlink_and_admits_ordinary_paths() {
    use std::os::unix::fs::symlink;

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("docs").join("a.md"), b"a").unwrap();
    symlink(root.join("docs"), root.join("linkdir")).unwrap();
    symlink(root.join("docs").join("a.md"), root.join("link.md")).unwrap();

    // Ordinary paths are admitted, including one that does not exist yet
    // (a create names its destination before it is there).
    assert_eq!(resolve_inside(root, "docs/a.md"), Ok(()));
    assert_eq!(resolve_inside(root, "docs/not-yet.md"), Ok(()));
    assert_eq!(resolve_inside(root, "docs"), Ok(()));

    // `..` and absolute paths escape.
    assert_eq!(resolve_inside(root, "../outside.md"), Err("escape"));
    assert_eq!(resolve_inside(root, "docs/../../outside.md"), Err("escape"));
    assert_eq!(resolve_inside(root, "/etc/hosts"), Err("escape"));

    // A link is refused as a link, whichever component carries it, and
    // whether it points inside the root or out of it.
    assert_eq!(resolve_inside(root, "link.md"), Err("symlink"));
    assert_eq!(resolve_inside(root, "linkdir/a.md"), Err("symlink"));

    // `a/../b` normalises to `b` before any syscall, so the intermediate `a`
    // is never traversed and is not what the check is about.
    assert_eq!(resolve_inside(root, "docs/../docs/a.md"), Ok(()));
}

#[test]
fn is_valid_basename_accepts_plain_and_rejects_paths() {
    assert!(is_valid_basename("a.md"));
    assert!(is_valid_basename("My File.txt"));
    for bad in ["", ".", "..", "a/b", "a\\b", "/x"] {
        assert!(!is_valid_basename(bad), "{bad:?} must be invalid");
    }
}

// -----------------------------------------------------------------------
// FSA-FR-14: create_dir
// -----------------------------------------------------------------------

// -----------------------------------------------------------------
// FSA-FR-15: append_lines.
// -----------------------------------------------------------------

