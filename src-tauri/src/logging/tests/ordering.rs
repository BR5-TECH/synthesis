//! Sequences, append order, and the oldest match
//! (LGC-FR-05, LGC-FR-06, LGC-FR-14, LGC-FR-15).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// LGC-FR-05, LGC-FR-14, LGC-FR-15 — sequences survive a clear
// -----------------------------------------------------------------------

#[test]
fn sequences_never_reset_across_a_clear() {
    // LGC-FR-05: a cursor held from before a clear must never match a record
    // appended after one — which is exactly what a reset sequence would do.
    let buffer = LogBuffer::new();
    append(&buffer, vec![input(LogLevel::Info, &[Domain::Backend], "a")]);
    append(&buffer, vec![input(LogLevel::Info, &[Domain::Backend], "b")]);
    let pre_clear = all(&buffer).records.last().unwrap().sequence;

    let state = buffer.clear();
    assert_eq!(state.generation, 1);
    assert_eq!(state.buffer_total, 0);
    assert_eq!(state.dropped_total, 0);

    append(&buffer, vec![input(LogLevel::Info, &[Domain::Backend], "c")]);
    let fresh = all(&buffer).records[0].sequence;
    assert!(
        fresh > pre_clear,
        "sequence {fresh} must exceed the pre-clear {pre_clear}"
    );

    // And the stale cursor selects only post-clear records.
    let page = buffer
        .query(&LogFilter::default(), Some(Cursor::After(pre_clear)), 100)
        .unwrap();
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0].message, "c");
    assert_eq!(page.generation, 1);
}

// -----------------------------------------------------------------------
// LGC-FR-06 — append order is the buffer's order
// -----------------------------------------------------------------------

#[test]
fn a_batch_keeps_its_submitted_order_and_each_records_own_ts() {
    // LGC-FR-06: `ts` is the caller's, so a batched frontend record keeps the
    // instant it was emitted; ordering follows append order regardless.
    let buffer = LogBuffer::new();
    let mut batch = Vec::new();
    for (n, ts) in ["...T00:00:03.000Z", "...T00:00:01.000Z", "...T00:00:02.000Z"]
        .into_iter()
        .enumerate()
    {
        let mut i = input(LogLevel::Info, &[Domain::Frontend], &format!("m{n}"));
        i.ts = ts.to_string();
        batch.push(i);
    }
    append(&buffer, batch);

    let records = all(&buffer).records;
    assert_eq!(
        records.iter().map(|r| r.message.as_str()).collect::<Vec<_>>(),
        vec!["m0", "m1", "m2"],
        "submitted order, not ts order"
    );
    assert!(records[0].sequence < records[1].sequence);
    assert!(records[1].sequence < records[2].sequence);
    assert_eq!(records[0].ts, "...T00:00:03.000Z", "each keeps its own ts");
}

// -----------------------------------------------------------------------
// `find` — the oldest match, per LGC-FR-06's append order
// -----------------------------------------------------------------------

/// LGC-FR-06: the buffer's order is the order records were appended, so
/// "the oldest match" is a meaningful phrase and `find` answers with it.
///
/// Pinned rather than left to the shape of `iter().find()`, because the
/// ordering is what two callers rest on: a turn makes several `calling
/// model` calls, and the tests that read the record's fields mean the
/// **first** one. The module those callers live in also holds its own
/// `find(records, message)`, which answers with the **newest** — so the two
/// names disagree, and only an assertion keeps this one from being
/// "corrected" to match its neighbour.
#[test]
fn find_answers_with_the_oldest_match_and_none_where_there_is_none() {
    let buffer = LogBuffer::new();
    assert!(
        buffer.find(|_| true).is_none(),
        "an empty buffer matches nothing, whatever the predicate says"
    );

    for n in 0..4 {
        let i = with_fields(
            input(LogLevel::Info, &[Domain::Frontend], "calling model"),
            &[("turnId", serde_json::json!(format!("turn-{n}")))],
        );
        append(&buffer, vec![i]);
    }
    // A record the predicate below must not answer with, appended last.
    append(
        &buffer,
        vec![input(LogLevel::Info, &[Domain::Frontend], "model answered")],
    );

    let hit = buffer
        .find(|r| r.message == "calling model")
        .expect("four records carry that message");
    assert_eq!(
        hit.fields.get("turnId").and_then(|v| v.as_str()),
        Some("turn-0"),
        "the oldest of the four, not the newest",
    );
    assert!(
        buffer.find(|r| r.message == "never emitted").is_none(),
        "no match is None rather than the nearest record",
    );
}
