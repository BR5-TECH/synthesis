//! Hardening added after review, clustered on the followed-symlink branch.

use super::*;

// ---------------------------------------------------------------------------
// Hardening added after review. The gaps clustered in one place: the
// `follow_symlinks(true)` branch, which the application never takes today and
// which therefore nothing else would catch.
// ---------------------------------------------------------------------------

/// FSA-FR-18 NFR: one instance is shared across the whole backend as an `Arc`,
/// which is only sound while the type stays thread-safe. A `Cell`/`Rc` field
/// added later would break that silently; this fails to compile instead.
#[test]
fn the_instance_is_shareable_across_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<FsAccess>();
    assert_send_sync::<FsAccessState>();
}

#[cfg(unix)]
#[test]
fn open_file_refuses_a_link_in_every_write_mode() {
    // Review M1: `Append` and `Truncate` through a link are the modes that
    // would write *into the link's target*, and neither was covered.
    let (_tmp, access, root) = rooted();
    let target = root.join("real.txt");
    write_raw(&target, "target content");
    let before = fs::metadata(&target).unwrap().modified().unwrap();
    let link = root.join("link.txt");
    symlink(&target, &link).unwrap();

    for mode in [OpenMode::ReadOnly, OpenMode::Append, OpenMode::Truncate] {
        let err = access.open_file(&link, mode).err().expect("must refuse");
        assert!(
            matches!(err, FsError::SymlinkRefused { .. }),
            "{mode:?} through a link must be SymlinkRefused, got {err:?}"
        );
    }
    assert_eq!(fs::read_to_string(&target).unwrap(), "target content");
    assert_eq!(fs::metadata(&target).unwrap().modified().unwrap(), before);
}

#[cfg(unix)]
#[test]
fn append_lines_and_ensure_gitignored_refuse_a_link_too() {
    // Review 4i: both were absent from the TS-19 sweep, and `append_lines`
    // returns early on an empty batch so its gate ordering genuinely differs.
    let (_tmp, access, root) = rooted();
    let target = root.join("real.jsonl");
    write_raw(&target, "keep\n");
    let link = root.join("log.jsonl");
    symlink(&target, &link).unwrap();

    let err = access
        .append_lines(&link, &["injected".to_string()])
        .unwrap_err();
    assert!(matches!(err, FsError::SymlinkRefused { .. }), "got {err:?}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "keep\n");

    // An empty batch short-circuits before the gate; that is a deliberate
    // no-op, and the point is that it still writes nothing.
    access.append_lines(&link, &[]).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "keep\n");

    // `.gitignore` itself being a link is refused rather than written through.
    let syn = root.join(".synthesis");
    fs::create_dir_all(&syn).unwrap();
    let real_ignore = root.join("real.gitignore");
    write_raw(&real_ignore, "# theirs\n");
    symlink(&real_ignore, syn.join(".gitignore")).unwrap();
    let err = access.ensure_gitignored(&syn).unwrap_err();
    assert!(matches!(err, FsError::SymlinkRefused { .. }), "got {err:?}");
    assert_eq!(fs::read_to_string(&real_ignore).unwrap(), "# theirs\n");
}

#[cfg(unix)]
#[test]
fn following_validates_a_relative_target_the_same_as_an_absolute_one() {
    // Review M3: every symlink in the original suite had an ABSOLUTE target, so
    // the join-then-normalise branch for relative targets — the textbook
    // `../../../etc` escape — was never executed.
    let tmp = TempDir::new().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    let outside_dir = TempDir::new().unwrap();
    let outside = fs::canonicalize(outside_dir.path()).unwrap();
    let access = FsAccess::builder()
        .allow_root(&root)
        .follow_symlinks(true)
        .build()
        .unwrap();

    // A relative target that stays inside, nested deep enough that the
    // `probe.parent()` join actually matters.
    let deep = root.join("a").join("b");
    write_raw(&deep.join("real.md"), "inside");
    symlink("real.md", deep.join("sibling")).unwrap();
    assert_eq!(access.read_text(deep.join("sibling")).unwrap(), "inside");

    // A relative target that climbs back up but stays inside.
    write_raw(&root.join("top.md"), "top");
    symlink("../../top.md", deep.join("up")).unwrap();
    assert_eq!(access.read_text(deep.join("up")).unwrap(), "top");

    // A relative target that climbs clean out of the root is refused, and the
    // file it points at is never opened.
    write_raw(&outside.join("secret.md"), "secret");
    let hops = root.components().count();
    let escape: PathBuf = std::iter::repeat_n("..", hops + 4)
        .collect::<PathBuf>()
        .join(outside.strip_prefix("/").unwrap())
        .join("secret.md");
    symlink(&escape, deep.join("escape")).unwrap();
    let err = access.read_text(deep.join("escape")).unwrap_err();
    assert!(
        matches!(err, FsError::EscapesAllowedRoots { .. }),
        "a relative target escaping the root must be refused, got {err:?}"
    );
}

