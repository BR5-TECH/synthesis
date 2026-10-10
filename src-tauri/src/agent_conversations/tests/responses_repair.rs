//! The Custom gateway's tolerances for `text.format` and for `output_text`
//! parts without text (`CVL-conversation-loop.md` CVL-FR-TQRD), and the typed
//! classification of framework errors (CVL-FR-21, CVL-FR-18).
//!
//! The unit tests assert the repair on bodies alone. The wire tests make one
//! call through the production seam to a loopback server and observe what the
//! loop receives.

use super::*;
use crate::ai_shared::ModelMode;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::AtomicBool;

use super::super::responses_repair::{is_responses_route, repair_reply, ReplyRepairs};

fn repaired(body: serde_json::Value) -> Option<serde_json::Value> {
    repaired_with(body).map(|(fixed, _)| fixed)
}

fn repaired_with(body: serde_json::Value) -> Option<(serde_json::Value, ReplyRepairs)> {
    let bytes = serde_json::to_vec(&body).unwrap();
    repair_reply(&bytes).map(|(fixed, repairs)| (serde_json::from_slice(&fixed).unwrap(), repairs))
}

// CVL-FR-TQRD: a `text` object with no `format` is read as plain text, and the
// rest of the body is kept as it was.
#[test]
fn a_text_object_without_a_format_is_read_as_plain_text() {
    let fixed = repaired(serde_json::json!({
        "id": "resp_1",
        "text": {},
        "output": [{ "type": "message" }],
    }))
    .expect("the body is repaired");
    assert_eq!(
        fixed,
        serde_json::json!({
            "id": "resp_1",
            "text": { "format": { "type": "text" } },
            "output": [{ "type": "message" }],
        })
    );
}

// CVL-FR-TQRD: a null `format` is repaired the same way, and the other keys of
// the `text` object stay.
#[test]
fn a_null_format_is_read_as_plain_text() {
    let fixed = repaired(serde_json::json!({ "text": { "format": null, "verbosity": "low" } }))
        .expect("the body is repaired");
    assert_eq!(
        fixed,
        serde_json::json!({ "text": { "format": { "type": "text" }, "verbosity": "low" } })
    );
}

// CVL-FR-TQRD: every body the requirement does not name passes unchanged. The
// repair gives `None`, so the caller sends the original bytes.
#[test]
fn every_other_body_is_left_as_it_is() {
    let untouched = [
        br#"{"text":{"format":{"type":"text"}}}"#.to_vec(),
        br#"{"text":{"format":{"type":"json_schema","name":"n","schema":{}}}}"#.to_vec(),
        br#"{"id":"resp_1","output":[]}"#.to_vec(),
        br#"{"text":null}"#.to_vec(),
        br#"{"text":"plain"}"#.to_vec(),
        br#"[{"text":{}}]"#.to_vec(),
        br#"[{"type":"message","content":[{"type":"output_text","text":null}]}]"#.to_vec(),
        br#"{"output":[{"type":"message","content":[{"type":"output_text","text":"a"},{"type":"output_text","text":""},{"type":"refusal","refusal":"no"}]}]}"#.to_vec(),
        br#"{"output":{"type":"message","content":[{"type":"output_text","text":null}]}}"#.to_vec(),
        // Both tolerances read the top level alone, not a nested object.
        br#"{"response":{"text":{},"output":[{"type":"message","content":[{"type":"output_text","text":null}]}]}}"#.to_vec(),
        // Only a null or absent `text` is removed, not any other non-string.
        br#"{"output":[{"type":"message","content":[{"type":"output_text","text":5},{"type":"output_text","text":{"value":"x"}}]}]}"#.to_vec(),
        b"not json at all".to_vec(),
        Vec::new(),
    ];
    for body in untouched {
        assert!(
            repair_reply(&body).is_none(),
            "{}",
            String::from_utf8_lossy(&body)
        );
    }
}

