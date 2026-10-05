//! TLC-FR-07: argument decoding.

use super::*;

// ---------------------------------------------------------------------------
// TLC-FR-07: argument decoding
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_field_is_ignored_and_a_limit_of_any_shape_decodes() {
    let args: NoteSearchArgs =
        serde_json::from_value(serde_json::json!({ "query": "q", "wat": true }))
            .expect("an unknown field is ignored");
    assert_eq!(args.query, "q");
    assert_eq!(args.limit, None);

    for (sent, expected) in [
        (serde_json::json!(3), Some(3)),
        (serde_json::json!(3.7), Some(3)),
        (serde_json::json!("4"), Some(4)),
        (serde_json::json!(null), None),
        (serde_json::json!(true), None),
    ] {
        let args: NoteSearchArgs =
            serde_json::from_value(serde_json::json!({ "query": "q", "limit": sent }))
                .expect("the query survives whatever shape the limit took");
        assert_eq!(args.limit, expected);
    }
}
