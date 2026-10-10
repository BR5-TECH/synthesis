//! What one reply of the framework becomes, and what one of its failures
//! becomes (`CVL-conversation-loop.md` CVL-FR-21, CVL-FR-28, CVL-FR-35,
//! CVL-FR-WQZD, CVL-FR-UALC, CVL-FR-ETAH).
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;
use rig::completion::message::{AssistantContent, Reasoning};
use rig::completion::{FinishReason, Usage};
use rig::test_utils::MockTurn;

/// One round through the production extraction, against a scripted turn of
/// the framework's own mock.
fn reply_to(turn: MockTurn) -> Result<ModelReply, CallFailure> {
    let request = wire_request();
    let built =
        build_completion_request(&request, &opening_exchange(&request, false), &any_endpoint());
    let model = rig::test_utils::MockCompletionModel::from_turns([turn]);
    block_on_with_timeout(
        async move { rig_seam::run_model(model, built).await },
        Duration::from_secs(5),
    )
}

fn list_skills_call() -> AssistantContent {
    AssistantContent::ToolCall(rig_tool_call("call_1", "list_skills", serde_json::json!({})))
}

// CVL-FR-28, CVL-FR-WQZD: a reply carries the provider's own identifiers for
// the response and the request, the reason it ended, and every count the
// provider reported — the input total with cache reads and cache writes in it.
#[test]
fn a_reply_carries_the_providers_identifiers_and_every_reported_count() {
    let turn = MockTurn::text("The answer.")
        .with_usage(
            Usage::new()
                .input_tokens(5_000)
                .cached_input_tokens(4_000)
                .cache_creation_input_tokens(300)
                .output_tokens(70)
                .reasoning_tokens(20),
        )
        .with_response_id("msg_1")
        .with_provider_request_id("req_1")
        .with_finish_reason(FinishReason::Stop);
    let reply = reply_to(turn).expect("the reply is read");

    assert_eq!(reply.text, "The answer.");
    assert_eq!(reply.prompt_tokens, Some(5_000));
    assert_eq!(reply.output_tokens, Some(70));
    assert_eq!(reply.cache_write_tokens, Some(300));
    assert_eq!(reply.reasoning_tokens, Some(20));
    assert_eq!(
        reply.input_tokens,
        Some(InputTokens {
            cached: 4_000,
            uncached: 700,
        }),
    );
    assert_eq!(reply.response_id.as_deref(), Some("msg_1"));
    assert_eq!(reply.provider_request_id.as_deref(), Some("req_1"));
    assert_eq!(reply.finish_reason.as_deref(), Some("stop"));
}

// CVL-FR-WQZD: a count the provider did not report is absent, never zero.
#[test]
fn a_reply_without_a_usage_report_carries_no_count() {
    let reply = reply_to(MockTurn::text("The answer.")).expect("the reply is read");
    assert_eq!(reply.prompt_tokens, None);
    assert_eq!(reply.output_tokens, None);
    assert_eq!(reply.cache_write_tokens, None);
    assert_eq!(reply.reasoning_tokens, None);
    assert_eq!(reply.input_tokens, None);
    assert_eq!(reply.provider_request_id, None);
}

// CVL-FR-UALC: a reply the framework reports as failed supplies neither prose
// nor a tool request, whatever it holds, and nothing of it goes back into the
// exchange.
#[test]
fn a_filtered_reply_supplies_neither_prose_nor_a_tool_request() {
    let turn = MockTurn::from_contents([AssistantContent::text("Partial."), list_skills_call()])
        .with_finish_reason(FinishReason::ContentFilter);
    let reply = reply_to(turn).expect("the reply is read");
    assert_eq!(reply.text, "");
    assert!(reply.tool_calls.is_empty(), "no tool of it is dispatched");
    assert_eq!(reply.turn, None, "nothing of it is appended");
    assert_eq!(reply.finish_reason.as_deref(), Some("content_filter"));
}

