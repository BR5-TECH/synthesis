//! The tree reads of the project index (ASC-FR-KVIE, ASC-FR-AGGG,
//! ASC-FR-SKRN, ASC-FR-YNJR, ASC-FR-AVVY, ASC-FR-QKXR, ASC-FR-OGRB,
//! ASC-FR-YSWU).
//!
//! One part of `../tests/mod.rs`.

use super::*;

/// A small project with specs, a skill, plain files, and an assigned folder.
fn sample() -> (TempDir, ProjectIndex) {
    let tmp = project(&[
        ("specifications/ui/LIB.md", "x"),
        ("specifications/core/ASC.md", "x"),
        (".claude/skills/a/SKILL.md", "---\nname: alpha\n---\n"),
        ("src/main.rs", "fn main() {}"),
        ("src/lib.rs", ""),
        ("Zeta.md", ""),
        ("alpha.txt", ""),
        ("empty/", ""),
        ("notes/n.md", ""),
    ]);
    let root = crate::fs::RootFs::for_root(tmp.path());
    assign(&root, "notes", ArtifactType::Scenario, Scope::Folder).unwrap();
    let index = ProjectIndex::build(&root);
    (tmp, index)
}

fn names(nodes: &[TreeNode]) -> Vec<&str> {
    nodes.iter().map(|n| n.name.as_str()).collect()
}

// ASC-FR-KVIE: the children of a folder, folders first, then files, each by
// byte order of the name, with no child listed twice.
#[test]
fn children_are_folders_first_then_files_by_name() {
    let (_tmp, index) = sample();
    let root = index.children("").unwrap();
    assert_eq!(
        names(&root),
        vec![
            ".claude",
            ".synthesis",
            "empty",
            "notes",
            "specifications",
            "src",
            "Zeta.md",
            "alpha.txt"
        ]
    );
    assert_eq!(names(&index.children("specifications").unwrap()), vec!["core", "ui"]);
    assert!(root.iter().all(|n| n.children.is_none()), "a child carries no nested children");
}

// ASC-FR-KVIE: a file, a missing path, and an ignored path have no children.
#[test]
fn children_of_a_file_or_a_missing_folder_is_none() {
    let (_tmp, index) = sample();
    assert!(index.children("src/main.rs").is_none());
    assert!(index.children("nope").is_none());
}

// ASC-FR-OGRB, ASC-FR-YSWU, ASC-FR-08: the flags of each folder.
#[test]
fn folders_carry_subtree_types_and_has_children() {
    let (_tmp, index) = sample();
    let root = index.children("").unwrap();
    let by = |n: &str| root.iter().find(|c| c.name == n).unwrap().clone();
    assert_eq!(by("specifications").subtree_types, Some(vec![ArtifactType::Spec]));
    assert_eq!(by("specifications").has_artifacts, Some(true));
    assert_eq!(by("src").subtree_types, Some(vec![]));
    assert_eq!(by("src").has_artifacts, Some(false));
    assert_eq!(by("src").has_children, Some(true));
    assert_eq!(by("empty").has_children, Some(false));
    // A folder-scope assignment counts for the folder itself.
    assert_eq!(by("notes").subtree_types, Some(vec![ArtifactType::Scenario]));
    let full = ProjectIndex::build(&crate::fs::RootFs::for_root(_tmp.path()));
    let top = full.children("").unwrap();
    let claude = top.iter().find(|c| c.name == ".claude").unwrap();
    assert_eq!(claude.subtree_types, Some(vec![ArtifactType::Skill]));
}

// ASC-FR-OGRB: the types follow the order of ASC-FR-02, each once.
#[test]
fn subtree_types_are_in_type_order_and_unique() {
    let tmp = project(&[
        ("mix/b.scenario.md", ""),
        ("mix/a.spec.md", ""),
        ("mix/c.spec.md", ""),
        ("mix/AGENTS.md", ""),
    ]);
    let index = ProjectIndex::build(&crate::fs::RootFs::for_root(tmp.path()));
    let mix = index.children("").unwrap().into_iter().find(|c| c.name == "mix").unwrap();
    assert_eq!(
        mix.subtree_types,
        Some(vec![ArtifactType::Agent, ArtifactType::Spec, ArtifactType::Scenario])
    );
}

// ASC-FR-AGGG: the children of the root and of each ancestor, root first, and
// of the folder itself when the path is a folder.
#[test]
fn path_lists_each_folder_on_the_way() {
    let (_tmp, index) = sample();
    let folders =
        |p: &str| -> Vec<String> { index.path(p).unwrap().into_iter().map(|f| f.folder).collect() };
    assert_eq!(
        folders("specifications/ui/LIB.md"),
        vec!["", "specifications", "specifications/ui"]
    );
    assert_eq!(folders("specifications/ui"), vec!["", "specifications", "specifications/ui"]);
    assert_eq!(folders("Zeta.md"), vec![""]);
    assert!(index.path("specifications/ui/nope.md").is_none());
}

