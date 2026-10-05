//! The declared name through a real scan, and the tree-changed payload
//! (ASC-FR-19).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

// ASC-FR-19: the whole of it, off a real filesystem — the `.md` gate and the
// FSA read `scan` wires the name through are covered nowhere else.
#[test]
fn ts21_scan_carries_each_skills_declared_name() {
    let tmp = project_with_synthesis();
    let root = &crate::fs::RootFs::for_root(tmp.path());
    fs::create_dir_all(root.join(".claude/skills/code-review/references")).unwrap();
    fs::create_dir_all(root.join(".claude/skills/nameless")).unwrap();
    fs::write(
        root.join(".claude/skills/code-review/SKILL.md"),
        "---\nname: Code review\ndescription: d\n---\n\n# Body\n",
    )
    .unwrap();
    fs::write(
        root.join(".claude/skills/code-review/references/rules.md"),
        "---\nname: Not a skill\n---\n",
    )
    .unwrap();
    fs::write(
        root.join(".claude/skills/nameless/SKILL.md"),
        "---\ndescription: no name here\n---\n",
    )
    .unwrap();

    let tree = scan(root);

    let named = find(&tree, ".claude/skills/code-review/SKILL.md").unwrap();
    assert_eq!(named.artifact_type, Some(ArtifactType::Skill));
    assert_eq!(named.display_name.as_deref(), Some("Code review"));
    // Two skills in one tree are told apart by their declared names, which is
    // what ASC-FR-19 exists for — their filenames are identical.
    let nameless = find(&tree, ".claude/skills/nameless/SKILL.md").unwrap();
    assert_eq!(nameless.artifact_type, Some(ArtifactType::Skill));
    assert_eq!(nameless.display_name, None);
    // The supporting file classifies as `skill` too, and still names nothing.
    let beside = find(&tree, ".claude/skills/code-review/references/rules.md").unwrap();
    assert_eq!(beside.artifact_type, Some(ArtifactType::Skill));
    assert_eq!(beside.display_name, None);
}

#[test]
fn ts21_declared_name_ignores_a_block_scalar_indicator() {
    // `name: |` opens a block scalar whose value is on the lines beneath;
    // reading the indicator would render a node as "|".
    assert_eq!(
        read_content_facts("---\nname: |\n  Code review\n---\n").declared_name,
        None,
    );
    assert_eq!(
        read_content_facts("---\nname: >\n  Code review\n---\n").declared_name,
        None,
    );
}

#[test]
fn ts21_skill_entry_point_is_matched_case_insensitively() {
    // The UI applies the same rule to decide what the picker offers
    // (`FLO-flow.md` FLO-FR-10); the two must agree on what a skill's file is.
    assert!(names_itself("a/Skill.md", Some(ArtifactType::Skill)));
    assert!(names_itself("SKILL.md", Some(ArtifactType::Skill)));
    assert!(!names_itself("a/skill.markdown", Some(ArtifactType::Skill)));
    assert!(!names_itself("a/SKILL.md", Some(ArtifactType::Prompt)));
    assert!(!names_itself("a/SKILL.md", None));
}

#[test]
fn tree_changed_payload_serializes_camelcase() {
    let json = serde_json::to_value(TreeChangedPayload {
        change_count: 5,
        removed_paths: vec!["docs/gone.md".to_string()],
    })
    .unwrap();
    assert_eq!(json.get("changeCount").and_then(|v| v.as_u64()), Some(5));
    // ASC-FR-22: the removed paths cross the wire camelCased alongside it.
    assert_eq!(
        json.get("removedPaths").and_then(|v| v.as_array()),
        Some(&vec![serde_json::json!("docs/gone.md")])
    );
    // Always present: an empty list rather than an absent field, so a
    // consumer never has to tell "nothing removed" from "not reported".
    let empty = serde_json::to_value(TreeChangedPayload {
        change_count: 1,
        removed_paths: Vec::new(),
    })
    .unwrap();
    assert_eq!(
        empty.get("removedPaths").and_then(|v| v.as_array()),
        Some(&vec![])
    );
}
