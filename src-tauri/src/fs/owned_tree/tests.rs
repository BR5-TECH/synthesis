//! Scenario coverage for `delete_owned_tree` —
//! `specifications/core/FSA-filesystem-access.md` FSA-FR-ZUCF.
//!
//! The operation exists because FSA-FR-17's refusal cannot govern the
//! application's own directories, so the tests come in pairs: what the strict
//! removal refuses, this one removes; and what this one must still refuse, it
//! refuses before it touches the disk. A refusal is asserted on two things —
//! the typed error, and a filesystem that is byte-for-byte what it was.

use crate::fs::{FsAccess, FsError};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs::symlink;

/// An owned root, plus an allowlisted root that is **not** owned. The pair is
/// what separates "the application made this" from "the user owns this".
fn rooted() -> (TempDir, TempDir, FsAccess, PathBuf, PathBuf) {
    let owned_tmp = TempDir::new().unwrap();
    let plain_tmp = TempDir::new().unwrap();
    let owned = fs::canonicalize(owned_tmp.path()).unwrap();
    let plain = fs::canonicalize(plain_tmp.path()).unwrap();
    let access = FsAccess::builder()
        .owned_root(&owned)
        .allow_root(&plain)
        .build()
        .unwrap();
    (owned_tmp, plain_tmp, access, owned, plain)
}

fn write_raw(path: &Path, content: &str) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, content).unwrap();
}

/// The shape that wedged a graduation run: a checkout holding a dependency
/// store of symbolic links, one of which points outside the tree.
#[cfg(unix)]
fn tree_with_link(root: &Path, outside: &Path) -> PathBuf {
    let tree = root.join("rv");
    write_raw(&tree.join("src/main.rs"), "fn main() {}");
    fs::create_dir_all(tree.join("node_modules/.pnpm")).unwrap();
    symlink(outside, tree.join("node_modules/mdn-data")).unwrap();
    tree
}

// FSA-FR-ZUCF: a link inside the tree is unlinked rather than followed, so the
// tree goes and what it pointed at stays.
#[cfg(unix)]
#[test]
fn a_tree_holding_a_link_is_removed_and_the_link_target_survives() {
    let (_owned_tmp, _plain_tmp, access, owned, plain) = rooted();
    let target = plain.join("mdn-data");
    write_raw(&target.join("index.js"), "module.exports = {};");
    let tree = tree_with_link(&owned, &target);

    access.delete_owned_tree(&tree).unwrap();

    assert!(!tree.exists(), "the tree was not removed");
    assert!(
        target.join("index.js").exists(),
        "the link was followed: its target was removed with the tree"
    );
}

// FSA-FR-ZUCF / FSA-FR-17: the pair that states why this operation exists. The
// strict removal refuses the very tree this one reclaims.
#[cfg(unix)]
#[test]
fn the_strict_removal_refuses_the_tree_this_one_reclaims() {
    let (_owned_tmp, _plain_tmp, access, owned, plain) = rooted();
    let target = plain.join("mdn-data");
    write_raw(&target.join("index.js"), "module.exports = {};");
    let tree = tree_with_link(&owned, &target);

    let err = access.delete_path(&tree, true).unwrap_err();
    assert!(
        matches!(err, FsError::SymlinkRefused { .. }),
        "expected SymlinkRefused from delete_path, got {err:?}"
    );
    assert!(tree.exists(), "the refused removal took something anyway");

    access.delete_owned_tree(&tree).unwrap();
    assert!(!tree.exists());
}

// FSA-FR-ZUCF: a link at the leaf is unlinked, and never followed to its target.
#[cfg(unix)]
#[test]
fn a_link_at_the_leaf_is_unlinked_rather_than_followed() {
    let (_owned_tmp, _plain_tmp, access, owned, plain) = rooted();
    let target = plain.join("real");
    write_raw(&target.join("keep.txt"), "keep");
    let link = owned.join("link");
    symlink(&target, &link).unwrap();

    access.delete_owned_tree(&link).unwrap();

    assert!(
        fs::symlink_metadata(&link).is_err(),
        "the link itself is still there"
    );
    assert!(target.join("keep.txt").exists(), "the target was removed");
}

