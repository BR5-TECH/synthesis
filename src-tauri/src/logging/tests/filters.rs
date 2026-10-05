//! The filters, the text query, the event payload, and what is never kept
//! (LGC-FR-09 to LGC-FR-13, LGC-FR-16).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// LGC-FR-09 — the level floor and the domain intersection
// -----------------------------------------------------------------------

#[test]
fn min_level_is_a_floor_and_domains_intersect() {
    let buffer = LogBuffer::new();
    append(
        &buffer,
        vec![
            input(LogLevel::Debug, &[Domain::Frontend], "d"),
            input(LogLevel::Info, &[Domain::Frontend], "i"),
            input(LogLevel::Warn, &[Domain::Ai, Domain::Backend], "w"),
            input(LogLevel::Error, &[Domain::Remote], "e"),
        ],
    );

    let warn_up = LogFilter {
        min_level: LogLevel::Warn,
        ..Default::default()
    };
    let page = buffer.query(&warn_up, None, 100).unwrap();
    assert_eq!(
        page.records.iter().map(|r| r.message.as_str()).collect::<Vec<_>>(),
        vec!["w", "e"],
        "a floor, not an equality"
    );
    assert_eq!(page.matched_total, 2);
    assert_eq!(page.buffer_total, 4, "the buffer total ignores the filter");

    // A record carrying {ai, backend} matches a filter naming only `ai`.
    let ai_only = LogFilter {
        domains: vec![Domain::Ai],
        ..Default::default()
    };
    let page = buffer.query(&ai_only, None, 100).unwrap();
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0].message, "w");

    // An empty domain list matches every record.
    let page = buffer.query(&LogFilter::default(), None, 100).unwrap();
    assert_eq!(page.records.len(), 4);
}

// -----------------------------------------------------------------------
// LGC-FR-10 / LGC-FR-11 — the text query and the invalid one
// -----------------------------------------------------------------------

#[test]
fn the_query_matches_the_message_and_the_fields_case_insensitively() {
    let buffer = LogBuffer::new();
    append(
        &buffer,
        vec![
            input(LogLevel::Error, &[Domain::Backend], "scan aborted"),
            with_fields(
                input(LogLevel::Info, &[Domain::Backend], "scanning"),
                &[("path", serde_json::json!("/vendor"))],
            ),
        ],
    );

    let hits = |q: &str, rx: bool| -> Vec<String> {
        buffer
            .query(
                &LogFilter {
                    query: Some(q.to_string()),
                    query_is_regex: rx,
                    ..Default::default()
                },
                None,
                100,
            )
            .unwrap()
            .records
            .into_iter()
            .map(|r| r.message)
            .collect()
    };

    assert_eq!(hits("ABORT", false), vec!["scan aborted"], "case-insensitive");
    assert_eq!(hits("vendor", false), vec!["scanning"], "matches a field value");
    assert_eq!(hits("path", false), vec!["scanning"], "matches a field key");
    assert_eq!(hits("", false).len(), 2, "an empty query matches everything");
    assert_eq!(hits("^scan a", true), vec!["scan aborted"], "regex");
    // A literal query does not assign meaning to a regex metacharacter.
    assert!(hits("scan a.orted", false).is_empty());
}

#[test]
fn an_uncompilable_regex_is_the_typed_invalid_query() {
    let buffer = LogBuffer::new();
    append(&buffer, vec![input(LogLevel::Info, &[Domain::Backend], "x")]);
    let bad = LogFilter {
        query: Some("(".to_string()),
        query_is_regex: true,
        ..Default::default()
    };
    assert_eq!(buffer.query(&bad, None, 10), Err(INVALID_QUERY.to_string()));
    assert_eq!(
        buffer.export_text(&bad),
        Err(INVALID_QUERY.to_string()),
        "the export path rejects on the same terms, before writing"
    );
    // The same pattern as a literal is a plain substring, not an error.
    let literal = LogFilter {
        query: Some("(".to_string()),
        ..Default::default()
    };
    assert!(buffer.query(&literal, None, 10).is_ok());
}

// -----------------------------------------------------------------------
// LGC-FR-12 — the event carries no record content
// -----------------------------------------------------------------------

#[test]
fn the_event_payload_carries_no_record_content() {
    // LGC-FR-12: a consumer cannot evaluate a filter from the event, which is
    // what makes "all filtering happens on the backend" structural rather
    // than merely intended.
    let buffer = LogBuffer::new();
    append(
        &buffer,
        vec![input(LogLevel::Error, &[Domain::Backend], "a secret-ish message")],
    );
    let json = serde_json::to_value(buffer.state()).unwrap();
    let object = json.as_object().unwrap();
    let mut keys: Vec<&str> = object.keys().map(|k| k.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "bufferTotal",
            "droppedTotal",
            "generation",
            "highestSequence"
        ],
        "the event carries the buffer's shape and nothing of its contents"
    );
    assert!(!json.to_string().contains("secret-ish"));
}

// -----------------------------------------------------------------------
// LGC-FR-13 — no filter state is retained
// -----------------------------------------------------------------------

#[test]
fn no_filter_is_retained_between_calls() {
    let buffer = LogBuffer::new();
    append(
        &buffer,
        vec![
            input(LogLevel::Debug, &[Domain::Backend], "d"),
            input(LogLevel::Error, &[Domain::Backend], "e"),
        ],
    );
    let strict = LogFilter {
        min_level: LogLevel::Error,
        ..Default::default()
    };
    assert_eq!(buffer.query(&strict, None, 10).unwrap().records.len(), 1);
    assert_eq!(
        buffer.query(&LogFilter::default(), None, 10).unwrap().records.len(),
        2,
        "the second call is unaffected by the first"
    );
}

// -----------------------------------------------------------------------
// LGC-FR-16 — nothing is redacted
// -----------------------------------------------------------------------

#[test]
fn a_credential_shaped_value_is_stored_verbatim() {
    // LGC-FR-16: this module rewrites nothing. The test exists to pin that
    // it is the EMITTER's obligation — a future "helpful" scrub here would
    // be a silent contract change, and the panel would start showing
    // something other than what was logged.
    let buffer = LogBuffer::new();
    let secret = "ghp_0123456789abcdefghijklmnopqrstuvwxyz";
    append(
        &buffer,
        vec![with_fields(
            input(LogLevel::Debug, &[Domain::Remote], secret),
            &[("token", serde_json::json!(secret))],
        )],
    );
    let r = &all(&buffer).records[0];
    assert_eq!(r.message, secret);
    assert_eq!(r.fields.get("token"), Some(&serde_json::json!(secret)));
}