// CVL-FR-UALC: a finish reason the framework does not know ends a reply
// normally, so a gateway that sends a reason of its own keeps its answer and
// its tool calls.
#[test]
fn an_unknown_finish_reason_ends_a_reply_normally() {
    let turn = MockTurn::from_contents([AssistantContent::text("Looking."), list_skills_call()])
        .with_finish_reason(FinishReason::Other("gateway_done".into()));
    let reply = reply_to(turn).expect("the reply is read");
    assert_eq!(reply.text, "Looking.");
    assert_eq!(reply.tool_calls.len(), 1);
    assert!(reply.turn.is_some());
    assert_eq!(reply.finish_reason.as_deref(), Some("gateway_done"));
}

// CVL-FR-UALC: a reply cut off at the output-token limit is not failed.
#[test]
fn a_reply_cut_off_at_the_output_limit_keeps_its_prose() {
    let turn = MockTurn::text("A long answer").with_finish_reason(FinishReason::Length);
    let reply = reply_to(turn).expect("the reply is read");
    assert_eq!(reply.text, "A long answer");
    assert_eq!(reply.finish_reason.as_deref(), Some("length"));
}

// CVL-FR-ETAH: a reply that asks for tools goes back into the exchange as the
// framework returned it, its thinking included.
#[test]
fn a_reply_goes_back_into_the_exchange_with_its_thinking() {
    let turn = MockTurn::from_contents([
        AssistantContent::Reasoning(Reasoning::new("I should list the skills.")),
        list_skills_call(),
    ]);
    let reply = reply_to(turn).expect("the reply is read");
    assert_eq!(reply.tool_calls.len(), 1);

    let rig::completion::Message::Assistant(turn) = assistant_message(&reply) else {
        panic!("the reply goes back as an assistant turn");
    };
    assert_eq!(
        turn.content,
        vec![
            AssistantContent::Reasoning(Reasoning::new("I should list the skills.")),
            list_skills_call(),
        ],
        "the reply goes back unchanged",
    );
    assert!(turn.origin.is_some(), "the turn keeps where it came from");
    assert_eq!(turn.tool_calls().count(), 1);
}

// CVL-FR-ETAH, CVL-FR-VSSU: the markers leave the turn that goes back into the
// exchange, as they leave the reply the loop reads.
#[test]
fn the_turn_that_goes_back_loses_the_citation_markers_too() {
    let marked = "See this\u{E200}cite\u{E201}.";
    let turn = MockTurn::from_contents([
        AssistantContent::text(marked),
        AssistantContent::ToolCall(rig_tool_call(
            "call_1",
            "search_specifications",
            serde_json::json!({ "query": marked }),
        )),
    ]);
    let mut reply = reply_to(turn).expect("the reply is read");
    let removed = strip_reply_citation_markers(&mut reply);
    // Six characters in the text and six in the argument, counted once.
    assert_eq!(removed, 12);

    let rig::completion::Message::Assistant(turn) = assistant_message(&reply) else {
        panic!("the reply goes back as an assistant turn");
    };
    let replayed = serde_json::to_string(&turn.content).expect("the turn renders");
    assert!(!replayed.contains('\u{E200}'), "{replayed}");
    assert!(!replayed.contains('\u{E201}'), "{replayed}");
    assert!(replayed.contains("See this."), "{replayed}");
    let replayed_call = turn.tool_calls().next().expect("the call goes back");
    assert_eq!(replayed_call.function.arguments, reply.tool_calls[0].function.arguments);
}

// CVL-FR-ETAH: a reply whose client returned no turn goes back as its text
// and its calls.
#[test]
fn a_reply_without_a_turn_goes_back_as_its_text_and_its_calls() {
    let reply = ModelReply {
        text: "Looking.".into(),
        tool_calls: vec![rig_tool_call("call_1", "list_skills", serde_json::json!({}))],
        ..Default::default()
    };
    let rig::completion::Message::Assistant(turn) = assistant_message(&reply) else {
        panic!("the reply goes back as an assistant turn");
    };
    assert_eq!(turn.content.len(), 2);
    assert_eq!(turn.tool_calls().count(), 1);
}

