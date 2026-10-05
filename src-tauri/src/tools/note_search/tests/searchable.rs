//! NST-FR-03, NST-FR-04, NST-FR-05: what is searchable.

use super::*;

// ---------------------------------------------------------------------------
// NST-FR-03, NST-FR-04, NST-FR-05: what is searchable
// ---------------------------------------------------------------------------

/// NST-FR-04: every note the store persists is searchable on identical terms.
#[test]
fn every_persisted_note_is_searchable_whatever_it_is_filed_against() {
    let fixture = NoteFixture::new();
    fixture.write("specifications/live.md", "# Live\n\nnothing here\n");
    let project = fixture.note("the teardown is never reported");
    let attached = fixture.scoped_note(entity("specifications/live.md"), "teardown of the panel", None, None);
    let orphaned = fixture.scoped_note(entity("specifications/gone.md"), "teardown never runs", None, None);
    let revised = fixture.scoped_note(NoteScope::Project, "teardown against an old revision", None, Some("abc1234"));
    fixture.reindex();

    let mut expected = vec![project, attached, orphaned, revised];
    expected.sort();
    assert_eq!(ids(&fixture.search("teardown", Some(20))), expected);
}

/// NST-FR-04: a note's scope, its entity path, its reminder, its revision, and
/// its timestamps are in no index, so a query never matches on them.
#[test]
fn where_a_note_was_filed_and_when_are_matchable_nowhere() {
    let fixture = NoteFixture::new();
    fixture.write("specifications/sarcophagus.md", "# S\n\nnothing\n");
    fixture.scoped_note(
        entity("specifications/sarcophagus.md"),
        "a body naming nothing else",
        Some("2027-03-04T05:06:07Z"),
        Some("deadbeef"),
    );
    fixture.reindex();

    for query in ["sarcophagus", "2027", "deadbeef", "2026"] {
        assert!(
            fixture.search(query, Some(20)).is_empty(),
            "{query:?} matches on nothing a note was filed under",
        );
    }
    assert_eq!(fixture.search("body", Some(20)).len(), 1);
}

/// NST-FR-05: ranking reaches the `notes` index alone.
#[test]
fn no_other_index_is_reachable_through_this_tool() {
    let fixture = NoteFixture::new();
    fixture.write(
        "specifications/core/thing.md",
        "# Thing\n\nThe sarcophagus is discussed at length here.\n",
    );
    fixture.write(
        "flows/thing.flow",
        "# Flow\n\nThe sarcophagus step runs first.\n",
    );
    crate::tools::tests::write_skill(
        &fixture.root().to_path_buf(),
        ".claude/skills",
        "digger",
        "name: digger\ndescription: opens a sarcophagus",
        "The sarcophagus opening procedure.",
    );
    let draft = crate::drafts::create_draft_at_root(&fixture.root(), Some("sarcophagus"))
        .expect("the scaffold completes");
    let prompt = draft
        .draft
        .prompt_path
        .clone()
        .expect("a created draft holds its prompt");
    crate::drafts::save_draft_file_impl(
        &fixture.root(),
        &draft.draft.id,
        &prompt,
        "# Plan\n\nOpen the sarcophagus and report.\n",
    )
    .expect("the prompt writes");

    let note = fixture.note("the sarcophagus is still sealed");
    fixture.reindex();

    let matches = fixture.search("sarcophagus", Some(20));
    assert_eq!(ids(&matches), vec![note], "the note alone is returned");
}

/// NST-FR-03: the result is drawn from exactly what the `notes` index returned,
/// and this tool reads no note file and enumerates no folder of its own to get
/// it.
#[test]
fn the_result_is_exactly_what_the_notes_index_produced() {
    let fixture = NoteFixture::new();
    let a = fixture.note("alpha the teardown");
    let b = fixture.note("bravo the teardown twice teardown");
    fixture.reindex();

    let indexer = fixture.fixture.app.state::<crate::bm25_index::Bm25Indexer>();
    // NST-FR-08: the applied limit and nothing more. Asking the index for
    // exactly what the tool was asked for is what makes the equality below a
    // claim about the tool's one call rather than about a coincidence.
    let hits = crate::bm25_index::search(&indexer, &[IndexId::Notes], "teardown", 5);
    let from_index: Vec<(Option<String>, f32)> =
        hits.iter().map(|h| (h.note_id.clone(), h.score)).collect();
    drop(indexer);

    let matches = fixture.search("teardown", Some(5));
    // NST-FR-09, NST-FR-10: the `score` a match carries **is** the delegated
    // hit's, so this pins the value and not only the ordering — an ordering
    // assertion alone is satisfied by every score being the same number.
    assert_eq!(
        matches
            .iter()
            .map(|m| (Some(m.id.clone()), m.score))
            .collect::<Vec<_>>(),
        from_index,
        "same notes, same order, same scores, and nothing this tool added",
    );
    assert!(
        matches.iter().any(|m| m.score != 0.0),
        "precondition: the scores are real numbers rather than a constant",
    );
    assert!(
        matches[0].score > matches[matches.len() - 1].score
            || matches.len() == 1,
        "precondition: the scores actually differ, so equality above means something",
    );
    assert_eq!(ids(&matches), {
        let mut e = vec![a, b];
        e.sort();
        e
    });

    // The tool holds no index and applies no ranking: its whole source of order
    // is the delegated call, which the equality above already pins. What is
    // asserted here is that its source names no walk of its own.
    const SOURCE: &str = include_str!("../../note_search.rs");
    let code: String = SOURCE
        .lines()
        .filter(|line| !line.trim_start().starts_with("//") && !line.trim_start().starts_with("///"))
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in ["list_dir", "read_dir", "read_text", "read_toml", "list_all_notes"] {
        assert!(
            !code.contains(forbidden),
            "NST-FR-03: this tool must not reach for {forbidden:?}",
        );
    }
}
