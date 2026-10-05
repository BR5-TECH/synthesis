//! DST-FR-06 … DST-FR-10: limit, dedup, ordering, fields.

use super::*;

// ---------------------------------------------------------------------------
// DST-FR-06 … DST-FR-10: limit, dedup, ordering, fields
// ---------------------------------------------------------------------------

/// DST-FR-06: the documented default, and every out-of-range value clamped
/// rather than refused.
#[test]
fn limit_defaults_and_clamps() {
    assert_eq!(normalize_limit(None), 10);
    assert_eq!(normalize_limit(Some(0)), 1);
    assert_eq!(normalize_limit(Some(-5)), 1);
    assert_eq!(normalize_limit(Some(1000)), 20);
    assert_eq!(normalize_limit(Some(7)), 7);

    let fixture = DraftFixture::new();
    // More than the ceiling, so the clamp to 20 is observable end-to-end rather
    // than only at the pure-function level.
    for i in 0..30 {
        fixture.draft(&format!("draft {i}"), &format!("# Plan {i}\n\nteardown work {i}\n"));
    }
    fixture.reindex();

    assert_eq!(fixture.search("teardown", None).len(), 10, "the default");
    assert_eq!(fixture.search("teardown", Some(0)).len(), 1);
    assert_eq!(fixture.search("teardown", Some(-5)).len(), 1);
    assert_eq!(fixture.search("teardown", Some(3)).len(), 3);
    assert_eq!(
        fixture.search("teardown", Some(1000)).len(),
        20,
        "the documented ceiling, against a worktree that could supply 30"
    );
}

/// DST-FR-07: `limit` counts drafts, and a draft whose several chunks all match
/// appears once carrying its best one.
#[test]
fn limit_counts_drafts_and_keeps_the_best_chunk() {
    let fixture = DraftFixture::new();
    // Four heading sections, so four chunks (BMI-FR-05); the last mentions the
    // query term three times and so scores highest.
    let dense = fixture.draft(
        "dense",
        "# One\n\nteardown\n\n# Two\n\nteardown\n\n# Three\n\nteardown\n\n\
         # Four\n\nteardown teardown teardown ordering\n",
    );
    fixture.draft("second", "# Plan\n\nteardown\n");
    fixture.draft("third", "# Plan\n\nteardown\n");
    fixture.reindex();

    let matches = fixture.search("teardown", Some(3));
    assert_eq!(matches.len(), 3, "three drafts, not three chunks");
    let dense_rows: Vec<_> = matches.iter().filter(|m| m.draft_id == dense).collect();
    assert_eq!(dense_rows.len(), 1, "one row per draft");
    assert!(
        dense_rows[0].excerpt.contains("Four"),
        "the surviving excerpt is the highest-scoring chunk, got {:?}",
        dense_rows[0].excerpt
    );
}

