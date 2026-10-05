//! Cursors, paging, and the real publishing path (LGC-FR-22).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// LGC-FR-22 — cursors and paging
// -----------------------------------------------------------------------

#[test]
fn cursors_page_backwards_and_forwards_and_always_ascend() {
    let buffer = LogBuffer::new();
    let batch: Vec<LogInput> = (0..300)
        .map(|n| input(LogLevel::Info, &[Domain::Backend], &format!("m{n}")))
        .collect();
    append(&buffer, batch);

    // No cursor: the NEWEST page, so a panel opening on a full buffer starts
    // at the end rather than paging to it.
    let newest = buffer.query(&LogFilter::default(), None, 100).unwrap();
    assert_eq!(newest.records.len(), 100);
    assert_eq!(newest.records[0].message, "m200");
    assert_eq!(newest.records.last().unwrap().message, "m299");

    // `before`: the page preceding it, still ascending.
    let oldest_shown = newest.records[0].sequence;
    let previous = buffer
        .query(&LogFilter::default(), Some(Cursor::Before(oldest_shown)), 100)
        .unwrap();
    assert_eq!(previous.records.len(), 100);
    assert_eq!(previous.records[0].message, "m100");
    assert_eq!(previous.records.last().unwrap().message, "m199");

    // `after`: only the delta.
    let newest_shown = newest.records.last().unwrap().sequence;
    append(
        &buffer,
        (0..5)
            .map(|n| input(LogLevel::Info, &[Domain::Backend], &format!("new{n}")))
            .collect(),
    );
    let delta = buffer
        .query(&LogFilter::default(), Some(Cursor::After(newest_shown)), 100)
        .unwrap();
    assert_eq!(delta.records.len(), 5);
    assert_eq!(delta.records[0].message, "new0");

    // An evicted sequence is positional, not an error.
    let stale = buffer
        .query(&LogFilter::default(), Some(Cursor::After(0)), 10)
        .unwrap();
    assert_eq!(stale.records.len(), 10);

    // The cursor composes with the filter rather than bypassing it.
    let filtered = buffer
        .query(
            &LogFilter {
                query: Some("new".to_string()),
                ..Default::default()
            },
            Some(Cursor::After(newest_shown)),
            100,
        )
        .unwrap();
    assert_eq!(filtered.records.len(), 5);
    assert_eq!(filtered.matched_total, 5);
}

#[test]
fn the_cursor_deserialises_from_the_documented_wire_shape() {
    // `{ after: n }` / `{ before: n }` — the shape `../ui/LOG-logs.md` sends.
    let after: Cursor = serde_json::from_value(serde_json::json!({ "after": 12 })).unwrap();
    assert!(matches!(after, Cursor::After(12)));
    let before: Cursor = serde_json::from_value(serde_json::json!({ "before": 7 })).unwrap();
    assert!(matches!(before, Cursor::Before(7)));
}

#[test]
fn an_empty_buffer_answers_with_an_empty_page() {
    // LGC-FR-20: no project need be open for any of this to answer.
    let buffer = LogBuffer::new();
    let page = buffer.query(&LogFilter::default(), None, 100).unwrap();
    assert!(page.records.is_empty());
    assert_eq!(page.matched_total, 0);
    assert_eq!(page.buffer_total, 0);
    assert_eq!(page.highest_sequence, None);
    assert_eq!(buffer.export_text(&LogFilter::default()).unwrap().1, 0);
}

/// A `'static` buffer for the tests that drive the real publishing path.
/// Leaked deliberately — `append_and_publish` hands the buffer to a
/// background thread, which is only sound for a buffer that outlives it,
/// and in production that buffer is the process-lifetime [`BUFFER`].
fn leaked_buffer() -> &'static LogBuffer {
    Box::leak(Box::new(LogBuffer::new()))
}

#[test]
fn the_publishing_path_settles_every_debt_a_burst_leaves() {
    // The end-to-end counterpart of
    // `a_debt_incurred_after_a_leading_edge_emit_is_never_stranded`: the
    // real `append_and_publish`, the real CAS, and the real flush thread.
    // The unit test above pins the clock arithmetic; this one pins that the
    // scheduling loop actually keeps sleeping on an unsettled debt.
    let buffer = leaked_buffer();
    let sink = Recorder::default();

    for n in 0..500u64 {
        append_and_publish(&sink, buffer, vec![input(
            LogLevel::Info,
            &[Domain::Backend],
            &format!("m{n}"),
        )]);
    }
    // The burst returns promptly: no emit waits out a coalescing window.
    assert_eq!(buffer.state().buffer_total, 500);

    // Give the scheduling loop room to settle whatever it owes.
    for _ in 0..40 {
        if !buffer.is_flush_pending() && sink.last().buffer_total == 500 {
            break;
        }
        std::thread::sleep(COALESCE_WINDOW);
    }

    assert!(
        !buffer.is_flush_pending(),
        "a debt was left outstanding with no thread to settle it"
    );
    assert_eq!(
        sink.last().buffer_total,
        500,
        "the last delivered state must describe the whole burst"
    );
    assert!(
        sink.len() < 500,
        "500 appends must not become 500 events, got {}",
        sink.len()
    );
}