// FSA-FR-ZUCF: an allowlisted root the application does not own is refused, and
// the refusal happens before anything is removed.
#[test]
fn a_path_under_an_allowlisted_but_unowned_root_is_refused() {
    let (_owned_tmp, _plain_tmp, access, _owned, plain) = rooted();
    let tree = plain.join("project");
    write_raw(&tree.join("a.md"), "a");

    let err = access.delete_owned_tree(&tree).unwrap_err();

    assert!(
        matches!(err, FsError::EscapesAllowedRoots { .. }),
        "expected EscapesAllowedRoots, got {err:?}"
    );
    assert!(tree.join("a.md").exists(), "a refused removal took something");
}

// FSA-FR-ZUCF: an instance owning no root refuses every path, which is what
// keeps this operation off an agent session (FSA-FR-29).
#[test]
fn an_instance_that_owns_no_root_refuses_every_path() {
    let tmp = TempDir::new().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    let access = FsAccess::builder().allow_root(&root).build().unwrap();
    let tree = root.join("anything");
    write_raw(&tree.join("a.md"), "a");

    let err = access.delete_owned_tree(&tree).unwrap_err();

    assert!(
        matches!(err, FsError::EscapesAllowedRoots { .. }),
        "expected EscapesAllowedRoots, got {err:?}"
    );
    assert!(tree.join("a.md").exists());
}

// FSA-FR-ZUCF: the operation reclaims what is *under* an owned root, never the
// root the instance was built on.
#[test]
fn the_owned_root_itself_is_refused() {
    let (_owned_tmp, _plain_tmp, access, owned, _plain) = rooted();
    write_raw(&owned.join("a.md"), "a");

    let err = access.delete_owned_tree(&owned).unwrap_err();

    assert!(
        matches!(err, FsError::EscapesAllowedRoots { .. }),
        "expected EscapesAllowedRoots, got {err:?}"
    );
    assert!(owned.join("a.md").exists());
}

// FSA-FR-ZUCF: a path outside every root is refused, and nothing is read on the
// way to the refusal.
#[test]
fn a_path_outside_every_root_is_refused() {
    let (_owned_tmp, _plain_tmp, access, _owned, _plain) = rooted();
    let outside = TempDir::new().unwrap();
    let tree = fs::canonicalize(outside.path()).unwrap().join("elsewhere");
    write_raw(&tree.join("a.md"), "a");

    let err = access.delete_owned_tree(&tree).unwrap_err();

    assert!(
        matches!(err, FsError::EscapesAllowedRoots { .. }),
        "expected EscapesAllowedRoots, got {err:?}"
    );
    assert!(tree.join("a.md").exists());
}

// FSA-FR-ZUCF: a missing path is the typed error, distinct from an I/O failure.
#[test]
fn a_missing_path_is_the_typed_error() {
    let (_owned_tmp, _plain_tmp, access, owned, _plain) = rooted();

    let err = access.delete_owned_tree(owned.join("gone")).unwrap_err();

    assert!(matches!(err, FsError::NotFound { .. }), "got {err:?}");
}

// FSA-FR-ZUCF / FSA-FR-19: a relative path is refused, as it is everywhere else.
#[test]
fn a_relative_path_is_refused() {
    let (_owned_tmp, _plain_tmp, access, _owned, _plain) = rooted();

    let err = access.delete_owned_tree(PathBuf::from("rv")).unwrap_err();

    assert!(matches!(err, FsError::RelativePath { .. }), "got {err:?}");
}

// FSA-FR-ZUCF / FSA-FR-10: a path that climbs out of the owned root is judged by
// where it ends, so it cannot reach a tree the instance does not own.
#[test]
fn a_path_that_climbs_out_of_the_owned_root_is_refused() {
    let (_owned_tmp, _plain_tmp, access, owned, plain) = rooted();
    let tree = plain.join("project");
    write_raw(&tree.join("a.md"), "a");
    let climbing = owned.join("..").join(plain.file_name().unwrap()).join("project");

    let err = access.delete_owned_tree(&climbing).unwrap_err();

    assert!(
        matches!(err, FsError::EscapesAllowedRoots { .. }),
        "expected EscapesAllowedRoots, got {err:?}"
    );
    assert!(tree.join("a.md").exists());
}

// FSA-FR-ZUCF / FSA-FR-17: the exemption covers the tree below the target, not
// the way to it. A link among the components between the root and the target is
// refused, so a caller cannot compose a path that leaves the owned root through
// one.
#[cfg(unix)]
#[test]
fn a_link_among_the_components_on_the_way_to_the_target_is_refused() {
    let (_owned_tmp, _plain_tmp, access, owned, plain) = rooted();
    let real = plain.join("elsewhere");
    write_raw(&real.join("keep.txt"), "keep");
    // `g` is a link out of the owned root, and `g/rv` is under it lexically.
    symlink(&real, owned.join("g")).unwrap();

    let err = access.delete_owned_tree(owned.join("g").join("rv")).unwrap_err();

    assert!(
        matches!(err, FsError::SymlinkRefused { .. }),
        "expected SymlinkRefused, got {err:?}"
    );
    assert!(real.join("keep.txt").exists(), "the refusal took something");
}