// CVL-FR-TQRD: an `output_text` part whose `text` is null or absent is removed
// from its `message` item. The item stays, also with no parts, and every other
// part keeps its place.
#[test]
fn an_output_text_part_without_text_is_removed() {
    let (fixed, repairs) = repaired_with(serde_json::json!({
        "text": { "format": { "type": "text" } },
        "output": [
            {
                "type": "message",
                "id": "msg_1",
                "content": [{ "type": "output_text", "text": null, "annotations": [] }]
            },
            {
                "type": "message",
                "id": "msg_2",
                "content": [
                    { "type": "output_text", "text": "Kept." },
                    { "type": "output_text", "annotations": [] },
                    { "type": "refusal", "refusal": "Also kept." },
                    { "type": "output_text", "text": "" }
                ]
            }
        ],
    }))
    .expect("the body is repaired");
    assert_eq!(
        fixed,
        serde_json::json!({
            "text": { "format": { "type": "text" } },
            "output": [
                { "type": "message", "id": "msg_1", "content": [] },
                {
                    "type": "message",
                    "id": "msg_2",
                    "content": [
                        { "type": "output_text", "text": "Kept." },
                        { "type": "refusal", "refusal": "Also kept." },
                        { "type": "output_text", "text": "" }
                    ]
                }
            ],
        })
    );
    assert_eq!(repairs, ReplyRepairs { text_format: false, null_text_parts: 2 });
}

// CVL-FR-TQRD: only `message` items lose parts. An item of another type keeps
// a part without text, and so does a part of another type.
#[test]
fn only_output_text_parts_of_message_items_are_removed() {
    let other_items = serde_json::json!({
        "text": { "format": { "type": "text" } },
        "output": [
            { "type": "function_call", "id": "fc_1", "call_id": "c", "name": "n", "arguments": "{}", "text": null },
            { "type": "reasoning", "id": "rs_1", "summary": [], "content": [{ "type": "output_text", "text": null }] },
            { "type": "web_search_call", "content": [{ "type": "output_text", "text": null }] },
            { "type": "message", "content": [{ "type": "summary_text", "text": null }] },
            { "type": "message" }
        ],
    });
    assert_eq!(repaired(other_items), None);
}

// CVL-FR-TQRD: one body can need both tolerances, and both are reported.
#[test]
fn both_tolerances_apply_to_one_body() {
    let (fixed, repairs) = repaired_with(serde_json::json!({
        "text": {},
        "output": [{ "type": "message", "content": [{ "type": "output_text", "text": null }] }],
    }))
    .expect("the body is repaired");
    assert_eq!(
        fixed,
        serde_json::json!({
            "text": { "format": { "type": "text" } },
            "output": [{ "type": "message", "content": [] }],
        })
    );
    assert_eq!(repairs, ReplyRepairs { text_format: true, null_text_parts: 1 });
    assert!(repairs.any());
    assert!(!ReplyRepairs::default().any());
}

// CVL-FR-TQRD: the route check names the Responses path alone.
#[test]
fn the_route_check_names_the_responses_path_alone() {
    let request = |uri: &str| {
        rig::http_client::Request::builder()
            .uri(uri)
            .body(Vec::<u8>::new())
            .unwrap()
    };
    assert!(is_responses_route(&request("https://gw.example/v1/responses")));
    assert!(is_responses_route(&request("https://gw.example/v1/responses/")));
    assert!(!is_responses_route(&request("https://gw.example/v1/chat/completions")));
    assert!(!is_responses_route(&request("https://gw.example/v1/models")));
    assert!(!is_responses_route(&request("https://gw.example/v1/responses/x")));
}

// CVL-FR-TQRD, CVL-FR-ZPGW: the adapter repairs the replies of the Custom
// gateway and of no other provider.
#[test]
fn only_the_custom_gateway_is_configured_to_repair() {
    let endpoint = |provider: &str| AiApiCall {
        turn_timeout_ms: None,
        provider: provider.into(),
        base_url: "https://gw.example.com".into(),
        api_key: None,
        model_id: Some("m".into()),
        reasoning: None,
        accepts_image_input: false,
        model_mode: Some(ModelMode::Responses),
    };
    assert!(openai_adapter_config(&endpoint("custom")).repair_replies);
    assert!(!openai_adapter_config(&endpoint("openai")).repair_replies);
}

