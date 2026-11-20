//! Change sets over the repository set: files of a submodule, pointer
//! entries, and the base of a submodule in Branch mode (CHC-FR-TMNY,
//! CHC-FR-JFNC, CHC-FR-EBLI, CHC-FR-AIZV, CHC-FR-RYNS).
//!
//! One part of `../tests/mod.rs`. These run on a real superproject whose
//! submodules libgit2 cloned (`repo_set::tests::fixture`).

use super::*;
use crate::changes::repo_set::tests::fixture::{commit_all, write, Superproject};

fn uncommitted(p: &Superproject) -> ChangeSet {
    uncommitted_change_set(&crate::fs::RootFs::for_root(p.root())).unwrap()
}

fn find<'a>(set: &'a ChangeSet, path: &str) -> &'a ChangeEntry {
    set.entries.iter().find(|e| e.path == path).unwrap_or_else(|| panic!("no {path}: {set:?}"))
}

// CHC-FR-TMNY: a project with no submodule lists one repository, and every
// entry names it.
#[test]
fn a_project_with_no_submodule_lists_one_repository() {
    let p = Superproject::new(&[("a.md", "a\n")]);
    write(&p.main(), "a.md", "a\nb\n");
    let set = uncommitted(&p);
    assert_eq!(set.repositories.len(), 1);
    assert_eq!(set.repositories[0].path, "");
    assert_eq!(set.repositories[0].branch.as_deref(), Some("main"));
    let entry = find(&set, "a.md");
    assert_eq!(entry.repository, "");
    assert_eq!(entry.entry_kind, EntryKind::File);
}

// CHC-FR-TMNY, CHC-FR-JFNC: an edit inside a submodule is an entry of that
// submodule, at its project-relative path, and never an entry of the parent;
// every member is listed, also one with no change.
#[test]
fn a_change_inside_a_submodule_belongs_to_the_submodule() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    p.add_submodule("", "libs/quiet", &[("q.md", "q\n")]);
    write(&sub, "a.md", "a\nchanged\n");
    write(&sub, "new.md", "new\n");
    write(&p.main(), "top.md", "top\nchanged\n");

    let set = uncommitted(&p);
    let repos: Vec<&str> = set.repositories.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(repos, ["", "libs/quiet", "libs/sub"]);
    let edited = find(&set, "libs/sub/a.md");
    assert_eq!(edited.repository, "libs/sub");
    assert_eq!(edited.change_status, ChangeStatus::Modified);
    assert_eq!((edited.added_lines, edited.removed_lines), (Some(1), Some(0)));
    assert_eq!(find(&set, "libs/sub/new.md").change_status, ChangeStatus::Untracked);
    assert_eq!(find(&set, "top.md").repository, "");
    // A dirty work tree moves no pointer.
    assert!(set.entries.iter().all(|e| e.entry_kind == EntryKind::File), "{:?}", set.entries);
}

// CHC-FR-EBLI, GSM-FR-FCOF: a submodule whose checked-out commit is not the
// recorded one gives one pointer entry of its parent.
#[test]
fn a_moved_submodule_gives_a_pointer_entry() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    let recorded = sub.head().unwrap().peel_to_commit().unwrap().id();
    write(&sub, "a.md", "a\nmore\n");
    let moved = commit_all(&sub, "sub work");

    let set = uncommitted(&p);
    let pointer = find(&set, "libs/sub");
    assert_eq!(pointer.entry_kind, EntryKind::Submodule);
    assert_eq!(pointer.repository, "");
    assert_eq!(pointer.name, "sub");
    assert_eq!(pointer.change_status, ChangeStatus::Modified);
    assert_eq!(pointer.previous_commit, Some(recorded.to_string()));
    assert_eq!(pointer.current_commit, Some(moved.to_string()));
    assert_eq!((pointer.added_lines, pointer.removed_lines), (Some(1), Some(1)));
    assert!(!pointer.is_binary && pointer.artifact_type.is_none());
    // The committed submodule work is no uncommitted file entry.
    assert!(set.entries.iter().all(|e| e.path != "libs/sub/a.md"));
    // CHC-FR-21: the totals count the pointer entry.
    assert_eq!(totals_of(&set).file_count, 1);
}

// CHC-FR-EBLI: a nested submodule's pointer is an entry of its own parent.
#[test]
fn a_nested_pointer_belongs_to_its_parent_submodule() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    let inner = p.add_submodule("libs/sub", "inner", &[("i.md", "i\n")]);
    assert!(uncommitted(&p).entries.is_empty(), "the fixture starts with no change");
    write(&inner, "i.md", "i\n2\n");
    commit_all(&inner, "inner work");
    let set = uncommitted(&p);
    let pointer = find(&set, "libs/sub/inner");
    assert_eq!(pointer.entry_kind, EntryKind::Submodule);
    assert_eq!(pointer.repository, "libs/sub");
}

