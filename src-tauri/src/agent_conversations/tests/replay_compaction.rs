//! Compaction and recovery of a turn that opens with a replay
//! (`ERP-exchange-replay.md` ERP-FR-DSAP, ERP-FR-WZAV).

use super::*;
use crate::agent_conversations::compaction::{
    compacted_exchange_after, compacted_request, has_older_history, latest_round_start_after,
    older_history_text,
};
use crate::agent_conversations::overflow::cut_latest_results_after;
use rig::completion::Message;

fn text_section(tag: &str, body: &str) -> InputSection {
    InputSection { tag: tag.into(), body: body.into(), ..Default::default() }
}

/// A request whose replay holds one block with the material and one read.
fn replayed_request() -> AgentRequest {
    let call = rig_tool_call("old-1", "read_file", serde_json::json!({ "path": "a.md" }));
    AgentRequest {
        instructions: "p".into(),
        input: vec![
            text_section(TAG_DISCUSSION_HISTORY, "ada: between"),
            text_section(TAG_CURRENT_COMMENT, "ada: now?"),
        ],
        stable_head_sections: 1,
        replay: vec![ReplayBlock {
            opening: vec![
                text_section(TAG_ARTIFACT, "MATERIAL-TEXT"),
                text_section(TAG_DISCUSSION_HISTORY, ""),
                text_section(TAG_CURRENT_COMMENT, "ada: first?"),
            ],
            rounds: vec![
                Message::Assistant(rig::completion::message::AssistantMessage::new(vec![
                    rig::completion::message::AssistantContent::ToolCall(call.clone()),
                ])),
                tool_result_message("old-1", "read_file", "R".repeat(5_000)),
            ],
            answer: Some("OLD-ANSWER".into()),
        }],
        ..Default::default()
    }
}

// ERP-FR-DSAP, CVL-FR-XCOY: the replay is older history, its text holds the
// rounds and the answer and no material, and the latest round is a round of
// the current turn alone.
#[test]
fn the_replay_is_older_history_without_its_material() {
    let request = replayed_request();
    let exchange = opening_exchange(&request, false);
    let opening = opening_of(&request);
    assert_eq!(opening, 4);
    assert_eq!(exchange.len(), 5);
    assert_eq!(latest_round_start_after(&exchange, opening), exchange.len());
    assert!(has_older_history(&request, &exchange));
    let text = older_history_text(&request, &exchange);
    assert!(text.contains("<earlier_turns>") && text.contains("OLD-ANSWER"), "{text}");
    assert!(text.contains("RRRR") && text.contains("first?"), "{text}");
    assert!(!text.contains("MATERIAL-TEXT"), "{text}");
}

// ERP-FR-DSAP, CVL-FR-BSMY: a compacted request carries no replay, its input
// holds the material first, and the compacted exchange keeps the latest round
// of the current turn.
#[test]
fn a_compacted_request_carries_no_replay() {
    let request = replayed_request();
    let mut exchange = opening_exchange(&request, false);
    let call = rig_tool_call("new-1", "read_file", serde_json::json!({ "path": "b.md" }));
    exchange.push(Message::Assistant(rig::completion::message::AssistantMessage::new(vec![
        rig::completion::message::AssistantContent::ToolCall(call.clone()),
    ])));
    exchange.push(tool_result_message("new-1", "read_file", "NEW"));
    let compacted = compacted_request(&request, "THE SUMMARY", SECTION_MAX_CHARS);
    assert!(compacted.replay.is_empty());
    assert_eq!(compacted.input[0].tag, TAG_ARTIFACT);
    assert_eq!(compacted.stable_head_sections, 1);
    assert!(section_of(&compacted.input, TAG_DISCUSSION_HISTORY).body.contains("THE SUMMARY"));
    let out = compacted_exchange_after(&compacted, false, &exchange, opening_of(&request));
    assert_eq!(out.len(), 3, "the input, then the latest round");
    assert_eq!(tool_results(&out), vec!["NEW".to_string()]);
}

// ERP-FR-WZAV, CVL-FR-QJLU: a recovery cuts the results of the current turn
// alone and never a replayed one.
#[test]
fn a_recovery_never_cuts_a_replayed_result() {
    let request = replayed_request();
    let mut exchange = opening_exchange(&request, false);
    let opening = opening_of(&request);
    assert_eq!(cut_latest_results_after(&mut exchange, 100, opening), 0);
    assert!(tool_results(&exchange)[0].len() >= 5_000, "the replayed result is whole");
    let call = rig_tool_call("new-1", "read_file", serde_json::json!({ "path": "b.md" }));
    exchange.push(Message::Assistant(rig::completion::message::AssistantMessage::new(vec![
        rig::completion::message::AssistantContent::ToolCall(call.clone()),
    ])));
    exchange.push(tool_result_message("new-1", "read_file", "N".repeat(5_000)));
    assert_eq!(cut_latest_results_after(&mut exchange, 1_000, opening), 1);
    let results = tool_results(&exchange);
    assert!(
        results[0].len() >= 5_000 && results[1].len() < 5_000,
        "{:?}",
        results.iter().map(String::len).collect::<Vec<_>>()
    );
}

// AGC-FR-35, ERP-FR-ISTB: a picture of the material counts where a replay
// holds the material and the current input holds none, and the multimodal
// opening carries it.
#[test]
fn a_picture_in_a_replayed_opening_counts() {
    let mut request = replayed_request();
    assert_eq!(request_image_count(&request), 0);
    let image = InputPart::Image(InputImage {
        data: "AAAA".into(),
        media_type: "image/png".into(),
        context: "![d](d.png)".into(),
        metadata: "image/png d.png".into(),
    });
    request.replay[0].opening[0].parts = vec![InputPart::Text("MATERIAL-TEXT".into()), image];
    assert!(!input_has_images(&request.input), "the current input holds none");
    assert_eq!(request_image_count(&request), 1);
    let exchange = opening_exchange(&request, true);
    let Message::User { content } = &exchange[0] else { panic!("user") };
    assert!(content.iter().any(|p| matches!(p, rig::completion::message::UserContent::Image(_))));
}

// CVL-FR-39, ERP-FR-ISTB: with a replay, the stable head is the material of
// the first opening, and it is a prefix of the first message.
#[test]
fn the_stable_head_is_the_material_of_the_first_opening() {
    let request = replayed_request();
    let exchange = opening_exchange(&request, false);
    let head = stable_head_text(&request, &exchange).expect("a head");
    assert!(head.contains("MATERIAL-TEXT") && !head.contains("first?"), "{head}");
    assert!(text_of_first(&exchange).starts_with(&head));
}

fn text_of_first(exchange: &[Message]) -> String {
    crate::agent_conversations::compaction::message_text(&exchange[0])
}