#[cfg(unix)]
#[test]
fn following_refuses_an_ancestor_link_that_escapes() {
    // Review M2: the ancestor rule was only tested in the direction that
    // succeeds, so a refactor validating only the final hop would stay green.
    let tmp = TempDir::new().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    let outside_dir = TempDir::new().unwrap();
    let outside = fs::canonicalize(outside_dir.path()).unwrap();
    let access = FsAccess::builder()
        .allow_root(&root)
        .follow_symlinks(true)
        .build()
        .unwrap();

    write_raw(&outside.join("f.md"), "outside content");
    symlink(&outside, root.join("linkdir")).unwrap();

    let err = access.read_text(root.join("linkdir").join("f.md")).unwrap_err();
    assert!(
        matches!(err, FsError::EscapesAllowedRoots { .. }),
        "an escaping ANCESTOR link must be refused, got {err:?}"
    );
    // And a write through it leaves the target byte-for-byte intact.
    let err = access
        .write_text_atomic(root.join("linkdir").join("f.md"), "clobbered")
        .unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
    assert_eq!(fs::read_to_string(outside.join("f.md")).unwrap(), "outside content");
}

#[cfg(unix)]
#[test]
fn following_refuses_a_write_through_an_escaping_link_without_touching_the_target() {
    // Review M5: the escaping-link case was only ever exercised as a READ.
    // Containment failures on the write side are the ones that destroy data.
    let tmp = TempDir::new().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    let outside_dir = TempDir::new().unwrap();
    let outside = fs::canonicalize(outside_dir.path()).unwrap();
    let access = FsAccess::builder()
        .allow_root(&root)
        .follow_symlinks(true)
        .build()
        .unwrap();

    let target = outside.join("x.md");
    write_raw(&target, "secret");
    let before = fs::metadata(&target).unwrap().modified().unwrap();
    symlink(&target, root.join("out")).unwrap();

    for result in [
        access.write_text_atomic(root.join("out"), "clobbered"),
        access.delete_path(root.join("out"), false),
        access.append_lines(root.join("out"), &["x".to_string()]),
        access.create_file(root.join("out"), CreateMode::Truncate),
    ] {
        let err = result.unwrap_err();
        assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
    }
    assert_eq!(fs::read_to_string(&target).unwrap(), "secret");
    assert_eq!(fs::metadata(&target).unwrap().modified().unwrap(), before);
}

#[cfg(unix)]
#[test]
fn following_copies_a_tree_holding_an_inside_link_without_leaving_a_partial_copy() {
    // Review M4 — this was a live defect: `check_tree` accepted an
    // inside-pointing link under follow mode, then the recursive copy refused
    // it unconditionally AFTER the destination had already been created.
    let tmp = TempDir::new().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    let access = FsAccess::builder()
        .allow_root(&root)
        .follow_symlinks(true)
        .build()
        .unwrap();

    let tree = root.join("tree");
    write_raw(&tree.join("a.md"), "a");
    write_raw(&tree.join("nested").join("b.md"), "b");
    write_raw(&root.join("target.md"), "linked content");
    symlink(root.join("target.md"), tree.join("link.md")).unwrap();

    let dst = root.join("copy");
    access.copy_path(&tree, &dst).expect("an inside link is followed, not refused");

    assert_eq!(fs::read_to_string(dst.join("a.md")).unwrap(), "a");
    assert_eq!(fs::read_to_string(dst.join("nested").join("b.md")).unwrap(), "b");
    // The link was dereferenced: the copy holds the target's CONTENT, and the
    // copy itself is a regular file rather than a second link.
    assert_eq!(fs::read_to_string(dst.join("link.md")).unwrap(), "linked content");
    assert!(!fs::symlink_metadata(dst.join("link.md")).unwrap().file_type().is_symlink());
}

#[cfg(unix)]
#[test]
fn following_refuses_a_tree_copy_before_creating_the_destination() {
    // The companion of the test above: when the tree's link ESCAPES, the
    // refusal must precede every write, destination directory included.
    let tmp = TempDir::new().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    let outside_dir = TempDir::new().unwrap();
    let outside = fs::canonicalize(outside_dir.path()).unwrap();
    let access = FsAccess::builder()
        .allow_root(&root)
        .follow_symlinks(true)
        .build()
        .unwrap();

    let tree = root.join("tree");
    write_raw(&tree.join("a.md"), "a");
    write_raw(&outside.join("t.md"), "t");
    symlink(outside.join("t.md"), tree.join("escape")).unwrap();

    let dst = root.join("copy");
    let err = access.copy_path(&tree, &dst).unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
    assert!(
        !dst.exists(),
        "the destination must not exist at all after a refused copy"
    );
}

