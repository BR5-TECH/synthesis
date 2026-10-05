//! What a node reports about itself: natural location, `has_artifacts`, and
//! folder-scope assignment (PST-FR-07, ASC-FR-08, ASC-FR-18).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

// ------------------------------------------------------------------
// PST-FR-07 / FLO-FR-24: the scanned tree places a Flow at its natural
// filesystem location — there is no synthetic "flows"/"Flows" grouping node.
// ------------------------------------------------------------------
#[test]
fn a_flow_appears_at_its_natural_location_not_a_flows_group() {
    let tmp = project_with_synthesis();
    let root = &crate::fs::RootFs::for_root(tmp.path());
    fs::create_dir_all(root.join("workflows")).unwrap();
    fs::write(
        root.join("workflows/review.flow"),
        "{\"version\":1,\"nodes\":[],\"edges\":[]}\n",
    )
    .unwrap();

    let tree = scan(root);
    let flow = find(&tree, "workflows/review.flow").expect("flow node present");
    assert_eq!(flow.artifact_type, Some(ArtifactType::Flow));
    // It is a child of its real parent folder, not of any top-level group.
    let parent = find(&tree, "workflows").unwrap();
    assert!(parent
        .children
        .as_ref()
        .unwrap()
        .iter()
        .any(|c| c.path == "workflows/review.flow"));
    // No synthetic flows grouping at any level.
    assert!(find(&tree, "flows").is_none());
    assert!(find(&tree, "Flows").is_none());
}

// ------------------------------------------------------------------
// ASC-FR-04: the content tiebreak fires ONLY when path inference fails — a
// path-classified file is never overridden by a conflicting frontmatter.
// ------------------------------------------------------------------
#[test]
fn content_tiebreak_does_not_override_a_path_classified_file() {
    let entries = [
        entry(".claude", true),
        entry(".claude/skills", true),
        entry(".claude/skills/x.md", false),
    ];
    // A content closure that WOULD say "spec" — it must never be consulted
    // because the path already classifies the file as a skill.
    let content = |_rel: &str| -> ContentFacts {
        ContentFacts {
            artifact_type: Some(ArtifactType::Spec),
            declared_name: None,
        }
    };
    let tree = build_tree("p", &entries, &LibraryAssignments::default(), &content);
    let x = find(&tree, ".claude/skills/x.md").unwrap();
    assert_eq!(x.artifact_type, Some(ArtifactType::Skill));
    assert_eq!(x.type_source, Some(TypeSource::Inferred));
}

// ------------------------------------------------------------------
// ASC-FR-08: empty-folder has_artifacts + folder assign flips it.
// ------------------------------------------------------------------
#[test]
fn ts9_empty_folder_has_no_artifacts_until_assigned() {
    let entries = [entry("misc", true), entry("misc/readme.txt", false)];
    // No recognizer matches `readme.txt`, no assignments -> empty.
    let tree = build_tree("p", &entries, &LibraryAssignments::default(), &no_content);
    assert_eq!(find(&tree, "misc").unwrap().has_artifacts, Some(false));

    // Folder-assign `misc` as prompt -> has_artifacts true.
    let assignments = LibraryAssignments {
        assignments: BTreeMap::from([(
            "misc".to_string(),
            Assignment {
                artifact_type: ArtifactType::Prompt,
                scope: Scope::Folder,
            },
        )]),
    };
    let tree = build_tree("p", &entries, &assignments, &no_content);
    assert_eq!(find(&tree, "misc").unwrap().has_artifacts, Some(true));
    // And the file inside now inherits the type.
    assert_eq!(
        find(&tree, "misc/readme.txt").unwrap().type_source,
        Some(TypeSource::Inherited)
    );
}

#[test]
fn has_artifacts_propagates_up_nested_folders() {
    let entries = [
        entry("a", true),
        entry("a/b", true),
        entry("a/b/c", true),
        entry("a/b/c/foo.spec.md", false),
        entry("empty", true),
        entry("empty/sub", true),
    ];
    let tree = build_tree("p", &entries, &LibraryAssignments::default(), &no_content);
    // The artifact is deep under a/b/c; every ancestor reports has_artifacts.
    assert_eq!(find(&tree, "a").unwrap().has_artifacts, Some(true));
    assert_eq!(find(&tree, "a/b").unwrap().has_artifacts, Some(true));
    assert_eq!(find(&tree, "a/b/c").unwrap().has_artifacts, Some(true));
    // The `empty` subtree has no artifacts at any level.
    assert_eq!(find(&tree, "empty").unwrap().has_artifacts, Some(false));
    assert_eq!(find(&tree, "empty/sub").unwrap().has_artifacts, Some(false));
}

