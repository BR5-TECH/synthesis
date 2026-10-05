//! The `library.toml` round trip and the shapes the frontend types against.
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

// ------------------------------------------------------------------
// library.toml roundtrip + wire shape.
// ------------------------------------------------------------------
#[test]
fn assignments_roundtrip_through_toml() {
    let tmp = project_with_synthesis();
    let root = &crate::fs::RootFs::for_root(tmp.path());
    assign(root, "a/b.flow", ArtifactType::Flow, Scope::File).unwrap();
    assign(root, "specs", ArtifactType::Spec, Scope::Folder).unwrap();
    let loaded = load_assignments(root);
    assert_eq!(
        loaded.assignments.get("a/b.flow").unwrap().artifact_type,
        ArtifactType::Flow
    );
    assert_eq!(loaded.assignments.get("specs").unwrap().scope, Scope::Folder);
}

#[test]
fn tree_node_serializes_camelcase_and_omits_empty_optionals() {
    let file = TreeNode {
        id: "a.md".into(),
        name: "a.md".into(),
        path: "a.md".into(),
        node_kind: NodeKind::File,
        artifact_type: Some(ArtifactType::Skill),
        type_source: Some(TypeSource::Inferred),
        display_name: None,
        has_artifacts: None,
        children: None,
    };
    let json = serde_json::to_value(&file).unwrap();
    assert_eq!(json.get("nodeKind").and_then(|v| v.as_str()), Some("file"));
    assert_eq!(json.get("artifactType").and_then(|v| v.as_str()), Some("skill"));
    assert_eq!(json.get("typeSource").and_then(|v| v.as_str()), Some("inferred"));
    // A node with nothing to declare omits the field entirely, which is what
    // tells the UI to go by the basename (ASC-FR-19).
    assert!(json.get("displayName").is_none());
    // File nodes omit folder-only fields.
    assert!(json.get("hasArtifacts").is_none());
    assert!(json.get("children").is_none());

    // …and the wire name when there is one. This is the field the Flow
    // canvas reads (`../ui/FLO-flow.md` FLO-FR-11), so the camelCase spelling
    // is part of the contract, not an implementation detail.
    let named = TreeNode {
        display_name: Some("Code review".into()),
        ..file
    };
    assert_eq!(
        serde_json::to_value(&named)
            .unwrap()
            .get("displayName")
            .and_then(|v| v.as_str()),
        Some("Code review"),
    );
}

#[test]
fn artifact_type_and_scope_deserialize_from_wire_strings() {
    // Mirrors the command param decoding the frontend drives.
    let t: ArtifactType = serde_json::from_str("\"flow\"").unwrap();
    assert_eq!(t, ArtifactType::Flow);
    // ASC-FR-02 renamed the type; the previous spelling stays readable so a
    // stored assignment written under it is not lost.
    let legacy: ArtifactType = serde_json::from_str("\"harness\"").unwrap();
    assert_eq!(legacy, ArtifactType::Flow);
    // …and `flow` is the only spelling ever produced.
    assert_eq!(
        serde_json::to_value(ArtifactType::Flow).unwrap().as_str(),
        Some("flow")
    );
    let s: Scope = serde_json::from_str("\"folder\"").unwrap();
    assert_eq!(s, Scope::Folder);
    assert!(serde_json::from_str::<ArtifactType>("\"unknown\"").is_err());
    assert!(serde_json::from_str::<Scope>("\"page\"").is_err());
}
