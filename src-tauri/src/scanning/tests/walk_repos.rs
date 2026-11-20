//! Repositories inside a project (ASC-FR-SFCR, ASC-FR-TWVB, ASC-FR-LXOQ,
//! ASC-FR-YLGN).
//!
//! One part of `../tests/mod.rs`.

use super::*;

fn build(tmp: &TempDir) -> (crate::fs::RootFs, ProjectIndex) {
    let root = crate::fs::RootFs::for_root(tmp.path());
    let index = ProjectIndex::build(&root);
    (root, index)
}

fn held(index: &ProjectIndex, rel: &str) -> bool {
    index.node(rel).is_some()
}

// ASC-FR-SFCR: a `.git` entry, a folder or a file, at any depth, is never a
// node, and a symbolic link named `.git` makes no repository root.
#[test]
fn git_entries_are_never_nodes() {
    let tmp = project(&[
        (".git/HEAD", "ref: x"),
        ("a/.git", "gitdir: ../.git/modules/a"),
        ("a/x.md", ""),
        ("b/c/.git/HEAD", "ref: y"),
        ("b/c/y.md", ""),
    ]);
    let (_root, index) = build(&tmp);
    let all = index.search_paths("", PathKind::Any, None).paths;
    assert!(!all.iter().any(|p| repos::has_git_segment(p)), "{all:?}");
    assert!(held(&index, "a/x.md") && held(&index, "b/c/y.md"));
    assert!(index.repo_roots.contains("a") && index.repo_roots.contains("b/c"));
    assert!(is_pruned_dir("a/.git/x"));
}

#[cfg(unix)]
#[test]
fn a_linked_git_entry_makes_no_root() {
    let tmp = project(&[("target/", ""), ("d/x.md", "")]);
    std::os::unix::fs::symlink(tmp.path().join("target"), tmp.path().join("d/.git")).unwrap();
    let (_root, index) = build(&tmp);
    assert!(!index.repo_roots.contains("d"));
}

// ASC-FR-TWVB: a rule of the parent never hides a file of a nested
// repository, in the build, in the watcher filter, and in a one-node read.
#[test]
fn parent_rules_stop_at_a_nested_repository() {
    let tmp = project(&[
        (".gitignore", "*.md\n"),
        ("top.md", ""),
        ("vendor/lib/.git", "gitdir: x"),
        ("vendor/lib/a.md", ""),
    ]);
    let (root, index) = build(&tmp);
    assert!(!held(&index, "top.md"));
    assert!(held(&index, "vendor/lib/a.md"));
    let mut batch = vec!["top.md".to_string(), "vendor/lib/a.md".to_string()];
    drop_gitignored(&root, &mut batch);
    assert_eq!(batch, vec!["vendor/lib/a.md"]);
    assert!(node_at(&root, "vendor/lib/a.md").is_some());
    assert!(node_at(&root, "top.md").is_none());
}

// ASC-FR-TWVB: the rules of a nested repository apply below it alone.
#[test]
fn nested_rules_apply_only_below_the_nested_root() {
    let tmp = project(&[
        ("out.log", ""),
        ("sub/.git", "gitdir: x"),
        ("sub/.gitignore", "*.log\n"),
        ("sub/in.log", ""),
    ]);
    let (_root, index) = build(&tmp);
    assert!(held(&index, "out.log"));
    assert!(!held(&index, "sub/in.log"));
}

// ASC-FR-TWVB: a nested repository the parent ignores is outside the tree.
#[test]
fn an_ignored_nested_repository_is_absent() {
    let tmp = project(&[
        (".gitignore", "vendor/\n"),
        ("vendor/lib/.git", "gitdir: x"),
        ("vendor/lib/a.md", ""),
    ]);
    let (_root, index) = build(&tmp);
    assert!(!held(&index, "vendor/lib"));
    assert!(!index.repo_roots.contains("vendor/lib"));
}

