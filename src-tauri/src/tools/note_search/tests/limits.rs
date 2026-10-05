//! NST-FR-06, NST-FR-07, NST-FR-08, NST-FR-09: the limit, uniqueness, and ordering.

use super::*;

// ---------------------------------------------------------------------------
// NST-FR-06, NST-FR-07, NST-FR-08, NST-FR-09: the limit, uniqueness, and ordering
// ---------------------------------------------------------------------------

/// NST-FR-06: the default and the clamp.
#[test]
fn the_limit_defaults_to_five_and_clamps_into_one_through_twenty() {
    let fixture = NoteFixture::new();
    for i in 0..30 {
        fixture.note(&format!("note {i} about the teardown"));
    }
    fixture.reindex();

    assert_eq!(fixture.search("teardown", None).len(), 5, "the default");
    assert_eq!(fixture.search("teardown", Some(0)).len(), 1);
    assert_eq!(fixture.search("teardown", Some(-5)).len(), 1);
    assert_eq!(fixture.search("teardown", Some(1000)).len(), 20);
    assert_eq!(normalize_limit(None), 5);
    assert_eq!(normalize_limit(Some(0)), 1);
    assert_eq!(normalize_limit(Some(-5)), 1);
    assert_eq!(normalize_limit(Some(1000)), 20);
}

/// NST-FR-07, NST-FR-08: distinct notes, one underlying call, and fewer than `limit` when
/// fewer matched.
#[test]
fn a_result_names_each_note_once_and_never_widens_its_one_call() {
    let fixture = NoteFixture::new();
    // Deliberately different densities rather than ten interchangeable notes:
    // ten equally-scoring documents are a tie, and which five of them the BM25
    // engine hands back is its own business, so an assertion about *which* five
    // came out would be an assertion about the engine.
    for i in 0..10 {
        fixture.note(&format!(
            "{} note {i} about the panel",
            "teardown ".repeat(i + 1),
        ));
    }
    fixture.reindex();

    let matches = fixture.search("teardown", Some(5));
    assert_eq!(matches.len(), 5);
    let unique: std::collections::BTreeSet<&String> = matches.iter().map(|m| &m.id).collect();
    assert_eq!(unique.len(), 5, "no id appears twice");

    // NST-FR-08: one call asking for the applied limit and nothing more. Ten
    // notes match and five were asked for, so a tool that overfetched — as
    // `search_drafts` legitimately does — or that repeated the call to fill the
    // result would return a different five from the index's own top five.
    let indexer = fixture.fixture.app.state::<crate::bm25_index::Bm25Indexer>();
    let top_five: Vec<Option<String>> =
        crate::bm25_index::search(&indexer, &[IndexId::Notes], "teardown", 5)
            .into_iter()
            .map(|h| h.note_id)
            .collect();
    drop(indexer);
    assert!(
        matches.windows(2).all(|p| p[0].score > p[1].score),
        "precondition: the fixture's scores strictly differ, so the top five are \
         unambiguous rather than an arbitrary five of a tie: {:?}",
        matches.iter().map(|m| m.score).collect::<Vec<_>>(),
    );
    assert_eq!(
        matches.iter().map(|m| Some(m.id.clone())).collect::<Vec<_>>(),
        top_five,
        "exactly the index's own top five, so the call was neither widened nor repeated",
    );

    // NST-FR-08: one call for the applied limit and nothing more, so a repeated
    // id in the hit stream is dropped rather than making the tool ask again.
    let duplicated = vec![
        hit(&matches[0].id, 3.0),
        hit(&matches[0].id, 2.0),
        hit(&matches[1].id, 1.0),
    ];
    let resolved = resolve_matches(duplicated, &fixture.root(), 5);
    assert_eq!(
        resolved.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
        vec![matches[0].id.clone(), matches[1].id.clone()],
        "a repeated id is dropped rather than returned twice",
    );

    // Two matching notes and a limit of five is a success carrying two.
    let small = NoteFixture::new();
    small.note("one teardown");
    small.note("two teardown");
    small.reindex();
    assert_eq!(small.search("teardown", Some(5)).len(), 2);
}
/// NST-FR-09: descending score, best first.
#[test]
fn matches_are_ordered_by_descending_score() {
    let fixture = NoteFixture::new();
    let apt = fixture.note("teardown teardown teardown teardown of the teardown");
    fixture.note("teardown mentioned once among many other unrelated words here");
    fixture.note("a passing mention of teardown in a long note about other matters entirely");
    fixture.reindex();

    let matches = fixture.search("teardown", Some(5));
    assert!(matches.len() >= 2);
    for pair in matches.windows(2) {
        assert!(
            pair[0].score >= pair[1].score,
            "scores descend: {:?}",
            matches.iter().map(|m| m.score).collect::<Vec<_>>(),
        );
    }
    assert_eq!(matches[0].id, apt, "the first is the most apt");
}
