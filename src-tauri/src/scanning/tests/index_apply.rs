//! A watcher burst applied to the project index (ASC-FR-UWMI, ASC-FR-JYXO,
//! ASC-FR-15, ASC-FR-HPUY).
//!
//! The rule under test: after `apply`, the index equals a fresh build of the
//! disk, and the folders it reports are the folders whose answer changed. A
//! sentinel file that no burst names tells a rebuild from an update: only a
//! rebuild can see it.
//!
//! One part of `../tests/mod.rs`.

use super::*;

const SENTINEL: &str = "sentinel/unnamed.md";

fn write(root: &crate::fs::RootFs, rel: &str, body: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

/// Build the index, then write the sentinel that only a rebuild sees.
fn built(tmp: &TempDir) -> (crate::fs::RootFs, ProjectIndex) {
    let root = crate::fs::RootFs::for_root(tmp.path());
    let index = ProjectIndex::build(&root);
    write(&root, SENTINEL, "");
    (root, index)
}

fn rebuilt(index: &ProjectIndex) -> bool {
    candidate_paths(index).iter().any(|p| p == SENTINEL)
}

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// The index, apart from the sentinel, equals a fresh build of the disk.
fn assert_matches_disk(root: &crate::fs::RootFs, index: &ProjectIndex) {
    fs::remove_dir_all(root.join("sentinel")).ok();
    let mut copy = index.clone();
    copy.apply(root, &paths(&["sentinel"]), false, &[]);
    assert_eq!(copy.full_tree("p"), fresh_tree(root));
}

// ASC-FR-UWMI: a created file, a deleted file, and a folder moved in with
// content are applied with no rebuild, and the index then equals a build.
#[test]
fn single_changes_are_applied_without_a_walk() {
    let tmp = project(&[("docs/a.md", ""), ("docs/old.md", ""), ("keep.txt", "")]);
    let (root, mut index) = built(&tmp);
    write(&root, "docs/new.spec.md", "");
    fs::remove_file(root.join("docs/old.md")).unwrap();
    write(&root, "moved/deep/x.md", "");
    write(&root, "moved/y.md", "");
    index.apply(&root, &paths(&["docs/new.spec.md", "docs/old.md", "moved"]), false, &[]);
    assert!(!rebuilt(&index), "no rebuild for plain paths");
    assert!(candidate_paths(&index).contains(&"moved/deep/x.md".to_string()));
    assert_matches_disk(&root, &index);
}

// ASC-FR-UWMI: a file replaced by a folder of the same name, and back.
#[test]
fn a_file_and_a_folder_can_swap_at_one_path() {
    let tmp = project(&[("thing", "file"), ("other/", "")]);
    let (root, mut index) = built(&tmp);
    fs::remove_file(root.join("thing")).unwrap();
    write(&root, "thing/inner.md", "");
    fs::remove_dir_all(root.join("other")).unwrap();
    write(&root, "other", "now a file");
    index.apply(&root, &paths(&["thing", "other"]), false, &[]);
    assert!(!rebuilt(&index));
    assert_matches_disk(&root, &index);
}

// ASC-FR-UWMI, ASC-FR-09, ASC-FR-23: a pruned, an ignored, and a linked path
// never enter the index.
#[test]
fn pruned_ignored_and_linked_paths_stay_out() {
    let tmp = project(&[(".gitignore", "build/\n"), ("real/a.md", "")]);
    let (root, mut index) = built(&tmp);
    write(&root, ".synthesis/cache/db", "");
    write(&root, "build/out.md", "");
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("real"), root.join("link")).unwrap();
    index.apply(
        &root,
        &paths(&[".synthesis/cache/db", "build/out.md", "link", "link/a.md"]),
        false,
        &[],
    );
    assert!(!rebuilt(&index));
    let held = candidate_paths(&index);
    assert!(!held.iter().any(|p| p.starts_with(".synthesis/cache")));
    assert!(!held.iter().any(|p| p.starts_with("build")));
    assert!(!held.iter().any(|p| p.starts_with("link")));
}

// ASC-FR-UWMI: an ignore file, `.gitmodules`, an attribution change, and a
// burst over the threshold each build the index again.
#[test]
fn rule_changes_and_large_bursts_rebuild() {
    for burst in [vec![".gitignore"], vec!["a/.gitignore"], vec![".gitmodules"]] {
        let tmp = project(&[("a/x.md", "")]);
        let (root, mut index) = built(&tmp);
        index.apply(&root, &paths(&burst), false, &[]);
        assert!(rebuilt(&index), "{burst:?} must rebuild");
    }
    let tmp = project(&[("a/x.md", "")]);
    let (root, mut index) = built(&tmp);
    index.apply(&root, &[], true, &[]);
    assert!(rebuilt(&index), "an attribution change rebuilds");

    let tmp = project(&[("a/x.md", "")]);
    let (root, mut index) = built(&tmp);
    let many: Vec<String> = (0..=REBUILD_THRESHOLD).map(|i| format!("a/f{i}.md")).collect();
    index.apply(&root, &many[..REBUILD_THRESHOLD], false, &[]);
    assert!(!rebuilt(&index), "exactly the threshold is applied path by path");
    index.apply(&root, &many, false, &[]);
    assert!(rebuilt(&index), "one path over the threshold rebuilds");
}