// ------------------------------------------------------------------
// ASC-FR-18 / ASC-FR-07: a folder node reports its OWN folder-scope
// assignment, and only that — never a type derived from its contents.
// ------------------------------------------------------------------
#[test]
fn ts20_a_folder_reports_its_own_assignment_and_nothing_else() {
    let entries = [
        entry("specifications", true),
        entry("specifications/x.md", false),
        entry("src", true),
        entry("src/AGENTS.md", false),
    ];
    let assignments = LibraryAssignments {
        assignments: BTreeMap::from([(
            "specifications".to_string(),
            Assignment {
                artifact_type: ArtifactType::Spec,
                scope: Scope::Folder,
            },
        )]),
    };
    let tree = build_tree("p", &entries, &assignments, &no_content);

    let specs = find(&tree, "specifications").unwrap();
    assert_eq!(specs.artifact_type, Some(ArtifactType::Spec));
    assert_eq!(specs.type_source, Some(TypeSource::Assigned));

    // `src/` holds an `agent`-classified file but carries no assignment of
    // its own, so it reports neither field: no inheritance travels upward
    // into a directory (ASC-FR-18).
    assert_eq!(
        find(&tree, "src/AGENTS.md").unwrap().artifact_type,
        Some(ArtifactType::Agent)
    );
    let src = find(&tree, "src").unwrap();
    assert_eq!(src.artifact_type, None);
    assert_eq!(src.type_source, None);

    // ASC-FR-07: clearing the assignment leaves the folder untyped again.
    let tree = build_tree("p", &entries, &LibraryAssignments::default(), &no_content);
    let specs = find(&tree, "specifications").unwrap();
    assert_eq!(specs.artifact_type, None);
    assert_eq!(specs.type_source, None);
}

#[test]
fn ts20_a_subfolder_does_not_wear_its_ancestors_assignment() {
    // ASC-FR-18: `artifact_type` on a folder node is that folder's OWN
    // assignment. Falling back to the nearest ancestor here — the natural way
    // to get this wrong — would make every nested folder under an assigned one
    // wear its chip (LIB-FR-08) and show under that type's lens (LIB-FR-09),
    // whether or not it was ever typed.
    //
    // Note that folder *creation* materialises inheritance as a real
    // assignment on the new folder (PST-FR-25), so a folder made through the
    // app inside a typed one does report a type — via its own entry, which is
    // what this distinguishes from.
    // `curated/` rather than `specifications/`: the latter is a path-inference
    // recognizer, which would make the file below `inferred` and hide the
    // inheritance this pins.
    let entries = [
        entry("curated", true),
        entry("curated/inner", true),
        entry("curated/inner/x.txt", false),
    ];
    let assignments = LibraryAssignments {
        assignments: BTreeMap::from([(
            "curated".to_string(),
            Assignment {
                artifact_type: ArtifactType::Spec,
                scope: Scope::Folder,
            },
        )]),
    };
    let tree = build_tree("p", &entries, &assignments, &no_content);

    assert_eq!(
        find(&tree, "curated").unwrap().artifact_type,
        Some(ArtifactType::Spec)
    );
    let inner = find(&tree, "curated/inner").unwrap();
    assert_eq!(
        inner.artifact_type, None,
        "a subfolder carries no type of its own"
    );
    assert_eq!(inner.type_source, None);
    // Its *files* do inherit, which is the ASC-FR-06 rule this must not be
    // confused with.
    let deep = find(&tree, "curated/inner/x.txt").unwrap();
    assert_eq!(deep.artifact_type, Some(ArtifactType::Spec));
    assert_eq!(deep.type_source, Some(TypeSource::Inherited));
}

#[test]
fn inherited_folder_type_resolves_the_nearest_ancestor_assignment() {
    // The lookup folder creation uses (PST-FR-25). Nearest-ancestor, ignoring
    // file-scope entries, and `None` where there is nothing to inherit.
    let tmp = project_with_synthesis();
    let root = &crate::fs::RootFs::for_root(tmp.path());
    assign(root, "a", ArtifactType::Prompt, Scope::Folder).unwrap();
    assign(root, "a/b", ArtifactType::Spec, Scope::Folder).unwrap();
    assign(root, "f", ArtifactType::Agent, Scope::File).unwrap();

    // Nearest wins over the more distant ancestor.
    assert_eq!(
        inherited_folder_type(root, "a/b/new"),
        Some(ArtifactType::Spec)
    );
    assert_eq!(inherited_folder_type(root, "a/new"), Some(ArtifactType::Prompt));
    // A folder's own assignment is not "inherited" by itself — the walk starts
    // at the parent.
    assert_eq!(inherited_folder_type(root, "a"), None);
    // A file-scope ancestor entry is not inheritable.
    assert_eq!(inherited_folder_type(root, "f/new"), None);
    // Nothing above it at all.
    assert_eq!(inherited_folder_type(root, "elsewhere/new"), None);
}

#[test]
fn ts20_a_file_scope_assignment_on_a_folder_path_does_not_type_the_folder() {
    // Only a *folder*-scope assignment types a folder (ASC-FR-18). A stored
    // `scope = file` entry whose path happens to name a directory is not one,
    // and must not tag it — nor make it look artifact-bearing (ASC-FR-08).
    let entries = [entry("misc", true)];
    let assignments = LibraryAssignments {
        assignments: BTreeMap::from([(
            "misc".to_string(),
            Assignment {
                artifact_type: ArtifactType::Prompt,
                scope: Scope::File,
            },
        )]),
    };
    let tree = build_tree("p", &entries, &assignments, &no_content);
    let misc = find(&tree, "misc").unwrap();
    assert_eq!(misc.artifact_type, None);
    assert_eq!(misc.type_source, None);
    assert_eq!(misc.has_artifacts, Some(false));
}