// ASC-FR-TWVB: the `info/exclude` of the git directory that the root's `.git`
// entry names, a folder or a `gitdir:` file, applies to the build and to the
// watcher filter.
#[test]
fn info_exclude_applies_through_a_folder_and_a_gitdir_file() {
    let folder = project(&[
        (".git/info/exclude", "/secret.md\nbuild/\n"),
        ("secret.md", ""),
        ("build/x", ""),
        ("ok.md", ""),
    ]);
    let (root, index) = build(&folder);
    assert!(!held(&index, "secret.md") && !held(&index, "build/x") && held(&index, "ok.md"));
    let mut batch = vec!["secret.md".to_string(), "ok.md".to_string()];
    drop_gitignored(&root, &mut batch);
    assert_eq!(batch, vec!["ok.md"]);

    let file = project(&[
        ("gd/info/exclude", "/secret.md\n"),
        (".git", "gitdir: gd"),
        ("secret.md", ""),
        ("ok.md", ""),
    ]);
    let (_root, index) = build(&file);
    assert!(!held(&index, "secret.md") && held(&index, "ok.md"));
}

// ASC-FR-TWVB: a linked worktree reads `info/exclude` from the common git
// directory that its `commondir` names.
#[test]
fn a_linked_worktree_reads_the_common_exclude() {
    let tmp = project(&[
        ("main/info/exclude", "/hidden.md\n"),
        ("main/worktrees/w/commondir", "../.."),
        ("main/worktrees/w/info/exclude", "/other.md\n"),
        ("hidden.md", ""),
        ("other.md", ""),
    ]);
    fs::write(tmp.path().join(".git"), "gitdir: main/worktrees/w\n").unwrap();
    let (_root, index) = build(&tmp);
    assert!(!held(&index, "hidden.md"));
    assert!(held(&index, "other.md"), "the worktree's own exclude is not read");
}

// ASC-FR-TWVB: a `.ignore` file of another tool is not read.
#[test]
fn a_dot_ignore_file_is_not_a_rule() {
    let tmp = project(&[(".ignore", "x.md\n"), ("x.md", "")]);
    let (_root, index) = build(&tmp);
    assert!(held(&index, "x.md"));
}

// ASC-FR-LXOQ: a path convention matches relative to the nearest repository
// root, and the assignments stay relative to the project root.
#[test]
fn conventions_match_relative_to_the_nearest_root() {
    let tmp = project(&[
        ("vendor/lib/.git", "gitdir: x"),
        ("vendor/lib/specifications/X.md", ""),
        ("plain/specifications/Y.md", ""),
    ]);
    let (root, index) = build(&tmp);
    let type_of = |i: &ProjectIndex, rel: &str| i.node(rel).and_then(|n| n.artifact_type);
    assert_eq!(type_of(&index, "vendor/lib/specifications/X.md"), Some(ArtifactType::Spec));
    assert_eq!(type_of(&index, "plain/specifications/Y.md"), None);
    assign(&root, "vendor/lib/specifications/X.md", ArtifactType::Prompt, Scope::File).unwrap();
    let index = ProjectIndex::build(&root);
    assert_eq!(type_of(&index, "vendor/lib/specifications/X.md"), Some(ArtifactType::Prompt));
    let (t, _) =
        classify_path(&root, "vendor/lib/specifications/X.md", &LibraryAssignments::default());
    assert_eq!(t, Some(ArtifactType::Spec), "a one-off classification finds the root on disk");
}

// ASC-FR-YLGN: a listed root is a submodule, a listed folder with no `.git`
// is uninitialised, and an unlisted root is a nested repository. A listed path
// that is not on disk gives no node.
#[test]
fn folders_are_marked_by_their_boundary_kind() {
    let tmp = project(&[
        (".gitmodules", "[submodule \"a\"]\n\tpath = libs/a\n[submodule \"b\"]\n\tpath = libs/b\n[submodule \"c\"]\n\tpath = libs/c\n"),
        ("libs/a/.git", "gitdir: x"),
        ("libs/a/r.md", ""),
        ("libs/b/", ""),
        ("tools/x/.git", "gitdir: y"),
        ("tools/x/t.md", ""),
    ]);
    let (_root, index) = build(&tmp);
    let kind = |rel: &str| index.node(rel).and_then(|n| n.nested_repo);
    assert_eq!(kind("libs/a"), Some(NestedRepo::Submodule));
    assert_eq!(kind("libs/b"), Some(NestedRepo::Uninitialised));
    assert_eq!(kind("tools/x"), Some(NestedRepo::Repository));
    assert_eq!(kind("libs"), None);
    assert!(index.node("libs/c").is_none());
}

