//! The rollback across the repository set (GTC-FR-DGJV, GSM-FR-PQZX).
//!
//! One part of `mod.rs`, on a real superproject (`changes::repo_set::tests::fixture`).

use super::*;
use crate::changes::repo_set::tests::fixture::{commit_all, write, Superproject};

fn read(p: &Superproject, rel: &str) -> String {
    std::fs::read_to_string(p.root().join(rel)).unwrap()
}

// GTC-FR-DGJV: each path is rolled back in the repository that owns it, and
// the outcome keeps the order of the request.
#[test]
fn each_path_is_rolled_back_in_its_own_repository() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    write(&sub, "a.md", "a\nchanged\n");
    write(&sub, "new.md", "new\n");
    write(&p.main(), "top.md", "top\nchanged\n");

    let root = crate::fs::RootFs::for_root(p.root());
    let paths = ["libs/sub/a.md".into(), "top.md".into(), "libs/sub/new.md".into()];
    let (outcome, index_error) = rollback_named_paths(&root, &paths).unwrap();
    assert!(index_error.is_none());
    let order: Vec<&str> = outcome.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(order, ["libs/sub/a.md", "top.md", "libs/sub/new.md"]);
    assert_eq!(outcome.entries[0].restored_paths, ["libs/sub/a.md"]);
    assert_eq!(outcome.entries[2].removed_paths, ["libs/sub/new.md"]);
    assert_eq!(read(&p, "libs/sub/a.md"), "a\n");
    assert_eq!(read(&p, "top.md"), "top\n");
    assert!(!p.root().join("libs/sub/new.md").exists());
    let after = crate::changes::uncommitted_change_set(&root).unwrap();
    assert!(after.entries.is_empty(), "{:?}", after.entries);
}

// GTC-FR-DGJV, GSM-FR-PQZX: a pointer path fails as `submodule_pointer` and
// nothing is written for it; the other paths are still rolled back.
#[test]
fn a_pointer_path_is_refused_and_writes_nothing() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    write(&sub, "a.md", "a\nmore\n");
    let moved = commit_all(&sub, "moved");
    write(&p.main(), "top.md", "top\nchanged\n");

    // The moved pointer is staged in the parent, so a rollback that reset the
    // index entry would show.
    let main = p.main();
    let mut index = main.index().unwrap();
    index.add_path(Path::new("libs/sub")).unwrap();
    index.write().unwrap();
    let staged =
        |main: &Repository| main.index().unwrap().get_path(Path::new("libs/sub"), 0).unwrap().id;
    assert_eq!(staged(&main), moved);
    let root = crate::fs::RootFs::for_root(p.root());
    let (outcome, _) = rollback_named_paths(&root, &["libs/sub".into(), "top.md".into()]).unwrap();
    assert_eq!(staged(&p.main()), moved, "the pointer stays staged");
    let pointer = &outcome.entries[0];
    assert_eq!(pointer.outcome, ROLLBACK_FAILED);
    assert_eq!(pointer.failures.len(), 1);
    assert_eq!(pointer.failures[0].kind, ROLLBACK_SUBMODULE_POINTER);
    assert!(pointer.restored_paths.is_empty() && pointer.removed_paths.is_empty());
    assert_eq!(sub.head().unwrap().peel_to_commit().unwrap().id(), moved);
    assert_eq!(read(&p, "libs/sub/a.md"), "a\nmore\n");
    assert_eq!(outcome.entries[1].restored_paths, ["top.md"]);
}

// GTC-FR-DGJV, CHC-FR-09: a rename inside a submodule is undone there, and
// both identities are reported project-relative.
#[test]
fn a_rename_inside_a_submodule_is_undone_with_project_paths() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("old.md", "a body long enough to pair\n")]);
    std::fs::rename(p.root().join("libs/sub/old.md"), p.root().join("libs/sub/new.md")).unwrap();
    let root = crate::fs::RootFs::for_root(p.root());
    let set = crate::changes::uncommitted_change_set(&root).unwrap();
    let renamed = set.entries.iter().find(|e| e.path == "libs/sub/new.md").unwrap();
    assert_eq!(renamed.previous_path.as_deref(), Some("libs/sub/old.md"));

    let (outcome, _) = rollback_named_paths(&root, &["libs/sub/new.md".into()]).unwrap();
    let entry = &outcome.entries[0];
    assert_eq!(entry.previous_path.as_deref(), Some("libs/sub/old.md"));
    assert_eq!(entry.restored_paths, ["libs/sub/old.md"]);
    assert_eq!(entry.removed_paths, ["libs/sub/new.md"]);
    assert!(p.root().join("libs/sub/old.md").exists());
    drop(sub);
}

// GTC-FR-DGJV: a staged edit inside a submodule is undone in the index of the
// submodule.
#[test]
fn a_staged_submodule_edit_is_undone_in_its_own_index() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    write(&sub, "a.md", "a\nstaged\n");
    let mut index = sub.index().unwrap();
    index.add_path(Path::new("a.md")).unwrap();
    index.write().unwrap();
    let root = crate::fs::RootFs::for_root(p.root());
    let (outcome, _) = rollback_named_paths(&root, &["libs/sub/a.md".into()]).unwrap();
    assert_eq!(outcome.entries[0].restored_paths, ["libs/sub/a.md"]);
    let status = p.sub("libs/sub").status_file(Path::new("a.md")).unwrap();
    assert!(status.is_empty(), "index and work tree agree with HEAD: {status:?}");
}
