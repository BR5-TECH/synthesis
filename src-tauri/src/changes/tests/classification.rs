//! The artifact type and the stable id an entry carries, and the flat list
//! (CHC-FR-10, CHC-FR-11, CHC-FR-12).
//!
//! One part of `../tests/mod.rs`, which holds the fixture these run against.

use super::*;

// -----------------------------------------------------------------------
// CHC-FR-10 / CHC-FR-11 — classification and ids
// -----------------------------------------------------------------------

#[test]
fn entries_carry_the_library_artifact_type_and_the_same_stable_id() {
    let f = Fixture::new();
    f.write(".claude/skills/foo.md", "# skill\n");
    f.write("package.json", "{}\n");
    f.commit("A");
    f.write(".claude/skills/foo.md", "# skill\nmore\n");
    f.write("package.json", "{ }\n");

    let set = uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap();
    let skill = entry(&set, ".claude/skills/foo.md");
    assert_eq!(skill.artifact_type, Some(ArtifactType::Skill));
    assert_eq!(skill.type_source, Some(TypeSource::Inferred));
    let unclassified = entry(&set, "package.json");
    assert_eq!(unclassified.artifact_type, None);

    // CHC-FR-11: the id is what `load_project_tree` reports.
    let tree = scanning::scan(&crate::fs::RootFs::for_root(f.root()));
    let tree_id = find_tree_id(&tree, ".claude/skills/foo.md")
        .expect("the Library tree holds the same file");
    assert_eq!(skill.id, tree_id);
    assert_eq!(skill.id, skill.path, "the id is the project-relative path");
}

// CHC-FR-10: a changed file carries exactly the type the Library shows for
// the same path, which means the resolved one — a user's assignment beats
// the path convention here too. The Diff tab routes on this field
// (`../ui/DFV-diff-viewer.md` DFV-FR-17), so an entry that reported the
// inferred type would send an assigned Flow to the wrong rendering.
#[test]
fn entries_carry_an_assigned_type_over_the_inferred_one() {
    let f = Fixture::new();
    f.write("workflows/review.flow", "{}\n");
    f.write("workflows/pipeline", "{}\n");
    f.commit("A");
    scanning::assign(
        &crate::fs::RootFs::for_root(f.root()),
        "workflows/review.flow",
        ArtifactType::Spec,
        scanning::Scope::File,
    )
    .unwrap();
    scanning::assign(
        &crate::fs::RootFs::for_root(f.root()),
        "workflows/pipeline",
        ArtifactType::Flow,
        scanning::Scope::File,
    )
    .unwrap();
    f.write("workflows/review.flow", "{ }\n");
    f.write("workflows/pipeline", "{ }\n");

    let set = uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap();

    let overridden = entry(&set, "workflows/review.flow");
    assert_eq!(overridden.artifact_type, Some(ArtifactType::Spec));
    assert_eq!(overridden.type_source, Some(TypeSource::Assigned));
    let assigned = entry(&set, "workflows/pipeline");
    assert_eq!(assigned.artifact_type, Some(ArtifactType::Flow));
    assert_eq!(assigned.type_source, Some(TypeSource::Assigned));
}

fn find_tree_id(node: &scanning::TreeNode, path: &str) -> Option<String> {
    if node.path == path {
        return Some(node.id.clone());
    }
    node.children
        .as_ref()?
        .iter()
        .find_map(|c| find_tree_id(c, path))
}

#[test]
fn a_deleted_artifact_keeps_its_path_derived_type_without_a_content_read() {
    // CHC-FR-10: the content tiebreak needs a file that is no
    // longer on disk, so a deleted path is classified from convention alone.
    let f = Fixture::new();
    f.write("specifications/ui/GON-gone.md", "# spec\n");
    f.commit("A");
    f.remove("specifications/ui/GON-gone.md");

    let set = uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap();
    let e = entry(&set, "specifications/ui/GON-gone.md");
    assert_eq!(e.change_status, ChangeStatus::Deleted);
    assert_eq!(
        e.artifact_type,
        Some(ArtifactType::Spec),
        "the path recognizer still applies to a file that is gone"
    );
}

#[test]
fn a_deleted_markdown_file_is_not_classified_from_its_vanished_content() {
    // The stronger half of CHC-FR-10: a path that only the content tiebreak
    // could classify comes back unclassified once the file is deleted,
    // proving no content read stands behind the result.
    let f = Fixture::new();
    f.write("notes/thing.md", "---\ntype: scenario\n---\nbody\n");
    f.commit("A");

    // While it exists, the content tiebreak classifies it.
    f.write("notes/thing.md", "---\ntype: scenario\n---\nbody\nmore\n");
    let present = uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap();
    assert_eq!(
        entry(&present, "notes/thing.md").artifact_type,
        Some(ArtifactType::Scenario)
    );

    f.remove("notes/thing.md");
    let absent = uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap();
    assert_eq!(
        entry(&absent, "notes/thing.md").artifact_type,
        None,
        "no content tiebreak is attempted for a deleted path"
    );
}

// -----------------------------------------------------------------------
// CHC-FR-12 — the list is flat
// -----------------------------------------------------------------------

#[test]
fn entries_are_a_single_flat_list_with_no_folders_or_untracked_grouping() {
    let f = Fixture::new();
    f.write("a/b/c.md", "c\n");
    f.commit("A");
    f.write("a/b/c.md", "c\nchanged\n");
    f.write("a/d/new.md", "new\n");

    let set = uncommitted_change_set(&crate::fs::RootFs::for_root(f.root())).unwrap();
    let got = paths(&set);
    assert!(got.contains(&"a/b/c.md".to_string()), "{got:?}");
    assert!(got.contains(&"a/d/new.md".to_string()), "{got:?}");
    assert!(
        !got.iter().any(|p| p == "a" || p == "a/b" || p == "a/d"),
        "no folder entries: {got:?}"
    );
    // The untracked entry sits in the same list as the modified one.
    let json = serde_json::to_value(&set).unwrap();
    assert!(json.get("entries").unwrap().is_array());
    assert!(
        json.as_object().unwrap().len() == 2,
        "only `comparison` and `entries`, no separate untracked collection"
    );
}