// FSA-FR-ZUCF / FSA-FR-18: an owned root reached through a symbolic link holds
// the paths composed from the spelling the caller was given.
//
// `contain` normalises lexically and never canonicalises, so a caller path built
// from the non-canonical spelling matches only because the owned set carries
// both. Without that, `short_data_dir()` under a symlinked home would refuse
// every production path and no review checkout could ever be reclaimed — the
// defect this whole operation exists to end, returned with a green suite.
#[cfg(unix)]
#[test]
fn an_owned_root_reached_through_a_link_accepts_both_spellings() {
    let real_tmp = TempDir::new().unwrap();
    let link_tmp = TempDir::new().unwrap();
    let real = fs::canonicalize(real_tmp.path()).unwrap();
    // `/tmp` → `/private/tmp` is this shape on macOS, and a symlinked home is
    // the shape that matters in production.
    let spelled = fs::canonicalize(link_tmp.path()).unwrap().join("home");
    symlink(&real, &spelled).unwrap();

    let access = FsAccess::builder().owned_root(&spelled).build().unwrap();

    // The spelling the caller holds.
    write_raw(&spelled.join("g/run/rv/a.txt"), "a");
    access.delete_owned_tree(spelled.join("g/run/rv")).unwrap();
    assert!(!real.join("g/run/rv").exists());

    // And the canonical spelling of the same directory.
    write_raw(&real.join("g/run/rv/a.txt"), "a");
    access.delete_owned_tree(real.join("g/run/rv")).unwrap();
    assert!(!real.join("g/run/rv").exists());
}

// FSA-FR-ZUCF: the session scratch directory is allowlisted but is not the
// application's to reclaim through this operation, so it is refused like any
// other unowned root.
#[test]
fn the_session_temp_directory_is_not_an_owned_root() {
    let tmp = TempDir::new().unwrap();
    let owned = fs::canonicalize(tmp.path()).unwrap();
    let access = FsAccess::builder()
        .owned_root(&owned)
        .session_temp(true)
        .build()
        .unwrap();
    let scratch = access.session_temp_dir().expect("a scratch directory").to_path_buf();
    write_raw(&scratch.join("work/a.txt"), "a");

    let err = access.delete_owned_tree(scratch.join("work")).unwrap_err();

    assert!(
        matches!(err, FsError::EscapesAllowedRoots { .. }),
        "expected EscapesAllowedRoots, got {err:?}"
    );
    assert!(scratch.join("work/a.txt").exists());
}

// FSA-FR-ZUCF: a plain file at the target is removed, which is the other half of
// the leaf branch the link case covers.
#[test]
fn a_plain_file_at_the_target_is_removed() {
    let (_owned_tmp, _plain_tmp, access, owned, _plain) = rooted();
    let file = owned.join("run/record.toml");
    write_raw(&file, "state = \"blocked\"\n");

    access.delete_owned_tree(&file).unwrap();

    assert!(!file.exists());
}

// FSA-FR-ZUCF: a link whose target has gone is unlinked like any other, at the
// leaf and inside the tree. A partial dependency install leaves exactly this.
#[cfg(unix)]
#[test]
fn a_dangling_link_is_unlinked_at_the_leaf_and_inside_the_tree() {
    let (_owned_tmp, _plain_tmp, access, owned, _plain) = rooted();
    let tree = owned.join("rv");
    write_raw(&tree.join("src/main.rs"), "fn main() {}");
    symlink(owned.join("gone"), tree.join("node_modules/broken")).ok();
    fs::create_dir_all(tree.join("node_modules")).ok();
    let inside = tree.join("node_modules/broken");
    let _ = fs::remove_file(&inside);
    symlink(owned.join("gone"), &inside).unwrap();

    access.delete_owned_tree(&tree).unwrap();
    assert!(fs::symlink_metadata(&tree).is_err(), "the tree is still there");

    let leaf = owned.join("dangling");
    symlink(owned.join("also-gone"), &leaf).unwrap();
    access.delete_owned_tree(&leaf).unwrap();
    assert!(
        fs::symlink_metadata(&leaf).is_err(),
        "the dangling link is still there"
    );
}
