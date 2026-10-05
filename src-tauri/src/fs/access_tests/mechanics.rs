//! Mechanics only the instance can reach, and the coverage restored after review.

use super::*;

// ---------------------------------------------------------------------------
// Mechanics that only the instance can reach now (FSA-FR-19)
//
// These moved here wholesale when the gated free functions were removed: they
// exercise the *mechanics* behind the gate — atomicity, streaming, concurrent
// appends — which nothing else in the suite covers.
// ---------------------------------------------------------------------------

#[test]
fn ts7_an_interrupted_atomic_write_never_leaves_a_half_written_target() {
    // FSA-FR-04. We cannot kill the process mid-write, so we provoke the same
    // failure mode: a rename that cannot succeed. The target must be left
    // wholly its previous self.
    let (_tmp, access, root) = rooted();
    let target = root.join("rec.txt");
    access.write_text_atomic(&target, "v1").unwrap();

    // Renaming a file onto a non-empty directory fails on every platform here.
    let blocker = root.join("blocker_dir");
    fs::create_dir_all(blocker.join("nested")).unwrap();
    fs::write(blocker.join("nested").join("x"), b"x").unwrap();

    let res = access.write_bytes_atomic(&blocker, b"v2");
    assert!(res.is_err(), "expected the rename onto a non-empty dir to fail");

    assert_eq!(access.read_text(&target).unwrap(), "v1", "untouched");
    assert!(blocker.join("nested").join("x").exists(), "and so is the blocker");

    // No orphan sibling temp file survives the failure.
    let leftovers: Vec<String> = fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".blocker_dir.tmp."))
        .collect();
    assert!(leftovers.is_empty(), "orphan temp files left behind: {leftovers:?}");
}

#[test]
fn ts7_a_successful_atomic_write_leaves_only_the_target() {
    let (_tmp, access, root) = rooted();
    access.write_text_atomic(root.join("t.txt"), "hello").unwrap();
    let entries: Vec<String> = fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, vec!["t.txt".to_string()], "no temp file survives a success");
}

