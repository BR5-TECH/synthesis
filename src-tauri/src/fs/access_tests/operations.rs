//! Directory listing, file info, creation, handles, the symlink policy, and copy.

use super::*;

// ---------------------------------------------------------------------------
// FSA-FR-22 — list_dir (FSA-FR-22)
// ---------------------------------------------------------------------------

#[test]
fn ts24_list_dir_reports_one_level_sorted_and_unfiltered() {
    let (_tmp, access, root) = rooted();
    let dir = root.join("d");
    write_raw(&dir.join("b.txt"), "b");
    write_raw(&dir.join("a.txt"), "a");
    write_raw(&dir.join("sub").join("child.txt"), "c");
    write_raw(&dir.join(".env"), "SECRET=1");
    #[cfg(unix)]
    symlink(dir.join("a.txt"), dir.join("l")).unwrap();

    let entries = access.list_dir(&dir).unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();

    #[cfg(unix)]
    {
        assert_eq!(names, vec![".env", "a.txt", "b.txt", "l", "sub"]);
        // A link is named as a link, never as its target's kind.
        let l = entries.iter().find(|e| e.name == "l").unwrap();
        assert_eq!(l.kind, EntryKind::Symlink);
    }
    #[cfg(not(unix))]
    assert_eq!(names, vec![".env", "a.txt", "b.txt", "sub"]);

    // Hidden files are included — omitting belongs to the caller that knows why.
    assert!(names.contains(&".env"));
    // One level deep: `sub`'s own children are absent.
    assert!(!names.contains(&"child.txt"));
    let sub = entries.iter().find(|e| e.name == "sub").unwrap();
    assert_eq!(sub.kind, EntryKind::Dir);
    let a = entries.iter().find(|e| e.name == "a.txt").unwrap();
    assert_eq!(a.kind, EntryKind::File);
}

#[test]
fn ts24_list_dir_refusals_are_typed_and_distinct() {
    let (_tmp, access, root) = rooted();
    write_raw(&root.join("regular.txt"), "x");

    // Missing.
    let err = access.list_dir(root.join("nope")).unwrap_err();
    assert!(matches!(err, FsError::NotFound { .. }), "got {err:?}");

    // Exists but is not a directory — distinct from missing.
    let err = access.list_dir(root.join("regular.txt")).unwrap_err();
    assert!(matches!(err, FsError::NotADirectory { .. }), "got {err:?}");

    // Outside every root — and the directory is never opened.
    let outside = TempDir::new().unwrap();
    let err = access.list_dir(outside.path()).unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
}

// ---------------------------------------------------------------------------
// FSA-FR-17 — file_info (FSA-FR-23)
// ---------------------------------------------------------------------------

#[test]
fn ts25_file_info_describes_the_entry_without_reading_it() {
    let (_tmp, access, root) = rooted();
    let p = root.join("big.bin");
    write_raw(&p, &"x".repeat(2048));

    let info = access.file_info(&p).unwrap();
    assert_eq!(info.kind, EntryKind::File);
    assert_eq!(info.size, 2048);
    assert_eq!(
        info.modified,
        fs::metadata(&p).unwrap().modified().ok(),
        "the reported mtime is the filesystem's, not the moment of the call"
    );
    assert!(!info.readonly);

    // A directory reports as one.
    let d = root.join("d");
    fs::create_dir_all(&d).unwrap();
    assert_eq!(access.file_info(&d).unwrap().kind, EntryKind::Dir);

    // Missing is typed.
    let err = access.file_info(root.join("nope")).unwrap_err();
    assert!(matches!(err, FsError::NotFound { .. }), "got {err:?}");
}

