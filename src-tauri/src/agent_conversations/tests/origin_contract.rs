//! The origin the frontend sends is the origin this module reads.
//!
//! One part of `../tests/mod.rs`. The cases come from the file the frontend's
//! own test reads (`src/test/contracts/conversationOrigin.json`), so a change to
//! the shape on one side fails a test on the other. Tauri reads the `origin`
//! argument of `dispatch_agent_turn` with serde before the command runs, so a
//! shape this side cannot read is a dispatch that fails with no log line.

use super::*;

const CONTRACT: &str = include_str!("../../../../src/test/contracts/conversationOrigin.json");

fn cases() -> Vec<(String, serde_json::Value)> {
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(CONTRACT).expect("the contract file is JSON");
    parsed
        .into_iter()
        .map(|case| {
            let kind = case["kind"].as_str().expect("each case names a kind").to_string();
            (kind, case["origin"].clone())
        })
        .collect()
}

// AGC-FR-05: every origin the frontend sends reads as the kind it names.
#[test]
fn every_origin_the_frontend_sends_is_read() {
    let cases = cases();
    assert_eq!(cases.len(), 5, "the contract holds one case per origin kind");
    for (kind, json) in cases {
        let origin: ConversationOrigin = serde_json::from_value(json.clone())
            .unwrap_or_else(|e| panic!("the {kind} origin does not deserialize: {e}"));
        assert_eq!(origin.kind().as_str(), kind);
        assert_eq!(origin.discussion_id(), json["discussionId"].as_str().unwrap());
    }
}

// AGC-FR-05: an origin goes back to the frontend in the shape it came in, so a
// turn's origin matches the discussion the frontend holds.
#[test]
fn an_origin_serializes_in_the_shape_the_frontend_reads() {
    for (kind, json) in cases() {
        let origin: ConversationOrigin = serde_json::from_value(json.clone()).unwrap();
        let back = serde_json::to_value(&origin).unwrap();
        assert_eq!(back, json, "the {kind} origin does not round-trip");
    }
}

// AGC-FR-05: the tagged shape the origin had before is refused, which is what a
// frontend still sending it meets.
#[test]
fn the_old_tagged_origin_is_refused() {
    let old = serde_json::json!({
        "kind": "draft_discussion",
        "threadId": "disc-draft",
        "draftId": "draft-1",
    });
    let error = serde_json::from_value::<ConversationOrigin>(old)
        .expect_err("the old shape names no discussion id");
    assert!(error.to_string().contains("discussionId"), "{error}");
}
