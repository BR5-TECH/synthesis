//! The OpenSpec path conventions (ASC-FR-WOQW, ASC-FR-YODA, ASC-FR-NBNP).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use std::collections::BTreeMap;

use super::*;

// ASC-FR-WOQW: a `spec.md` below `openspec/specs/` is a spec, at any depth.
#[test]
fn a_capability_spec_file_is_an_inferred_spec() {
    for rel in
        ["openspec/specs/spec.md", "openspec/specs/auth/spec.md", "openspec/specs/ui/theme/spec.md"]
    {
        assert_eq!(infer_from_path(rel), Some(ArtifactType::Spec), "{rel}");
    }
}

// ASC-FR-WOQW: only the file named `spec.md` is a spec there, and the rule is
// anchored at the project root.
#[test]
fn other_files_of_the_specs_folder_and_nested_openspec_folders_get_no_openspec_type() {
    for rel in ["openspec/specs/auth/notes.md", "docs/openspec/specs/auth/spec.md"] {
        assert_eq!(infer_from_path(rel), None, "{rel}");
    }
}

// ASC-FR-WOQW: every Markdown file of an active change is a spec, the delta
// files included.
#[test]
fn the_markdown_files_of_an_active_change_are_inferred_specs() {
    for rel in [
        "openspec/changes/add-login/proposal.md",
        "openspec/changes/add-login/design.md",
        "openspec/changes/add-login/tasks.md",
        "openspec/changes/add-login/specs/auth/spec.md",
        "openspec/changes/add-login/NOTES.MD",
    ] {
        assert_eq!(infer_from_path(rel), Some(ArtifactType::Spec), "{rel}");
    }
    assert_eq!(infer_from_path("openspec/changes/add-login/.openspec.yaml"), None);
}

// ASC-FR-WOQW: `openspec/config.yaml` holds instructions; a config file
// anywhere else does not.
#[test]
fn the_openspec_config_file_is_inferred_instructions() {
    assert_eq!(infer_from_path("openspec/config.yaml"), Some(ArtifactType::Instructions));
    assert_eq!(infer_from_path("openspec/schemas/config.yaml"), None);
    assert_eq!(infer_from_path("tools/openspec/config.yaml"), None);
}

// ASC-FR-YODA: no path convention types a file of an archived change, the CALM
// suffix and the frontmatter marker included.
#[test]
fn an_archived_change_gets_no_inferred_type() {
    for rel in [
        "openspec/changes/archive/2025-01-23-add-login/proposal.md",
        "openspec/changes/archive/2025-01-23-add-login/specs/auth/spec.md",
        "openspec/changes/archive/2025-01-23-add-login/AGENTS.md",
        "openspec/changes/archive/2025-01-23-add-login/system.calm.json",
    ] {
        assert_eq!(infer_from_path(rel), None, "{rel}");
    }

    let rel = "openspec/changes/archive/2025-01-23-add-login/proposal.md";
    let entries = [entry(rel, false)];
    let content = facts_for(rel, "---\ntype: prompt\n---\nbody\n");
    let tree = build_tree("p", &entries, &LibraryAssignments::default(), &content);
    let node = find(&tree, rel).unwrap();
    assert_eq!(node.artifact_type, None);
    assert_eq!(node.type_source, None);
}

// ASC-FR-YODA: an assignment still types a file of an archived change, at file
// scope and through a folder.
#[test]
fn an_archived_change_is_typed_by_an_assignment() {
    let file = "openspec/changes/archive/2025-01-23-add-login/proposal.md";
    let other = "openspec/changes/archive/2025-01-23-add-login/design.md";
    let entries = [entry(file, false), entry(other, false)];
    let assignments = LibraryAssignments {
        assignments: BTreeMap::from([
            (file.to_string(), Assignment { artifact_type: ArtifactType::Doc, scope: Scope::File }),
            (
                "openspec/changes/archive".to_string(),
                Assignment { artifact_type: ArtifactType::Spec, scope: Scope::Folder },
            ),
        ]),
    };
    let tree = build_tree("p", &entries, &assignments, &no_content);
    let assigned = find(&tree, file).unwrap();
    assert_eq!(assigned.artifact_type, Some(ArtifactType::Doc));
    assert_eq!(assigned.type_source, Some(TypeSource::Assigned));
    let inherited = find(&tree, other).unwrap();
    assert_eq!(inherited.artifact_type, Some(ArtifactType::Spec));
    assert_eq!(inherited.type_source, Some(TypeSource::Inherited));
}

// ASC-FR-NBNP: the conventions need no setting. A scan of a real project that
// stores `openspecMode = "off"` still types its OpenSpec files.
#[test]
fn the_conventions_apply_while_the_openspec_mode_is_off() {
    let tmp = project_with_synthesis();
    let dir = tmp.path();
    fs::write(dir.join(".synthesis/project.toml"), "openspecMode = \"off\"\n").unwrap();
    fs::create_dir_all(dir.join("openspec/specs/auth")).unwrap();
    fs::write(dir.join("openspec/specs/auth/spec.md"), "# Auth\n").unwrap();
    fs::write(dir.join("openspec/config.yaml"), "schema: spec-driven\n").unwrap();

    let tree = scan(&crate::fs::RootFs::for_root(dir));
    let spec = find(&tree, "openspec/specs/auth/spec.md").unwrap();
    assert_eq!(spec.artifact_type, Some(ArtifactType::Spec));
    assert_eq!(spec.type_source, Some(TypeSource::Inferred));
    let config = find(&tree, "openspec/config.yaml").unwrap();
    assert_eq!(config.artifact_type, Some(ArtifactType::Instructions));
}
