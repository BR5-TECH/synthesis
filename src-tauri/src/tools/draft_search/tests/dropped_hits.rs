//! `resolve_matches` directly (DST-FR-07, DST-FR-08, DST-FR-13).

use super::*;

// ---------------------------------------------------------------------------
// `resolve_matches` directly (DST-FR-07, DST-FR-08, DST-FR-13)
// ---------------------------------------------------------------------------

/// A hit as the drafts index produces one.
fn hit(draft_id: Option<&str>, text: &str, score: f32) -> crate::bm25_index::ChunkHit {
    crate::bm25_index::ChunkHit {
        index: crate::bm25_index::IndexId::Drafts,
        path: "prompt.md".to_string(),
        node_id: None,
        draft_id: draft_id.map(str::to_string),
        note_id: None,
        document_id: None,
        chunk_ordinal: 0,
        score,
        text: text.to_string(),
    }
}

/// DST-FR-13: a hit carrying no `draft_id` names no draft to resolve and is
/// skipped.
///
/// Unreachable through the tool — every hit of the drafts index carries one
/// (BMI-FR-04) — so it is asserted here, where the branch can actually be
/// reached.
#[test]
fn a_hit_without_a_draft_id_is_skipped() {
    let fixture = DraftFixture::new();
    let id = fixture.draft("real", "# Plan\n\nbody\n");

    let matches = resolve_matches(
        vec![hit(None, "orphan chunk", 9.0), hit(Some(&id), "real chunk", 1.0)],
        &fixture.root(),
        10,
    );
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].draft_id, id);
    assert_eq!(matches[0].excerpt, "real chunk");
}

/// DST-FR-07 + DST-FR-13 together: a draft is claimed by its **best** chunk
/// before resolution, so a draft dropped on its top chunk cannot be re-admitted
/// by a lower one.
///
/// This is the assertion that fails if `seen.insert` and `draft_record` are ever
/// swapped — the end-to-end tests all use one chunk per draft and would not
/// notice. Re-admission would hand the model a row `read_draft` cannot load,
/// which is exactly what DST-FR-13 exists to prevent.
#[test]
fn a_dropped_draft_is_not_readmitted_by_a_lower_chunk() {
    let fixture = DraftFixture::new();
    let good = fixture.draft("good", "# Plan\n\nbody\n");
    let broken = fixture.draft("broken", "# Plan\n\nbody\n");
    make_inconsistent(&fixture, &broken);

    let matches = resolve_matches(
        vec![
            hit(Some(&broken), "broken best chunk", 9.0),
            hit(Some(&broken), "broken second chunk", 8.0),
            hit(Some(&broken), "broken third chunk", 7.0),
            hit(Some(&good), "good chunk", 1.0),
        ],
        &fixture.root(),
        10,
    );
    assert_eq!(matches.len(), 1, "the unreadable draft appears zero times");
    assert_eq!(matches[0].draft_id, good);
}

/// DST-FR-08: dropped hits do not consume the `limit` budget.
///
/// The break is checked against how many rows have *survived*, so a run of
/// unreadable hits at the top of the ranking still leaves room for the readable
/// ones behind them.
#[test]
fn dropped_hits_do_not_consume_the_limit() {
    let fixture = DraftFixture::new();
    let gone = fixture.draft("gone", "# Plan\n\nbody\n");
    let a = fixture.draft("a", "# Plan\n\nbody\n");
    let b = fixture.draft("b", "# Plan\n\nbody\n");
    crate::drafts::delete_draft_impl(&fixture.root(), &fixture.root(), &gone).expect("deletes");

    let matches = resolve_matches(
        vec![
            hit(Some(&gone), "stale", 9.0),
            hit(Some("01JQZ0000000000000000000AA"), "never existed", 8.0),
            hit(Some(&a), "a chunk", 2.0),
            hit(Some(&b), "b chunk", 1.0),
        ],
        &fixture.root(),
        2,
    );
    assert_eq!(matches.len(), 2, "the two readable drafts still fill the limit");
    assert_eq!(matches[0].draft_id, a);
    assert_eq!(matches[1].draft_id, b);
}