// ASC-FR-SKRN: a match ignores letter case, holds its ancestors as nested
// nodes, and the lens decides which files count.
#[test]
fn filter_nests_matches_under_their_ancestors() {
    let (_tmp, index) = sample();
    let all = index.filter("  LIB ", Lens::Named(LensName::AllFiles), 100);
    assert!(!all.truncated);
    assert_eq!(names(&all.root), vec!["specifications", "src"]);
    let spec = &all.root[0];
    assert_eq!(names(spec.children.as_ref().unwrap()), vec!["ui"]);
    // Under All artifacts, the untyped `src/lib.rs` is not a match.
    let typed = index.filter("lib", Lens::Named(LensName::AllArtifacts), 100);
    assert_eq!(names(&typed.root), vec!["specifications"]);
    // A type lens with no match drops the folder.
    assert!(index.filter("lib", Lens::Type(ArtifactType::Skill), 100).root.is_empty());
    // A folder with its own assignment matches by its own name.
    let notes = index.filter("notes", Lens::Named(LensName::AllArtifacts), 100);
    assert_eq!(names(&notes.root), vec!["notes"]);
}

// ASC-FR-SKRN: at most `limit` matches, and `truncated` says more exist.
#[test]
fn filter_stops_at_the_limit() {
    let (_tmp, index) = sample();
    let lens = Lens::Named(LensName::AllFiles);
    let every = index.filter(".", lens, 1000);
    let count = |r: &FilterResult| -> usize {
        fn go(nodes: &[TreeNode]) -> usize {
            nodes
                .iter()
                .map(|n| {
                    usize::from(n.node_kind == NodeKind::File || n.name.contains('.'))
                        + n.children.as_deref().map(go).unwrap_or(0)
                })
                .sum()
        }
        go(&r.root)
    };
    let total = count(&every);
    assert!(!every.truncated);
    assert!(!index.filter(".", lens, total).truncated, "exactly the limit is not truncated");
    let cut = index.filter(".", lens, total - 1);
    assert!(cut.truncated);
    assert_eq!(count(&cut), total - 1);
    assert!(index.filter(".", lens, 0).truncated);
}

// ASC-FR-YNJR: every typed file and every assigned folder, in tree order.
#[test]
fn artifacts_lists_typed_files_and_assigned_folders() {
    let (_tmp, index) = sample();
    let paths: Vec<String> = index.artifacts().into_iter().map(|n| n.path).collect();
    assert_eq!(
        paths,
        vec![
            ".claude/skills/a/SKILL.md",
            "notes",
            "notes/n.md",
            "specifications/core/ASC.md",
            "specifications/ui/LIB.md",
        ]
    );
    let skill = index.artifacts().into_iter().find(|n| n.name == "SKILL.md").unwrap();
    assert_eq!(skill.display_name.as_deref(), Some("alpha"));
}

// ASC-FR-AVVY: the path holds the text, the kind narrows, and the limit cuts.
#[test]
fn search_paths_matches_the_path_and_the_kind() {
    let (_tmp, index) = sample();
    let folders = index.search_paths("spec", PathKind::Folder, None);
    assert_eq!(folders.paths, vec!["specifications", "specifications/core", "specifications/ui"]);
    let files = index.search_paths("specifications/u", PathKind::File, None);
    assert_eq!(files.paths, vec!["specifications/ui/LIB.md"]);
    let any = index.search_paths("", PathKind::Any, None);
    // Eight files and ten folders, `.synthesis` with them.
    assert_eq!(any.paths.len(), index.candidates().len() + 10, "every node, files and folders");
    let cut = index.search_paths("", PathKind::Any, Some(2));
    assert_eq!(cut.paths.len(), 2);
    assert!(cut.truncated);
}

// ASC-FR-QKXR: the held paths of a list, in input order.
#[test]
fn existing_keeps_held_paths_in_input_order() {
    let (_tmp, index) = sample();
    let asked = vec![
        "src/main.rs".to_string(),
        "gone.md".to_string(),
        "specifications".to_string(),
        String::new(),
        LIBRARY_TOML_REL.to_string(),
    ];
    assert_eq!(index.existing(&asked), vec!["src/main.rs", "specifications"]);
}

// ASC-FR-SKRN, ASC-FR-AVVY: the wire spelling of a lens and a kind.
#[test]
fn lens_and_kind_read_the_wire_spelling() {
    let lens: Lens = serde_json::from_str("\"all_artifacts\"").unwrap();
    assert_eq!(lens, Lens::Named(LensName::AllArtifacts));
    let lens: Lens = serde_json::from_str("\"spec\"").unwrap();
    assert_eq!(lens, Lens::Type(ArtifactType::Spec));
    assert!(serde_json::from_str::<Lens>("\"bogus\"").is_err());
    let kind: PathKind = serde_json::from_str("\"folder\"").unwrap();
    assert_eq!(kind, PathKind::Folder);
}

// ASC-FR-OGRB, ASC-FR-YSWU, ASC-FR-YLGN: the folder fields cross the wire in
// camelCase, and a file node carries none of them.
#[test]
fn folder_fields_serialise_camelcase() {
    let (_tmp, index) = sample();
    let root = index.children("").unwrap();
    let folder = serde_json::to_value(root.iter().find(|n| n.name == "notes").unwrap()).unwrap();
    assert_eq!(folder["subtreeTypes"], serde_json::json!(["scenario"]));
    assert_eq!(folder["hasChildren"], serde_json::json!(true));
    assert!(folder.get("nestedRepo").is_none());
    let file = serde_json::to_value(root.iter().find(|n| n.name == "Zeta.md").unwrap()).unwrap();
    for key in ["subtreeTypes", "hasChildren", "nestedRepo", "hasArtifacts", "children"] {
        assert!(file.get(key).is_none(), "{key} on a file");
    }
}
