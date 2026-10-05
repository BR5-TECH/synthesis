//! Construction, roots, containment, relative paths, and the session temp directory.

use super::*;

// ---------------------------------------------------------------------------
// FSA-FR-18 — construction (FSA-FR-18)
// ---------------------------------------------------------------------------

#[test]
fn ts20_builder_accepts_a_real_directory_and_refuses_the_rest() {
    // Positive: one existing directory as a root yields an instance, and an
    // operation on a file inside it succeeds.
    let tmp = TempDir::new().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    let access = FsAccess::builder().allow_root(&root).build().unwrap();
    access.write_text_atomic(root.join("a.md"), "hello").unwrap();
    assert_eq!(access.read_text(root.join("a.md")).unwrap(), "hello");

    // Negative: no root at all.
    let err = FsAccess::builder().build().unwrap_err();
    assert!(
        matches!(err, FsAccessBuildError::NoRoots),
        "expected NoRoots, got {err:?}"
    );

    // Negative: a root that does not exist.
    let missing = root.join("nowhere");
    let err = FsAccess::builder().allow_root(&missing).build().unwrap_err();
    assert!(
        matches!(err, FsAccessBuildError::RootNotADirectory { .. }),
        "expected RootNotADirectory for a missing root, got {err:?}"
    );

    // Negative: a root naming a regular file.
    let file = root.join("f.txt");
    write_raw(&file, "x");
    let err = FsAccess::builder().allow_root(&file).build().unwrap_err();
    assert!(
        matches!(err, FsAccessBuildError::RootNotADirectory { .. }),
        "expected RootNotADirectory for a file root, got {err:?}"
    );
}

#[test]
fn ts20_a_built_instance_reports_its_configuration() {
    let (_tmp, access, root) = rooted();
    assert!(access.roots().iter().any(|r| *r == root));
    // FSA-FR-17: refusal is the default — nothing had to ask for it.
    assert!(!access.follows_symlinks());
    // FSA-FR-20: no temp directory unless one was asked for.
    assert!(access.session_temp_dir().is_none());
}

// ---------------------------------------------------------------------------
// FSA-FR-21 — several roots, and the boundary around them (FSA-FR-10/21)
// ---------------------------------------------------------------------------

#[test]
fn ts21_every_allowlisted_root_is_reachable_and_nothing_else_is() {
    let worktree = TempDir::new().unwrap();
    let appdata = TempDir::new().unwrap();
    let shortdata = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let w = fs::canonicalize(worktree.path()).unwrap();
    let a = fs::canonicalize(appdata.path()).unwrap();
    let s = fs::canonicalize(shortdata.path()).unwrap();
    let o = fs::canonicalize(outside.path()).unwrap();

    let access = FsAccess::builder()
        .allow_root(&w)
        .allow_root(&a)
        .allow_root(&s)
        .session_temp(true)
        .build()
        .unwrap();
    let temp = access.session_temp_dir().unwrap().to_path_buf();

    // Positive: a file written and read back under each of the four roots.
    for (label, root) in [
        ("worktree", &w),
        ("appdata", &a),
        ("shortdata", &s),
        ("temp", &temp),
    ] {
        let p = root.join("probe.txt");
        access.write_text_atomic(&p, label).unwrap();
        assert_eq!(
            access.read_text(&p).unwrap(),
            label,
            "{label} root should be readable and writable"
        );
    }

    // Negative: anywhere else is refused, and nothing is created there.
    let denied = o.join("x.md");
    let err = access.write_text_atomic(&denied, "nope").unwrap_err();
    assert!(
        matches!(err, FsError::EscapesAllowedRoots { .. }),
        "expected EscapesAllowedRoots, got {err:?}"
    );
    assert!(!denied.exists(), "a refused write must create nothing");
}

// ---------------------------------------------------------------------------
// FSA-FR-10 — containment, normalisation, cross-instance isolation
// ---------------------------------------------------------------------------