#[test]
fn ts8_the_checksum_streams_and_agrees_with_the_in_memory_one() {
    // FSA-FR-07: computed without holding the file in memory, and byte-identical
    // to `sha256_bytes` over the same content — which is what lets the watcher
    // compare a file it read against a body the Editor just wrote (PST-FR-16).
    let (_tmp, access, root) = rooted();
    let big: String = std::iter::repeat_n("abcdefghij", 200_000).collect();
    let p = root.join("big.bin");
    access.write_text_atomic(&p, &big).unwrap();
    assert_eq!(
        access.sha256_file(&p).unwrap(),
        crate::fs::sha256_bytes(big.as_bytes())
    );

    // And the empty-file vector.
    let e = root.join("empty.bin");
    access.write_text_atomic(&e, "").unwrap();
    assert_eq!(
        access.sha256_file(&e).unwrap(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    let err = access.sha256_file(root.join("gone.bin")).unwrap_err();
    assert!(matches!(err, FsError::NotFound { .. }), "got {err:?}");
}

#[test]
fn ts17_two_appenders_both_land_and_neither_payload_is_split() {
    // FSA-FR-15: the guarantee that makes a shared append-only log safe for a
    // second process. Each append is one write call, so two of them interleave
    // between payloads rather than inside one.
    use std::sync::Arc;
    let tmp = TempDir::new().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    let access = Arc::new(FsAccess::builder().allow_root(&root).build().unwrap());
    let log = root.join("log.jsonl");

    let handles: Vec<_> = (0..2)
        .map(|w| {
            let access = Arc::clone(&access);
            let log = log.clone();
            std::thread::spawn(move || {
                for i in 0..50 {
                    access
                        .append_lines(&log, &[format!("w{w}-{i}-aaa"), format!("w{w}-{i}-bbb")])
                        .unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }

    let body = fs::read_to_string(&log).unwrap();
    let lines: Vec<&str> = body.lines().collect();
    assert_eq!(lines.len(), 200, "every line from both writers is present");
    // No line is a splice of two payloads: each is exactly one written line.
    for l in &lines {
        assert!(
            l.starts_with("w0-") || l.starts_with("w1-"),
            "a line was split or interleaved mid-payload: {l:?}"
        );
    }
    // And each writer's pair stayed adjacent, which is the actual FR-15 claim.
    for pair in lines.chunks(2) {
        assert!(
            pair[0].ends_with("-aaa") && pair[1].ends_with("-bbb"),
            "a two-line append was split by a concurrent one: {pair:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn fr6_a_permission_failure_is_io_rather_than_missing() {
    // FSA-FR-06: "exists but cannot be read" is a different answer from
    // "is not there", and a caller that conflates them reports the wrong thing.
    use std::os::unix::fs::PermissionsExt;
    // `chmod` does not gate root, and the guard this replaced asked
    // `std::env::var("USER")` — which a container shell leaves unset, so the
    // environment most likely to run as root was the one it did not recognise.
    if crate::fs::permission_probe::skip_without_enforcement(
        "fr6_a_permission_failure_is_io_rather_than_missing",
        crate::fs::permission_probe::Injection::Read,
    ) {
        return;
    }
    let (_tmp, access, root) = rooted();
    let p = root.join("locked.bin");
    write_raw(&p, "secret");
    let original = fs::metadata(&p).unwrap().permissions();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o000)).unwrap();

    let result = access.read_bytes(&p);
    fs::set_permissions(&p, original).unwrap();

    let err = result.expect_err("expected an error on a chmod 000 file");
    assert!(matches!(err, FsError::Io(_)), "expected Io, got {err:?}");
}

// ---------------------------------------------------------------------------
// Coverage restored after review: scenarios the deleted free-function tests
// carried that the instance suite had not picked up.
// ---------------------------------------------------------------------------

#[test]
fn fr13_a_directory_is_never_copied_into_its_own_descendant() {
    // The unbounded-recursion guard. `copy_path(d, d)` exits on the earlier
    // "exists" branch, so only a *descendant* destination reaches it — which
    // means nothing was exercising it at all.
    let (_tmp, access, root) = rooted();
    let d = root.join("d");
    write_raw(&d.join("a.md"), "a");
    write_raw(&d.join("sub").join("b.md"), "b");

    let err = access.copy_path(&d, d.join("sub").join("d")).unwrap_err();
    assert!(
        matches!(err, FsError::Io(_)),
        "copying a directory into its own descendant must be refused, got {err:?}"
    );
    assert!(!d.join("sub").join("d").exists(), "and nothing is created");
    // No runaway nesting: the tree is exactly what it was.
    assert!(d.join("a.md").is_file() && d.join("sub").join("b.md").is_file());

    // A sibling copy still works, so the guard is not simply refusing everything.
    access.copy_path(&d, root.join("copy")).unwrap();
    assert_eq!(fs::read_to_string(root.join("copy").join("a.md")).unwrap(), "a");

    // FSA-FR-13: an occupied destination is the typed "exists".
    let err = access.copy_path(&d, root.join("copy")).unwrap_err();
    assert!(matches!(err, FsError::AlreadyExists { .. }), "got {err:?}");
}

#[test]
fn ts15_a_directory_tree_copies_under_the_default_policy() {
    // The recursive copy was only ever exercised with `follow_symlinks(true)`,
    // which is not the policy the application runs on.
    let (_tmp, access, root) = rooted();
    let src = root.join("tree");
    write_raw(&src.join("a.md"), "a");
    write_raw(&src.join("x").join("y").join("deep.md"), "deep");
    fs::create_dir_all(src.join("empty")).unwrap();

    access.copy_path(&src, root.join("dst")).unwrap();
    assert_eq!(fs::read_to_string(root.join("dst").join("a.md")).unwrap(), "a");
    assert_eq!(
        fs::read_to_string(root.join("dst").join("x").join("y").join("deep.md")).unwrap(),
        "deep"
    );
    assert!(root.join("dst").join("empty").is_dir(), "empty dirs come too");
    assert!(src.join("a.md").is_file(), "the source is untouched — copy, not move");
}

#[test]
fn fr11_fr12_fr13_a_missing_source_inside_a_root_is_typed_missing() {
    // Distinct from the escape refusal: these all reach the operation and find
    // nothing there, which is a different answer for a caller to act on.
    let (_tmp, access, root) = rooted();
    let absent = root.join("nope.md");

    let err = access.rename_path(&absent, "other.md").unwrap_err();
    assert!(matches!(err, FsError::NotFound { .. }), "rename: got {err:?}");
    let err = access.copy_path(&absent, root.join("dst.md")).unwrap_err();
    assert!(matches!(err, FsError::NotFound { .. }), "copy: got {err:?}");
    assert!(!root.join("dst.md").exists());
    let err = access.delete_path(&absent, true).unwrap_err();
    assert!(matches!(err, FsError::NotFound { .. }), "delete: got {err:?}");
}

#[test]
fn fr11_an_empty_directory_is_removed_under_recursive_false() {
    // The positive half of the emptiness probe of FSA-FR-11 — the half
    // `LCM-FR-11` relies on to remove a folder without asking.
    let (_tmp, access, root) = rooted();
    let e = root.join("e");
    fs::create_dir_all(&e).unwrap();
    access.delete_path(&e, false).unwrap();
    assert!(!e.exists(), "an already-empty directory goes without `recursive`");

    // And a file goes whatever `recursive` says.
    let f = root.join("f.md");
    write_raw(&f, "x");
    access.delete_path(&f, true).unwrap();
    assert!(!f.exists());
}

#[test]
fn fr5_every_write_primitive_creates_its_missing_parents() {
    // FSA-FR-05. `append_lines` has its own parent-creation path, separate from
    // the atomic writes', so each is asserted rather than assumed.
    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct C {
        name: String,
    }
    let (_tmp, access, root) = rooted();

    access.write_bytes_atomic(root.join("a/b/c/bytes.bin"), b"B").unwrap();
    access.write_text_atomic(root.join("d/e/f/text.md"), "T").unwrap();
    access
        .write_toml_atomic(root.join("g/h/i/conf.toml"), &C { name: "n".into() })
        .unwrap();
    access
        .append_lines(root.join("j/k/l/log.jsonl"), &["L".to_string()])
        .unwrap();
    access.create_file(root.join("m/n/o/new.txt"), CreateMode::Exclusive).unwrap();

    assert_eq!(fs::read(root.join("a/b/c/bytes.bin")).unwrap(), b"B");
    assert_eq!(fs::read_to_string(root.join("d/e/f/text.md")).unwrap(), "T");
    assert_eq!(access.read_toml::<C>(root.join("g/h/i/conf.toml")).unwrap().name, "n");
    assert_eq!(fs::read_to_string(root.join("j/k/l/log.jsonl")).unwrap(), "L\n");
    assert!(root.join("m/n/o/new.txt").is_file());
}

#[test]
fn ts4_read_toml_separates_missing_from_malformed_from_wrong_shape() {
    #[derive(serde::Deserialize, Debug)]
    struct C {
        #[allow(dead_code)]
        name: String,
        #[allow(dead_code)]
        count: i64,
    }
    let (_tmp, access, root) = rooted();

    let err = access.read_toml::<C>(root.join("absent.toml")).unwrap_err();
    assert!(matches!(err, FsError::NotFound { .. }), "missing: got {err:?}");

    let bad = root.join("bad.toml");
    fs::write(&bad, b"name = = =\n").unwrap();
    let err = access.read_toml::<C>(&bad).unwrap_err();
    assert!(matches!(err, FsError::Toml { .. }), "syntax: got {err:?}");

    // Valid TOML of the wrong shape — the case a schema change produces, and a
    // different failure from a syntax error even though both are `Toml`.
    let shape = root.join("shape.toml");
    fs::write(&shape, b"not_a_field = true\n").unwrap();
    let err = access.read_toml::<C>(&shape).unwrap_err();
    assert!(matches!(err, FsError::Toml { .. }), "shape: got {err:?}");
}

#[test]
fn fr8_ensure_gitignored_is_idempotent_and_line_exact() {
    let (_tmp, access, root) = rooted();
    let syn = root.join(".synthesis");

    // FSA-FR-05: it creates `.synthesis/` itself rather than needing one.
    access.ensure_gitignored(&syn).unwrap();
    assert!(syn.join(".gitignore").is_file());

    let read_lines = || -> Vec<String> {
        fs::read_to_string(syn.join(".gitignore"))
            .unwrap()
            .lines()
            .map(|l| l.trim().to_string())
            .collect()
    };
    // Line-exact, not merely "contains": a `# cache/` comment would satisfy a
    // substring check while ignoring nothing.
    for required in ["/cache/", "/local.toml", "/proposals/"] {
        assert_eq!(
            read_lines().iter().filter(|l| *l == required).count(),
            1,
            "{required} must appear exactly once, as its own line"
        );
    }

    // Idempotent: a second call appends nothing and rewrites nothing.
    let before = fs::read_to_string(syn.join(".gitignore")).unwrap();
    access.ensure_gitignored(&syn).unwrap();
    assert_eq!(fs::read_to_string(syn.join(".gitignore")).unwrap(), before);

    // A file already holding SOME required entries gains only the missing ones,
    // and the user's own lines survive in place.
    fs::write(syn.join(".gitignore"), "# mine\nnode_modules/\n/cache/\n").unwrap();
    access.ensure_gitignored(&syn).unwrap();
    let lines = read_lines();
    assert_eq!(lines.iter().filter(|l| *l == "/cache/").count(), 1, "no duplicate");
    assert!(lines.iter().any(|l| l == "node_modules/"), "user entry preserved");
    assert!(lines.iter().any(|l| l == "# mine"), "user comment preserved");
    for required in ["/local.toml", "/proposals/"] {
        assert_eq!(lines.iter().filter(|l| *l == required).count(), 1);
    }
}

/// FSA-FR-08 / DRS-FR-ISPI: an existing project's `drafts/` ignore entry is
/// removed, in every spelling that ignores the folder, and every other line of
/// the file is left exactly where it was.
#[test]
fn fr8_ts_ispi_the_drafts_ignore_entry_is_removed_and_nothing_else_moves() {
    let (_tmp, access, root) = rooted();
    let syn = root.join(".synthesis");
    fs::create_dir_all(&syn).unwrap();

    for spelling in ["drafts/", "drafts", "/drafts/", "/drafts", "drafts/**", "drafts/  "] {
        fs::write(
            syn.join(".gitignore"),
            format!("# mine\ncache/\n{spelling}\nlocal.toml\nproposals/\nnode_modules/\n"),
        )
        .unwrap();
        access.ensure_gitignored(&syn).unwrap();
        let content = fs::read_to_string(syn.join(".gitignore")).unwrap();
        assert_eq!(
            content, "# mine\nnode_modules/\n/cache/\n/local.toml\n/proposals/\n",
            "{spelling:?} goes with the superseded unanchored entries, and the \
             author's own lines stay where they were"
        );
    }

    // A negation ignores nothing, so it is the author's own line and stays.
    fs::write(syn.join(".gitignore"), "/cache/\n/local.toml\n/proposals/\n!drafts/\n").unwrap();
    access.ensure_gitignored(&syn).unwrap();
    assert_eq!(
        fs::read_to_string(syn.join(".gitignore")).unwrap(),
        "/cache/\n/local.toml\n/proposals/\n!drafts/\n",
    );

    // FSA-FR-08 / DRS-FR-ISPI: the migration writes `.gitignore` and no other
    // file, so a draft already on disk is not read, moved, or rewritten by it.
    let draft = syn.join("drafts/a-draft/files");
    fs::create_dir_all(&draft).unwrap();
    fs::write(draft.join("p.md"), "the prompt").unwrap();
    fs::write(syn.join(".gitignore"), "drafts/\n").unwrap();
    access.ensure_gitignored(&syn).unwrap();
    assert_eq!(fs::read_to_string(draft.join("p.md")).unwrap(), "the prompt");
}

#[cfg(unix)]
#[test]
fn ts19_the_remaining_operations_refuse_a_link_and_leave_no_trace() {
    // Ops the TS-19 sweep did not name: `copy_path` with a symlinked leaf,
    // `list_dir` on one, and `create_dir` through a symlinked ancestor. Plus
    // the leg that proves a refused write leaves no orphan temp file.
    let (_tmp, access, root) = rooted();
    let target = root.join("real.md");
    write_raw(&target, "target");
    let link = root.join("link.md");
    symlink(&target, &link).unwrap();


    let err = access.copy_path(&link, root.join("copy.md")).unwrap_err();
    assert!(matches!(err, FsError::SymlinkRefused { .. }), "copy: got {err:?}");
    assert!(!root.join("copy.md").exists());

    let err = access.list_dir(&link).unwrap_err();
    assert!(matches!(err, FsError::SymlinkRefused { .. }), "list_dir: got {err:?}");

    let realdir = root.join("realdir");
    fs::create_dir_all(&realdir).unwrap();
    symlink(&realdir, root.join("linkdir")).unwrap();
    let err = access.create_dir(root.join("linkdir").join("sub")).unwrap_err();
    assert!(matches!(err, FsError::SymlinkRefused { .. }), "create_dir: got {err:?}");
    assert!(!realdir.join("sub").exists(), "nothing was created through the link");

    // A refused write leaves no sibling temp file behind.
    let _ = access.write_text_atomic(&link, "x");
    let strays: Vec<String> = fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp."))
        .collect();
    assert!(strays.is_empty(), "refused write left temp files: {strays:?}");
}

#[test]
fn root_fs_resolves_relative_against_its_root_and_confines_the_under_helpers() {
    // `RootFs` had no direct tests: `abs()`'s dispatch and the four `*_under`
    // helpers were only reached through consumer modules.
    let tmp = TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(tmp.path());
    write_raw(&root.join("a.md"), "hello");

    // Relative joins to the root; absolute is taken as given.
    assert_eq!(root.read_text("a.md").unwrap(), "hello");
    assert_eq!(root.read_text(root.join("a.md")).unwrap(), "hello");

    // The `*_under` helpers keep the caller's own boundary: a `..` that leaves
    // `base` is the module-level `PathEscape`, decided before the sandbox is
    // consulted at all.
    let base = root.join("sub");
    fs::create_dir_all(&base).unwrap();
    let err = root.delete_under(&base, "../a.md", false).unwrap_err();
    assert!(matches!(err, FsError::PathEscape { .. }), "got {err:?}");
    assert!(root.join("a.md").is_file(), "and nothing was removed");

    for r in [
        root.rename_under(&base, "../a.md", "b.md"),
        root.copy_under(&base, "../a.md", "c.md"),
        root.create_dir_under(&base, "../d"),
    ] {
        assert!(matches!(r.unwrap_err(), FsError::PathEscape { .. }));
    }
    assert!(!root.join("b.md").exists() && !root.join("c.md").exists());

    // And the happy path still works within the base.
    write_raw(&base.join("x.md"), "x");
    root.rename_under(&base, "x.md", "y.md").unwrap();
    assert!(base.join("y.md").is_file() && !base.join("x.md").exists());
}
