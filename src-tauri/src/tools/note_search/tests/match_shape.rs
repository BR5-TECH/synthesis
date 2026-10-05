//! NST-FR-10, NST-FR-11, NST-FR-12: what a match carries.

use super::*;

// ---------------------------------------------------------------------------
// NST-FR-10, NST-FR-11, NST-FR-12: what a match carries
// ---------------------------------------------------------------------------

/// NST-FR-10: exactly four fields, and no path under `.synthesis/` anywhere.
#[test]
fn a_match_carries_exactly_id_scope_body_and_score() {
    let fixture = NoteFixture::new();
    fixture.note("the teardown of the overlay");
    fixture.reindex();

    let output = block_on(fixture.tool().call(NoteSearchArgs {
        query: "teardown".into(),
        limit: None,
    }))
    .expect("the call succeeds");
    let value = serde_json::to_value(&output).expect("serializes");
    let entry = &value["notes"][0];
    // The object's keys, sorted — `serde_json`'s map orders them itself, so the
    // claim is which four are present rather than what order they took.
    let mut keys: Vec<&str> = entry
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort();
    assert_eq!(keys, vec!["body", "id", "scope", "score"]);
    for absent in ["index", "path", "node_id", "nodeId", "chunk_ordinal", "chunkOrdinal"] {
        assert!(
            entry.get(absent).is_none(),
            "a match must not carry {absent:?}",
        );
    }
    let rendered = serde_json::to_string(&value).unwrap();
    assert!(
        !rendered.contains(".synthesis/"),
        "no path under `.synthesis/` appears in a result: {rendered}",
    );
    assert!(!rendered.contains(".toml"));
}

/// NST-FR-11: `body` is the stored body byte-for-byte and whole.
#[test]
fn the_body_is_the_whole_stored_body_byte_for_byte() {
    let fixture = NoteFixture::new();
    let body = "The overlay does not close on Escape.\n\nIt also leaves the teardown \
                half done, which is what I keep tripping over.\n\nWorth a look before the \
                next release.";
    let id = fixture.note(body);
    fixture.reindex();

    let matches = fixture.search("teardown", None);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].body, body, "byte for byte, and whole");
    assert_eq!(
        matches[0].body,
        crate::notes::note_record(&fixture.root(), &id)
            .expect("the note resolves")
            .body,
        "and it is what notes storage holds",
    );
}

/// NST-FR-11 at the bound: a note of exactly 1 KiB, and one of multibyte
/// characters, each come back whole.
///
/// The tool returns whole notes because a note is small, which is the property
/// notes storage guarantees rather than one this tool checks — so the interesting
/// case is the largest note the store will ever accept.
#[test]
fn a_note_at_the_storage_bound_comes_back_whole() {
    let fixture = NoteFixture::new();
    // Exactly 1024 bytes, ending in the query term so the note is findable.
    let tail = " teardown";
    let body = format!("{}{tail}", "a".repeat(1024 - tail.len()));
    assert_eq!(body.len(), crate::notes::MAX_NOTE_BODY_BYTES);
    fixture.note(&body);

    // And one whose characters are multibyte, so a byte-slicing bug would show.
    let emoji = format!("{} teardown", "\u{1F600}".repeat(20));
    fixture.note(&emoji);
    fixture.reindex();

    let matches = fixture.search("teardown", Some(20));
    let bodies: std::collections::BTreeSet<&str> =
        matches.iter().map(|m| m.body.as_str()).collect();
    assert!(bodies.contains(body.as_str()), "the 1 KiB note, whole");
    assert!(bodies.contains(emoji.as_str()), "the multibyte note, whole");
}

/// BMI-FR-05 / NST-FR-16: a note whose body is empty or nothing but whitespace
/// contributes no document, so it is findable by no query.
///
/// Notes storage allows one — the panel renders such a row as an empty note —
/// and an empty document has no terms to match and no meaningful length, which
/// is why the pass records the note and indexes nothing for it.
#[test]
fn an_empty_note_contributes_no_document_and_is_found_by_nothing() {
    let fixture = NoteFixture::new();
    fixture.note("");
    fixture.note("   \n\t  ");
    let real = fixture.note("a note with teardown in it");
    fixture.reindex();

    assert_eq!(ids(&fixture.search("teardown", Some(20))), vec![real]);
    // A blank query refuses before it ever reaches the index (NST-FR-14), so an
    // empty note is unreachable from both directions.
    for query in ["", " "] {
        assert_eq!(
            fixture.refusal(query, None).kind(),
            ToolErrorKind::InvalidArgs,
        );
    }
}

/// NST-FR-12: `scope` is the note's current record, resolved rather than
/// carried on the hit.
#[test]
fn scope_and_body_are_the_current_record_rather_than_the_indexed_hit() {
    let fixture = NoteFixture::new();
    fixture.write("specifications/a.md", "# A\n\nnothing\n");
    let attached = fixture.scoped_note(entity("specifications/a.md"), "teardown attached", None, None);
    let project = fixture.note("teardown project wide");
    fixture.reindex();

    let matches = fixture.search("teardown", Some(20));
    let attached_match = matches.iter().find(|m| m.id == attached).expect("found");
    assert_eq!(
        attached_match.scope,
        entity("specifications/a.md"),
        "an entity scope carries the entity id and its last-known path",
    );
    let project_match = matches.iter().find(|m| m.id == project).expect("found");
    assert_eq!(project_match.scope, NoteScope::Project);
    assert_eq!(project_match.scope.entity_id(), None);

    // NST-FR-12: "in the shape notes storage defines" is a claim about what the
    // **model** reads, so it is asserted on the serialized form rather than on
    // Rust equality — a `serde` rename here would change the contract without
    // touching a single `PartialEq`.
    let value = serde_json::to_value(NoteSearchOutput {
        notes: matches.clone(),
    })
    .expect("serializes");
    let rendered = value["notes"].as_array().expect("an array").clone();
    let scope_of = |id: &str| {
        rendered
            .iter()
            .find(|m| m["id"] == id)
            .expect("the match")["scope"]
            .clone()
    };
    assert_eq!(
        scope_of(&attached),
        serde_json::json!({
            "kind": "entity",
            "entityId": "specifications/a.md",
            "entityPath": "specifications/a.md",
        }),
    );
    assert_eq!(scope_of(&project), serde_json::json!({ "kind": "project" }));

    // Moved to project scope and its body edited, with no pass in between.
    crate::notes::update_note_in(
        &fixture.root(),
        &attached,
        crate::notes::NoteFields {
            scope: Some(NoteScope::Project),
            body: Some("teardown moved and rewritten".to_string()),
            ..Default::default()
        },
        "2026-02-02T00:00:00Z",
    )
    .expect("the move lands");

    let after = fixture.search("teardown", Some(20));
    let moved = after.iter().find(|m| m.id == attached).expect("still found");
    assert_eq!(moved.scope, NoteScope::Project, "the new scope");
    assert_eq!(moved.body, "teardown moved and rewritten", "the new body");

    // And the index still holds the old body, so neither came from the hit.
    let indexer = fixture.fixture.app.state::<crate::bm25_index::Bm25Indexer>();
    let hits = crate::bm25_index::search(&indexer, &[IndexId::Notes], "attached", 5);
    assert_eq!(hits.len(), 1, "the pass has not run yet");
    assert_eq!(hits[0].text, "teardown attached");
}