use crate::scanning::index::REBUILD_THRESHOLD;

// ASC-FR-UWMI, ASC-FR-SFCR: a `.git` entry that appears in a folder the
// index holds moves a boundary and rebuilds; one below an ignored folder
// does not.
#[test]
fn a_moved_boundary_rebuilds_and_an_ignored_one_does_not() {
    let tmp =
        project(&[(".gitignore", "node_modules/\n"), ("sub/a.md", ""), ("node_modules/p/x", "")]);
    let (root, mut index) = built(&tmp);
    write(&root, "node_modules/p/.git/HEAD", "ref: x");
    index.apply(&root, &[], false, &paths(&["node_modules/p"]));
    assert!(!rebuilt(&index), "a boundary under an ignored folder is outside the tree");

    write(&root, "sub/.git", "gitdir: ../.git/modules/sub");
    index.apply(&root, &[], false, &paths(&["sub"]));
    assert!(rebuilt(&index));
    assert!(index.repo_roots.contains("sub"));
}

// ASC-FR-UWMI, ASC-FR-SFCR: a new folder that holds a `.git` entry rebuilds,
// as it is a repository root.
#[test]
fn a_new_nested_repository_rebuilds() {
    let tmp = project(&[("a.md", "")]);
    let (root, mut index) = built(&tmp);
    write(&root, "vendor/lib/.git", "gitdir: x");
    write(&root, "vendor/lib/specifications/S.md", "");
    index.apply(&root, &paths(&["vendor"]), false, &[]);
    assert!(rebuilt(&index));
    assert!(index.repo_roots.contains("vendor/lib"));
}

// ASC-FR-LXOQ, ASC-FR-UWMI: a removed repository root leaves the root set, so
// a plain folder made at that path is classified from the project root.
#[test]
fn a_removed_repository_root_leaves_no_stale_boundary() {
    let tmp = project(&[("sub/.git", "gitdir: x"), ("sub/specifications/S.md", "")]);
    let (root, mut index) = built(&tmp);
    assert!(index.repo_roots.contains("sub"));
    fs::remove_dir_all(root.join("sub")).unwrap();
    index.apply(&root, &paths(&["sub"]), false, &[]);
    assert!(!index.repo_roots.contains("sub"));
    write(&root, "sub/specifications/S.md", "");
    index.apply(&root, &paths(&["sub"]), false, &[]);
    assert!(!rebuilt(&index));
    assert_matches_disk(&root, &index);
}

// ASC-FR-JYXO, ASC-FR-15: a content edit that keeps the type changes no
// answer, so no folder is reported.
#[test]
fn a_content_edit_that_keeps_the_type_reports_nothing() {
    let tmp = project(&[("docs/a.md", "one"), ("specifications/s.md", "x")]);
    let (root, mut index) = built(&tmp);
    write(&root, "docs/a.md", "two");
    write(&root, "specifications/s.md", "y");
    let changed = index.apply(&root, &paths(&["docs/a.md", "specifications/s.md"]), false, &[]);
    assert!(changed.is_empty(), "{changed:?}");
}

// ASC-FR-JYXO, ASC-FR-15: frontmatter that gives a type reports the parent and
// every ancestor whose subtree types moved, in path order.
#[test]
fn a_new_type_reports_its_folder_and_ancestors() {
    let tmp = project(&[("a/b/n.md", "plain")]);
    let (root, mut index) = built(&tmp);
    write(&root, "a/b/n.md", "---\ntype: scenario\n---\n");
    let changed = index.apply(&root, &paths(&["a/b/n.md"]), false, &[]);
    assert_eq!(changed, vec!["", "a", "a/b"]);
}

// ASC-FR-JYXO, ASC-FR-19: a skill's new declared name is a change of its
// folder.
#[test]
fn a_new_skill_name_reports_its_folder() {
    let tmp = project(&[(".claude/skills/s/SKILL.md", "---\nname: one\n---\n")]);
    let (root, mut index) = built(&tmp);
    write(&root, ".claude/skills/s/SKILL.md", "---\nname: two\n---\n");
    let changed = index.apply(&root, &paths(&[".claude/skills/s/SKILL.md"]), false, &[]);
    assert_eq!(changed, vec![".claude/skills/s"]);
}