const OVERLOADED: &str =
    r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded, try later."}}"#;

// CVL-FR-35, CVL-FR-21: a structured error is recorded in the provider's own
// terms — its status, its own code as text, its own id for the request, and
// its own message — whichever provider carries the call.
#[test]
fn a_structured_provider_error_carries_the_providers_own_terms() {
    let failure = reply_to(MockTurn::provider_response_error(
        http::StatusCode::SERVICE_UNAVAILABLE,
        OVERLOADED,
        "req_overloaded",
    ))
    .expect_err("the provider refused");
    assert_eq!(failure.failure, FAIL_UNREACHABLE);
    assert_eq!(failure.class, class::HTTP_STATUS);
    assert_eq!(failure.status, Some(503));
    assert_eq!(failure.provider_code.as_deref(), Some("overloaded_error"));
    assert_eq!(failure.provider_request_id.as_deref(), Some("req_overloaded"));
    assert_eq!(failure.provider_message.as_deref(), Some("Overloaded, try later."));
}

// CVL-FR-35: the message is bounded by character, a provider being free to
// answer at any length.
#[test]
fn a_structured_error_message_is_bounded() {
    let long = "é".repeat(PROVIDER_MESSAGE_LIMIT * 2);
    let body = serde_json::json!({ "error": { "message": long, "code": "too_long" } }).to_string();
    let failure = reply_to(MockTurn::provider_response_error(
        http::StatusCode::BAD_REQUEST,
        body,
        "req_long",
    ))
    .expect_err("the provider refused");
    let message = failure.provider_message.expect("a message");
    assert_eq!(message, format!("{}…", "é".repeat(PROVIDER_MESSAGE_LIMIT)));
    assert_eq!(failure.provider_code.as_deref(), Some("too_long"));
}

// CVL-FR-35: a short message is kept as the provider wrote it, also where it
// holds a word that looks like part of a credential name.
#[test]
fn a_short_structured_error_message_is_kept_as_written() {
    let written = "Unknown parameter 'api_key' in a risk-free request.";
    let body = serde_json::json!({ "error": { "message": written } }).to_string();
    let failure = reply_to(MockTurn::provider_response_error(
        http::StatusCode::BAD_REQUEST,
        body,
        "req_short",
    ))
    .expect_err("the provider refused");
    assert_eq!(failure.provider_message.as_deref(), Some(written));
}

// CVL-FR-35: an error envelope under a success status is a structured refusal
// all the same, and names no status that did not fail.
#[test]
fn an_error_envelope_under_a_success_status_names_no_status() {
    let failure = rig_seam::classify_completion_error(&rig::ProviderError::ProviderResponse(
        rig::ProviderResponseError::new(http::StatusCode::OK, OVERLOADED)
            .with_provider_request_id(Some("req_ok".into())),
    ));
    assert_eq!(failure.status, None);
    assert_eq!(failure.provider_code.as_deref(), Some("overloaded_error"));
    assert_eq!(failure.provider_request_id.as_deref(), Some("req_ok"));
    assert_eq!(failure.provider_message.as_deref(), Some("Overloaded, try later."));
}

// CVL-FR-21: a credential the provider refused is `rejected`, whether the
// framework reports it as such or as a 401 or 403 status.
#[test]
fn a_refused_credential_is_rejected() {
    let body = r#"{"error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
    let refused = rig::ProviderError::InvalidAuthentication(rig::ProviderResponseError::new(
        http::StatusCode::UNAUTHORIZED,
        body,
    ));
    let failure = rig_seam::classify_completion_error(&refused);
    assert_eq!(failure.failure, FAIL_REJECTED);
    assert_eq!(failure.class, class::AUTH);
    assert_eq!(failure.status, Some(401));

    let failure = reply_to(MockTurn::provider_response_error(
        http::StatusCode::FORBIDDEN,
        body,
        "req_forbidden",
    ))
    .expect_err("the provider refused");
    assert_eq!(failure.failure, FAIL_REJECTED);
    assert_eq!(failure.class, class::AUTH);
    assert_eq!(failure.provider_request_id.as_deref(), Some("req_forbidden"));
}