#[cfg(unix)]
#[test]
fn ts25_file_info_names_a_link_as_a_link_but_refuses_a_linked_ancestor() {
    let (_tmp, access, root) = rooted();
    let target = root.join("real.bin");
    write_raw(&target, &"y".repeat(2048));
    let link = root.join("link.bin");
    symlink(&target, &link).unwrap();

    // The leaf link is *described*, not refused: reporting is not traversal.
    let info = access.file_info(&link).unwrap();
    assert_eq!(info.kind, EntryKind::Symlink);
    assert_eq!(
        info.size,
        fs::symlink_metadata(&link).unwrap().len(),
        "a link's size is its own, never its target's"
    );

    // But a link among the ancestors refuses, because reaching the entry would
    // mean traversing one.
    let dir = root.join("realdir");
    fs::create_dir_all(&dir).unwrap();
    write_raw(&dir.join("f.md"), "inner");
    symlink(&dir, root.join("linkdir")).unwrap();
    let err = access
        .file_info(root.join("linkdir").join("f.md"))
        .unwrap_err();
    assert!(matches!(err, FsError::SymlinkRefused { .. }), "got {err:?}");
}

// ---------------------------------------------------------------------------
// FSA-FR-24 — create_file (FSA-FR-24)
// ---------------------------------------------------------------------------

#[test]
fn ts26_create_file_honours_both_modes_and_refuses_the_rest() {
    let (_tmp, access, root) = rooted();

    // Exclusive creates a zero-length file...
    let n = root.join("n.txt");
    access.create_file(&n, CreateMode::Exclusive).unwrap();
    assert_eq!(fs::metadata(&n).unwrap().len(), 0);

    // ...and refuses a second time, leaving content alone.
    fs::write(&n, b"content").unwrap();
    let err = access.create_file(&n, CreateMode::Exclusive).unwrap_err();
    assert!(matches!(err, FsError::AlreadyExists { .. }), "got {err:?}");
    assert_eq!(fs::read_to_string(&n).unwrap(), "content");

    // Truncate empties what is there.
    access.create_file(&n, CreateMode::Truncate).unwrap();
    assert_eq!(fs::metadata(&n).unwrap().len(), 0);

    // A directory at the destination is "exists" under BOTH modes — truncating
    // a directory means nothing.
    let d = root.join("d");
    fs::create_dir_all(d.join("keep")).unwrap();
    for mode in [CreateMode::Truncate, CreateMode::Exclusive] {
        let err = access.create_file(&d, mode).unwrap_err();
        assert!(matches!(err, FsError::AlreadyExists { .. }), "got {err:?}");
    }
    assert!(d.join("keep").is_dir(), "the directory must be untouched");

    // Missing parents are created (FSA-FR-05).
    let deep = root.join("p").join("q.txt");
    access.create_file(&deep, CreateMode::Exclusive).unwrap();
    assert!(deep.is_file());

    // Outside every root: refused, nothing created.
    let outside = TempDir::new().unwrap();
    let denied = outside.path().join("x.txt");
    let err = access
        .create_file(&denied, CreateMode::Exclusive)
        .unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
    assert!(!denied.exists());
}

// ---------------------------------------------------------------------------
// FSA-FR-25 / FSA-FR-26 — open_file and the handle (FSA-FR-25, FSA-FR-26)
// ---------------------------------------------------------------------------

#[test]
fn ts27_open_file_modes_behave_as_specified() {
    let (_tmp, access, root) = rooted();
    let p = root.join("h.txt");
    write_raw(&p, "abc");

    // ReadOnly reads to the end.
    {
        let mut h = access.open_file(&p, OpenMode::ReadOnly).unwrap();
        assert_eq!(h.read_to_string().unwrap(), "abc");
    }

    // Append writes at the end, leaving what was there.
    {
        let mut h = access.open_file(&p, OpenMode::Append).unwrap();
        h.write_all(b"def").unwrap();
    }
    assert_eq!(fs::read_to_string(&p).unwrap(), "abcdef");

    // Truncate starts from empty.
    {
        let mut h = access.open_file(&p, OpenMode::Truncate).unwrap();
        h.write_all(b"z").unwrap();
    }
    assert_eq!(fs::read_to_string(&p).unwrap(), "z");

    // ReadOnly on a missing file is typed and creates nothing — parents included.
    let missing = root.join("gone").join("nope.txt");
    let err = access.open_file(&missing, OpenMode::ReadOnly).unwrap_err();
    assert!(matches!(err, FsError::NotFound { .. }), "got {err:?}");
    assert!(
        !root.join("gone").exists(),
        "ReadOnly must not create parent directories"
    );

    // Append and Truncate each create the file when absent.
    for (name, mode) in [("a.txt", OpenMode::Append), ("t.txt", OpenMode::Truncate)] {
        let fresh = root.join("made").join(name);
        {
            let mut h = access.open_file(&fresh, mode).unwrap();
            h.write_all(b"new").unwrap();
        }
        assert_eq!(fs::read_to_string(&fresh).unwrap(), "new");
    }

    // Outside every root: refused in every mode, and no handle is produced.
    let outside = TempDir::new().unwrap();
    for mode in [OpenMode::ReadOnly, OpenMode::Append, OpenMode::Truncate] {
        let denied = outside.path().join("x.txt");
        let err = access.open_file(&denied, mode).unwrap_err();
        assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
        assert!(!denied.exists());
    }
}

