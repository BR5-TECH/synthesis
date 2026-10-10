//! A request the framework refuses to build (`CVL-conversation-loop.md`
//! CVL-FR-21, CVL-FR-18; `AGC-agent-conversations.md` AGC-FR-15).
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

/// Text that stands for a part of the request in a framework error.
const REQUEST_TEXT: &str = "the tool result for `call-0` (list_skills) has no content";

// CVL-FR-21: a request the framework refused to build is `request`, which is
// `invalid_request`, classified by its kind. Its text does not reach the
// failure.
#[test]
fn a_request_the_framework_refused_to_build_is_an_invalid_request() {
    let error = rig::ProviderError::request(REQUEST_TEXT);
    let failure = rig_seam::classify_completion_error(&error);
    assert_eq!(failure.failure, FAIL_INVALID_REQUEST);
    assert_eq!(failure.class, class::REQUEST);
    assert_eq!(failure.status, None);
    assert_eq!(failure.provider_code, None);
    assert_eq!(failure.provider_message, None);
    assert_eq!(failure.tls, None);
}

// CVL-FR-21: the kind settles the class, also where the text holds a word the
// text classification would read as a transport failure.
#[test]
fn a_refused_request_is_not_classified_from_its_text() {
    let error = rig::ProviderError::request("connection refused: dns error, HTTP 503");
    let failure = rig_seam::classify_completion_error(&error);
    assert_eq!(failure.failure, FAIL_INVALID_REQUEST);
    assert_eq!(failure.class, class::REQUEST);
}

// CVL-FR-18, AGC-FR-15: `invalid_request` is recoverable by the author and is
// never repeated by the loop itself. A script that names it gets the class
// `request`.
#[test]
fn an_invalid_request_is_offered_again_but_not_repeated() {
    let failure = CallFailure::from(FAIL_INVALID_REQUEST);
    assert_eq!(failure.class, class::REQUEST);
    assert!(failure.recoverable());
    assert!(!failure.repeatable());
    assert!(is_recoverable(FAIL_INVALID_REQUEST));
    assert!(!is_repeated_automatically(FAIL_INVALID_REQUEST));
}

// CVL-FR-18, CVL-FR-21, AGC-FR-15: a turn whose request could not be built
// fails at once after one physical attempt, is offered to the author again,
// and gains the thread no line.
#[test]
fn a_turn_whose_request_could_not_be_built_is_offered_again_and_not_repeated() {
    let h = Harness::new(vec![Err(FAIL_INVALID_REQUEST); MAX_ATTEMPTS]);
    h.create_agent("arch", "");
    let (terminal, thread) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_INVALID_REQUEST));
    assert!(terminal.retry_permitted, "a request that could not be built is recoverable");
    assert_eq!(h.seam.call_count(), 1, "the same request is not built again");
    assert_eq!(h.threads("spec.md")[0].comments.len(), 1, "the thread gained no line");
    let entries = h.turns().recoverable_failures(None);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, terminal.id);
    assert_eq!(entries[0].trigger_comment_id, thread.comments[0].id);

    let failed = wait_for_record(&terminal.id, "model call failed");
    assert_eq!(failed.level, crate::logging::LogLevel::Error);
    assert_eq!(require(&failed, "failure"), FAIL_INVALID_REQUEST);
    assert_eq!(require(&failed, "failureClass"), class::REQUEST);
    assert_eq!(require(&failed, "retrying"), "false");
    let ended = wait_for_record(&terminal.id, "agent turn ended");
    assert_eq!(require(&ended, "modelCalls"), "1");
    assert_eq!(require(&ended, "physicalCalls"), "1", "one physical attempt");
}

// CVL-FR-21, CVL-FR-18: through the production seam, a Responses request that
// the framework refuses to build fails as `invalid_request` and is not
// repeated. Nothing is sent: the port is closed, so a request that was sent
// would fail as `unreachable` instead.
#[test]
fn the_production_seam_reports_a_refused_responses_request_as_invalid_request() {
    let endpoint = AiApiCall {
        turn_timeout_ms: None,
        provider: "custom".into(),
        base_url: "http://127.0.0.1:9".into(),
        api_key: Some("not-a-real-key".into()),
        model_id: Some("claude-sonnet-5-5".into()),
        reasoning: None,
        accepts_image_input: false,
        model_mode: Some(crate::ai_shared::ModelMode::Responses),
    };
    let request = wire_request();
    let mut exchange = opening_exchange(&request, false);
    // A user message that holds no content: the shape the framework refuses
    // to build a request from.
    exchange.push(rig::completion::Message::User { content: Vec::new() });

    let failure = RigCompletion
        .complete(&request, &exchange, &endpoint, Duration::from_secs(20))
        .expect_err("the framework refuses to build the request");
    assert_eq!(failure.failure, FAIL_INVALID_REQUEST);
    assert_eq!(failure.class, class::REQUEST);
    assert!(!failure.repeatable());
    assert_eq!(failure.provider_message, None);
}