// ASC-FR-JYXO: a deleted folder reports the folders above it, never itself.
#[test]
fn a_deleted_folder_reports_its_parent() {
    let tmp = project(&[("a/b/x.txt", ""), ("a/y.txt", "")]);
    let (root, mut index) = built(&tmp);
    fs::remove_dir_all(root.join("a/b")).unwrap();
    let changed = index.apply(&root, &paths(&["a/b"]), false, &[]);
    assert!(changed.contains(&"a".to_string()), "{changed:?}");
    assert!(!changed.contains(&"a/b".to_string()), "{changed:?}");
}

// ASC-FR-JYXO: when a burst ends in a rebuild, the folders that its earlier
// paths changed are still reported.
#[test]
fn a_rebuild_mid_burst_still_reports_earlier_folders() {
    let tmp = project(&[("docs/a.md", "")]);
    let (root, mut index) = built(&tmp);
    write(&root, "docs/new.spec.md", "");
    write(&root, "vendor/lib/.git", "gitdir: x");
    let changed = index.apply(&root, &paths(&["docs/new.spec.md", "vendor/lib"]), false, &[]);
    assert!(rebuilt(&index));
    assert!(changed.contains(&"docs".to_string()), "{changed:?}");
}

// ASC-FR-05, ASC-FR-JYXO: an assignment classifies the folder's subtree
// again, and reports the folder, its ancestors, and its subfolders.
#[test]
fn reclassify_after_a_folder_assignment() {
    let tmp = project(&[("a/b/c/x.md", ""), ("a/y.md", ""), (".synthesis/", "")]);
    let root = crate::fs::RootFs::for_root(tmp.path());
    let mut index = ProjectIndex::build(&root);
    assign(&root, "a/b", ArtifactType::Prompt, Scope::Folder).unwrap();
    let changed = index.reclassify(&root, "a/b");
    for folder in ["", "a", "a/b", "a/b/c"] {
        assert!(changed.contains(&folder.to_string()), "{folder} in {changed:?}");
    }
    let x = index.candidates().iter().find(|c| c.path == "a/b/c/x.md").unwrap().artifact_type;
    assert_eq!(x, Some(ArtifactType::Prompt));
    clear(&root, "a/b").unwrap();
    index.reclassify(&root, "a/b");
    assert_eq!(index.full_tree("p"), fresh_tree(&root));
    assert!(index.reclassify(&root, "not/here").is_empty());
}

// ASC-FR-UWMI, ASC-FR-YLGN: a listed folder created after the build is
// marked as an uninitialised submodule, as a build marks it.
#[test]
fn a_new_listed_folder_is_an_uninitialised_submodule() {
    let tmp = project(&[(".gitmodules", "[submodule \"x\"]\n\tpath = libs/x\n"), ("libs/", "")]);
    let (root, mut index) = built(&tmp);
    fs::create_dir_all(root.join("libs/x")).unwrap();
    index.apply(&root, &paths(&["libs/x"]), false, &[]);
    let node = index.node("libs/x").unwrap();
    assert_eq!(node.nested_repo, Some(NestedRepo::Uninitialised));
}

// ASC-FR-UWMI, ASC-FR-JYXO: a folder moved in over a folder the index holds
// is walked again, and the folder itself is reported.
#[test]
fn a_folder_moved_over_a_held_folder_is_walked_again() {
    let tmp = project(&[("docs/a.md", ""), ("new/b.spec.md", "")]);
    let (root, mut index) = built(&tmp);
    fs::rename(root.join("docs"), root.join("docs.old")).unwrap();
    fs::rename(root.join("new"), root.join("docs")).unwrap();
    let changed = index.apply(&root, &paths(&["docs", "docs.old", "new"]), false, &[]);
    assert!(!rebuilt(&index));
    assert!(changed.contains(&"docs".to_string()), "{changed:?}");
    assert!(candidate_paths(&index).contains(&"docs/b.spec.md".to_string()));
    assert_matches_disk(&root, &index);
}

// ASC-FR-JYXO: a folder that a burst creates is reported with its parent.
#[test]
fn a_created_folder_is_reported_itself() {
    let tmp = project(&[("a/x.md", "")]);
    let (root, mut index) = built(&tmp);
    write(&root, "a/b/y.md", "");
    let changed = index.apply(&root, &paths(&["a/b"]), false, &[]);
    assert!(
        changed.contains(&"a".to_string()) && changed.contains(&"a/b".to_string()),
        "{changed:?}"
    );
}

// ASC-FR-UWMI: a folder event whose entries match the index walks nothing.
#[test]
fn a_folder_event_with_the_same_entries_is_no_walk() {
    let tmp = project(&[("a/x.md", "one")]);
    let (root, mut index) = built(&tmp);
    write(&root, "a/x.md", "two");
    let changed = index.apply(&root, &paths(&["a", "a/x.md"]), false, &[]);
    assert!(changed.is_empty(), "{changed:?}");
    assert!(!rebuilt(&index));
}
