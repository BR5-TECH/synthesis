//! DST-FR-11, DST-FR-12, DST-FR-04, DST-FR-05: what is searched.

use super::*;

// ---------------------------------------------------------------------------
// DST-FR-11, DST-FR-12, DST-FR-04, DST-FR-05: what is searched
// ---------------------------------------------------------------------------

/// DST-FR-11, DST-FR-12: every status draft storage retains is searchable on identical
/// terms, and each result's `draft_id` loads through `read_draft`.
#[test]
fn every_retained_status_is_searchable_and_readable() {
    let fixture = DraftFixture::new();
    let active = fixture.draft("active work", "# Plan\n\nThe teardown sequence.\n");
    let archived = fixture.draft("shelved work", "# Plan\n\nThe teardown ordering.\n");
    fixture.archive(&archived);
    fixture.reindex();

    let matches = fixture.search("teardown", None);
    let by_id: std::collections::HashMap<_, _> =
        matches.iter().map(|m| (m.draft_id.clone(), m)).collect();
    assert_eq!(matches.len(), 2, "both statuses are searchable");
    assert_eq!(by_id[&active].status, DraftStatus::Active);
    assert_eq!(by_id[&archived].status, DraftStatus::Archived);

    // DST-FR-11: the id is what carries forward, unchanged.
    let root = fixture.root();
    for m in &matches {
        let loaded = crate::tools::draft_read::read(
            &root,
            &crate::tools::draft_read::ReadDraftArgs {
                draft_id: m.draft_id.clone(),
            },
        )
        .expect("a returned draft loads");
        assert_eq!(loaded.draft_id, m.draft_id);
        assert!(loaded.content.contains("teardown"));
    }
}

/// DST-FR-04: a term that lives only in an accepted history entry, a proposal
/// candidate, a comment log, or a conversation log reaches nothing.
///
/// The exclusion is the drafts index's (BMI-FR-04) rather than a filter here,
/// which is why this asserts on the tool's answer rather than on a code path.
#[test]
fn history_and_sibling_storage_are_never_searched() {
    let fixture = DraftFixture::new();
    let id = fixture.draft("live work", "# Plan\n\nThe live prompt says nothing unusual.\n");
    let dir = crate::drafts::draft_dir(&fixture.root(), &id).expect("the draft resolves");

    // Everything a draft's folder holds beside its prompt, each carrying a term
    // the live prompt does not.
    std::fs::write(dir.join("history").join("e1.snapshot"), "sarcophagus in a snapshot\n")
        .expect("write");
    std::fs::write(dir.join("history").join("e1.toml"), "id = \"e1\"\n").expect("write");
    std::fs::write(dir.join("proposals").join("p1.md"), "sarcophagus in a candidate\n")
        .expect("write");
    std::fs::write(dir.join("proposals").join("p.content"), "sarcophagus in a proposal\n")
        .expect("write");
    std::fs::write(dir.join("conversation.jsonl"), "sarcophagus in a conversation\n")
        .expect("write");
    fixture.reindex();

    assert!(
        fixture.search("sarcophagus", None).is_empty(),
        "no snapshot, candidate, comment, or conversation is searchable"
    );
    assert_eq!(
        fixture.search("unusual", None).len(),
        1,
        "the live prompt is still searchable"
    );
}

/// DST-FR-05: the `drafts` index alone. A specification and a skill carrying the
/// same term are unreachable through this tool.
#[test]
fn reaches_no_index_but_drafts() {
    let fixture = DraftFixture::new();
    let root_path = fixture.root().path().to_path_buf();
    std::fs::create_dir_all(root_path.join("specifications")).expect("mkdir");
    std::fs::write(
        root_path.join("specifications").join("s.md"),
        "# Spec\n\nsarcophagus in a specification\n",
    )
    .expect("write");
    crate::tools::tests::write_skill(
        &root_path,
        ".claude/skills",
        "digger",
        "name: digger\ndescription: sarcophagus in a skill",
        "sarcophagus in a skill body",
    );
    std::fs::create_dir_all(root_path.join("flows")).expect("mkdir");
    std::fs::write(
        root_path.join("flows").join("dig.flow"),
        r#"{"nodes": [{"id": "1", "label": "sarcophagus in a flow"}]}"#,
    )
    .expect("write");
    // DST-FR-05 names the `notes` index among the ten this tool cannot reach.
    crate::notes::create_note_in(
        &fixture.root(),
        crate::notes::NoteScope::Project,
        "sarcophagus in a note".to_string(),
        None,
        None,
        "2026-01-01T00:00:00Z",
    )
    .expect("the note writes");
    let id = fixture.draft("dig", "# Plan\n\nsarcophagus in a draft\n");
    fixture.reindex();

    let matches = fixture.search("sarcophagus", None);
    assert_eq!(matches.len(), 1, "the draft alone");
    assert_eq!(matches[0].draft_id, id);
}