#[test]
fn ts28_a_handle_enforces_its_mode_on_every_call_and_flushes_on_drop() {
    let (_tmp, access, root) = rooted();
    let p = root.join("m.txt");
    write_raw(&p, "original");

    // A write through ReadOnly is refused and the file is byte-for-byte intact.
    {
        let mut h = access.open_file(&p, OpenMode::ReadOnly).unwrap();
        let err = h.write_all(b"nope").unwrap_err();
        assert!(
            matches!(err, FsError::WrongMode { attempted: "write", .. }),
            "got {err:?}"
        );
    }
    assert_eq!(fs::read_to_string(&p).unwrap(), "original");

    // A read through Append is refused too — the mode is checked per call.
    {
        let mut h = access.open_file(&p, OpenMode::Append).unwrap();
        let mut buf = Vec::new();
        let err = h.read_to_end(&mut buf).unwrap_err();
        assert!(
            matches!(err, FsError::WrongMode { attempted: "read", .. }),
            "got {err:?}"
        );
        assert!(buf.is_empty());
        assert_eq!(h.mode(), OpenMode::Append);
    }

    // Bytes written but never explicitly flushed are on disk once the handle
    // is dropped.
    let d = root.join("drop.txt");
    {
        let mut h = access.open_file(&d, OpenMode::Truncate).unwrap();
        h.write_all(b"flushed-by-drop").unwrap();
    }
    assert_eq!(fs::read_to_string(&d).unwrap(), "flushed-by-drop");
}

// NOTE on the remaining clause of FSA-FR-26 — "a handle does not outlive its
// instance". That is enforced by the borrow checker rather than at runtime:
// `FileHandle<'a>` borrows the `FsAccess` it came from, so the code that would
// demonstrate the failure does not compile and therefore cannot be written as a
// `#[test]`. Asserting it would need a `trybuild`-style compile-fail harness,
// which the repo does not carry; the lifetime in the signature is the guarantee.

// ---------------------------------------------------------------------------
// FSA-FR-17 — the default symlink policy (FSA-FR-17)
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn ts19_every_operation_refuses_a_link_under_the_default_policy() {
    let (_tmp, access, root) = rooted();
    let target = root.join("real.md");
    write_raw(&target, "target content");
    let before = fs::metadata(&target).unwrap().modified().unwrap();
    let link = root.join("link.md");
    symlink(&target, &link).unwrap();

    // Every operation that would touch the entry refuses it.
    let errs: Vec<FsError> = vec![
        access.read_text(&link).unwrap_err(),
        access.write_text_atomic(&link, "x").unwrap_err(),
        access.delete_path(&link, false).unwrap_err(),
        access.rename_path(&link, "other.md").unwrap_err(),
        access.sha256_file(&link).unwrap_err(),
        access.open_file(&link, OpenMode::ReadOnly).err().unwrap(),
        access.create_file(&link, CreateMode::Truncate).unwrap_err(),
    ];
    for err in &errs {
        assert!(
            matches!(err, FsError::SymlinkRefused { .. }),
            "expected SymlinkRefused, got {err:?}"
        );
    }
    // The link is still there, and its target is untouched in content and mtime.
    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(fs::read_to_string(&target).unwrap(), "target content");
    assert_eq!(fs::metadata(&target).unwrap().modified().unwrap(), before);
}

