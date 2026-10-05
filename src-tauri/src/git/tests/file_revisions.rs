//! The `get_file_revisions` scenarios (GTC-FR-16): both sides of a path,
//! read whole rather than windowed.
//!
//! One part of `mod.rs`, which holds the fixtures these run against.

use super::*;

// -----------------------------------------------------------------------
// File revisions (GTC-FR-16)
// -----------------------------------------------------------------------

#[test]
fn file_revisions_return_both_sides_whole_rather_than_windowed() {
    // GTC-FR-16: `old` is the committed text, `new` the working-directory
    // text, each complete — that is what the side-by-side mode renders from,
    // and windowing either would leave it unable to show the whole file.
    let f = Fixture::new();
    let body: String = (1..=40).map(|n| format!("line {n}\n")).collect();
    f.write("notes.md", &body);
    f.commit("seed");
    f.write("notes.md", &body.replace("line 20\n", "line twenty\n"));

    let revisions = file_revisions_for_scope(
        &crate::fs::RootFs::for_root(f.root()),
        &DiffScope::Path {
            path: "notes.md".into(),
            previous_path: None,
        },
    )
    .unwrap();

    assert!(!revisions.is_binary);
    assert_eq!(revisions.old.as_deref(), Some(body.as_str()));
    assert!(revisions.new.as_deref().unwrap().contains("line twenty\n"));
    assert_eq!(
        revisions.new.as_deref().unwrap().lines().count(),
        40,
        "the new side is the whole file, not just the changed region"
    );
}

#[test]
fn an_added_file_has_no_old_side_and_a_deleted_one_has_no_new_side() {
    // GTC-FR-16: `null` rather than an empty string, so the Diff tab can
    // tell "this revision has no such file" from "the file is empty".
    let f = Fixture::new();
    f.write("kept.md", "one\n");
    f.write("removed.md", "gone\n");
    f.commit("seed");
    f.write("added.md", "brand new\n");
    std::fs::remove_file(f.root().join("removed.md")).unwrap();

    let added = file_revisions_for_scope(
        &crate::fs::RootFs::for_root(f.root()),
        &DiffScope::Path {
            path: "added.md".into(),
            previous_path: None,
        },
    )
    .unwrap();
    assert_eq!(added.old, None);
    assert_eq!(added.new.as_deref(), Some("brand new\n"));

    let removed = file_revisions_for_scope(
        &crate::fs::RootFs::for_root(f.root()),
        &DiffScope::Path {
            path: "removed.md".into(),
            previous_path: None,
        },
    )
    .unwrap();
    assert_eq!(removed.old.as_deref(), Some("gone\n"));
    assert_eq!(removed.new, None);
}

#[test]
fn an_emptied_file_reads_as_empty_text_rather_than_as_absent() {
    // The distinction GTC-FR-16 draws only earns its keep if the empty case
    // really comes back as `Some("")`.
    let f = Fixture::new();
    f.write("notes.md", "had content\n");
    f.commit("seed");
    f.write("notes.md", "");

    let revisions = file_revisions_for_scope(
        &crate::fs::RootFs::for_root(f.root()),
        &DiffScope::Path {
            path: "notes.md".into(),
            previous_path: None,
        },
    )
    .unwrap();
    assert_eq!(revisions.new.as_deref(), Some(""));
}

#[test]
fn a_binary_path_returns_neither_text_and_agrees_with_get_diff() {
    // GTC-FR-16: the two operations must never disagree about what a path
    // is, or the tab would render a binary state under one mode and
    // mojibake under another.
    let f = Fixture::new();
    f.write_bytes("logo.png", b"\x89PNG\r\n\x1a\n\x00\x00stuff");
    f.commit("seed");
    f.write_bytes("logo.png", b"\x89PNG\r\n\x1a\n\x00\x00different");

    let scope = DiffScope::Path {
        path: "logo.png".into(),
        previous_path: None,
    };
    let revisions = file_revisions_for_scope(&crate::fs::RootFs::for_root(f.root()), &scope).unwrap();
    let diff = diff_for_scope(&f.root(), &scope).unwrap();

    assert!(revisions.is_binary);
    assert_eq!(revisions.old, None);
    assert_eq!(revisions.new, None);
    assert!(diff.is_binary, "get_diff flags the same path binary");
}

#[test]
fn a_renamed_entry_reads_its_old_side_from_the_pre_rename_path() {
    // Without the previous path the old revision reads as absent and the
    // whole file renders as an addition — contradicting the `+n −n` the
    // panel row that opened the tab already showed (CHC-FR-09).
    let f = Fixture::new();
    f.write("main.rs", "fn main() {}\n");
    f.commit("seed");
    std::fs::rename(f.root().join("main.rs"), f.root().join("lib.rs")).unwrap();
    f.write("lib.rs", "fn main() {}\nfn extra() {}\n");

    let revisions = file_revisions_for_scope(
        &crate::fs::RootFs::for_root(f.root()),
        &DiffScope::Path {
            path: "lib.rs".into(),
            previous_path: Some("main.rs".into()),
        },
    )
    .unwrap();

    assert_eq!(revisions.old.as_deref(), Some("fn main() {}\n"));
    assert_eq!(
        revisions.new.as_deref(),
        Some("fn main() {}\nfn extra() {}\n")
    );
}

