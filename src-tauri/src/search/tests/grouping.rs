//! How a hit is grouped (SCC-FR-08, SCC-FR-09).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// SCC-FR-08 / SCC-FR-09 — grouping
// -----------------------------------------------------------------------

#[test]
fn ts10_grouping_splits_artifacts_entities_and_plain_files() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write(root, ".claude/skills/foo.md", "needle\n");
    write(root, ".synthesis/playbooks/release.md", "needle\n");
    write(root, "src/main.rs", "needle\n");
    write(root, "workflows/review.flow", "{\"needle\":1}\n");

    let (sink, _) = search(root, "needle", SearchMode::LiteralInsensitive, SearchScope::Full);
    let by_path: std::collections::HashMap<String, SearchHit> = sink
        .hits()
        .into_iter()
        .map(|h| (h.path.clone(), h))
        .collect();

    let skill = &by_path[".claude/skills/foo.md"];
    assert_eq!(skill.group, SearchGroup::Artifact);
    assert_eq!(skill.subtype, Some(ArtifactType::Skill));
    assert_eq!(skill.edit_context, Some(EditContext::Standalone));

    // SCC-FR-08: a Flow is an artifact whose edit context is its own canvas,
    // which is what routes a result to the Flow tab rather than the Editor.
    let flow = &by_path["workflows/review.flow"];
    assert_eq!(flow.group, SearchGroup::Artifact);
    assert_eq!(flow.subtype, Some(ArtifactType::Flow));
    assert_eq!(flow.edit_context, Some(EditContext::Flow));

    assert_eq!(by_path[".synthesis/playbooks/release.md"].group, SearchGroup::Playbook);
    let file = &by_path["src/main.rs"];
    assert_eq!(file.group, SearchGroup::File);
    assert_eq!(file.subtype, None, "the file group carries no subtype");
    assert_eq!(file.edit_context, None);
}

#[test]
fn ts11_an_entity_directory_outranks_artifact_classification() {
    // SCC-FR-08: a playbook that ALSO classifies as an artifact is grouped as
    // a playbook. `.spec.md` is enough to classify it (ASC-FR-03).
    assert_eq!(
        group_for(".synthesis/playbooks/release.spec.md", Some(ArtifactType::Spec)),
        SearchGroup::Playbook
    );
    assert_eq!(
        group_for(".synthesis/workstreams/w.md", Some(ArtifactType::Skill)),
        SearchGroup::Workstream
    );
    assert_eq!(
        group_for(".synthesis/roles/r.md", Some(ArtifactType::Agent)),
        SearchGroup::Role
    );
    // A `.synthesis/` path that is NOT one of the three entity directories
    // falls through to ordinary classification.
    assert_eq!(group_for(".synthesis/notes/x.md", None), SearchGroup::File);
}

#[test]
fn ts12_the_run_and_history_groups_are_never_populated() {
    // SCC-FR-09: they are part of the payload shape and this engine, which
    // matches only files under the content root, never produces them.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write(root, "runs/run-7e3.md", "needle\n");
    write(root, "history/old.md", "needle\n");
    write(root, ".claude/skills/s.md", "needle\n");

    let (sink, _) = search(root, "needle", SearchMode::LiteralInsensitive, SearchScope::Full);
    assert!(!sink.hits().is_empty(), "precondition: something matched");
    for hit in sink.hits() {
        assert!(
            !matches!(hit.group, SearchGroup::Run | SearchGroup::History),
            "{hit:?}"
        );
    }
}
