//! Persisted assignments, folder inheritance and clearing
//! (ASC-FR-05, ASC-FR-06, ASC-FR-07, ASC-FR-08).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

// ------------------------------------------------------------------
// ASC-FR-05: file assignment persisted + reflected.
// ------------------------------------------------------------------
#[test]
fn ts5_assign_file_persists_and_classifies() {
    let tmp = project_with_synthesis();
    let root = &crate::fs::RootFs::for_root(tmp.path());
    fs::create_dir_all(root.join("notes")).unwrap();
    fs::write(root.join("notes/x.md"), "body").unwrap();

    assign(root, "notes/x.md", ArtifactType::Scenario, Scope::File).unwrap();

    // Persisted to `.synthesis/library.toml`.
    let stored = load_assignments(root);
    assert_eq!(
        stored.assignments.get("notes/x.md"),
        Some(&Assignment {
            artifact_type: ArtifactType::Scenario,
            scope: Scope::File,
        })
    );

    let tree = scan(root);
    let x = find(&tree, "notes/x.md").unwrap();
    assert_eq!(x.artifact_type, Some(ArtifactType::Scenario));
    assert_eq!(x.type_source, Some(TypeSource::Assigned));
}

// ------------------------------------------------------------------
// ASC-FR-06 / ASC-FR-08 / ASC-FR-05: folder assignment -> inherited +
// has_artifacts.
// ------------------------------------------------------------------
#[test]
fn ts6_folder_assignment_inherited_by_children() {
    // Use a neutral folder name (`plans`) that does NOT match a path
    // recognizer, so folder inheritance is unambiguously what classifies
    // the children.
    let entries = [
        entry("plans", true),
        entry("plans/a.md", false),
        entry("plans/b.md", false),
    ];
    let assignments = LibraryAssignments {
        assignments: BTreeMap::from([(
            "plans".to_string(),
            Assignment {
                artifact_type: ArtifactType::Spec,
                scope: Scope::Folder,
            },
        )]),
    };
    let tree = build_tree("p", &entries, &assignments, &no_content);
    let a = find(&tree, "plans/a.md").unwrap();
    let b = find(&tree, "plans/b.md").unwrap();
    assert_eq!(a.artifact_type, Some(ArtifactType::Spec));
    assert_eq!(a.type_source, Some(TypeSource::Inherited));
    assert_eq!(b.type_source, Some(TypeSource::Inherited));
    assert_eq!(find(&tree, "plans").unwrap().has_artifacts, Some(true));
}

// ------------------------------------------------------------------
// ASC-FR-06: per-file assignment beats folder inheritance.
// ------------------------------------------------------------------
#[test]
fn ts7_per_file_assignment_wins_over_folder() {
    let assignments = LibraryAssignments {
        assignments: BTreeMap::from([
            (
                "plans".to_string(),
                Assignment {
                    artifact_type: ArtifactType::Spec,
                    scope: Scope::Folder,
                },
            ),
            (
                "plans/a.md".to_string(),
                Assignment {
                    artifact_type: ArtifactType::Instructions,
                    scope: Scope::File,
                },
            ),
        ]),
    };
    let entries = [entry("plans", true), entry("plans/a.md", false)];
    let tree = build_tree("p", &entries, &assignments, &no_content);
    let a = find(&tree, "plans/a.md").unwrap();
    assert_eq!(a.artifact_type, Some(ArtifactType::Instructions));
    assert_eq!(a.type_source, Some(TypeSource::Assigned));
}

#[test]
fn path_inference_beats_folder_inheritance() {
    // ASC-FR-06 level 2 (path) outranks level 3 (folder inheritance).
    let assignments = LibraryAssignments {
        assignments: BTreeMap::from([(
            ".claude/skills".to_string(),
            Assignment {
                artifact_type: ArtifactType::Scratchpad,
                scope: Scope::Folder,
            },
        )]),
    };
    let entries = [
        entry(".claude", true),
        entry(".claude/skills", true),
        entry(".claude/skills/foo.md", false),
    ];
    let tree = build_tree("p", &entries, &assignments, &no_content);
    let foo = find(&tree, ".claude/skills/foo.md").unwrap();
    // Path says skill (level 2) — folder-inherited scratchpad (level 3)
    // must not win.
    assert_eq!(foo.artifact_type, Some(ArtifactType::Skill));
    assert_eq!(foo.type_source, Some(TypeSource::Inferred));
}

