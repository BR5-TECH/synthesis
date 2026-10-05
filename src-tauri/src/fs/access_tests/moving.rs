//! Moves across parents, followed symlinks, root canonicalisation, and the chosen destination.

use super::*;

// ---------------------------------------------------------------------------
// FSA-FR-10, FSA-FR-17 — `move_path` across parents (FSA-FR-30)
// ---------------------------------------------------------------------------

#[test]
fn ts34_move_relocates_a_subtree_whole_and_carries_its_typed_refusals() {
    let (_tmp, access, root) = rooted();
    let tree = root.join("a").join("tree");
    write_raw(&tree.join("one.md"), "one");
    write_raw(&tree.join("nested").join("two.md"), "two");
    fs::create_dir_all(root.join("b")).unwrap();

    // Positive: the whole subtree arrives under a different parent, and the
    // source is gone rather than copied.
    access.move_path(&tree, root.join("b").join("tree")).unwrap();
    assert_eq!(
        fs::read_to_string(root.join("b/tree/one.md")).unwrap(),
        "one"
    );
    assert_eq!(
        fs::read_to_string(root.join("b/tree/nested/two.md")).unwrap(),
        "two"
    );
    assert!(!tree.exists(), "the source is relocated, not duplicated");

    // Negative: an occupied destination is the typed "exists" and is untouched.
    write_raw(&root.join("a").join("x"), "x");
    let err = access
        .move_path(root.join("a").join("x"), root.join("b").join("tree"))
        .unwrap_err();
    assert!(matches!(err, FsError::AlreadyExists { .. }), "got {err:?}");
    assert!(root.join("b/tree/one.md").is_file(), "and it is untouched");
    assert!(root.join("a/x").is_file(), "and the source stayed put");

    // Negative: a destination inside the source's own subtree.
    let err = access
        .move_path(root.join("b").join("tree"), root.join("b/tree/inner"))
        .unwrap_err();
    assert!(
        matches!(err, FsError::InvalidDestination { .. }),
        "got {err:?}"
    );
    assert!(!root.join("b/tree/inner").exists(), "and nothing moved");

    // Negative: the source itself is likewise inside its own subtree.
    let err = access
        .move_path(root.join("b").join("tree"), root.join("b").join("tree"))
        .unwrap_err();
    assert!(
        matches!(err, FsError::InvalidDestination { .. }),
        "got {err:?}"
    );

    // Negative: a missing source is the typed "missing".
    let err = access
        .move_path(root.join("a/missing"), root.join("b/missing"))
        .unwrap_err();
    assert!(matches!(err, FsError::NotFound { .. }), "got {err:?}");

    // Negative: a destination outside every allowlisted root.
    let outside = TempDir::new().unwrap();
    let denied = outside.path().join("leak");
    let err = access.move_path(root.join("a/x"), &denied).unwrap_err();
    assert!(
        matches!(err, FsError::EscapesAllowedRoots { .. }),
        "got {err:?}"
    );
    assert!(!denied.exists());
    assert!(root.join("a/x").is_file(), "and the source stayed put");
}

#[cfg(unix)]
#[test]
fn ts34_move_refuses_a_tree_holding_a_link_before_anything_moves() {
    let (_tmp, access, root) = rooted();
    let tree = root.join("a").join("tree");
    write_raw(&tree.join("deep").join("keep.md"), "keep");
    symlink(root.join("a"), tree.join("deep").join("link")).unwrap();

    let err = access
        .move_path(&tree, root.join("b").join("tree2"))
        .unwrap_err();
    assert!(matches!(err, FsError::SymlinkRefused { .. }), "got {err:?}");
    assert!(
        tree.join("deep").join("keep.md").is_file(),
        "the source still holds every entry it held"
    );
    assert!(
        !root.join("b").join("tree2").exists(),
        "and nothing was created at the destination"
    );
}

#[test]
fn ts16_create_dir_never_adopts_an_existing_entry() {
    let (_tmp, access, root) = rooted();

    // Positive: creates, and creates missing parents along the way.
    access.create_dir(root.join("a").join("b").join("c")).unwrap();
    assert!(root.join("a").join("b").join("c").is_dir());
    assert_eq!(
        fs::read_dir(root.join("a").join("b").join("c")).unwrap().count(),
        0,
        "a created directory is empty"
    );

    // Negative: an existing directory is "exists", and its contents survive.
    write_raw(&root.join("a").join("b").join("c").join("keep.md"), "keep");
    let err = access.create_dir(root.join("a").join("b").join("c")).unwrap_err();
    assert!(matches!(err, FsError::AlreadyExists { .. }), "got {err:?}");
    assert_eq!(
        fs::read_to_string(root.join("a").join("b").join("c").join("keep.md")).unwrap(),
        "keep"
    );

    // Negative: a *file* occupying the name is "exists" too, and is unchanged.
    write_raw(&root.join("f"), "i am a file");
    let err = access.create_dir(root.join("f")).unwrap_err();
    assert!(matches!(err, FsError::AlreadyExists { .. }), "got {err:?}");
    assert_eq!(fs::read_to_string(root.join("f")).unwrap(), "i am a file");

    // Negative: outside every root.
    let outside = TempDir::new().unwrap();
    let denied = outside.path().join("d");
    let err = access.create_dir(&denied).unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
    assert!(!denied.exists());
}