#[test]
fn ts12_escapes_are_refused_and_normalisation_is_by_where_the_path_ends() {
    let (_tmp, access, root) = rooted();
    let (_other_tmp, _other_access, other_root) = rooted();

    // `..` that leaves the root.
    // Named uniquely: `up` resolves into the SHARED system temp directory, and
    // asserting on a common filename there would collide with whatever a
    // neighbouring test happens to have left behind.
    let up = root.join("..").join("fsa-ts12-must-never-exist.md");
    let err = access.write_text_atomic(&up, "x").unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
    assert!(!up.exists(), "a refused write must create nothing");

    // A path under no root at all.
    let err = access.read_text(Path::new("/elsewhere/x.md")).unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");

    // A path under a root allowlisted for a *different* instance.
    let err = access
        .write_text_atomic(other_root.join("x.md"), "x")
        .unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
    assert!(
        !other_root.join("x.md").exists(),
        "another instance's root must be untouched"
    );

    // Positive: a path that climbs out of a subdirectory and back in is judged
    // by where it *ends*, so it proceeds against the normalised location.
    let round_trip = root.join("sub").join("..").join("a.md");
    access.write_text_atomic(&round_trip, "landed").unwrap();
    assert_eq!(fs::read_to_string(root.join("a.md")).unwrap(), "landed");
    assert!(
        !root.join("sub").exists(),
        "normalisation is lexical: the `sub` it passed through was never created"
    );
}

// ---------------------------------------------------------------------------
// FSA-FR-19 — relative paths (FSA-FR-19)
// ---------------------------------------------------------------------------

#[test]
fn ts22_a_relative_path_is_refused_without_consulting_the_working_directory() {
    let (_tmp, access, root) = rooted();
    // Seed a real file so a working-directory-relative resolution would have
    // something to find if it were (wrongly) attempted.
    write_raw(&root.join("a.md"), "seeded");

    for op in ["read", "write"] {
        let err = match op {
            "read" => access.read_text("a.md").unwrap_err(),
            _ => access.write_text_atomic("a.md", "x").unwrap_err(),
        };
        assert!(
            matches!(err, FsError::RelativePath { .. }),
            "{op} of a relative path must be RelativePath, got {err:?}"
        );
    }
    // The seeded file is untouched: nothing resolved it against anything.
    assert_eq!(fs::read_to_string(root.join("a.md")).unwrap(), "seeded");
}

// ---------------------------------------------------------------------------
// FSA-FR-20 — the session temp directory (FSA-FR-20)
// ---------------------------------------------------------------------------

#[test]
fn ts23_session_temp_is_created_allowlisted_and_removed_with_the_instance() {
    let tmp = TempDir::new().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();

    let temp_path = {
        let access = FsAccess::builder()
            .allow_root(&root)
            .session_temp(true)
            .build()
            .unwrap();
        let temp = access.session_temp_dir().unwrap().to_path_buf();

        // Exists, empty, and inside the allowlist.
        assert!(temp.is_dir());
        assert_eq!(fs::read_dir(&temp).unwrap().count(), 0);
        let scratch = temp.join("scratch.txt");
        access.write_text_atomic(&scratch, "work").unwrap();
        assert_eq!(access.read_text(&scratch).unwrap(), "work");

        temp
    }; // instance dropped here

    // Dropping the instance removes the directory and everything under it.
    assert!(
        !temp_path.exists(),
        "the session temp directory must not outlive its instance"
    );
}

#[test]
fn ts23_two_instances_hold_distinct_and_mutually_unreachable_temp_dirs() {
    let tmp = TempDir::new().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    let a = FsAccess::builder()
        .allow_root(&root)
        .session_temp(true)
        .build()
        .unwrap();
    let b = FsAccess::builder()
        .allow_root(&root)
        .session_temp(true)
        .build()
        .unwrap();

    let ta = a.session_temp_dir().unwrap().to_path_buf();
    let tb = b.session_temp_dir().unwrap().to_path_buf();
    assert_ne!(ta, tb, "each instance owns its own temp directory");

    // A directory is allowlisted only for the instance that made it.
    let err = a.write_text_atomic(tb.join("x"), "x").unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
    let err = b.write_text_atomic(ta.join("x"), "x").unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
}

#[test]
fn ts23_without_session_temp_none_is_created() {
    let (_tmp, access, _root) = rooted();
    assert!(access.session_temp_dir().is_none());
}