// CVL-FR-21: a body the framework could not decode is `decode`, which is
// `invalid_response`, and its text does not reach the failure.
#[test]
fn a_body_the_framework_could_not_decode_is_an_invalid_response() {
    let json = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
    let failure =
        rig_seam::classify_completion_error(&rig::ProviderError::Json(std::sync::Arc::new(json)));
    assert_eq!(failure.failure, FAIL_INVALID_RESPONSE);
    assert_eq!(failure.class, class::DECODE);
    assert_eq!(failure.status, None);
    assert_eq!(failure.provider_message, None);
}

// CVL-FR-21: a reply that decoded but does not answer the request is
// `decode`, which is `invalid_response`, and its text does not reach the
// failure.
#[test]
fn a_reply_that_does_not_answer_the_request_is_an_invalid_response() {
    let error = rig::ProviderError::Response("connection refused: HTTP 503".into());
    let failure = rig_seam::classify_completion_error(&error);
    assert_eq!(failure.failure, FAIL_INVALID_RESPONSE);
    assert_eq!(failure.class, class::DECODE);
    assert_eq!(failure.status, None);
    assert_eq!(failure.provider_message, None);
}

// CVL-FR-21: an error whose kind does not settle its class still goes to the
// text classification.
#[test]
fn an_error_of_another_kind_is_classified_from_its_text() {
    let other = rig::ProviderError::Provider("something else".into());
    assert_eq!(rig_seam::classify_completion_error(&other).failure, FAIL_UNREACHABLE);
    let refused = rig::ProviderError::Provider("HTTP 503 unavailable".into());
    let failure = rig_seam::classify_completion_error(&refused);
    assert_eq!(failure.class, class::HTTP_STATUS);
    assert_eq!(failure.status, Some(503));
}

// CVL-FR-18, AGC-FR-15: `invalid_response` is recoverable by the author and
// is never repeated by the loop itself.
#[test]
fn an_invalid_response_is_offered_again_but_not_repeated() {
    let failure = CallFailure::new(FAIL_INVALID_RESPONSE, class::DECODE);
    assert!(failure.recoverable());
    assert!(!failure.repeatable());
    assert!(is_recoverable(FAIL_INVALID_RESPONSE));
    assert!(!is_repeated_automatically(FAIL_INVALID_RESPONSE));
}

/// One loopback server that answers exactly one HTTP request.
pub(super) struct OneShot {
    pub(super) addr: SocketAddr,
    accepted: Arc<AtomicBool>,
    handle: std::thread::JoinHandle<String>,
}

impl OneShot {
    /// The request line the server received. A call refused before it is sent
    /// leaves the server waiting in `accept`, and an empty connection releases
    /// it, so that case fails in the test and does not hang. The connection is
    /// made only while the server still waits, so it cannot reach a listener
    /// another test bound to the same port after this one closed.
    pub(super) fn request_line(self) -> String {
        if !self.accepted.load(Ordering::SeqCst) {
            drop(TcpStream::connect(self.addr));
        }
        self.handle.join().unwrap()
    }
}

/// Serve exactly one HTTP request on loopback with the given status and body,
/// and hand back the request line. A connection that sends nothing gives an
/// empty string, which is how a call that never reached the server shows.
pub(super) fn answer_once(status: &'static str, body: String) -> OneShot {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicBool::new(false));
    let flag = accepted.clone();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        flag.store(true, Ordering::SeqCst);
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut received = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let n = stream.read(&mut buffer).unwrap_or(0);
            if n == 0 {
                break;
            }
            received.extend_from_slice(&buffer[..n]);
            let Some(end) = received.windows(4).position(|w| w == b"\r\n\r\n") else {
                continue;
            };
            let head = String::from_utf8_lossy(&received[..end]).to_lowercase();
            let length = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if received.len() >= end + 4 + length {
                break;
            }
        }
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        String::from_utf8_lossy(&received)
            .lines()
            .next()
            .unwrap_or_default()
            .to_string()
    });
    OneShot { addr, accepted, handle }
}

