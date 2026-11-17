//! Recovery from a context overflow (CVL-FR-PFNV, CVL-FR-QJLU, CVL-FR-LQDM,
//! CVL-FR-LZDN). One part of `../tests/mod.rs`.

use super::support::{rig_tool_call, tool_result_message};
use super::*;
use crate::agent_conversations::overflow::{cut_latest_results, is_context_overflow};
use rig::completion::message::{ToolResultContent, UserContent};
use rig::completion::Message;

fn refused(status: Option<u16>, code: Option<&str>, message: Option<&str>) -> CallFailure {
    CallFailure {
        status,
        provider_code: code.map(str::to_string),
        provider_message: message.map(str::to_string),
        ..CallFailure::new(FAIL_INVALID_REQUEST, class::HTTP_STATUS)
    }
}

// CVL-FR-PFNV: a 400 or a 413 whose code or message holds a marker, without
// case, is a context overflow.
#[test]
fn a_refusal_with_a_marker_is_a_context_overflow() {
    for (status, code, message) in [
        (400, None, Some("prompt is too long: 250000 tokens > 200000 maximum")),
        (400, Some("context_length_exceeded"), None),
        (400, None, Some("This model's Maximum Context Length is 128000 tokens")),
        (413, Some("request_too_large"), None),
        (400, None, Some("Input is too long for the CONTEXT WINDOW of this model")),
        (400, None, Some("too many tokens in the request")),
    ] {
        assert!(
            is_context_overflow(&refused(Some(status), code, message)),
            "{status} {code:?} {message:?}"
        );
    }
}

// CVL-FR-PFNV: each marker alone makes an overflow, in the code or in the
// message, and in any case.
#[test]
fn each_marker_alone_is_an_overflow() {
    assert_eq!(
        crate::agent_conversations::overflow::OVERFLOW_MARKERS,
        [
            "context_length_exceeded",
            "prompt is too long",
            "maximum context length",
            "context window",
            "too many tokens",
            "input is too long",
            "request_too_large",
        ]
    );
    for marker in crate::agent_conversations::overflow::OVERFLOW_MARKERS {
        for status in [400, 413] {
            let upper = marker.to_uppercase();
            assert!(is_context_overflow(&refused(Some(status), Some(&upper), None)), "{marker}");
            let message = format!("The request failed: {marker}.");
            assert!(is_context_overflow(&refused(Some(status), None, Some(&message))), "{marker}");
        }
    }
}

// CVL-FR-PFNV: another status, a 400 without a marker, and a failure with no
// status are no context overflow.
#[test]
fn other_refusals_are_no_context_overflow() {
    assert!(!is_context_overflow(&refused(Some(400), Some("invalid_request"), Some("bad field"))));
    assert!(!is_context_overflow(&refused(Some(429), None, Some("too many tokens per minute"))));
    assert!(!is_context_overflow(&refused(Some(500), None, Some("prompt is too long"))));
    assert!(!is_context_overflow(&refused(None, None, Some("prompt is too long"))));
    assert!(!is_context_overflow(&refused(Some(400), None, None)));
}

fn tool_round(results: &[(&str, String)]) -> Vec<Message> {
    use rig::completion::message::AssistantContent;
    let mut exchange = vec![Message::user("input")];
    let calls = results
        .iter()
        .enumerate()
        .map(|(i, (name, _))| {
            AssistantContent::ToolCall(rig_tool_call(
                &format!("call-{i}"),
                name,
                serde_json::json!({}),
            ))
        })
        .collect::<Vec<_>>();
    exchange.push(Message::Assistant(rig::completion::AssistantMessage::new(calls)));
    for (i, (name, text)) in results.iter().enumerate() {
        exchange.push(tool_result_message(&format!("call-{i}"), name, text.clone()));
    }
    exchange
}

fn result_text(message: &Message) -> String {
    let Message::User { content } = message else { panic!("a user message") };
    content
        .iter()
        .filter_map(|part| match part {
            UserContent::ToolResult(r) => Some(
                r.content
                    .iter()
                    .filter_map(|c| match c {
                        ToolResultContent::Text(t) => Some(t.text.clone()),
                        _ => None,
                    })
                    .collect::<String>(),
            ),
            _ => None,
        })
        .collect()
}

// CVL-FR-QJLU: each result of the latest round above the bound is cut to it,
// `load_skill` excepted, and the count says how many were cut.
#[test]
fn the_latest_results_are_cut_and_a_skill_is_kept() {
    let mut exchange = tool_round(&[
        ("read_file", "f".repeat(5_000)),
        ("load_skill", "s".repeat(5_000)),
        ("search_notes", "n".repeat(50)),
    ]);
    assert_eq!(cut_latest_results(&mut exchange, 1_000), 1);
    let cut = result_text(&exchange[2]);
    assert!(cut.chars().count() <= 1_000);
    assert!(cut.starts_with(&"f".repeat(800)));
    assert!(cut.contains(" of 5000 characters. Read the rest in parts with offset and limit.]"));
    assert_eq!(result_text(&exchange[3]), "s".repeat(5_000));
    assert_eq!(result_text(&exchange[4]), "n".repeat(50));
}

// CVL-FR-LQDM: an exchange with nothing to cut changes nothing.
#[test]
fn nothing_above_the_bound_cuts_nothing() {
    let mut exchange = tool_round(&[("read_file", "f".repeat(50))]);
    let before = exchange.clone();
    assert_eq!(cut_latest_results(&mut exchange, 100), 0);
    assert_eq!(exchange, before);
}