#[test]
fn instance_delete_and_rename_carry_their_typed_errors() {
    // FSA-FR-11 / FSA-FR-12 through the instance surface.
    let (_tmp, access, root) = rooted();

    let f = root.join("f.md");
    write_raw(&f, "x");
    access.delete_path(&f, false).unwrap();
    assert!(!f.exists());

    // Missing is distinct from not-empty.
    let err = access.delete_path(&f, false).unwrap_err();
    assert!(matches!(err, FsError::NotFound { .. }), "got {err:?}");

    // A non-empty directory under `recursive = false` is the emptiness probe.
    let d = root.join("d");
    write_raw(&d.join("child.md"), "c");
    let err = access.delete_path(&d, false).unwrap_err();
    assert!(matches!(err, FsError::NotEmpty { .. }), "got {err:?}");
    assert!(d.join("child.md").exists(), "nothing may be removed");
    access.delete_path(&d, true).unwrap();
    assert!(!d.exists());

    // Rename: bare basename only, and never an overwrite.
    let a = root.join("a.md");
    let b = root.join("b.md");
    write_raw(&a, "a");
    write_raw(&b, "b");
    let err = access.rename_path(&a, "b.md").unwrap_err();
    assert!(matches!(err, FsError::AlreadyExists { .. }), "got {err:?}");
    assert_eq!(fs::read_to_string(&a).unwrap(), "a");
    let err = access.rename_path(&a, "sub/c.md").unwrap_err();
    assert!(matches!(err, FsError::InvalidName { .. }), "got {err:?}");
    access.rename_path(&a, "renamed.md").unwrap();
    assert!(root.join("renamed.md").exists() && !a.exists());
}

// ---------------------------------------------------------------------------
// FSA-FR-27 — follow_symlinks(true) (FSA-FR-27)
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn ts29_following_resolves_inside_the_allowlist_and_refuses_every_escape() {
    let tmp = TempDir::new().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    let outside_dir = TempDir::new().unwrap();
    let outside = fs::canonicalize(outside_dir.path()).unwrap();

    let access = FsAccess::builder()
        .allow_root(&root)
        .follow_symlinks(true)
        .build()
        .unwrap();

    // A link pointing inside the root resolves and reads through.
    write_raw(&root.join("real.md"), "inside content");
    symlink(root.join("real.md"), root.join("in")).unwrap();
    assert_eq!(access.read_text(root.join("in")).unwrap(), "inside content");

    // A link pointing outside is refused, and its target is never opened.
    write_raw(&outside.join("x.md"), "secret");
    symlink(outside.join("x.md"), root.join("out")).unwrap();
    let err = access.read_text(root.join("out")).unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");

    // A chain whose LAST hop lands back inside is still refused, because an
    // intermediate hop left the allowlist. This is the per-hop rule.
    write_raw(&root.join("c.md"), "final");
    symlink(root.join("c.md"), outside.join("b")).unwrap();
    symlink(outside.join("b"), root.join("a")).unwrap();
    let err = access.read_text(root.join("a")).unwrap_err();
    assert!(
        matches!(err, FsError::EscapesAllowedRoots { .. }),
        "a chain that leaves the allowlist mid-way must be refused, got {err:?}"
    );

    // A cycle is reported rather than followed forever.
    symlink(root.join("loop"), root.join("loop")).unwrap();
    let err = access.read_text(root.join("loop")).unwrap_err();
    assert!(matches!(err, FsError::SymlinkCycle { .. }), "got {err:?}");

    // A two-link mutual cycle likewise.
    symlink(root.join("p2"), root.join("p1")).unwrap();
    symlink(root.join("p1"), root.join("p2")).unwrap();
    let err = access.read_text(root.join("p1")).unwrap_err();
    assert!(matches!(err, FsError::SymlinkCycle { .. }), "got {err:?}");
}

#[cfg(unix)]
#[test]
fn ts29_a_tree_whose_link_escapes_is_refused_before_removal() {
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
    write_raw(&tree.join("a").join("keep.md"), "keep");
    write_raw(&outside.join("target.md"), "out");
    symlink(outside.join("target.md"), tree.join("a").join("escape")).unwrap();

    let err = access.delete_path(&tree, true).unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
    assert_eq!(
        fs::read_to_string(tree.join("a").join("keep.md")).unwrap(),
        "keep",
        "every entry must survive a refused removal"
    );
    assert!(outside.join("target.md").exists());
}

