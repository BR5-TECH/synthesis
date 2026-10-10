//! What a tool result carries of the call it answers
//! (`CVL-conversation-loop.md` CVL-FR-EKHH).
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

/// The `id` and the `call_id` of every tool result in an exchange, in order.
fn result_ids(exchange: &[rig::completion::Message]) -> Vec<(String, Option<String>)> {
    exchange
        .iter()
        .flat_map(|message| match message {
            rig::completion::Message::User { content } => content.iter().cloned().collect(),
            _ => Vec::new(),
        })
        .filter_map(|part| match part {
            rig::completion::message::UserContent::ToolResult(result) => {
                Some((result.id, result.call_id))
            }
            _ => None,
        })
        .collect()
}

/// One reply that reaches each place the loop appends a result: a blank
/// question is refused, so its result is appended and the loop goes on; a
/// second question is told that only one is carried out; and an ordinary tool
/// is dispatched.
fn reply_reaching_every_result() -> ScriptedReply {
    asks("   ")
        .and_calls(
            crate::tools::ask_user_comment::NAME,
            serde_json::json!({ "question": "And this one?" }),
        )
        .and_calls(crate::tools::skill_list::NAME, serde_json::json!({}))
}

// CVL-FR-EKHH, CVL-FR-12, CVL-FR-15: a refused ending call, a call refused as
// one too many, and a dispatched call each give a result with the id and the
// `call_id` of its own call.
#[test]
fn every_tool_result_carries_the_call_id_of_its_call() {
    let h = Harness::scripted(vec![
        Ok(reply_reaching_every_result().with_call_ids("call_x-")),
        Ok(ScriptedReply::answer("Done.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges.len(), 2, "the results went back to the model");
    assert_eq!(
        result_ids(&exchanges[1]),
        vec![
            ("call-0".to_string(), Some("call_x-0".to_string())),
            ("call-1".to_string(), Some("call_x-1".to_string())),
            ("call-2".to_string(), Some("call_x-2".to_string())),
        ],
    );
    assert_eq!(tool_results(&exchanges[1]).len(), 3);
}

// CVL-FR-EKHH: a call that has no `call_id` gives its result none, so the
// requests of a provider that gives none do not change.
#[test]
fn a_call_without_a_call_id_gives_its_result_none() {
    let h = Harness::scripted(vec![
        Ok(reply_reaching_every_result()),
        Ok(ScriptedReply::answer("Done.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    assert_eq!(
        result_ids(&h.seam.exchanges()[1]),
        vec![
            ("call-0".to_string(), None),
            ("call-1".to_string(), None),
            ("call-2".to_string(), None),
        ],
    );
}

// CVL-FR-EKHH: the exchange after a tool call builds a request on the OpenAI
// Responses route, and each `function_call_output` names the `call_id` of its
// `function_call`. Without the `call_id`, the framework refuses to build it.
#[test]
fn the_exchange_after_a_tool_call_builds_a_responses_request() {
    let h = Harness::scripted(vec![
        Ok(reply_reaching_every_result().with_call_ids("call_x-")),
        Ok(ScriptedReply::answer("Done.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let requests = h.seam.requests();
    let (request, endpoint) = &requests[1];
    let exchange = &h.seam.exchanges()[1];
    let built = rig_seam::build_completion_request(request, exchange, endpoint);
    let converted = rig::providers::openai::responses_api::CompletionRequest::try_from((
        "model".to_string(),
        built,
    ))
    .expect("the Responses route builds the request");

    let wire = serde_json::to_value(&converted).expect("the request serializes");
    let items = wire["input"].as_array().expect("the request has input items");
    let of_type = |kind: &str| -> Vec<&serde_json::Value> {
        items.iter().filter(|item| item["type"] == kind).collect()
    };
    let call_ids = |kind: &str| -> Vec<String> {
        of_type(kind)
            .iter()
            .map(|item| item["call_id"].as_str().unwrap_or_default().to_string())
            .collect()
    };
    let expected = vec!["call_x-0", "call_x-1", "call_x-2"];
    assert_eq!(call_ids("function_call"), expected, "{wire}");
    assert_eq!(call_ids("function_call_output"), expected, "{wire}");
}