#[cfg(unix)]
#[test]
fn following_accepts_a_link_into_a_different_allowlisted_root() {
    // Review 4b: containment is decided against the whole root set, so a link
    // from one root into another is legitimate. Pinned so a future
    // "must stay in the same root" refactor is a deliberate change.
    let a_dir = TempDir::new().unwrap();
    let b_dir = TempDir::new().unwrap();
    let a = fs::canonicalize(a_dir.path()).unwrap();
    let b = fs::canonicalize(b_dir.path()).unwrap();
    let access = FsAccess::builder()
        .allow_root(&a)
        .allow_root(&b)
        .follow_symlinks(true)
        .build()
        .unwrap();

    write_raw(&b.join("shared.md"), "from b");
    symlink(b.join("shared.md"), a.join("into-b")).unwrap();
    assert_eq!(access.read_text(a.join("into-b")).unwrap(), "from b");
}

#[cfg(unix)]
#[test]
fn following_reports_a_long_acyclic_chain_rather_than_walking_it_forever() {
    // Review 4d: the hop bound was a dead branch — both cycle tests trip the
    // visited-set instead. A long ACYCLIC chain is the only thing that reaches
    // `hops > MAX_SYMLINK_HOPS`.
    let tmp = TempDir::new().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    let access = FsAccess::builder()
        .allow_root(&root)
        .follow_symlinks(true)
        .build()
        .unwrap();

    write_raw(&root.join("link-0"), "end of the chain");
    for i in 1..=45 {
        symlink(root.join(format!("link-{}", i - 1)), root.join(format!("link-{i}"))).unwrap();
    }
    // A short chain resolves.
    assert_eq!(access.read_text(root.join("link-5")).unwrap(), "end of the chain");
    // A chain past the bound is refused rather than walked.
    let err = access.read_text(root.join("link-45")).unwrap_err();
    assert!(
        matches!(err, FsError::SymlinkCycle { .. }),
        "a chain longer than the hop bound must be refused, got {err:?}"
    );
}

#[cfg(unix)]
#[test]
fn file_info_identifies_a_link_under_both_policies() {
    // Review 4f: FSA-FR-23 states unconditionally that `file_info` resolves
    // nothing. Under follow mode it previously reported the TARGET's kind,
    // which would break `ASC-artifact-scanning` ASC-FR-23's "is this a link?"
    // question for any follow-mode instance.
    let tmp = TempDir::new().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    write_raw(&root.join("real.md"), "content");
    symlink(root.join("real.md"), root.join("link.md")).unwrap();

    for follow in [false, true] {
        let access = FsAccess::builder()
            .allow_root(&root)
            .follow_symlinks(follow)
            .build()
            .unwrap();
        let info = access.file_info(root.join("link.md")).unwrap();
        assert_eq!(
            info.kind,
            EntryKind::Symlink,
            "file_info must name a link as a link with follow_symlinks({follow})"
        );
        assert_eq!(
            info.size,
            fs::symlink_metadata(root.join("link.md")).unwrap().len(),
            "and report the link's OWN size"
        );
    }
}

#[cfg(unix)]
#[test]
fn following_handles_dangling_links_on_their_own_terms() {
    // Review 4c: none of the four dangling combinations was covered.
    let tmp = TempDir::new().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    let outside_dir = TempDir::new().unwrap();
    let outside = fs::canonicalize(outside_dir.path()).unwrap();

    symlink(root.join("gone.md"), root.join("dangling-inside")).unwrap();
    symlink(outside.join("gone.md"), root.join("dangling-outside")).unwrap();

    // Default policy: both are refused as links, without resolving either.
    let refusing = FsAccess::builder().allow_root(&root).build().unwrap();
    for name in ["dangling-inside", "dangling-outside"] {
        let err = refusing.read_text(root.join(name)).unwrap_err();
        assert!(matches!(err, FsError::SymlinkRefused { .. }), "{name}: got {err:?}");
    }

    // Follow policy: the one pointing inside is resolved and then simply
    // missing; the one pointing outside is refused for leaving the allowlist.
    let following = FsAccess::builder()
        .allow_root(&root)
        .follow_symlinks(true)
        .build()
        .unwrap();
    let err = following.read_text(root.join("dangling-inside")).unwrap_err();
    assert!(matches!(err, FsError::NotFound { .. }), "got {err:?}");
    let err = following.read_text(root.join("dangling-outside")).unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
}

