//! NST-FR-13, NST-FR-14, NST-FR-15, NST-FR-16: dropping, refusing, and emptiness.

use super::*;

// ---------------------------------------------------------------------------
// NST-FR-13, NST-FR-16, NST-FR-14, NST-FR-15, NST-FR-16: dropping, refusing, and emptiness
// ---------------------------------------------------------------------------

/// NST-FR-13, NST-FR-16: a hit naming a note that no longer resolves is omitted, and the
/// call still succeeds.
#[test]
fn a_hit_whose_note_no_longer_resolves_is_omitted_silently() {
    let fixture = NoteFixture::new();
    let kept = fixture.note("alpha teardown");
    let deleted = fixture.note("bravo teardown");
    let damaged = fixture.note("charlie teardown");
    fixture.reindex();
    assert_eq!(fixture.search("teardown", Some(20)).len(), 3);

    crate::notes::delete_note_in(&fixture.root(), &fixture.root(), &deleted).expect("the deletion lands");
    // Made unreadable in place, so the index still holds its document.
    std::fs::write(
        fixture.root().join(format!(".synthesis/notes/{damaged}.toml")),
        "this is not = = toml",
    )
    .unwrap();

    let matches = fixture.search("teardown", Some(20));
    assert_eq!(ids(&matches), vec![kept], "exactly one match survives");
    // A success, not a refusal.
    assert!(block_on(fixture.tool().call(NoteSearchArgs {
        query: "teardown".into(),
        limit: Some(20),
    }))
    .is_ok());
}

/// NST-FR-13, NST-FR-16: when **every** matching hit is dropped, the call is an
/// empty success rather than a refusal — NST-FR-16's third case, which the test
/// above cannot reach because it always leaves a survivor.
#[test]
fn every_matching_hit_dropped_is_an_empty_success_rather_than_a_refusal() {
    let fixture = NoteFixture::new();
    let ids: Vec<String> = (0..3)
        .map(|i| fixture.note(&format!("note {i} about the teardown")))
        .collect();
    fixture.reindex();
    assert_eq!(fixture.search("teardown", Some(20)).len(), 3);

    // All three gone from under the index: one deleted, two made unreadable.
    crate::notes::delete_note_in(&fixture.root(), &fixture.root(), &ids[0]).expect("the deletion lands");
    for id in &ids[1..] {
        std::fs::write(
            fixture.root().join(format!(".synthesis/notes/{id}.toml")),
            "this is not = = toml",
        )
        .unwrap();
    }

    let output = block_on(fixture.tool().call(NoteSearchArgs {
        query: "teardown".into(),
        limit: Some(20),
    }))
    .expect("an answer rather than a failure");
    assert!(
        output.notes.is_empty(),
        "every row was dropped, and that is an empty list rather than a refusal",
    );
}

/// NST-FR-13: a hit carrying no `note_id` names no note to resolve and is
/// dropped rather than panicking or being returned half-formed.
#[test]
fn a_hit_naming_no_note_is_dropped() {
    let fixture = NoteFixture::new();
    let id = fixture.note("a note that does resolve");
    fixture.reindex();

    let mut stray = hit(&id, 1.0);
    stray.note_id = None;
    let resolved = resolve_matches(vec![stray, hit(&id, 0.5)], &fixture.root(), 5);
    assert_eq!(
        resolved.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
        vec![id],
        "the hit naming no note contributes nothing",
    );
}

/// NST-FR-14: an empty or blank query refuses rather than answering empty.
#[test]
fn an_empty_or_blank_query_refuses_with_invalid_args() {
    let fixture = NoteFixture::new();
    fixture.note("something about the teardown");
    fixture.reindex();

    for query in ["", "   ", "\n\t "] {
        let error = fixture.refusal(query, None);
        assert_eq!(error.kind(), ToolErrorKind::InvalidArgs);
        assert_eq!(error.retryable(), Some(true));
        assert_eq!(error.message(), crate::tools::EMPTY_NOTE_QUERY);
        assert!(error.message().contains("The query must describe the topic"));
    }
}

/// NST-FR-15: with no project open, the shared refusal rather than an empty
/// list.
#[test]
fn no_project_open_is_the_shared_refusal() {
    let app = crate::tools::tests::closed_project();
    let tool = NoteSearchTool::new(app.handle().clone(), "closed");
    let error = block_on(tool.call(NoteSearchArgs {
        query: "teardown".into(),
        limit: Some(3),
    }))
    .expect_err("no project is open");
    let rendered = tool.map_error(error);
    assert_eq!(rendered.kind(), ToolErrorKind::NotFound);
    assert_eq!(rendered.retryable(), Some(false));
    assert_eq!(rendered.message(), crate::tools::NO_PROJECT_OPEN);
}

/// NST-FR-16: nothing matching, and no note at all, are both successes.
#[test]
fn nothing_matching_and_no_note_at_all_are_both_empty_successes() {
    let fixture = NoteFixture::new();
    fixture.note("a note about something else entirely");
    fixture.reindex();
    assert!(fixture.search("sarcophagus", None).is_empty());

    let bare = NoteFixture::new();
    bare.reindex();
    let output = block_on(bare.tool().call(NoteSearchArgs {
        query: "anything".into(),
        limit: None,
    }))
    .expect("a worktree with no note is an answer rather than a failure");
    assert!(output.notes.is_empty());
}