#[test]
fn a_branch_scope_reads_its_old_side_from_the_merge_base() {
    // The Branch comparison's base is the merge-base, resolved exactly as
    // `get_diff` resolves it, so the two modes describe one comparison.
    let f = Fixture::new();
    f.write("a.md", "base\n");
    f.commit("base");
    let base_branch = f.current_branch();
    f.branch_and_checkout("feature");
    f.write("a.md", "on the feature branch\n");
    f.commit("feature work");
    f.write("a.md", "on the feature branch, edited\n");

    let revisions = file_revisions_for_scope(
        &crate::fs::RootFs::for_root(f.root()),
        &DiffScope::Branch {
            path: "a.md".into(),
            target_branch: base_branch,
            previous_path: None,
        },
    )
    .unwrap();

    assert_eq!(revisions.old.as_deref(), Some("base\n"));
    assert_eq!(
        revisions.new.as_deref(),
        Some("on the feature branch, edited\n"),
        "the new side is the working directory, committed edits included"
    );
}

#[test]
fn the_staged_set_names_no_file_so_it_is_the_typed_error() {
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("seed");
    f.write("a.md", "two\n");
    f.stage_all();

    assert_eq!(
        file_revisions_for_scope(&crate::fs::RootFs::for_root(f.root()), &DiffScope::Staged).unwrap_err(),
        ERR_SCOPE_NOT_A_FILE,
    );
}

#[test]
fn file_revisions_outside_a_repository_are_the_typed_error() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("a.md"), "one\n").unwrap();

    assert_eq!(
        file_revisions_for_scope(
            &crate::fs::RootFs::for_root(dir.path()),
            &DiffScope::Path {
                path: "a.md".into(),
                previous_path: None,
            },
        )
        .unwrap_err(),
        ERR_NOT_A_REPO,
    );
}

#[test]
fn a_path_escaping_the_content_root_is_refused_rather_than_read() {
    // The path arrives from the frontend. Resolving it under the root is
    // what keeps `../` from turning a Diff tab into a file reader for the
    // rest of the disk — and the refusal is an error, not a quiet "this
    // revision has no such file", which would read as a deletion.
    let f = Fixture::new();
    f.write("a.md", "inside\n");
    f.commit("seed");
    // The escaped path has to resolve to a real file for the refusal to be
    // about the escape rather than about a missing file, and "outside the
    // root" means the fixture's parent — which is the shared temp directory
    // itself, not a directory this test owns. The name is therefore made
    // unique per process and removed afterwards: as a fixed `outside.md` it
    // was one path on the machine, so a second run of this suite under a
    // different user found it already there and owned by someone else, and
    // failed on the write with `PermissionDenied` every single time.
    let name = format!("outside-{}.md", std::process::id());
    let outside = f.root().parent().unwrap().join(&name);
    std::fs::write(&outside, "secret\n").unwrap();

    let err = file_revisions_for_scope(
        &crate::fs::RootFs::for_root(f.root()),
        &DiffScope::Path {
            path: format!("../{name}"),
            previous_path: None,
        },
    )
    .unwrap_err();

    assert!(!err.contains("secret"), "and it discloses nothing: {err}");
    assert!(
        std::fs::read_to_string(&outside).unwrap().contains("secret"),
        "the file outside the root is untouched"
    );
    let _ = std::fs::remove_file(&outside);
}

#[test]
fn an_unreadable_path_is_an_error_rather_than_a_reported_deletion() {
    // `new: null` means "the comparison has no version here", which the
    // Diff tab renders as "this file does not exist in the new revision".
    // A read that failed for any other reason must not claim that.
    let f = Fixture::new();
    f.write("notes.md", "content\n");
    f.commit("seed");
    // Replace the file with a directory: present, but not readable as a file.
    std::fs::remove_file(f.root().join("notes.md")).unwrap();
    std::fs::create_dir(f.root().join("notes.md")).unwrap();

    let result = file_revisions_for_scope(
        &crate::fs::RootFs::for_root(f.root()),
        &DiffScope::Path {
            path: "notes.md".into(),
            previous_path: None,
        },
    );
    assert!(
        result.is_err(),
        "a failed read must not be reported as a deletion: {result:?}"
    );
}