#[test]
fn create_file_exclusive_lets_exactly_one_of_many_racing_callers_win() {
    // FSA-FR-24 states the race guarantee in prose ("two callers racing to
    // create the same path cannot both succeed") and nothing exercised it.
    use std::sync::Arc;
    let tmp = TempDir::new().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    let access = Arc::new(FsAccess::builder().allow_root(&root).build().unwrap());
    let target = root.join("contended.txt");

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let access = Arc::clone(&access);
            let target = target.clone();
            std::thread::spawn(move || access.create_file(&target, CreateMode::Exclusive))
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    let winners = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(winners, 1, "exactly one caller may create the file");
    for r in results.into_iter().filter(|r| r.is_err()) {
        let err = r.unwrap_err();
        assert!(matches!(err, FsError::AlreadyExists { .. }), "got {err:?}");
    }
}

#[test]
fn the_gate_answers_the_awkward_path_shapes() {
    let (_tmp, access, root) = rooted();

    // An empty directory lists as empty rather than erroring.
    let empty = root.join("empty");
    fs::create_dir_all(&empty).unwrap();
    assert_eq!(access.list_dir(&empty).unwrap(), vec![]);

    // A zero-length file is a file with size 0 and the empty-string checksum.
    let zero = root.join("zero.bin");
    fs::write(&zero, b"").unwrap();
    let info = access.file_info(&zero).unwrap();
    assert_eq!(info.kind, EntryKind::File);
    assert_eq!(info.size, 0);
    assert_eq!(access.read_text(&zero).unwrap(), "");
    assert_eq!(
        access.sha256_file(&zero).unwrap(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );

    // A trailing separator is dropped by component parsing, so it is a no-op.
    write_raw(&root.join("d").join("f.md"), "x");
    let with_sep = PathBuf::from(format!("{}/", root.join("d").display()));
    assert_eq!(access.list_dir(&with_sep).unwrap().len(), 1);

    // `..` at the very start flattens to the filesystem root and is then
    // judged as escaping — never popped above `/`.
    let err = access.read_text(Path::new("/../../../etc/hosts")).unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");

    // A `..`-only relative path is caught as relative, before normalisation.
    let err = access.read_text(Path::new("../x")).unwrap_err();
    assert!(matches!(err, FsError::RelativePath { .. }), "got {err:?}");

    // A sibling root whose name merely EXTENDS this one is not inside it:
    // containment is component-wise, not a string prefix.
    let sibling = PathBuf::from(format!("{}-evil", root.display()));
    fs::create_dir_all(&sibling).unwrap();
    let err = access.write_text_atomic(sibling.join("x.md"), "x").unwrap_err();
    assert!(
        matches!(err, FsError::EscapesAllowedRoots { .. }),
        "a name-extending sibling must not match the root, got {err:?}"
    );
    assert!(!sibling.join("x.md").exists());
    fs::remove_dir_all(&sibling).unwrap();
}

#[cfg(unix)]
#[test]
fn file_info_reports_a_read_only_file_as_read_only() {
    // `FileInfo.readonly` was only ever asserted `false`, so a hardcoded
    // `false` would have passed.
    use std::os::unix::fs::PermissionsExt;
    let (_tmp, access, root) = rooted();
    let p = root.join("locked.txt");
    write_raw(&p, "x");
    fs::set_permissions(&p, fs::Permissions::from_mode(0o444)).unwrap();
    assert!(access.file_info(&p).unwrap().readonly);
    fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(!access.file_info(&p).unwrap().readonly);
}

#[test]
fn a_session_temp_dir_outlives_the_swap_only_while_an_arc_still_holds_it() {
    // FSA-FR-20 meets `FsAccessState::replace`: the outgoing instance drops
    // when the LAST reference does, not when it is swapped out.
    let tmp = TempDir::new().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    let state = FsAccessState::default();
    state.install_for_worktree(&root).unwrap();

    let first = state.get().unwrap();
    let first_temp = first.session_temp_dir().unwrap().to_path_buf();
    assert!(first_temp.is_dir());

    // Swap while still holding the Arc: the directory survives, because the
    // instance that owns it is still alive in this scope.
    state.install_for_worktree(&root).unwrap();
    assert!(
        first_temp.is_dir(),
        "an outstanding Arc keeps the outgoing instance, and its temp dir, alive"
    );

    // Release it, and the directory goes with the instance.
    drop(first);
    assert!(
        !first_temp.exists(),
        "once the last reference is gone the temp directory is removed"
    );
}

