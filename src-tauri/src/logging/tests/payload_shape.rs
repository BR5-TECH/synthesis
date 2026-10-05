//! The documented payload shape and the domain set
//! (LGC-FR-01, LGC-FR-03).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// LGC-FR-01 — the documented payload shape
// -----------------------------------------------------------------------

#[test]
fn fields_are_flattened_so_a_consumer_never_recurses() {
    // LGC-FR-04: `fields` holds no nested object and no array. A caller that
    // passes one gets its JSON text rather than a discard, so the value is
    // still searchable (LGC-FR-10 matches against field text) and still
    // renders as exactly one row.
    let buffer = LogBuffer::new();
    append(
        &buffer,
        vec![with_fields(
            input(LogLevel::Info, &[Domain::Ai], "structured"),
            &[
                ("nested", serde_json::json!({ "b": 1 })),
                ("list", serde_json::json!([1, 2, 3])),
                ("scalar", serde_json::json!(7)),
            ],
        )],
    );
    let fields = &all(&buffer).records[0].fields;
    assert_eq!(fields.len(), 3, "flattened, not dropped");
    for (key, value) in fields {
        assert!(
            !value.is_object() && !value.is_array(),
            "{key} is still structured: {value}"
        );
    }
    assert_eq!(fields["nested"], serde_json::json!("{\"b\":1}"));
    assert_eq!(fields["list"], serde_json::json!("[1,2,3]"));
    // A scalar is untouched — flattening is not stringification of
    // everything.
    assert_eq!(fields["scalar"], serde_json::json!(7));

    // And the flattened text is searchable, so nothing became invisible.
    let hit = buffer
        .query(
            &LogFilter {
                query: Some("\"b\":1".to_string()),
                ..Default::default()
            },
            None,
            10,
        )
        .unwrap();
    assert_eq!(hit.records.len(), 1);
}

#[test]
fn an_oversized_record_with_no_fields_is_stored_whole() {
    // LGC-FR-08 gives up `fields` and nothing else. Collapsing an empty map
    // would insert a marker claiming fields were omitted when there were
    // none, which is worse than the size.
    let buffer = LogBuffer::new();
    let huge = "x".repeat(MAX_RECORD_BYTES * 2);
    append(&buffer, vec![input(LogLevel::Warn, &[Domain::Backend], &huge)]);
    let r = &all(&buffer).records[0];
    assert_eq!(r.message.len(), huge.len());
    assert!(r.fields.is_empty(), "no marker for fields that never existed");
}

#[test]
fn a_record_serialises_to_the_documented_shape() {
    // LGC-FR-01 / LGC-FR-03: camelCase fields, an UPPERCASE level, and
    // lowercase domain discriminants.
    let buffer = LogBuffer::new();
    append(
        &buffer,
        vec![with_fields(
            input(LogLevel::Error, &[Domain::Ai, Domain::Remote], "boom"),
            &[("path", serde_json::json!("vendor/"))],
        )],
    );
    let page = all(&buffer);
    let json = serde_json::to_value(&page.records[0]).unwrap();
    assert_eq!(json.get("level").and_then(|v| v.as_str()), Some("ERROR"));
    assert_eq!(
        json.get("domains").unwrap(),
        &serde_json::json!(["ai", "remote"])
    );
    assert_eq!(json.get("message").and_then(|v| v.as_str()), Some("boom"));
    assert_eq!(json.get("sequence").and_then(|v| v.as_u64()), Some(0));
    assert_eq!(
        json.get("fields").unwrap().get("path").unwrap(),
        &serde_json::json!("vendor/")
    );

    // The page's own shape, which the panel reads directly.
    let json = serde_json::to_value(&page).unwrap();
    for key in [
        "records",
        "generation",
        "matchedTotal",
        "bufferTotal",
        "droppedTotal",
        "highestSequence",
    ] {
        assert!(json.get(key).is_some(), "page is missing {key}");
    }
}

// -----------------------------------------------------------------------
// LGC-FR-03 — the domain set
// -----------------------------------------------------------------------

#[test]
fn a_record_carries_several_domains_and_an_empty_set_is_rejected() {
    let buffer = LogBuffer::new();
    append(
        &buffer,
        vec![input(LogLevel::Warn, &[Domain::Ai, Domain::Remote], "retry")],
    );
    assert_eq!(all(&buffer).records[0].domains, vec![Domain::Ai, Domain::Remote]);

    let before = buffer.state().buffer_total;
    append(&buffer, vec![input(LogLevel::Info, &[], "unattributable")]);
    assert_eq!(
        buffer.state().buffer_total,
        before,
        "a record with no domain is rejected, not appended"
    );
}