/// DST-FR-07: duplicates collapse to the first — which is the best, the
/// delegated call ordering by descending score — and `limit` stops the walk.
#[test]
fn duplicates_collapse_to_the_best_chunk_and_limit_stops_the_walk() {
    let fixture = DraftFixture::new();
    let a = fixture.draft("a", "# Plan\n\nbody\n");
    let b = fixture.draft("b", "# Plan\n\nbody\n");
    let c = fixture.draft("c", "# Plan\n\nbody\n");

    let hits = vec![
        hit(Some(&a), "a best", 9.0),
        hit(Some(&a), "a worse", 8.0),
        hit(Some(&b), "b best", 7.0),
        hit(Some(&a), "a worst", 6.0),
        hit(Some(&c), "c best", 5.0),
    ];
    let all = resolve_matches(hits.clone(), &fixture.root(), 10);
    assert_eq!(all.len(), 3, "one row per draft");
    assert_eq!(all[0].excerpt, "a best");
    assert_eq!(all[0].score, 9.0);
    assert_eq!(all[1].excerpt, "b best");
    assert_eq!(all[2].excerpt, "c best");

    let capped = resolve_matches(hits, &fixture.root(), 2);
    assert_eq!(capped.len(), 2);
    assert_eq!(capped[0].draft_id, a);
    assert_eq!(capped[1].draft_id, b);
}

/// DST-FR-12: two drafts sharing a name are two results.
///
/// Deduplication is by `draft_id` and by nothing else, so a name — or a
/// `prompt_path`, which is derived from it (DRS-FR-25) — never collapses two
/// distinct drafts into one row.
#[test]
fn two_drafts_of_the_same_name_are_two_results() {
    let fixture = DraftFixture::new();
    let first = fixture.draft("plan", "# Plan\n\nteardown the first way\n");
    let second = fixture.draft("plan", "# Plan\n\nteardown the second way\n");
    assert_ne!(first, second);
    fixture.reindex();

    let matches = fixture.search("teardown", None);
    assert_eq!(matches.len(), 2, "two drafts, not one");
    assert!(matches.iter().all(|m| m.name == "plan"));
    assert!(matches.iter().all(|m| m.prompt_path == "plan.md"));
    let ids: std::collections::HashSet<_> = matches.iter().map(|m| &m.draft_id).collect();
    assert_eq!(ids.len(), 2, "distinct ids");

    // And each reads back its own content.
    let root = fixture.root();
    for m in &matches {
        let loaded = crate::tools::draft_read::read(
            &root,
            &crate::tools::draft_read::ReadDraftArgs {
                draft_id: m.draft_id.clone(),
            },
        )
        .expect("loads");
        let expected = if loaded.draft_id == first { "first way" } else { "second way" };
        assert!(loaded.content.contains(expected));
    }
}

static DST_LIMIT_BUFFER: LogBuffer = LogBuffer::new();

/// DST-FR-21, the two remaining clauses: a `limit` no number could be made of is
/// absent from the record exactly as an unsent one is, and a no-project refusal
/// names both arguments and no applied limit.
#[test]
fn logs_omit_an_undecodable_limit_and_an_applied_limit_on_refusal() {
    let fixture = DraftFixture::new();
    fixture.draft("logged", "# Plan\n\nteardown\n");
    fixture.reindex();
    DST_LIMIT_BUFFER.clear();

    // A `limit` that decoded as nothing — the same call, as far as this tool is
    // concerned, as one that sent none at all.
    let args: DraftSearchArgs =
        serde_json::from_value(serde_json::json!({ "query": "teardown", "limit": "wat" }))
            .expect("decodes");
    block_on(fixture.logged(&DST_LIMIT_BUFFER).call(args)).expect("succeeds");

    let records = DST_LIMIT_BUFFER
        .query(&LogFilter { min_level: LogLevel::Debug, ..LogFilter::default() }, None, 1000)
        .expect("the buffer answers")
        .records;
    let success = records.first().expect("a record");
    assert!(
        success.fields.get("limit").is_none(),
        "an undecodable limit is recorded as no limit at all",
    );
    assert_eq!(success.fields["limitApplied"], serde_json::json!(10));

    // And the no-project refusal — the one refusal path reached with both
    // arguments present and no applied limit to record.
    DST_LIMIT_BUFFER.clear();
    let app = crate::tools::tests::closed_project();
    block_on(
        DraftSearchTool::with_buffer(app.handle().clone(), "closed", &DST_LIMIT_BUFFER).call(
            DraftSearchArgs {
                query: "teardown".into(),
                limit: Some(3),
            },
        ),
    )
    .expect_err("refuses");

    let refusal = DST_LIMIT_BUFFER
        .query(&LogFilter { min_level: LogLevel::Debug, ..LogFilter::default() }, None, 1000)
        .expect("the buffer answers")
        .records
        .into_iter()
        .find(|r| r.level == LogLevel::Warn)
        .expect("a WARN record");
    assert_eq!(refusal.fields["reason"], serde_json::json!("no_project_open"));
    assert_eq!(refusal.fields["query"], serde_json::json!("teardown"));
    assert_eq!(refusal.fields["limit"], serde_json::json!(3));
    assert!(
        refusal.fields.get("limitApplied").is_none(),
        "no applied limit on a refusal",
    );
}
