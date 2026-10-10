//! What a tool result carries of the call it answers
//! (`CVL-conversation-loop.md` CVL-FR-EKHH).
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

/// The call id and the tool name of every tool result in an exchange, in
/// order.
fn result_ids(exchange: &[rig::completion::Message]) -> Vec<(String, String)> {
    exchange
        .iter()
        .flat_map(|message| match message {
            rig::completion::Message::User { content } => content.iter().cloned().collect(),
            _ => Vec::new(),
        })
        .filter_map(|part| match part {
            rig::completion::message::UserContent::ToolResult(result) => {
                Some((result.call.to_string(), result.name.to_string()))
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
// one too many, and a dispatched call each give a result with the id of its
// own call, unchanged, and the name of its tool.
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
            ("call_x-0".to_string(), crate::tools::ask_user_comment::NAME.to_string()),
            ("call_x-1".to_string(), crate::tools::ask_user_comment::NAME.to_string()),
            ("call_x-2".to_string(), crate::tools::skill_list::NAME.to_string()),
        ],
    );
    assert_eq!(tool_results(&exchanges[1]).len(), 3);
}

// CVL-FR-EKHH: whatever id the provider gave a call, its result carries that
// id unchanged.
#[test]
fn a_result_carries_whatever_id_the_provider_gave_its_call() {
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
            ("call-0".to_string(), crate::tools::ask_user_comment::NAME.to_string()),
            ("call-1".to_string(), crate::tools::ask_user_comment::NAME.to_string()),
            ("call-2".to_string(), crate::tools::skill_list::NAME.to_string()),
        ],
    );
}

// CVL-FR-EKHH: the exchange after a tool call builds a request on the OpenAI
// Responses route, and each `function_call_output` names the `call_id` of its
// `function_call`.
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
    let wire = responses_body(built);
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

/// The JSON body the OpenAI Responses route sends for `built`, encoded by the
/// framework's own wire.
pub(super) fn responses_body(built: rig::completion::CompletionRequest) -> serde_json::Value {
    use rig::wire::Wire;
    let wire = rig::providers::openai::responses_api::wire::Responses::new(
        rig::providers::openai::OpenAIConfig::new("not-a-real-key"),
        "model",
    );
    let encoded = wire
        .encode(built, rig::wire::Mode::Unary)
        .expect("the Responses route builds the request");
    match encoded.request.body() {
        rig::wire::Body::Bytes(bytes) => {
            serde_json::from_slice(bytes).expect("the request body is JSON")
        }
        rig::wire::Body::Multipart(_) => panic!("a completion body is JSON"),
    }
}