// CHC-FR-RYNS: the path conventions match the path relative to the
// submodule root, so a file carries the type it carries in the Project panel.
#[test]
fn a_submodule_file_is_typed_relative_to_the_submodule_root() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("readme.md", "r\n")]);
    write(&sub, "specifications/x.md", "# X\n");
    let set = uncommitted(&p);
    let entry = find(&set, "libs/sub/specifications/x.md");
    let root = crate::fs::RootFs::for_root(p.root());
    let (expected, _) = crate::scanning::classify_path(
        &root,
        "libs/sub/specifications/x.md",
        &crate::scanning::LibraryAssignments::default(),
    );
    assert!(expected.is_some(), "the convention types the file");
    assert_eq!(entry.artifact_type, expected);
}

// CHC-FR-AIZV: in Branch mode a submodule is compared against the commit the
// main merge-base records for it, so its committed work since then shows.
#[test]
fn branch_mode_compares_a_submodule_against_the_recorded_base() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    let recorded = sub.head().unwrap().peel_to_commit().unwrap().id();
    let main = p.main();
    let head = main.head().unwrap().peel_to_commit().unwrap();
    main.branch("base", &head, false).unwrap();
    write(&sub, "a.md", "a\nmore\n");
    commit_all(&sub, "sub work");
    commit_all(&main, "record the new pointer");

    let set = branch_change_set(&crate::fs::RootFs::for_root(p.root()), "base").unwrap();
    let file = find(&set, "libs/sub/a.md");
    assert_eq!(file.repository, "libs/sub");
    assert_eq!(file.change_status, ChangeStatus::Modified);
    let pointer = find(&set, "libs/sub");
    assert_eq!(pointer.entry_kind, EntryKind::Submodule);
    assert_eq!(pointer.change_status, ChangeStatus::Modified);
    assert_eq!(pointer.previous_commit, Some(recorded.to_string()), "the pointer at the base");
}

// CHC-FR-AIZV: a submodule that the base does not record gives its pointer
// entry, as added, and no file entries.
#[test]
fn branch_mode_gives_a_new_submodule_as_one_added_pointer() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let main = p.main();
    let head = main.head().unwrap().peel_to_commit().unwrap();
    main.branch("base", &head, false).unwrap();
    p.add_submodule("", "libs/sub", &[("a.md", "a\n"), ("b.md", "b\n")]);

    let set = branch_change_set(&crate::fs::RootFs::for_root(p.root()), "base").unwrap();
    let at_root: Vec<&ChangeEntry> = set.entries.iter().filter(|e| e.path == "libs/sub").collect();
    assert_eq!(at_root.len(), 1, "one entry at the submodule root: {at_root:?}");
    let pointer = at_root[0];
    assert_eq!(pointer.entry_kind, EntryKind::Submodule);
    assert_eq!(pointer.change_status, ChangeStatus::Added);
    assert_eq!(pointer.previous_commit, None);
    assert_eq!((pointer.added_lines, pointer.removed_lines), (Some(1), Some(0)));
    assert!(set.entries.iter().all(|e| !e.path.starts_with("libs/sub/")), "{:?}", set.entries);
}

// CHC-FR-EBLI: a submodule added and not yet committed in its parent is one
// added pointer entry, and `.gitmodules` is a file entry of the parent.
#[test]
fn an_uncommitted_new_submodule_is_one_added_pointer() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    let main = p.main();
    let head = main.head().unwrap().peel_to_commit().unwrap();
    let parent = head.parent(0).unwrap();
    // Take the last commit back into the index, as before a commit.
    main.reset(parent.as_object(), git2::ResetType::Soft, None).unwrap();
    let set = uncommitted(&p);
    let pointer = find(&set, "libs/sub");
    assert_eq!(pointer.entry_kind, EntryKind::Submodule);
    assert_eq!(pointer.change_status, ChangeStatus::Added);
    assert_eq!(find(&set, ".gitmodules").repository, "");
    assert!(set.entries.iter().all(|e| !e.path.starts_with("libs/sub/")), "{:?}", set.entries);
}

// GSM-FR-VFJO: the set is read again for each change set, so a submodule
// initialised between two reads joins the second one.
#[test]
fn a_submodule_initialised_between_reads_joins_the_next_change_set() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    let clone_dir = p.tmp.path().join("clone");
    let clone = git2::Repository::clone(&p.root().to_string_lossy(), &clone_dir).unwrap();
    let root = crate::fs::RootFs::for_root(&clone_dir);
    assert_eq!(uncommitted_change_set(&root).unwrap().repositories.len(), 1);
    clone.find_submodule("libs/sub").unwrap().update(true, None).unwrap();
    assert_eq!(uncommitted_change_set(&root).unwrap().repositories.len(), 2);
}