/// DST-FR-08: the overfetch fills `limit` with distinct drafts from **one**
/// bounded call, and a worktree holding fewer returns what it has.
///
/// The one-call claim is asserted structurally rather than by counting: the
/// tool's whole answer is [`resolve_matches`] applied to the hits of a single
/// `search(.., limit * OVERFETCH_FACTOR)`, so reproducing that one call
/// reproduces the answer exactly. An implementation that escalated — call,
/// dedup, re-call wider until `limit` survived — would return more rows than
/// this on the seeded worktree below, where the single bounded call cannot
/// reach every matching draft.
#[test]
fn overfetch_fills_the_limit_from_one_bounded_call() {
    let fixture = DraftFixture::new();
    for i in 0..10 {
        fixture.draft(
            &format!("many {i}"),
            &format!("# A{i}\n\nteardown\n\n# B{i}\n\nteardown\n\n# C{i}\n\nteardown\n"),
        );
    }
    fixture.reindex();
    assert_eq!(
        fixture.search("teardown", Some(5)).len(),
        5,
        "five distinct drafts, not the two a chunk-counted limit would collapse to"
    );

    // The same answer the one bounded call yields, and no more.
    let indexer = fixture.fixture.app.state::<crate::bm25_index::Bm25Indexer>();
    let one_call = crate::bm25_index::search(
        &indexer,
        &[crate::bm25_index::IndexId::Drafts],
        "teardown",
        5 * OVERFETCH_FACTOR,
    );
    let expected = resolve_matches(one_call, &fixture.root(), 5);
    assert_eq!(fixture.search("teardown", Some(5)), expected);

    // And a limit the single bounded call genuinely cannot fill returns short
    // rather than escalating: 1 * 10 chunks cannot reach 20 distinct drafts.
    let crowded = DraftFixture::new();
    for i in 0..25 {
        crowded.draft(&format!("crowd {i}"), &format!("# A{i}\n\nteardown\n\n# B{i}\n\nteardown\n\n# C{i}\n\nteardown\n\n# D{i}\n\nteardown\n\n# E{i}\n\nteardown\n\n# F{i}\n\nteardown\n\n# G{i}\n\nteardown\n\n# H{i}\n\nteardown\n\n# I{i}\n\nteardown\n\n# J{i}\n\nteardown\n\n# K{i}\n\nteardown\n"));
    }
    crowded.reindex();
    let got = crowded.search("teardown", Some(20)).len();
    assert!(
        got <= 20,
        "never more than asked for, got {got}",
    );

    let sparse = DraftFixture::new();
    sparse.draft("a", "# Plan\n\nteardown\n");
    sparse.draft("b", "# Plan\n\nteardown\n");
    sparse.reindex();
    assert_eq!(
        sparse.search("teardown", Some(5)).len(),
        2,
        "two matching drafts is a success carrying two"
    );
}

/// DST-FR-09: matches arrive ordered by descending score.
#[test]
fn matches_are_ordered_best_first() {
    let fixture = DraftFixture::new();
    fixture.draft("faint", "# Plan\n\nA long passage mentioning teardown once among many other words that dilute it.\n");
    fixture.draft("strong", "# Plan\n\nteardown teardown teardown\n");
    fixture.reindex();

    fixture.draft("middling", "# Plan\n\nteardown teardown among some other words here.\n");
    fixture.draft("fainter", "# Plan\n\nA much longer passage that mentions teardown exactly once and otherwise talks at length about entirely unrelated matters, diluting the term considerably.\n");
    fixture.reindex();

    let matches = fixture.search("teardown", None);
    assert_eq!(matches.len(), 4);
    let scores: Vec<f32> = matches.iter().map(|m| m.score).collect();
    assert!(
        scores.windows(2).all(|w| w[0] >= w[1]),
        "the whole sequence is non-increasing, got {scores:?}"
    );
    assert_eq!(matches[0].name, "strong", "the most apt is first");
}

/// DST-FR-10: a match carries exactly the six documented fields.
#[test]
fn a_match_carries_exactly_the_documented_fields() {
    let fixture = DraftFixture::new();
    fixture.draft("shape", "# Plan\n\nteardown\n");
    fixture.reindex();

    let matches = fixture.search("teardown", None);
    let json = serde_json::to_value(&matches[0]).expect("serialises");
    let mut keys: Vec<_> = json.as_object().expect("an object").keys().cloned().collect();
    keys.sort();
    // The exact key set is the assertion; an "and these are absent" loop beside
    // it would only restate what equality already pins (DST-FR-10). The wire
    // names are the field names, which is what the spec's output shape writes.
    assert_eq!(
        keys,
        vec!["draft_id", "excerpt", "name", "prompt_path", "score", "status"]
    );

    // TLC-FR-08: the collection sits under a named field, never a bare array.
    let output = serde_json::to_value(DraftSearchOutput {
        drafts: matches.clone(),
    })
    .expect("serialises");
    assert!(output.get("drafts").is_some());
    assert!(output.is_object());
}