#[cfg(unix)]
#[test]
fn ts29_following_traverses_a_linked_ancestor_that_stays_inside() {
    let tmp = TempDir::new().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    let access = FsAccess::builder()
        .allow_root(&root)
        .follow_symlinks(true)
        .build()
        .unwrap();

    // The ancestor case, which the default policy refuses outright.
    let real = root.join("realdir");
    write_raw(&real.join("f.md"), "reached");
    symlink(&real, root.join("linkdir")).unwrap();
    assert_eq!(
        access.read_text(root.join("linkdir").join("f.md")).unwrap(),
        "reached"
    );
}

// ---------------------------------------------------------------------------
// FSA-FR-18 — root canonicalisation (FSA-FR-18)
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn ts31_a_root_reached_through_a_link_still_contains_the_paths_beneath_it() {
    // The `/tmp` -> `/private/tmp` shape: the root itself is named through a
    // symbolic link. Canonicalising at build is what makes paths under it
    // resolve; without it the instance would contain nothing.
    let tmp = TempDir::new().unwrap();
    let base = fs::canonicalize(tmp.path()).unwrap();
    let real = base.join("real-session");
    fs::create_dir_all(&real).unwrap();
    let linked = base.join("session");
    symlink(&real, &linked).unwrap();

    let access = FsAccess::builder().allow_root(&linked).build().unwrap();

    // Written and read back through the *linked* spelling of the root.
    let p = linked.join("a.md");
    access.write_text_atomic(&p, "through the link").unwrap();
    assert_eq!(access.read_text(&p).unwrap(), "through the link");
    // And the bytes really landed in the real directory.
    assert_eq!(
        fs::read_to_string(real.join("a.md")).unwrap(),
        "through the link"
    );

    // The canonical spelling works too — they are one directory.
    assert_eq!(access.read_text(real.join("a.md")).unwrap(), "through the link");
}

// ---------------------------------------------------------------------------
// FSA-FR-28 — the user-chosen destination (FSA-FR-28)
// ---------------------------------------------------------------------------

#[test]
fn ts32_a_user_chosen_path_writes_outside_the_allowlist_and_nothing_else_does() {
    let (_tmp, access, root) = rooted();
    let desktop = TempDir::new().unwrap();
    let chosen_path = desktop.path().join("sub").join("logs.jsonl");

    // Positive: the write lands although the path is outside every root, and
    // the destination's missing parent is created (FSA-FR-05).
    let chosen = UserChosenPath::from_frontend_response(&chosen_path).unwrap();
    access.write_text_at_user_choice(&chosen, "first").unwrap();
    assert_eq!(fs::read_to_string(&chosen_path).unwrap(), "first");

    // A file already there is replaced — choosing an occupied path in a save
    // dialog is how a user asks for that.
    access.write_text_at_user_choice(&chosen, "second").unwrap();
    assert_eq!(fs::read_to_string(&chosen_path).unwrap(), "second");

    // Negative: the SAME location as an ordinary path is refused, so the
    // exemption is reachable only through the token.
    let err = access.write_text_atomic(&chosen_path, "sneaky").unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
    assert_eq!(
        fs::read_to_string(&chosen_path).unwrap(),
        "second",
        "the refused ordinary write must not have touched the file"
    );

    // And the token does not widen anything else: reads outside stay refused.
    let err = access.read_text(&chosen_path).unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");

    // A user-chosen path inside a root is fine too — the exemption is about
    // where it may land, not about where it may not.
    let inside = UserChosenPath::from_frontend_response(root.join("in.jsonl")).unwrap();
    access.write_text_at_user_choice(&inside, "ok").unwrap();
    assert_eq!(fs::read_to_string(root.join("in.jsonl")).unwrap(), "ok");
}

#[cfg(unix)]
#[test]
fn ts32_a_user_chosen_path_is_still_subject_to_the_symlink_policy() {
    // FSA-FR-28 exempts the write from FSA-FR-10 "and from FSA-FR-10 alone".
    let (_tmp, access, root) = rooted();
    let desktop = TempDir::new().unwrap();
    let target = desktop.path().join("real.jsonl");
    write_raw(&target, "original");
    let link = desktop.path().join("link.jsonl");
    symlink(&target, &link).unwrap();

    let chosen = UserChosenPath::from_frontend_response(&link).unwrap();
    let err = access
        .write_text_at_user_choice(&chosen, "through the link")
        .unwrap_err();
    assert!(matches!(err, FsError::SymlinkRefused { .. }), "got {err:?}");
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "original",
        "the link's target must be untouched"
    );
    let _ = root;
}