#[test]
fn a_checkout_whose_line_endings_are_filtered_still_reads_as_unchanged() {
    // GTC-FR-16: the two operations must never disagree. `get_diff`
    // compares the *filtered* working-directory text, so under `eol=crlf`
    // it reports no change for an LF blob checked out as CRLF. Reading raw
    // bytes on both sides would make every line of a Windows checkout
    // render as changed under a diff that showed nothing.
    let f = Fixture::new();
    f.write(".gitattributes", "*.md text eol=crlf\n");
    f.write("notes.md", "one\ntwo\nthree\n");
    f.commit("seed");
    // What the checkout holds under that attribute.
    f.write_bytes("notes.md", b"one\r\ntwo\r\nthree\r\n");

    let scope = DiffScope::Path {
        path: "notes.md".into(),
        previous_path: None,
    };
    let diff = diff_for_scope(&f.root(), &scope).unwrap();
    let revisions = file_revisions_for_scope(&crate::fs::RootFs::for_root(f.root()), &scope).unwrap();

    assert!(
        diff.hunks.is_empty(),
        "the filtered comparison sees no change: {:?}",
        all_lines(&diff)
    );
    assert_eq!(
        revisions.old, revisions.new,
        "so neither may the whole-file read"
    );
    assert_eq!(revisions.new.as_deref(), Some("one\ntwo\nthree\n"));
}

#[test]
fn a_path_marked_undiffable_by_attributes_is_binary_to_both_operations() {
    // `-diff` makes a path binary to Git whatever its bytes say. A tab that
    // read it as text here would show the binary state in one mode and the
    // file's text in another.
    let f = Fixture::new();
    f.write(".gitattributes", "*.dat -diff\n");
    f.write("data.dat", "plain text, no NUL in sight\n");
    f.commit("seed");
    f.write("data.dat", "plain text, edited\n");

    let scope = DiffScope::Path {
        path: "data.dat".into(),
        previous_path: None,
    };
    let diff = diff_for_scope(&f.root(), &scope).unwrap();
    let revisions = file_revisions_for_scope(&crate::fs::RootFs::for_root(f.root()), &scope).unwrap();

    assert!(diff.is_binary, "get_diff honours the attribute");
    assert!(revisions.is_binary, "and so must the whole-file read");
    assert_eq!(revisions.old, None);
    assert_eq!(revisions.new, None);
}

#[test]
fn a_nested_project_reads_both_sides_of_the_same_file() {
    // The old side is resolved through the repository prefix and the new
    // side through the content root. If those ever named different files
    // the tab would diff one file against another, silently.
    let f = Fixture::new();
    f.write("proj/a.md", "a\n");
    f.write("proj/b.md", "b\n");
    f.commit("seed");
    f.write("proj/a.md", "a\nfrom a\n");
    f.write("proj/b.md", "b\nfrom b\n");

    let revisions = file_revisions_for_scope(
        &crate::fs::RootFs::for_root(f.root().join("proj")),
        &DiffScope::Path {
            path: "a.md".into(),
            previous_path: None,
        },
    )
    .unwrap();

    assert_eq!(revisions.old.as_deref(), Some("a\n"));
    assert_eq!(revisions.new.as_deref(), Some("a\nfrom a\n"));
    assert!(
        !revisions.new.as_deref().unwrap().contains("from b"),
        "a sibling file must not leak into either side"
    );
}

#[test]
fn a_branch_scope_reports_an_added_and_a_deleted_file_the_same_way() {
    // GTC-FR-16's `null` sides, taken against the merge base rather than
    // HEAD — the Branch comparison the Changes panel offers.
    let f = Fixture::new();
    f.write("kept.md", "base\n");
    f.write("removed.md", "gone\n");
    f.commit("base");
    let base_branch = f.current_branch();
    f.branch_and_checkout("feature");
    f.write("added.md", "brand new\n");
    std::fs::remove_file(f.root().join("removed.md")).unwrap();

    let revisions = |path: &str| {
        file_revisions_for_scope(
            &crate::fs::RootFs::for_root(f.root()),
            &DiffScope::Branch {
                path: path.into(),
                target_branch: base_branch.clone(),
                previous_path: None,
            },
        )
        .unwrap()
    };

    assert_eq!(revisions("added.md").old, None);
    assert_eq!(revisions("added.md").new.as_deref(), Some("brand new\n"));
    assert_eq!(revisions("removed.md").old.as_deref(), Some("gone\n"));
    assert_eq!(revisions("removed.md").new, None);
}

#[test]
fn file_revisions_read_the_active_worktree_and_no_other() {
    // GTC-FR-02, GTC-FR-16: a repository's linked worktree holds different content at
    // the same path; the answer must describe the worktree that roots the
    // project (GTC-FR-02).
    let f = WorktreeFixture::new();
    f.branch("side");
    let linked =
        crate::worktree::create_worktree_at(&f.root(), "side", &f.sibling("wt").to_string_lossy())
            .unwrap();
    std::fs::write(linked.join("a.md"), "linked worktree content\n").unwrap();
    f.write("a.md", "primary worktree content\n");

    let scope = DiffScope::Path {
        path: "a.md".into(),
        previous_path: None,
    };
    let primary = file_revisions_for_scope(&crate::fs::RootFs::for_root(f.root()), &scope).unwrap();
    let from_linked = file_revisions_for_scope(&crate::fs::RootFs::for_root(linked), &scope).unwrap();

    assert_eq!(primary.new.as_deref(), Some("primary worktree content\n"));
    assert_eq!(
        from_linked.new.as_deref(),
        Some("linked worktree content\n"),
        "each worktree answers for itself"
    );
}
