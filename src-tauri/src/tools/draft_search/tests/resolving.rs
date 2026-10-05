//! DST-FR-11, DST-FR-12, DST-FR-13, DST-FR-17: paths, currency, and dropped hits.

use super::*;

// ---------------------------------------------------------------------------
// DST-FR-11, DST-FR-12, DST-FR-13, DST-FR-17: paths, currency, and dropped hits
// ---------------------------------------------------------------------------

/// DST-FR-11: `prompt_path` is draft-relative and informational — it is neither
/// a `read_file` path to this prompt nor a draft id.
#[test]
fn prompt_path_is_informational_only() {
    let fixture = DraftFixture::new();
    fixture.draft("shape", "# Plan\n\nteardown\n");
    fixture.reindex();
    let m = fixture.search("teardown", None).remove(0);
    assert_eq!(m.prompt_path, "shape.md", "draft-relative, not project-relative");

    // Passed to `read_file`, it is resolved against the *project root* — so with
    // a real project file sitting at that same relative path, `read_file`
    // answers with that file and never with the draft's prompt. That is what
    // "the two paths name different things" means, and `is_err()` alone would
    // not have shown it.
    let root = fixture.root();
    std::fs::write(root.path().join("shape.md"), "# A project file\n\nnot the draft\n")
        .expect("write");
    let session_access = fixture
        .fixture
        .app
        .state::<crate::fs::FsAccessState>()
        .agent_session(&fixture.session)
        .expect("a session is open");
    let read = crate::tools::file_read::read(
        &session_access,
        root.path(),
        &crate::tools::file_read::ReadFileArgs {
            path: m.prompt_path.clone(),
            offset: None,
            limit: None,
        },
    )
    .expect("the project file reads");
    assert_eq!(read, "# A project file\n\nnot the draft\n");
    assert!(!read.contains("teardown"), "never the draft's prompt");

    // Passed as a draft id, it is the unknown-draft refusal.
    let as_id = crate::tools::draft_read::read(
        &root,
        &crate::tools::draft_read::ReadDraftArgs {
            draft_id: m.prompt_path.clone(),
        },
    );
    assert_eq!(as_id.unwrap_err(), ToolRefusal::DraftNotFound);
}

/// DST-FR-12: name and status come from the current record rather than from the
/// indexed hit, so a rename or an archive since the last pass is reported.
#[test]
fn name_and_status_are_current_not_indexed() {
    let fixture = DraftFixture::new();
    let id = fixture.draft("original name", "# Plan\n\nteardown\n");
    fixture.reindex();

    crate::drafts::rename_draft_impl(&fixture.root(), &id, "renamed since").expect("renames");
    fixture.archive(&id);
    // Deliberately *no* reindex: the hit still carries what the pass found.

    let m = fixture.search("teardown", None).remove(0);
    assert_eq!(m.name, "renamed since");
    assert_eq!(m.status, DraftStatus::Archived);
    assert_eq!(
        m.prompt_path, "renamed since.md",
        "the record's own pointer, which the rename moved (DRS-FR-25)"
    );
}

/// DST-FR-13, DST-FR-17: a hit whose draft has since been removed or made inconsistent is
/// dropped rather than returned as a row nothing could load.
#[test]
fn stale_and_inconsistent_hits_are_dropped() {
    let fixture = DraftFixture::new();
    let keep = fixture.draft("keeper", "# Plan\n\nteardown\n");
    let removed = fixture.draft("doomed", "# Plan\n\nteardown\n");
    let broken = fixture.draft("broken", "# Plan\n\nteardown\n");
    fixture.reindex();
    assert_eq!(fixture.search("teardown", None).len(), 3, "all three index");

    crate::drafts::delete_draft_impl(&fixture.root(), &fixture.root(), &removed).expect("deletes");
    make_inconsistent(&fixture, &broken);
    // Deliberately no reindex: the index still holds all three.

    let matches = fixture.search("teardown", None);
    assert_eq!(matches.len(), 1, "the two unreadable rows are dropped");
    assert_eq!(matches[0].draft_id, keep);

    // DST-FR-13: every row returned is one `read_draft` can load.
    let root = fixture.root();
    assert!(crate::tools::draft_read::read(
        &root,
        &crate::tools::draft_read::ReadDraftArgs {
            draft_id: matches[0].draft_id.clone()
        }
    )
    .is_ok());
    // And the inconsistent one is exactly the condition DRS-FR-15 reports.
    assert_eq!(
        crate::drafts::draft_record(&root, &broken).unwrap_err(),
        ERR_NOT_SINGLE_FILE
    );
}

/// DST-FR-14: the excerpt is the chunk's own text, unmodified.
#[test]
fn excerpt_is_the_chunk_verbatim() {
    let fixture = DraftFixture::new();
    fixture.draft("verbatim", "# Intent\n\nThe teardown sequence, written once.\n");
    fixture.reindex();

    let m = fixture.search("teardown", None).remove(0);
    // Byte-for-byte against the chunk as the index holds it, which is the only
    // form of this assertion that fails if the text is re-wrapped, trimmed, or
    // given a prefix.
    let indexer = fixture.fixture.app.state::<crate::bm25_index::Bm25Indexer>();
    let raw = crate::bm25_index::search(
        &indexer,
        &[crate::bm25_index::IndexId::Drafts],
        "teardown",
        10,
    );
    let hit = raw
        .iter()
        .find(|h| h.draft_id.as_deref() == Some(m.draft_id.as_str()))
        .expect("the index holds a chunk for this draft");
    assert_eq!(m.excerpt, hit.text, "the chunk verbatim");
    assert_eq!(m.score, hit.score);
    assert!(m.excerpt.starts_with("# Intent"), "the chunk from its heading");
}
