//! Replies and refusals read off a real wire, through the production seam to a
//! loopback server (`CVL-conversation-loop.md` CVL-FR-UALC, CVL-FR-EKHH,
//! CVL-FR-ETAH, CVL-FR-35, CVL-FR-HBNW).
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::responses_repair::call;
use super::*;
use crate::ai_shared::ModelMode;

/// The JSON body a wire sends for `built`.
fn body_of(encoded: rig::wire::Encoded) -> serde_json::Value {
    match encoded.request.body() {
        rig::wire::Body::Bytes(bytes) => serde_json::from_slice(bytes).expect("the body is JSON"),
        rig::wire::Body::Multipart(_) => panic!("a completion body is JSON"),
    }
}

/// The Messages wire the Anthropic carrier sends `model` through.
fn anthropic_wire(model: &str) -> rig::providers::anthropic::Messages {
    let client = rig::providers::anthropic::AnthropicConfig::new("not-a-real-key")
        .connect(rig_reqwest::ReqwestClient::from(reqwest013::Client::new()));
    let endpoint = AiApiCall {
        turn_timeout_ms: None,
        provider: "anthropic".into(),
        base_url: "https://example.invalid".into(),
        api_key: None,
        model_id: Some(model.into()),
        reasoning: None,
        accepts_image_input: false,
        model_mode: None,
    };
    anthropic_model(&client, &endpoint).wire
}

/// The exchange of the round after `reply`: the input, the reply, and one
/// result for each call it asked for.
fn next_exchange(reply: &ModelReply) -> Vec<rig::completion::Message> {
    let request = wire_request();
    let mut exchange = opening_exchange(&request, false);
    exchange.push(assistant_message(reply));
    for call in &reply.tool_calls {
        exchange.push(rig::completion::Message::tool_result(
            call.id.clone(),
            call.function.name.clone(),
            "RESULT",
        ));
    }
    exchange
}

// CVL-FR-UALC: a Chat Completions gateway that states no finish reason keeps
// its answer. That is no refusal, no filtered content, and no failure the
// provider reported.
#[test]
fn a_reply_that_states_no_finish_reason_keeps_its_answer() {
    let body = serde_json::json!({
        "id": "chat_1",
        "object": "chat.completion",
        "created": 0,
        "model": "claude-sonnet-5-5",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "The answer." },
            "finish_reason": null
        }],
    })
    .to_string();
    let (seen, outcome) = call("custom", Some(ModelMode::Chat), "http://{addr}", body);
    assert!(seen.starts_with("POST /v1/chat/completions "), "{seen}");
    let reply = outcome.expect("the reply is read");
    assert_eq!(reply.text, "The answer.");
    assert_eq!(reply.finish_reason, None);
}

// CVL-FR-EKHH, CVL-FR-ETAH: on the Responses route the id a result carries is
// the call's `call_id`, not its item id, and the next request presents the
// call with both, matched by its result's `call_id`.
#[test]
fn a_responses_tool_call_is_answered_by_its_call_id() {
    let body = serde_json::json!({
        "id": "resp_1",
        "object": "response",
        "created_at": 0,
        "status": "completed",
        "model": "claude-sonnet-5-5",
        "output": [{
            "type": "function_call",
            "id": "fc_1",
            "call_id": "call_1",
            "name": "list_skills",
            "arguments": "{}",
            "status": "completed"
        }],
        "tools": [],
        "text": { "format": { "type": "text" } },
    })
    .to_string();
    let (_, outcome) = call("openai", Some(ModelMode::Responses), "http://{addr}/v1", body);
    let reply = outcome.expect("the reply is read");
    assert_eq!(reply.tool_calls.len(), 1);
    assert_eq!(reply.tool_calls[0].id.to_string(), "call_1");

    let built = rig_seam::build_completion_request(
        &wire_request(),
        &next_exchange(&reply),
        &any_endpoint(),
    );
    let wire = super::tool_result_call_ids::responses_body(built);
    let items = wire["input"].as_array().expect("input items");
    let call_item = items
        .iter()
        .find(|item| item["type"] == "function_call")
        .expect("the call is presented again");
    assert_eq!(call_item["call_id"], "call_1", "{wire}");
    assert_eq!(call_item["id"], "fc_1", "the provider's item id is kept: {wire}");
    let output = items
        .iter()
        .find(|item| item["type"] == "function_call_output")
        .expect("the result is presented");
    assert_eq!(output["call_id"], "call_1", "{wire}");
}