// ------------------------------------------------------------------
// ASC-FR-07: clear reverts to inferred/inherited/unclassified.
// ------------------------------------------------------------------
#[test]
fn ts8_clear_reverts_assignment() {
    let tmp = project_with_synthesis();
    let root = &crate::fs::RootFs::for_root(tmp.path());
    fs::create_dir_all(root.join("notes")).unwrap();
    fs::write(root.join("notes/x.md"), "body").unwrap();

    assign(root, "notes/x.md", ArtifactType::Scenario, Scope::File).unwrap();
    assert_eq!(
        scan(root).children.as_ref().unwrap().is_empty(),
        false
    );
    clear(root, "notes/x.md").unwrap();

    let tree = scan(root);
    let x = find(&tree, "notes/x.md").unwrap();
    assert_eq!(x.artifact_type, None);
    assert_eq!(x.type_source, None);
    // And the stored map no longer holds it.
    assert!(load_assignments(root).assignments.get("notes/x.md").is_none());
}

#[test]
fn clear_missing_assignment_is_noop() {
    let tmp = project_with_synthesis();
    let root = &crate::fs::RootFs::for_root(tmp.path());
    // Nothing stored yet; clearing must not error and must not create a
    // library.toml.
    clear(root, "notes/x.md").unwrap();
    assert!(!library_toml_path(root).exists());
}

#[test]
fn clear_folder_assignment_reverts_children_and_has_artifacts() {
    // ASC-FR-07 at folder scope: clearing a folder-scope assignment must
    // revert its children from `inherited` to unclassified and flip the
    // folder's `has_artifacts` back to false.
    let tmp = project_with_synthesis();
    let root = &crate::fs::RootFs::for_root(tmp.path());
    fs::create_dir_all(root.join("misc")).unwrap();
    fs::write(root.join("misc/readme.txt"), "x").unwrap();

    assign(root, "misc", ArtifactType::Prompt, Scope::Folder).unwrap();
    let before = scan(root);
    assert_eq!(
        find(&before, "misc/readme.txt").unwrap().type_source,
        Some(TypeSource::Inherited)
    );
    assert_eq!(find(&before, "misc").unwrap().has_artifacts, Some(true));

    clear(root, "misc").unwrap();
    let after = scan(root);
    assert_eq!(find(&after, "misc/readme.txt").unwrap().artifact_type, None);
    assert_eq!(find(&after, "misc").unwrap().has_artifacts, Some(false));
}

#[test]
fn assign_and_clear_reject_paths_that_escape_the_root() {
    // ASC-FR-12 / FSA-FR-10: a crafted `..` path must be rejected before it
    // can be stored, and nothing is written.
    let tmp = project_with_synthesis();
    let root = &crate::fs::RootFs::for_root(tmp.path());
    let err = assign(root, "../../etc/passwd", ArtifactType::Spec, Scope::File).unwrap_err();
    assert!(matches!(err, fsa::FsError::PathEscape { .. }), "got {err:?}");
    assert!(clear(root, "../../etc/passwd").is_err());
    // No attribution file was created by the rejected calls.
    assert!(!library_toml_path(root).exists());
}

#[test]
fn an_assignment_for_a_path_that_went_missing_is_inert_rather_than_an_error() {
    // ASC-FR-24 tail / ASC-FR-24: an assignment naming a path an external
    // process deleted surfaces as nothing, fails no scan, and is *retained*
    // — there is no repair pass — so it applies again if the path returns.
    let tmp = project_with_synthesis();
    let root = &crate::fs::RootFs::for_root(tmp.path());
    fs::create_dir_all(root.join("specifications")).unwrap();
    fs::write(root.join("specifications/overview.md"), b"").unwrap();
    assign(
        root,
        "specifications/overview.md",
        ArtifactType::Spec,
        Scope::File,
    )
    .unwrap();

    fs::remove_file(root.join("specifications/overview.md")).unwrap();

    let tree = scan(root);
    assert!(find(&tree, "specifications/overview.md").is_none());
    assert!(
        load_assignments(root)
            .assignments
            .contains_key("specifications/overview.md"),
        "the assignment is retained rather than pruned"
    );

    // And it applies again the moment the path comes back.
    fs::write(root.join("specifications/overview.md"), b"").unwrap();
    let again = scan(root);
    let node = find(&again, "specifications/overview.md").unwrap();
    assert_eq!(node.artifact_type, Some(ArtifactType::Spec));
    assert_eq!(node.type_source, Some(TypeSource::Assigned));
}
