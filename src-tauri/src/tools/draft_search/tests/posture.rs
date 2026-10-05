//! DST-FR-20, DST-FR-21: read-only posture and logging.

use super::*;

// ---------------------------------------------------------------------------
// DST-FR-20, DST-FR-21: read-only posture and logging
// ---------------------------------------------------------------------------

/// DST-FR-20: searching changes nothing anywhere.
#[test]
fn is_read_only() {
    let fixture = DraftFixture::new();
    fixture.draft("one", "# Plan\n\nteardown\n");
    fixture.draft("two", "# Plan\n\nteardown ordering\n");
    fixture.reindex();

    let before = crate::tools::tests::tree_snapshot(fixture.root().path());
    for query in ["teardown", "ordering", "nothing here"] {
        for limit in [None, Some(1), Some(1000)] {
            let _ = block_on(fixture.tool().call(DraftSearchArgs {
                query: query.into(),
                limit,
            }));
        }
    }
    assert_eq!(
        before,
        crate::tools::tests::tree_snapshot(fixture.root().path()),
        "no prompt, record, history, proposal, comment log, or conversation changed"
    );
}

static DST_BUFFER: LogBuffer = LogBuffer::new();

/// DST-FR-21: the success and refusal records carry the named arguments and
/// nothing the call returned.
#[test]
fn logs_the_named_arguments_and_no_returned_material() {
    let fixture = DraftFixture::new();
    fixture.draft("logged", "# Plan\n\nteardown sequence\n");
    fixture.reindex();
    DST_BUFFER.clear();

    let tool = fixture.logged(&DST_BUFFER);
    block_on(tool.call(DraftSearchArgs {
        query: "teardown".into(),
        limit: Some(2),
    }))
    .expect("succeeds");
    block_on(tool.call(DraftSearchArgs {
        query: "".into(),
        limit: None,
    }))
    .expect_err("refuses");

    // Querying either domain returns the same two records — a tool call is
    // work the backend performs because a model asked for it, and a reader
    // filtering on either should see it.
    let of_domain = |domain: Domain| {
        DST_BUFFER
            .query(
                &LogFilter {
                    min_level: LogLevel::Debug,
                    domains: vec![domain],
                    ..LogFilter::default()
                },
                None,
                1000,
            )
            .expect("the buffer answers")
            .records
    };
    let records = of_domain(Domain::Ai);
    assert_eq!(records.len(), 2, "one success and one refusal");
    assert_eq!(
        records,
        of_domain(Domain::Backend),
        "both domains return the same two records",
    );
    let success = records
        .iter()
        .find(|r| r.level == LogLevel::Info && r.fields.get("tool").is_some())
        .expect("an INFO record");
    assert_eq!(success.fields["tool"], serde_json::json!(NAME));
    assert_eq!(success.fields["query"], serde_json::json!("teardown"));
    assert_eq!(success.fields["limit"], serde_json::json!(2));
    assert_eq!(success.fields["limitApplied"], serde_json::json!(2));
    assert_eq!(success.fields["drafts"], serde_json::json!(1));
    // Pin the exact key set, so a field added to the shared helpers for another
    // tool cannot silently widen what this one records.
    assert_eq!(
        crate::tools::tests::sorted_keys(success),
        vec!["drafts", "limit", "limitApplied", "query", "tool"],
    );

    let refusal = records
        .iter()
        .find(|r| r.level == LogLevel::Warn)
        .expect("a WARN record");
    assert_eq!(refusal.fields["reason"], serde_json::json!("invalid_arguments"));
    assert_eq!(refusal.fields["query"], serde_json::json!(""));
    assert!(
        refusal.fields.get("limit").is_none(),
        "the model sent none, which is itself the fact worth reading"
    );
    assert!(
        refusal.fields.get("limitApplied").is_none(),
        "no applied limit on a refusal"
    );

    // DST-FR-21: nothing the call returned reaches a record.
    for record in &records {
        let blob = serde_json::to_string(&record.fields).expect("serialises")
            + &record.message;
        for leaked in ["logged", "sequence", "# Plan", "teardown sequence", ".md"] {
            assert!(
                !blob.contains(leaked),
                "{leaked:?} leaked into a log record: {blob}"
            );
        }
    }
}

static DST_BOUND_BUFFER: LogBuffer = LogBuffer::new();

/// DST-FR-21: a pathological query is recorded as its first 512 characters and
/// an ellipsis, so the record keeps the tool name and the count.
#[test]
fn logs_a_bounded_query() {
    let fixture = DraftFixture::new();
    DST_BOUND_BUFFER.clear();
    let query = "sarcophagus ".repeat(2000);

    block_on(fixture.logged(&DST_BOUND_BUFFER).call(DraftSearchArgs {
        query: query.clone(),
        limit: Some(1000),
    }))
    .expect("succeeds");

    let records = DST_BOUND_BUFFER.query(&LogFilter { min_level: LogLevel::Debug, ..LogFilter::default() }, None, 1000)
        .expect("the buffer answers")
        .records;
    let record = records.first().expect("a record");
    let logged = record.fields["query"].as_str().expect("a string");
    assert_eq!(logged.chars().count(), 513, "512 characters and the ellipsis");
    assert!(logged.ends_with('…'));
    assert_eq!(record.fields["tool"], serde_json::json!(NAME));
    assert_eq!(record.fields["drafts"], serde_json::json!(0));
    // A clamped limit is readable as both the number sent and the one applied.
    assert_eq!(record.fields["limit"], serde_json::json!(1000));
    assert_eq!(record.fields["limitApplied"], serde_json::json!(20));
}

/// TLC-FR-07: a `limit` no number can be made of falls back to the default and
/// is absent from the record, exactly as an absent one is.
#[test]
fn a_limit_that_is_not_a_number_is_forgiven() {
    let args: DraftSearchArgs =
        serde_json::from_value(serde_json::json!({ "query": "teardown", "limit": "not a number" }))
            .expect("decodes rather than failing");
    assert!(args.limit.is_none());
    assert_eq!(normalize_limit(args.limit), 10);

    // And every spelling of a number a model plausibly emits does decode.
    for (sent, expected) in [
        (serde_json::json!("3"), 3),
        (serde_json::json!(3.7), 3),
        (serde_json::json!(4), 4),
    ] {
        let args: DraftSearchArgs =
            serde_json::from_value(serde_json::json!({ "query": "q", "limit": sent }))
                .expect("decodes");
        assert_eq!(args.limit, Some(expected));
    }

    // TLC-FR-07: an unrecognised field is ignored rather than rejected.
    let args: DraftSearchArgs =
        serde_json::from_value(serde_json::json!({ "query": "q", "wat": true }))
            .expect("an unknown field is ignored");
    assert_eq!(args.query, "q");
}