// ASC-FR-YLGN: a nested repository's own `.gitmodules` marks its children.
#[test]
fn a_nested_gitmodules_marks_its_own_children() {
    let tmp = project(&[
        ("outer/.git", "gitdir: x"),
        ("outer/.gitmodules", "[submodule \"i\"]\n\tpath = inner\n"),
        ("outer/inner/.git", "gitdir: y"),
        ("outer/inner/f.md", ""),
    ]);
    let (_root, index) = build(&tmp);
    assert_eq!(index.node("outer/inner").and_then(|n| n.nested_repo), Some(NestedRepo::Submodule));
    assert_eq!(index.node("outer").and_then(|n| n.nested_repo), Some(NestedRepo::Repository));
}

// ASC-FR-YLGN: the `path` values of a `.gitmodules` text.
#[test]
fn gitmodules_paths_are_parsed_and_normalised() {
    let text = "[submodule \"a\"]\n\tpath = \"libs/a/\"\n\tPath = libs/b ; a comment\n\tpath = ../out\n\tpath = .\n\tpath = ..\\\\up\n# path = commented\n\turl = x\n";
    assert_eq!(repos::parse_gitmodules(text), vec!["libs/a", "libs/b"]);
}

// ASC-FR-SFCR, ASC-FR-UWMI: the folders whose own `.git` entry a batch
// touched.
#[test]
fn git_boundaries_name_folders_of_touched_git_entries() {
    use notify_debouncer_mini::{DebouncedEvent, DebouncedEventKind};
    let root = std::path::Path::new("/p");
    let ev = |rel: &str| DebouncedEvent { path: root.join(rel), kind: DebouncedEventKind::Any };
    let batch =
        Ok(vec![ev("sub/.git"), ev(".git"), ev("sub/.git/HEAD"), ev("a/.git/modules/x/.git")]);
    assert_eq!(git_boundaries(&batch, root), vec!["sub"]);
    let failed = Err(notify_debouncer_mini::notify::Error::generic("x"));
    assert!(git_boundaries(&failed, root).is_empty());
    let exclude = Ok(vec![ev(".git/info/exclude")]);
    assert!(exclude_changed(&exclude, root));
    assert!(!exclude_changed(&Ok(vec![ev("a.md")]), root));
}

// ASC-FR-YLGN, ASC-FR-YSWU, LIB-FR-JZNT: a submodule that libgit2 cloned — a
// `.git` file that names `.git/modules/<name>` of the parent — is a folder
// with children, and its children are listed.
#[test]
fn a_real_submodule_is_a_folder_with_children() {
    use crate::changes::repo_set::tests::fixture::Superproject;
    let p = Superproject::new(&[("top.md", "top\n")]);
    p.add_submodule("", "libs/sub", &[("src/a.md", "a\n"), ("readme.md", "r\n")]);
    let root = crate::fs::RootFs::for_root(p.root());
    let index = ProjectIndex::build(&root);
    let libs = index.children("libs").expect("libs is a folder");
    assert_eq!(libs.len(), 1);
    assert_eq!(libs[0].path, "libs/sub");
    assert_eq!(libs[0].nested_repo, Some(NestedRepo::Submodule));
    assert_eq!(libs[0].has_children, Some(true));
    let names: Vec<String> =
        index.children("libs/sub").unwrap().into_iter().map(|n| n.path).collect();
    assert_eq!(names, ["libs/sub/src", "libs/sub/readme.md"]);
}

