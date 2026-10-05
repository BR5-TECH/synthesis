//! The documented contract shape (SCC-FR-01).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// SCC-FR-01 — the documented contract shape
// -----------------------------------------------------------------------

#[test]
fn ts1_a_search_streams_documented_hits_and_a_terminal_event() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write(root, "notes/a.md", "nothing here\nfind me on line two\n");

    let (sink, reason) = search(root, "find me", SearchMode::LiteralInsensitive, SearchScope::Capped);
    assert_eq!(reason, EndReason::Completed);
    assert_eq!(sink.end_reason(), EndReason::Completed);

    let hits = sink.hits();
    assert_eq!(hits.len(), 1, "{hits:?}");
    let json = serde_json::to_value(&hits[0]).unwrap();
    for field in ["id", "name", "path", "ordinal", "group", "matchKind"] {
        assert!(json.get(field).is_some(), "{field} missing from {json}");
    }
    assert_eq!(json.get("matchKind").and_then(|v| v.as_str()), Some("content"));
    assert_eq!(json.get("line").and_then(|v| v.as_u64()), Some(2));
    assert_eq!(
        json.get("snippet").and_then(|v| v.as_str()),
        Some("find me on line two")
    );

    // The events serialise with the payload field names the frontend reads.
    let payload = serde_json::to_value(&SearchEndedPayload {
        search_id: "s".into(),
        reason: EndReason::Capped,
    })
    .unwrap();
    assert_eq!(payload.get("searchId").and_then(|v| v.as_str()), Some("s"));
    assert_eq!(payload.get("reason").and_then(|v| v.as_str()), Some("capped"));
}