#[test]
fn a_single_batch_larger_than_the_cap_evicts_within_itself() {
    // The eviction loop runs per record rather than once per batch, so a
    // batch that overflows the ring on its own is bounded too — a frontend
    // flush is a batch, and nothing bounds its size but the ring.
    let buffer = LogBuffer::new();
    let batch: Vec<LogInput> = (0..BUFFER_CAPACITY + 5)
        .map(|n| input(LogLevel::Info, &[Domain::Frontend], &format!("m{n}")))
        .collect();
    append(&buffer, batch);

    let state = buffer.state();
    assert_eq!(state.buffer_total, BUFFER_CAPACITY);
    assert_eq!(state.dropped_total, 5);
    let records = all(&buffer).records;
    assert_eq!(records[0].message, "m5");
    assert_eq!(
        records.last().unwrap().message,
        format!("m{}", BUFFER_CAPACITY + 4)
    );
}

#[test]
fn a_before_cursor_naming_an_evicted_sequence_is_positional() {
    // The `after` half is covered by `cursors_page_backwards_and_forwards…`.
    // This is the arm the panel drives when the user scrolls to the top of a
    // buffer whose head has since been evicted: an empty page, not an error,
    // and not the newest page it would return for an absent cursor.
    let buffer = LogBuffer::new();
    append(
        &buffer,
        (0..10)
            .map(|n| input(LogLevel::Info, &[Domain::Backend], &format!("m{n}")))
            .collect(),
    );
    let oldest = all(&buffer).records[0].sequence;
    let page = buffer
        .query(&LogFilter::default(), Some(Cursor::Before(oldest)), 100)
        .unwrap();
    assert!(page.records.is_empty(), "nothing precedes the oldest record");
    assert_eq!(page.matched_total, 10, "the match set is still described");
}

#[test]
fn command_functions_are_in_scope() {
    // Compile-time guard: renaming or removing a command without updating
    // `generate_handler!` / `COMMAND_NAMES` fails to compile here.
    let _ = append_log_records;
    let _ = query_logs;
    let _ = export_logs;
}

#[test]
fn log_fields_builds_the_map_an_emit_site_would_otherwise_hand_roll() {
    let empty = crate::log_fields! {};
    assert!(empty.is_empty());

    let fields = crate::log_fields! {
        "remote" => "origin",
        "status" => 403,
        "retried" => true,
    };
    assert_eq!(fields.len(), 3);
    assert_eq!(fields["remote"], serde_json::json!("origin"));
    assert_eq!(fields["status"], serde_json::json!(403));
    assert_eq!(fields["retried"], serde_json::json!(true));
}

#[test]
fn the_internal_emit_api_is_the_one_lgc_fr_01_documents() {
    // LGC-FR-01 promises `log` *and its four level wrappers* as the internal
    // Rust API, and the engineer skill's conventions file tells every future
    // emit site to reach for them by name. Nothing else pins their
    // existence: no production caller uses the wrappers yet, so dropping one
    // would break the documented surface silently.
    let sink = Recorder::default();
    let buffer = leaked_buffer();
    let at = |level: LogLevel| {
        let mut f = Fields::new();
        f.insert("k".to_string(), serde_json::json!("v"));
        (level, f)
    };

    log_debug(&sink, buffer, &[Domain::Backend], "d", at(LogLevel::Debug).1);
    log_info(&sink, buffer, &[Domain::Frontend], "i", at(LogLevel::Info).1);
    log_warn(&sink, buffer, &[Domain::Ai], "w", at(LogLevel::Warn).1);
    log_error(&sink, buffer, &[Domain::Remote], "e", at(LogLevel::Error).1);

    let page = buffer.query(&LogFilter::default(), None, 100).unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|r| (r.level, r.message.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (LogLevel::Debug, "d"),
            (LogLevel::Info, "i"),
            (LogLevel::Warn, "w"),
            (LogLevel::Error, "e"),
        ],
        "each wrapper fixes its own level and changes nothing else"
    );
    // The wrappers stamp a timestamp of their own (LGC-FR-06) rather than
    // leaving it to the caller, which is what makes them usable from a
    // module that has no clock of its own.
    assert!(page.records.iter().all(|r| r.ts.ends_with('Z') && r.ts.len() == 24));
    assert!(page.records.iter().all(|r| r.fields.get("k").is_some()));
}