/// A Responses reply with one message and the `text` object the gateway sent.
fn responses_reply(text: serde_json::Value) -> String {
    serde_json::json!({
        "id": "resp_1",
        "object": "response",
        "created_at": 0,
        "status": "completed",
        "model": "claude-sonnet-5-5",
        "output": [{
            "type": "message",
            "id": "msg_1",
            "status": "completed",
            "role": "assistant",
            "content": [{ "type": "output_text", "annotations": [], "text": "The answer." }]
        }],
        "tools": [],
        "text": text,
    })
    .to_string()
}

pub(super) fn call(
    provider: &str,
    mode: Option<ModelMode>,
    base_url: &str,
    body: String,
) -> (String, Result<ModelReply, CallFailure>) {
    let server = answer_once("200 OK", body);
    let endpoint = AiApiCall {
        turn_timeout_ms: None,
        provider: provider.into(),
        base_url: base_url.replace("{addr}", &server.addr.to_string()),
        api_key: Some("not-a-real-key".into()),
        model_id: Some("claude-sonnet-5-5".into()),
        reasoning: None,
        accepts_image_input: false,
        model_mode: mode,
    };
    let request = wire_request();
    let exchange = opening_exchange(&request, false);
    let outcome = RigCompletion.complete(&request, &exchange, &endpoint, Duration::from_secs(20));
    (server.request_line(), outcome)
}

const RESPONSES: Option<ModelMode> = Some(ModelMode::Responses);

// CVL-FR-TQRD, CVL-FR-ZPGW: a Custom gateway reply with `"text": {}` delivers
// the answer of the model, and the loop is told that it was repaired.
#[test]
fn a_custom_gateway_reply_with_an_empty_text_object_delivers_its_answer() {
    let (seen, outcome) = call(
        "custom",
        RESPONSES,
        "http://{addr}",
        responses_reply(serde_json::json!({})),
    );
    assert!(seen.starts_with("POST /v1/responses "), "{seen}");
    let reply = outcome.expect("the repaired reply is read");
    assert_eq!(reply.text, "The answer.");
    assert_eq!(reply.reply_repairs, ReplyRepairs { text_format: true, null_text_parts: 0 });
}

// CVL-FR-TQRD: a Custom gateway reply that already has a format is read as it
// is, and nothing is recorded as repaired.
#[test]
fn a_custom_gateway_reply_with_a_format_is_not_repaired() {
    let (_, outcome) = call(
        "custom",
        RESPONSES,
        "http://{addr}",
        responses_reply(serde_json::json!({ "format": { "type": "text" } })),
    );
    let reply = outcome.expect("the reply is read");
    assert_eq!(reply.text, "The answer.");
    assert!(!reply.reply_repairs.any());
}

// CVL-FR-TQRD: no other provider receives the tolerance. The same body on
// `openai` reaches the framework as the provider sent it, and the framework
// reads it on its own terms.
#[test]
fn the_same_reply_on_openai_gets_no_repair() {
    let (seen, outcome) = call(
        "openai",
        RESPONSES,
        "http://{addr}/v1",
        responses_reply(serde_json::json!({})),
    );
    assert!(seen.starts_with("POST /v1/responses "), "{seen}");
    let reply = outcome.expect("the framework reads the reply");
    assert_eq!(reply.text, "The answer.");
    assert!(!reply.reply_repairs.any(), "openai gets no repair");
}