// CVL-FR-21: a reply that ended before the provider ended it is the connection
// reset in flight, which is `unreachable` and is repeated.
#[test]
fn a_reply_cut_off_in_transit_is_a_reset_connection() {
    let failure = rig_seam::classify_completion_error(&rig::ProviderError::Truncated);
    assert_eq!(failure.failure, FAIL_UNREACHABLE);
    assert_eq!(failure.class, class::RESET);
    assert!(failure.repeatable());
}

// CVL-FR-21: a structured error's rendered text names the route and the
// request id; neither is read for a status, which comes from the error itself.
#[test]
fn a_structured_error_is_not_classified_from_its_text() {
    let failure = reply_to(MockTurn::provider_response_error(
        http::StatusCode::BAD_REQUEST,
        r#"{"error":{"message":"connection refused 503 dns"}}"#,
        "req_401_403",
    ))
    .expect_err("the provider refused");
    assert_eq!(failure.status, Some(400));
    assert_eq!(failure.class, class::HTTP_STATUS);
}

/// A seam whose one reply carries every identifier and count the record of
/// CVL-FR-28 names.
struct IdentifiedReply;

impl CompletionSeam for IdentifiedReply {
    fn complete(
        &self,
        _request: &AgentRequest,
        _exchange: &[rig::completion::Message],
        _endpoint: &AiApiCall,
        _timeout: Duration,
    ) -> Result<ModelReply, CallFailure> {
        Ok(ModelReply {
            text: "A private answer.".into(),
            prompt_tokens: Some(5_000),
            output_tokens: Some(70),
            cache_write_tokens: Some(300),
            reasoning_tokens: Some(20),
            response_id: Some("msg_1".into()),
            provider_request_id: Some("req_1".into()),
            response_model: Some("claude-opus-5-5-20261001".into()),
            finish_reason: Some("stop".into()),
            ..Default::default()
        })
    }
}

// CVL-FR-28: the record of an answered call names the counts, the provider's
// identifiers, the model that answered, and why the reply ended — and no part
// of the reply.
#[test]
fn the_record_of_an_answered_call_names_its_identifiers_and_counts() {
    let h = Harness::with_patient_seam(Box::new(IdentifiedReply));
    h.create_agent("scribe", "Argue about structure.");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let turn = h
        .dispatch("scribe", ConversationOrigin::of(&thread), &thread.comments[0].id)
        .expect("dispatch");
    let ended = wait_for_terminal(&h, &turn.id);
    assert_eq!(ended.state, AgentTurnState::Delivered);

    let record = wait_for_record(&turn.id, "model answered");
    assert_eq!(require(&record, "promptTokens"), "5000");
    assert_eq!(require(&record, "outputTokens"), "70");
    assert_eq!(require(&record, "cacheWriteTokens"), "300");
    assert_eq!(require(&record, "reasoningTokens"), "20");
    assert_eq!(require(&record, "responseId"), "msg_1");
    assert_eq!(require(&record, "providerRequestId"), "req_1");
    assert_eq!(require(&record, "responseModel"), "claude-opus-5-5-20261001");
    assert_eq!(require(&record, "finishReason"), "stop");
    let rendered = format!("{record:?}");
    assert!(!rendered.contains("A private answer"), "{rendered}");
}

// CVL-FR-28: what the provider did not give is named as not reported.
#[test]
fn the_record_of_an_answered_call_names_what_was_not_reported() {
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer("Settled."))]);
    h.mount();
    h.create_agent("quiet", "");
    let (terminal, _) = run_one(&h, "quiet");
    let record = wait_for_record(&terminal.id, "model answered");
    for key in [
        "outputTokens",
        "cacheWriteTokens",
        "reasoningTokens",
        "providerRequestId",
        "responseId",
        "responseModel",
        "finishReason",
    ] {
        assert_eq!(require(&record, key), "not reported", "{key}");
    }
}