// ASC-FR-UWMI, ASC-FR-JYXO, ASC-FR-YLGN: a submodule initialised while the
// project is open turns from an empty, uninitialised folder into a submodule
// folder with children, and the burst names the folders that changed.
#[test]
fn a_submodule_initialised_while_open_gets_its_children() {
    use crate::changes::repo_set::tests::fixture::Superproject;
    let p = Superproject::new(&[("top.md", "top\n")]);
    p.add_submodule("", "libs/sub", &[("src/a.md", "a\n"), ("readme.md", "r\n")]);
    let clone_dir = p.tmp.path().join("clone");
    let clone = git2::Repository::clone(&p.root().to_string_lossy(), &clone_dir).unwrap();
    let root = crate::fs::RootFs::for_root(&clone_dir);
    let mut index = ProjectIndex::build(&root);
    let before = index.children("libs").unwrap();
    assert_eq!(before[0].nested_repo, Some(NestedRepo::Uninitialised));
    assert_eq!(before[0].has_children, Some(false));

    let mut submodule = clone.find_submodule("libs/sub").unwrap();
    submodule.update(true, None).unwrap();
    let burst: Vec<String> =
        ["libs/sub", "libs/sub/readme.md", "libs/sub/src", "libs/sub/src/a.md"]
            .iter()
            .map(|s| s.to_string())
            .collect();
    let changed = index.apply(&root, &burst, false, &["libs/sub".to_string()]);
    assert!(changed.contains(&"libs".to_string()), "{changed:?}");
    let after = index.children("libs").unwrap();
    assert_eq!(after[0].nested_repo, Some(NestedRepo::Submodule));
    assert_eq!(after[0].has_children, Some(true));
    assert_eq!(index.children("libs/sub").unwrap().len(), 2);
}

// ASC-FR-UWMI, ASC-FR-YLGN: however the watcher splits the events of a
// submodule init into bursts — the boundary alone, the files alone, or both
// in turn — the index ends with a submodule folder that holds its children.
#[test]
fn every_burst_shape_of_a_submodule_init_ends_with_a_submodule_folder() {
    use crate::changes::repo_set::tests::fixture::Superproject;
    let p = Superproject::new(&[("top.md", "top\n")]);
    p.add_submodule("", "libs/sub", &[("src/a.md", "a\n"), ("readme.md", "r\n")]);
    let files: Vec<String> = ["libs/sub/readme.md", "libs/sub/src", "libs/sub/src/a.md"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let boundary = vec!["libs/sub".to_string()];
    // Each burst is (paths, boundaries).
    type Burst = (Vec<String>, Vec<String>);
    let shapes: [(&str, Vec<Burst>); 3] = [
        ("boundary only", vec![(Vec::new(), boundary.clone())]),
        ("files only", vec![(files.clone(), Vec::new())]),
        ("boundary, then files", vec![(Vec::new(), boundary.clone()), (files.clone(), Vec::new())]),
    ];
    for (n, (shape, bursts)) in shapes.into_iter().enumerate() {
        let clone_dir = p.tmp.path().join(format!("clone{n}"));
        let clone = git2::Repository::clone(&p.root().to_string_lossy(), &clone_dir).unwrap();
        let root = crate::fs::RootFs::for_root(&clone_dir);
        let mut index = ProjectIndex::build(&root);
        clone.find_submodule("libs/sub").unwrap().update(true, None).unwrap();
        for (paths, boundaries) in bursts {
            index.apply(&root, &paths, false, &boundaries);
        }
        let node = &index.children("libs").unwrap()[0];
        assert_eq!(node.nested_repo, Some(NestedRepo::Submodule), "{shape}");
        assert_eq!(node.has_children, Some(true), "{shape}");
        assert_eq!(index.children("libs/sub").unwrap().len(), 2, "{shape}");
        assert!(index.repo_roots.contains("libs/sub"), "{shape}");
    }
}

// ASC-FR-YLGN: the index lists each repository boundary with its kind and
// child count, which the project index logs.
#[test]
fn the_index_lists_its_repository_boundaries() {
    use crate::changes::repo_set::tests::fixture::Superproject;
    let p = Superproject::new(&[("top.md", "top\n")]);
    p.add_submodule("", "libs/sub", &[("a.md", "a\n"), ("b.md", "b\n")]);
    let index = ProjectIndex::build(&crate::fs::RootFs::for_root(p.root()));
    assert_eq!(index.nested_folders(), [("libs/sub".to_string(), NestedRepo::Submodule, 2)]);
}
