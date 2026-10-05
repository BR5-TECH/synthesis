//! Scenario coverage for `delete_author_tree` —
//! `specifications/core/FSA-filesystem-access.md` FSA-FR-OWVT.

use crate::fs::{FsAccess, FsError};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs::symlink;

fn write_raw(path: &Path, content: &str) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn parent() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let path = fs::canonicalize(tmp.path()).unwrap();
    (tmp, path)
}

fn access_for(root: &Path) -> FsAccess {
    FsAccess::builder().author_root(root).build().unwrap()
}

// FSA-FR-OWVT: a link inside the tree is unlinked rather than followed, so the
// tree goes and what the link pointed at stays.
#[cfg(unix)]
#[test]
fn a_tree_holding_a_link_is_removed_and_the_link_target_survives() {
    let (_tmp, root) = parent();
    let (_outside_tmp, outside) = parent();
    write_raw(&outside.join("store/index.js"), "module.exports = {};");
    let tree = root.join("wt");
    write_raw(&tree.join("src/main.rs"), "fn main() {}");
    fs::create_dir_all(tree.join("node_modules")).unwrap();
    symlink(outside.join("store"), tree.join("node_modules/dep")).unwrap();
    symlink(outside.join("store/index.js"), tree.join("file-link")).unwrap();

    access_for(&root).delete_author_tree(&tree).unwrap();

    assert!(!tree.exists(), "the tree was not removed");
    assert!(outside.join("store/index.js").exists(), "a link was followed");
}

// FSA-FR-OWVT / FSA-FR-17: the strict removal still refuses the same tree.
#[cfg(unix)]
#[test]
fn the_strict_removal_still_refuses_a_tree_holding_a_link() {
    let (_tmp, root) = parent();
    let tree = root.join("wt");
    write_raw(&tree.join("a.txt"), "a");
    symlink(&root, tree.join("loop")).unwrap();

    let err = access_for(&root).delete_path(&tree, true).unwrap_err();

    assert!(matches!(err, FsError::SymlinkRefused { .. }), "{err:?}");
    assert!(tree.join("a.txt").exists());
}

// FSA-FR-OWVT: the root itself is never removed.
#[test]
fn the_root_itself_is_refused() {
    let (_tmp, root) = parent();
    write_raw(&root.join("a.txt"), "a");

    let err = access_for(&root).delete_author_tree(&root).unwrap_err();

    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "{err:?}");
    assert!(root.join("a.txt").exists());
}

// FSA-FR-OWVT: a path under an ordinary allowlisted root is not removable.
#[test]
fn a_path_under_a_plain_root_is_refused_and_nothing_is_removed() {
    let (_a, author) = parent();
    let (_b, plain) = parent();
    write_raw(&plain.join("wt/a.txt"), "a");
    let access = FsAccess::builder()
        .author_root(&author)
        .allow_root(&plain)
        .build()
        .unwrap();

    let err = access.delete_author_tree(plain.join("wt")).unwrap_err();

    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "{err:?}");
    assert!(plain.join("wt/a.txt").exists());
}

// FSA-FR-OWVT: an instance with no author root refuses every path.
#[test]
fn an_instance_without_an_author_root_refuses() {
    let (_tmp, root) = parent();
    write_raw(&root.join("wt/a.txt"), "a");
    let access = FsAccess::builder().allow_root(&root).build().unwrap();

    let err = access.delete_author_tree(root.join("wt")).unwrap_err();

    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "{err:?}");
    assert!(root.join("wt/a.txt").exists());
}

// FSA-FR-OWVT: a missing tree is a typed error.
#[test]
fn a_missing_tree_is_a_typed_error() {
    let (_tmp, root) = parent();

    let err = access_for(&root).delete_author_tree(root.join("gone")).unwrap_err();

    assert!(matches!(err, FsError::NotFound { .. }), "{err:?}");
}

// FSA-FR-OWVT / FSA-FR-17: a link on the way to the tree is refused.
#[cfg(unix)]
#[test]
fn a_link_on_the_way_to_the_tree_is_refused() {
    let (_tmp, root) = parent();
    let (_o, outside) = parent();
    write_raw(&outside.join("wt/a.txt"), "a");
    symlink(&outside, root.join("hop")).unwrap();

    let err = access_for(&root)
        .delete_author_tree(root.join("hop/wt"))
        .unwrap_err();

    assert!(matches!(err, FsError::SymlinkRefused { .. }), "{err:?}");
    assert!(outside.join("wt/a.txt").exists());
}
