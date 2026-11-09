//! The `architecture` type: a FINOS CALM document, recognised by its
//! `*.calm.json` name (ASC-FR-02, ASC-FR-03, ASC-FR-QVCN, ASC-FR-06).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

// ASC-FR-QVCN: a `*.calm.json` file at the root is inferred as `architecture`.
#[test]
fn a_calm_json_file_at_the_root_is_an_inferred_architecture() {
    assert_eq!(infer_from_path("system.calm.json"), Some(ArtifactType::Architecture));

    let entries = [entry("system.calm.json", false)];
    let tree = build_tree("p", &entries, &LibraryAssignments::default(), &no_content);
    let node = find(&tree, "system.calm.json").unwrap();
    assert_eq!(node.artifact_type, Some(ArtifactType::Architecture));
    assert_eq!(node.type_source, Some(TypeSource::Inferred));
}

// ASC-FR-QVCN: the suffix rule applies at any depth, and it outranks every
// folder convention of ASC-FR-03.
#[test]
fn the_calm_json_suffix_outranks_every_folder_convention() {
    for rel in [
        "specifications/x.calm.json",
        "specifications/core/deep/x.calm.json",
        ".claude/skills/review/x.calm.json",
        ".synthesis/skills/review/x.calm.json",
        ".claude/agents/x.calm.json",
        ".claude/commands/x.calm.json",
        ".github/prompts/x.calm.json",
        ".github/copilot-instructions.calm.json",
        "docs/architecture/system.calm.json",
    ] {
        assert_eq!(infer_from_path(rel), Some(ArtifactType::Architecture), "{rel}");
    }

    let entries = [entry("specifications", true), entry("specifications/x.calm.json", false)];
    let tree = build_tree("p", &entries, &LibraryAssignments::default(), &no_content);
    let node = find(&tree, "specifications/x.calm.json").unwrap();
    assert_eq!(node.artifact_type, Some(ArtifactType::Architecture));
    assert_eq!(node.type_source, Some(TypeSource::Inferred));
}

// ASC-FR-QVCN: the suffix is matched without regard to letter case.
#[test]
fn the_calm_json_suffix_ignores_letter_case() {
    for rel in ["X.CALM.JSON", "models/System.Calm.Json", "a.calm.JSON"] {
        assert_eq!(infer_from_path(rel), Some(ArtifactType::Architecture), "{rel}");
    }
}

// ASC-FR-QVCN: only the full `.calm.json` suffix makes an architecture. Any
// other JSON file gets no type from its name.
#[test]
fn other_json_files_are_not_an_architecture() {
    for rel in [
        "x.json",
        "calm.json",
        "models/xcalm.json",
        "system.calm",
        "system.calm.json.bak",
        "system.calm.jsonc",
        "calm/system.json",
    ] {
        assert_eq!(infer_from_path(rel), None, "{rel}");
    }
}

// ASC-FR-QVCN, ASC-FR-04: no content marker produces `architecture`.
#[test]
fn no_content_marker_produces_an_architecture() {
    let marked = "---\ntype: architecture\n---\nbody\n";
    assert_eq!(infer_from_content(marked), None);
    assert_eq!(infer_from_content("---\nkind: Architecture\n---\n"), None);

    let content = facts_for("notes/x.md", marked);
    let empty = LibraryAssignments::default();
    assert_eq!(classify_file("notes/x.md", &empty, &content), (None, None));
}

// ASC-FR-06, ASC-FR-QVCN: a file-scope assignment wins over the suffix, and an
// assignment is the only way any other JSON file reaches `architecture`.
#[test]
fn a_file_assignment_wins_over_the_calm_json_suffix() {
    let mut stored = LibraryAssignments::default();
    stored.assignments.insert(
        "system.calm.json".to_string(),
        Assignment { artifact_type: ArtifactType::Spec, scope: Scope::File },
    );
    stored.assignments.insert(
        "models/system.json".to_string(),
        Assignment { artifact_type: ArtifactType::Architecture, scope: Scope::File },
    );
    assert_eq!(
        classify_file("system.calm.json", &stored, &no_content),
        (Some(ArtifactType::Spec), Some(TypeSource::Assigned)),
    );
    assert_eq!(
        classify_file("models/system.json", &stored, &no_content),
        (Some(ArtifactType::Architecture), Some(TypeSource::Assigned)),
    );
}

// ASC-FR-06: the inferred `architecture` wins over a folder-scope assignment,
// and a folder-scope `architecture` assignment types the files below it.
#[test]
fn the_suffix_wins_over_a_folder_assignment_and_a_folder_can_give_the_type() {
    let mut stored = LibraryAssignments::default();
    stored.assignments.insert(
        "notes".to_string(),
        Assignment { artifact_type: ArtifactType::Scratchpad, scope: Scope::Folder },
    );
    stored.assignments.insert(
        "models".to_string(),
        Assignment { artifact_type: ArtifactType::Architecture, scope: Scope::Folder },
    );
    assert_eq!(
        classify_file("notes/system.calm.json", &stored, &no_content),
        (Some(ArtifactType::Architecture), Some(TypeSource::Inferred)),
    );
    assert_eq!(
        classify_file("models/payments.json", &stored, &no_content),
        (Some(ArtifactType::Architecture), Some(TypeSource::Inherited)),
    );
}

// ASC-FR-QVCN, ASC-FR-05: the same through a real scan and a real
// `.synthesis/library.toml`, and clearing the assignment gives the file back
// to the suffix rule (ASC-FR-07).
#[test]
fn a_scan_on_disk_classifies_a_calm_json_file() {
    let tmp = project_with_synthesis();
    let root = &crate::fs::RootFs::for_root(tmp.path());
    fs::create_dir_all(root.join("specifications")).unwrap();
    fs::write(root.join("specifications/system.calm.json"), "{}").unwrap();
    fs::write(root.join("plain.json"), "{}").unwrap();

    let tree = scan(root);
    let node = find(&tree, "specifications/system.calm.json").unwrap();
    assert_eq!(node.artifact_type, Some(ArtifactType::Architecture));
    assert_eq!(node.type_source, Some(TypeSource::Inferred));
    assert_eq!(find(&tree, "plain.json").unwrap().artifact_type, None);

    assign(root, "specifications/system.calm.json", ArtifactType::Spec, Scope::File).unwrap();
    assign(root, "plain.json", ArtifactType::Architecture, Scope::File).unwrap();
    let tree = scan(root);
    assert_eq!(
        find(&tree, "specifications/system.calm.json").unwrap().artifact_type,
        Some(ArtifactType::Spec),
    );
    let plain = find(&tree, "plain.json").unwrap();
    assert_eq!(plain.artifact_type, Some(ArtifactType::Architecture));
    assert_eq!(plain.type_source, Some(TypeSource::Assigned));

    clear(root, "specifications/system.calm.json").unwrap();
    let reverted = find(&scan(root), "specifications/system.calm.json").unwrap().clone();
    assert_eq!(reverted.artifact_type, Some(ArtifactType::Architecture));
    assert_eq!(reverted.type_source, Some(TypeSource::Inferred));
}
