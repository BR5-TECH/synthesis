//! What the scan walks and what it refuses, and the stability of the ids
//! (ASC-FR-09, ASC-FR-11, ASC-FR-13).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

// ------------------------------------------------------------------
// ASC-FR-09: scope rules — skip .git, gitignore, cache,
// never surface library.toml.
// ------------------------------------------------------------------
#[test]
fn ts10_scan_skips_git_gitignored_cache_and_attribution_file() {
    let tmp = project_with_synthesis();
    let root = &crate::fs::RootFs::for_root(tmp.path());

    // A real artifact that SHOULD appear.
    fs::create_dir_all(root.join(".claude/skills")).unwrap();
    fs::write(root.join(".claude/skills/foo.md"), "x").unwrap();

    // `.gitignore` excludes node_modules.
    fs::write(root.join(".gitignore"), "node_modules/\n").unwrap();
    fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
    fs::write(root.join("node_modules/pkg/index.js"), "x").unwrap();

    // `.git/` and `.synthesis/cache/` must be pruned.
    fs::create_dir_all(root.join(".git/objects")).unwrap();
    fs::write(root.join(".git/objects/abc"), "x").unwrap();
    fs::create_dir_all(root.join(".synthesis/cache")).unwrap();
    fs::write(root.join(".synthesis/cache/db.sqlite"), "x").unwrap();
    fs::create_dir_all(root.join(".synthesis/drafts")).unwrap();
    fs::write(root.join(".synthesis/drafts/wip.md"), "x").unwrap();
    // NTC-FR-16 / ASC-FR-09: the notes folder is committed, unlike the two
    // above, and is pruned all the same — a note file is not an artifact.
    fs::create_dir_all(root.join(".synthesis/notes")).unwrap();
    fs::write(root.join(".synthesis/notes/n1.toml"), "id = \"n1\"\n").unwrap();
    fs::create_dir_all(root.join(".synthesis/comments")).unwrap();
    fs::write(root.join(".synthesis/comments/a3f9.jsonl"), "{}\n").unwrap();

    // The attribution file exists but must never surface as a node.
    assign(root, ".claude/skills/foo.md", ArtifactType::Skill, Scope::File).unwrap();

    let tree = scan(root);
    assert!(find(&tree, ".claude/skills/foo.md").is_some());
    assert!(find(&tree, "node_modules").is_none(), "gitignored dir must be hidden");
    assert!(find(&tree, ".git").is_none(), ".git must be pruned");
    assert!(find(&tree, ".synthesis/cache").is_none(), "cache must be pruned");
    assert!(find(&tree, ".synthesis/drafts").is_none(), "drafts must be pruned");
    assert!(find(&tree, ".synthesis/notes").is_none(), "notes must be pruned");
    assert!(
        find(&tree, ".synthesis/notes/n1.toml").is_none(),
        "and no individual note file may surface in the Library (NTC-FR-16)"
    );
    assert!(
        find(&tree, ".synthesis/comments").is_none(),
        "comments must be pruned"
    );
    assert!(
        find(&tree, ".synthesis/comments/a3f9.jsonl").is_none(),
        "and no comment log may surface in the Library (CMS-FR-29)"
    );
    // ASC-FR-17 / SCC-FR-03: the candidate list is derived from this tree,
    // so the same prune keeps note bodies out of universal search results.
    assert!(
        !candidate_files(&tree)
            .iter()
            .any(|c| c.path.starts_with(".synthesis/notes")),
        "note files must not be search candidates"
    );
    assert!(
        find(&tree, ".synthesis/library.toml").is_none(),
        "attribution file must never be a node"
    );
}

// ------------------------------------------------------------------
// ASC-FR-11: scan never panics on missing/racy roots.
// ------------------------------------------------------------------
#[test]
fn ts12_scan_nonexistent_root_returns_empty_tree_without_panic() {
    // A governed root cannot be built for a directory that never existed
    // (FSA-FR-18), so the case this guards is the one that actually
    // happens: the root was real when the project opened and has since
    // gone — deleted, unmounted, or renamed out from under the session.
    let tmp = TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(tmp.path());
    std::fs::remove_dir_all(tmp.path()).unwrap();
    let tree = scan(&root);
    assert_eq!(tree.node_kind, NodeKind::Folder);
    assert_eq!(tree.children.as_ref().map(|c| c.len()), Some(0));
}

// ------------------------------------------------------------------
// ASC-FR-13: ids are stable across scans and equal the path.
// ------------------------------------------------------------------
#[test]
fn ts13_node_ids_are_stable_path_derived_keys() {
    let tmp = project_with_synthesis();
    let root = &crate::fs::RootFs::for_root(tmp.path());
    fs::create_dir_all(root.join(".claude/skills")).unwrap();
    fs::write(root.join(".claude/skills/foo.md"), "x").unwrap();

    let first = scan(root);
    let second = scan(root);
    let a = find(&first, ".claude/skills/foo.md").unwrap();
    let b = find(&second, ".claude/skills/foo.md").unwrap();
    assert_eq!(a.id, b.id, "id stable across scans");
    assert_eq!(a.id, ".claude/skills/foo.md", "id is the project-relative path");
}