// CVL-FR-21: a Custom gateway reply that is not a Responses body at all is an
// invalid response, and the repair does not hide it.
#[test]
fn a_custom_gateway_reply_that_cannot_be_read_is_an_invalid_response() {
    let (_, outcome) = call("custom", RESPONSES, "http://{addr}", r#"{"unexpected":true}"#.into());
    let failure = outcome.expect_err("the body cannot be decoded");
    assert_eq!(failure.failure, FAIL_INVALID_RESPONSE);
    assert_eq!(failure.class, class::DECODE);
    // CVL-FR-21: the kind settles the class before any text is read, so a
    // number in the decoder's message is not taken for a status, and no text
    // of it reaches the failure.
    assert_eq!(failure.status, None);
    assert_eq!(failure.provider_message, None);
}

// CVL-FR-TQRD: an unrestricted Custom model takes the Responses route, and its
// reply is repaired there as well.
#[test]
fn an_unrestricted_custom_model_is_repaired_on_the_responses_route() {
    let (seen, outcome) = call("custom", None, "http://{addr}", responses_reply(serde_json::json!({})));
    assert!(seen.starts_with("POST /v1/responses "), "{seen}");
    let reply = outcome.expect("the repaired reply is read");
    assert_eq!(reply.text, "The answer.");
    assert!(reply.reply_repairs.text_format);
}

// CVL-FR-TQRD: a Chat Completions reply of the Custom gateway passes unchanged,
// even when it carries a top-level `text` object and a part without text.
#[test]
fn a_chat_completions_reply_is_not_repaired() {
    let body = serde_json::json!({
        "id": "chat_1",
        "object": "chat.completion",
        "created": 0,
        "model": "claude-sonnet-5-5",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "The answer." },
            "finish_reason": "stop"
        }],
        "text": {},
        "output": [{ "type": "message", "content": [{ "type": "output_text", "text": null }] }],
    })
    .to_string();
    let (seen, outcome) = call("custom", Some(ModelMode::Chat), "http://{addr}", body);
    assert!(seen.starts_with("POST /v1/chat/completions "), "{seen}");
    let reply = outcome.expect("the chat reply is read");
    assert_eq!(reply.text, "The answer.");
    assert!(!reply.reply_repairs.any());
}

/// The reply the gateway sent: a message whose one part has `"text": null`,
/// optionally beside a valid tool call, and `"text": {}`.
fn null_text_reply(with_tool_call: bool) -> String {
    let mut output = vec![serde_json::json!({
        "type": "message",
        "id": "msg_example",
        "status": "completed",
        "role": "assistant",
        "content": [{ "type": "output_text", "text": null, "annotations": [] }]
    })];
    if with_tool_call {
        output.push(serde_json::json!({
            "type": "function_call",
            "id": "call_example",
            "call_id": "call_example",
            "name": "example_tool",
            "arguments": "{\"id\": \"example\"}",
            "status": "completed"
        }));
    }
    serde_json::json!({
        "id": "resp_example",
        "object": "response",
        "created_at": 1700000000,
        "status": "completed",
        "error": null,
        "incomplete_details": null,
        "instructions": null,
        "model": "example-model",
        "output": output,
        "tools": [],
        "text": {},
        "usage": {
            "input_tokens": 10,
            "input_tokens_details": { "cached_tokens": 0 },
            "output_tokens": 5,
            "output_tokens_details": { "reasoning_tokens": 0 },
            "total_tokens": 15
        }
    })
    .to_string()
}

// CVL-FR-TQRD: the Custom gateway reply with a null-text part beside a tool
// call delivers the tool call, and the loop is told what was repaired.
#[test]
fn a_custom_gateway_reply_with_a_null_text_part_delivers_its_tool_call() {
    let (seen, outcome) = call("custom", RESPONSES, "http://{addr}", null_text_reply(true));
    assert!(seen.starts_with("POST /v1/responses "), "{seen}");
    let reply = outcome.expect("the repaired reply is read");
    assert_eq!(reply.text, "");
    assert_eq!(reply.tool_calls.len(), 1);
    let tool_call = &reply.tool_calls[0];
    assert_eq!(tool_call.function.name, "example_tool");
    assert_eq!(tool_call.id.to_string(), "call_example");
    assert_eq!(tool_call.function.arguments_value(), serde_json::json!({ "id": "example" }));
    assert_eq!(reply.reply_repairs, ReplyRepairs { text_format: true, null_text_parts: 1 });
}