#[cfg(unix)]
#[test]
fn ts19_a_linked_ancestor_refuses_although_the_target_is_inside_the_root() {
    let (_tmp, access, root) = rooted();
    let real = root.join("realdir");
    write_raw(&real.join("f.md"), "inner");
    symlink(&real, root.join("linkdir")).unwrap();

    // No `..` anywhere, and the target lies inside the root — refused all the
    // same, because the rule is about the link rather than where it points.
    let through = root.join("linkdir").join("f.md");
    let err = access.read_text(&through).unwrap_err();
    assert!(matches!(err, FsError::SymlinkRefused { .. }), "got {err:?}");
    assert_eq!(fs::read_to_string(real.join("f.md")).unwrap(), "inner");
}

#[cfg(unix)]
#[test]
fn ts19_a_tree_holding_a_link_is_refused_before_anything_is_removed_or_copied() {
    let (_tmp, access, root) = rooted();
    let tree = root.join("tree");
    write_raw(&tree.join("a").join("b").join("keep.md"), "keep");
    write_raw(&root.join("elsewhere.md"), "e");
    symlink(root.join("elsewhere.md"), tree.join("a").join("b").join("l")).unwrap();

    let err = access.delete_path(&tree, true).unwrap_err();
    assert!(matches!(err, FsError::SymlinkRefused { .. }), "got {err:?}");
    let err = access.copy_path(&tree, root.join("copy")).unwrap_err();
    assert!(matches!(err, FsError::SymlinkRefused { .. }), "got {err:?}");

    // The refusal happened before anything was removed or written.
    assert_eq!(
        fs::read_to_string(tree.join("a").join("b").join("keep.md")).unwrap(),
        "keep"
    );
    assert!(
        fs::symlink_metadata(tree.join("a").join("b").join("l"))
            .unwrap()
            .file_type()
            .is_symlink(),
        "the link itself survives too — a delete that removed everything but it would pass otherwise"
    );
    assert!(tree.join("a").join("b").is_dir(), "and so do the directories above it");
    assert!(!root.join("copy").exists(), "nothing may be half-copied");
}

// ---------------------------------------------------------------------------
// FSA-FR-10 / FSA-FR-14 — copy and create_dir on the instance
// ---------------------------------------------------------------------------

#[test]
fn ts15_copy_never_overwrites_and_both_paths_go_through_the_gate() {
    let (_tmp, access, root) = rooted();
    let s = root.join("s.md");
    write_raw(&s, "source");

    // Positive: copies, creating the destination's parents, source untouched.
    let dst = root.join("dst").join("s.md");
    access.copy_path(&s, &dst).unwrap();
    assert_eq!(fs::read_to_string(&dst).unwrap(), "source");
    assert_eq!(fs::read_to_string(&s).unwrap(), "source");

    // Negative: a pre-existing destination is never overwritten.
    fs::write(&dst, b"do not clobber").unwrap();
    let err = access.copy_path(&s, &dst).unwrap_err();
    assert!(matches!(err, FsError::AlreadyExists { .. }), "got {err:?}");
    assert_eq!(fs::read_to_string(&dst).unwrap(), "do not clobber");

    // Negative: a destination outside every root is refused and nothing is written.
    let outside = TempDir::new().unwrap();
    let denied = outside.path().join("leak.md");
    let err = access.copy_path(&s, &denied).unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
    assert!(!denied.exists());

    // Negative: a *source* outside every root, likewise.
    let err = access
        .copy_path(outside.path().join("nope.md"), root.join("in.md"))
        .unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
    assert!(!root.join("in.md").exists(), "nothing may be written for a refused source");

    // FSA-FR-13: an occupied destination is the typed "exists" even when it IS
    // the source — the self-copy guard must not shadow the collision error.
    let d = root.join("selfdir");
    fs::create_dir_all(d.join("child")).unwrap();
    let err = access.copy_path(&d, &d).unwrap_err();
    assert!(
        matches!(err, FsError::AlreadyExists { .. }),
        "copying a directory onto itself is the typed exists error, got {err:?}"
    );
    assert!(d.join("child").is_dir(), "and the directory is untouched");
}