// CVL-FR-ETAH, CVL-FR-HBNW: an Anthropic reply that thought before it asked
// for a tool goes back with its thinking and its signature unchanged, and the
// tool result answers the provider's own call id.
#[test]
fn an_anthropic_reply_goes_back_with_its_signed_thinking() {
    let body = serde_json::json!({
        "id": "msg_1",
        "type": "message",
        "role": "assistant",
        "model": "claude-sonnet-5-5",
        "content": [
            { "type": "thinking", "thinking": "List the skills first.", "signature": "sig_abc" },
            { "type": "tool_use", "id": "toolu_1", "name": "list_skills", "input": {} }
        ],
        "stop_reason": "tool_use",
        "stop_sequence": null,
        "usage": { "input_tokens": 10, "output_tokens": 5 }
    })
    .to_string();
    let (seen, outcome) = call("anthropic", None, "http://{addr}/v1", body);
    assert!(seen.starts_with("POST /v1/messages "), "{seen}");
    let reply = outcome.expect("the reply is read");
    assert_eq!(reply.tool_calls.len(), 1);
    assert_eq!(reply.tool_calls[0].id.to_string(), "toolu_1");

    use rig::wire::Wire;
    let built = rig_seam::build_completion_request(
        &wire_request(),
        &next_exchange(&reply),
        &any_endpoint(),
    );
    let wire = anthropic_wire("claude-sonnet-5-5");
    let json = body_of(
        wire.encode(built, rig::wire::Mode::Unary)
            .expect("the Messages route builds the request"),
    );
    let messages = json["messages"].as_array().expect("messages");
    let assistant = messages
        .iter()
        .find(|message| message["role"] == "assistant")
        .expect("the reply is presented again");
    let content = assistant["content"].as_array().expect("content blocks");
    assert_eq!(
        content[0],
        serde_json::json!({
            "type": "thinking",
            "thinking": "List the skills first.",
            "signature": "sig_abc"
        }),
        "{json}",
    );
    assert_eq!(content[1]["type"], "tool_use", "{json}");
    assert_eq!(content[1]["id"], "toolu_1", "{json}");
    let answered = messages
        .iter()
        .filter(|message| message["role"] == "user")
        .flat_map(|message| message["content"].as_array().cloned().unwrap_or_default())
        .find(|block| block["type"] == "tool_result")
        .expect("the result is presented");
    assert_eq!(answered["tool_use_id"], "toolu_1", "{json}");
}

// CVL-FR-35, CVL-FR-21: an OpenAI-compatible refusal is recorded in the
// provider's own terms — its status, its own code, and its own message.
#[test]
fn an_openai_refusal_is_recorded_in_the_providers_own_terms() {
    let body = serde_json::json!({
        "error": {
            "message": "This model's maximum context length is exceeded.",
            "type": "invalid_request_error",
            "code": "context_length_exceeded"
        }
    })
    .to_string();
    let server = super::responses_repair::answer_once("400 Bad Request", body);
    let endpoint = AiApiCall {
        turn_timeout_ms: None,
        provider: "openai".into(),
        base_url: format!("http://{}/v1", server.addr),
        api_key: Some("not-a-real-key".into()),
        model_id: Some("claude-sonnet-5-5".into()),
        reasoning: None,
        accepts_image_input: false,
        model_mode: None,
    };
    let request = wire_request();
    let exchange = opening_exchange(&request, false);
    let failure = RigCompletion
        .complete(&request, &exchange, &endpoint, Duration::from_secs(20))
        .expect_err("the provider refuses");
    let _ = server.request_line();
    assert_eq!(failure.status, Some(400));
    assert_eq!(failure.class, class::HTTP_STATUS);
    assert_eq!(failure.provider_code.as_deref(), Some("context_length_exceeded"));
    assert_eq!(
        failure.provider_message.as_deref(),
        Some("This model's maximum context length is exceeded."),
    );
    assert!(!format!("{failure:?}").contains("not-a-real-key"));
}

// CVL-FR-HBNW: a model the framework's catalog does not mark for adaptive
// thinking is sent no thinking at all.
#[test]
fn a_model_the_catalog_does_not_mark_is_sent_no_thinking() {
    use rig::wire::Wire;
    let request = wire_request();
    let endpoint = AiApiCall {
        turn_timeout_ms: None,
        provider: "anthropic".into(),
        base_url: "https://example.invalid".into(),
        api_key: None,
        model_id: Some("claude-not-in-any-catalog".into()),
        reasoning: None,
        accepts_image_input: false,
        model_mode: None,
    };
    let built = anthropic_request(build_completion_request(
        &request,
        &opening_exchange(&request, false),
        &endpoint,
    ));
    let wire = anthropic_wire("claude-not-in-any-catalog");
    let json = body_of(
        wire.encode(built, rig::wire::Mode::Unary)
            .expect("the Messages route builds the request"),
    );
    assert!(json.get("thinking").is_none(), "{json}");
    assert_eq!(json["max_tokens"], 32_000);
}