// CVL-FR-TQRD: a part with text beside a part without text keeps its text. The
// body has a format, so the second tolerance is the only one applied.
#[test]
fn a_custom_gateway_message_keeps_the_parts_that_have_text() {
    let mut body: serde_json::Value = serde_json::from_str(&null_text_reply(false)).unwrap();
    body["text"] = serde_json::json!({ "format": { "type": "text" } });
    body["output"][0]["content"] = serde_json::json!([
        { "type": "output_text", "text": "The answer.", "annotations": [] },
        { "type": "output_text", "text": null, "annotations": [] }
    ]);
    let (_, outcome) = call("custom", RESPONSES, "http://{addr}", body.to_string());
    let reply = outcome.expect("the repaired reply is read");
    assert_eq!(reply.text, "The answer.");
    assert_eq!(reply.reply_repairs, ReplyRepairs { text_format: false, null_text_parts: 1 });
}

// CVL-FR-TQRD, CVL-FR-UALC: a reply that has nothing left after the repair
// holds no message and no tool call, which the loop takes as `empty_reply`,
// and the reply still tells the loop what was repaired.
#[test]
fn a_custom_gateway_reply_with_only_a_null_text_part_has_nothing_to_deliver() {
    let (_, outcome) = call("custom", RESPONSES, "http://{addr}", null_text_reply(false));
    let reply = outcome.expect("the repaired reply is read");
    assert_eq!(reply.text, "");
    assert!(reply.tool_calls.is_empty(), "nothing is left to deliver");
    assert_eq!(reply.reply_repairs, ReplyRepairs { text_format: true, null_text_parts: 1 });
}

// CVL-FR-TQRD: the same reply on `openai` gets no tolerance. The body has a
// format, so only the part without text differs from an ordinary reply.
#[test]
fn the_null_text_reply_on_openai_gets_no_repair() {
    let mut body: serde_json::Value = serde_json::from_str(&null_text_reply(true)).unwrap();
    body["text"] = serde_json::json!({ "format": { "type": "text" } });
    let (_, outcome) = call("openai", RESPONSES, "http://{addr}/v1", body.to_string());
    let reply = outcome.expect("the framework reads the reply");
    assert_eq!(reply.tool_calls.len(), 1);
    assert!(!reply.reply_repairs.any(), "openai gets no repair");
}

// CVL-FR-UALC, CVL-FR-13: a reply with no message and no tool call reaches the
// loop as a reply with neither, which the loop takes as `empty_reply`, and not
// as `unreachable`.
#[test]
fn a_reply_with_no_output_has_nothing_to_deliver() {
    let mut body: serde_json::Value =
        serde_json::from_str(&responses_reply(serde_json::json!({ "format": { "type": "text" } })))
            .unwrap();
    body["output"] = serde_json::json!([]);
    let (_, outcome) = call("custom", RESPONSES, "http://{addr}", body.to_string());
    let reply = outcome.expect("an empty reply is read");
    assert_eq!(reply.text, "");
    assert!(reply.tool_calls.is_empty());
    assert_eq!(reply.turn, None, "nothing of it goes back into the exchange");
}

// CVL-FR-18, AGC-FR-15, AGC-FR-31: a turn whose reply could not be read fails
// at once, is offered to the author again, and gains the thread no line.
#[test]
fn a_turn_whose_reply_could_not_be_read_is_offered_again_and_not_repeated() {
    let h = Harness::new(vec![Err(FAIL_INVALID_RESPONSE); MAX_ATTEMPTS]);
    h.create_agent("arch", "");
    let (terminal, thread) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_INVALID_RESPONSE));
    assert!(terminal.retry_permitted, "a reply that could not be read is recoverable");
    assert_eq!(h.seam.call_count(), 1, "the same reply is not asked for again");
    assert_eq!(h.threads("spec.md")[0].comments.len(), 1, "the thread gained no line");
    let entries = h.turns().recoverable_failures(None);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, terminal.id);
    assert_eq!(entries[0].trigger_comment_id, thread.comments[0].id);

    let failed = wait_for_record(&terminal.id, "model call failed");
    assert_eq!(failed.level, crate::logging::LogLevel::Error);
    assert_eq!(require(&failed, "failure"), FAIL_INVALID_RESPONSE);
    assert_eq!(require(&failed, "failureClass"), class::DECODE);
    assert_eq!(require(&failed, "retrying"), "false");
}

