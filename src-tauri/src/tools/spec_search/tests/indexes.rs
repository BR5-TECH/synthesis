//! SPS-FR-05: the spec index alone.

use super::*;

// ---------------------------------------------------------------------------
// SPS-FR-05 — the spec index alone (SPS-FR-05)
// ---------------------------------------------------------------------------

#[test]
fn no_other_index_is_reachable_through_this_tool() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(root, "specifications/core/a.md", &spec_body("sarcophagus", 1));
    // A skill, a flow, and a prompt each carrying the same term. Every one of
    // them lands in an index this tool must not reach.
    crate::tools::tests::write_skill(
        root,
        ".claude/skills",
        "digger",
        "name: digger\ndescription: a skill about sarcophagus handling",
        "sarcophagus sarcophagus sarcophagus",
    );
    write(root, "flows/x.flow", "{\"note\":\"sarcophagus\"}");
    write(root, "resources/prompts/p.md", "# sarcophagus\n\nsarcophagus\n");
    // SPS-FR-05 names the `notes` index among the ten this tool cannot reach.
    crate::notes::create_note_in(
        &crate::fs::RootFs::for_root(root),
        crate::notes::NoteScope::Project,
        "sarcophagus in a note".to_string(),
        None,
        None,
        "2026-01-01T00:00:00Z",
    )
    .expect("the note writes");
    let fixture = mounted(dir);

    let output = query(&fixture, "sarcophagus", Some(20));
    let paths: Vec<&str> = output
        .specifications
        .iter()
        .map(|m| m.path.as_str())
        .collect();

    assert_eq!(
        paths,
        vec!["specifications/core/a.md"],
        "only the spec index answers (SPS-FR-05)",
    );
}

