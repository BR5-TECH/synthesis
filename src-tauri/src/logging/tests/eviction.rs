//! The ring and an oversized record (LGC-FR-07, LGC-FR-08).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// LGC-FR-07 — the ring evicts oldest-first and counts what it dropped
// -----------------------------------------------------------------------

#[test]
fn a_full_buffer_evicts_the_oldest_and_counts_the_evictions() {
    let buffer = LogBuffer::new();
    let batch: Vec<LogInput> = (0..BUFFER_CAPACITY)
        .map(|n| input(LogLevel::Info, &[Domain::Backend], &format!("m{n}")))
        .collect();
    append(&buffer, batch);
    assert_eq!(buffer.state().buffer_total, BUFFER_CAPACITY);
    assert_eq!(buffer.state().dropped_total, 0);

    let more: Vec<LogInput> = (0..5)
        .map(|n| input(LogLevel::Info, &[Domain::Backend], &format!("extra{n}")))
        .collect();
    append(&buffer, more);

    let state = buffer.state();
    assert_eq!(state.buffer_total, BUFFER_CAPACITY, "the cap holds");
    assert_eq!(state.dropped_total, 5);
    let records = all(&buffer).records;
    assert_eq!(records[0].message, "m5", "the 5 oldest are gone");
    assert_eq!(records.last().unwrap().message, "extra4");
}

// -----------------------------------------------------------------------
// LGC-FR-08 — an oversized record costs one slot, not the buffer
// -----------------------------------------------------------------------

#[test]
fn an_oversized_records_fields_collapse_but_the_rest_survives() {
    let buffer = LogBuffer::new();
    let huge = "x".repeat(MAX_RECORD_BYTES * 2);
    append(
        &buffer,
        vec![with_fields(
            input(LogLevel::Warn, &[Domain::Backend], "big payload"),
            &[("blob", serde_json::json!(huge))],
        )],
    );

    let records = all(&buffer).records;
    assert_eq!(records.len(), 1, "appended, not rejected");
    let r = &records[0];
    assert_eq!(r.message, "big payload");
    assert_eq!(r.level, LogLevel::Warn);
    assert_eq!(r.domains, vec![Domain::Backend]);
    assert_eq!(r.ts, "2026-08-04T12:00:00.000Z");
    assert_eq!(r.fields.len(), 1);
    assert!(
        r.fields.contains_key(OMITTED_FIELD),
        "fields collapse to the marker: {:?}",
        r.fields
    );
    assert!(r.fields.get("blob").is_none());

    // A record inside the ceiling keeps its fields untouched.
    append(
        &buffer,
        vec![with_fields(
            input(LogLevel::Info, &[Domain::Backend], "small"),
            &[("errno", serde_json::json!(13))],
        )],
    );
    let records = all(&buffer).records;
    assert_eq!(
        records[1].fields.get("errno"),
        Some(&serde_json::json!(13))
    );
}