/// A seam whose one reply the Custom gateway repair changed. With `refused`,
/// the framework still refused the repaired reply as empty.
struct RepairedReply {
    repairs: ReplyRepairs,
    refused: bool,
}

impl CompletionSeam for RepairedReply {
    fn complete(
        &self,
        _request: &AgentRequest,
        _exchange: &[rig::completion::Message],
        _endpoint: &AiApiCall,
        _timeout: Duration,
    ) -> Result<ModelReply, CallFailure> {
        if self.refused {
            return Err(CallFailure {
                reply_repairs: self.repairs,
                ..CallFailure::from(FAIL_EMPTY_REPLY)
            });
        }
        Ok(ModelReply {
            text: "A private answer.".into(),
            reply_repairs: self.repairs,
            ..Default::default()
        })
    }
}

/// Run one turn on a `RepairedReply` seam, and give back the turn's end state
/// and its record of the repair.
fn repaired_turn(
    repairs: ReplyRepairs,
    refused: bool,
) -> (AgentTurnState, crate::logging::LogRecord) {
    let h = Harness::with_patient_seam(Box::new(RepairedReply { repairs, refused }));
    h.create_agent("scribe", "Argue about structure.");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let turn = h
        .dispatch("scribe", ConversationOrigin::of(&thread), &thread.comments[0].id)
        .expect("dispatch");
    let ended = wait_for_terminal(&h, &turn.id);
    let record = wait_for_record(&turn.id, "provider reply was repaired before it was read");
    (ended.state, record)
}

// CVL-FR-TQRD: a reply where only parts without text were removed is recorded,
// and each field names its own tolerance.
#[test]
fn a_reply_with_only_removed_parts_is_recorded() {
    let repairs = ReplyRepairs { text_format: false, null_text_parts: 1 };
    let (state, record) = repaired_turn(repairs, false);
    assert_eq!(state, AgentTurnState::Delivered);
    assert_eq!(require(&record, "textFormat"), "false");
    assert_eq!(require(&record, "nullTextParts"), "1");
}

// CVL-FR-TQRD, CVL-FR-21: a repaired reply that the framework still refused is
// recorded as repaired, so the log shows why the reply came back empty.
#[test]
fn a_repaired_reply_that_still_fails_is_recorded() {
    let repairs = ReplyRepairs { text_format: true, null_text_parts: 1 };
    let (state, record) = repaired_turn(repairs, true);
    assert_eq!(state, AgentTurnState::Failed);
    assert_eq!(record.level, crate::logging::LogLevel::Warn);
    assert_eq!(require(&record, "textFormat"), "true");
    assert_eq!(require(&record, "nullTextParts"), "1");
}

// CVL-FR-TQRD: a repaired reply is recorded at warning level with the provider
// and the model, and with no part of the reply.
#[test]
fn a_repaired_reply_is_recorded_without_its_content() {
    let repairs = ReplyRepairs { text_format: true, null_text_parts: 2 };
    let (state, record) = repaired_turn(repairs, false);
    assert_eq!(state, AgentTurnState::Delivered);
    assert_eq!(record.level, crate::logging::LogLevel::Warn);
    assert_eq!(require(&record, "agent"), "scribe");
    assert_eq!(require(&record, "textFormat"), "true");
    assert_eq!(require(&record, "nullTextParts"), "2");
    assert!(!require(&record, "provider").is_empty());
    assert!(!require(&record, "model").is_empty());
    let rendered = format!("{record:?}");
    assert!(!rendered.contains("A private answer"), "{rendered}");
}