/// A seam whose every call fails with a structured provider error.
struct StructuredRefusal;

impl CompletionSeam for StructuredRefusal {
    fn complete(
        &self,
        _request: &AgentRequest,
        _exchange: &[rig::completion::Message],
        _endpoint: &AiApiCall,
        _timeout: Duration,
    ) -> Result<ModelReply, CallFailure> {
        Err(CallFailure {
            failure: FAIL_UNREACHABLE,
            class: class::HTTP_STATUS,
            status: Some(400),
            provider_code: Some("invalid_request_error".into()),
            provider_request_id: Some("req_refused".into()),
            provider_message: Some("Refused.".into()),
            ..CallFailure::unreachable(class::HTTP_STATUS)
        })
    }
}

// CVL-FR-35, CVL-FR-28: the record of a failed attempt names the provider's
// code as text, its id for the request, and its message.
#[test]
fn the_record_of_a_failed_attempt_names_the_providers_terms() {
    let h = Harness::with_patient_seam(Box::new(StructuredRefusal));
    h.create_agent("scribe", "Argue about structure.");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let turn = h
        .dispatch("scribe", ConversationOrigin::of(&thread), &thread.comments[0].id)
        .expect("dispatch");
    wait_for_terminal(&h, &turn.id);
    let failed = wait_for_record(&turn.id, "model call failed");
    assert_eq!(require(&failed, "providerCode"), "invalid_request_error");
    assert_eq!(require(&failed, "providerRequestId"), "req_refused");
    assert_eq!(require(&failed, "providerMessage"), "Refused.");
}

/// A seam whose every reply the framework reported as filtered: nothing to
/// deliver, with the provider's reason and its identifiers.
struct FilteredReply;

impl CompletionSeam for FilteredReply {
    fn complete(
        &self,
        _request: &AgentRequest,
        _exchange: &[rig::completion::Message],
        _endpoint: &AiApiCall,
        _timeout: Duration,
    ) -> Result<ModelReply, CallFailure> {
        Ok(ModelReply {
            finish_reason: Some("content_filter".into()),
            provider_request_id: Some("req_filtered".into()),
            response_id: Some("msg_filtered".into()),
            ..Default::default()
        })
    }
}

// CVL-FR-UALC, CVL-FR-28: a reply with nothing to deliver is `empty_reply`,
// and the record of it says why, as far as the provider said.
#[test]
fn a_reply_with_nothing_to_deliver_is_recorded_with_its_reason() {
    let h = Harness::with_patient_seam(Box::new(FilteredReply));
    h.create_agent("scribe", "Argue about structure.");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let turn = h
        .dispatch("scribe", ConversationOrigin::of(&thread), &thread.comments[0].id)
        .expect("dispatch");
    wait_for_terminal(&h, &turn.id);
    let record = wait_for_record(&turn.id, "model reply held nothing to deliver");
    assert_eq!(require(&record, "finishReason"), "content_filter");
    assert_eq!(require(&record, "providerRequestId"), "req_filtered");
    assert_eq!(require(&record, "responseId"), "msg_filtered");
    let failed = wait_for_record(&turn.id, "model call failed");
    assert_eq!(require(&failed, "failure"), FAIL_EMPTY_REPLY);
}

// CVL-FR-35: the provider's message is read from the envelope's
// `error.message`, or a top-level `message`, and an empty one is no message.
#[test]
fn the_providers_message_is_read_from_its_envelope() {
    assert_eq!(
        rig_seam::provider_message_of(r#"{"error":{"message":"Nested."}}"#).as_deref(),
        Some("Nested."),
    );
    assert_eq!(
        rig_seam::provider_message_of(r#"{"message":"Top level."}"#).as_deref(),
        Some("Top level."),
    );
    assert_eq!(rig_seam::provider_message_of(r#"{"error":{"message":""}}"#), None);
    assert_eq!(rig_seam::provider_message_of("not json"), None);
}
