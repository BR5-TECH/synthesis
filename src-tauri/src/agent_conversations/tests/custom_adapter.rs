//! The Custom gateway as an integration of the existing Rig-backed adapter
//! (`AAP-ai-api-integrations.md` AAP-FR-ADPX, AAP-FR-RTMZ; `CVL-conversation-loop.md`
//! CVL-FR-ZPGW).
//!
//! The completion wire format is the framework's, so nothing here fixes a
//! request or a response body, apart from the output-token limit that a call
//! states (CVL-FR-FHFM, CVL-FR-KXTQ). What is asserted is what this application
//! supplies to the adapter — base URL, bearer secret, model, reasoning, route —
//! and which route the adapter takes for it, observed as the path of the one
//! request a loopback server receives.

use super::*;
use crate::ai_shared::ModelMode;
use std::io::{Read, Write};
use std::net::TcpListener;

const SECRET: &str = "gw-secret-ZXCV5678";

fn custom_endpoint(base_url: &str, mode: Option<ModelMode>) -> AiApiCall {
    AiApiCall {
        max_output_tokens: None,
        context_size: crate::ai_api::DEFAULT_CONTEXT_SIZE,
        provider: "custom".into(),
        base_url: base_url.into(),
        api_key: Some(SECRET.into()),
        model_id: Some("gw-model".into()),
        reasoning: None,
        accepts_image_input: false,
        model_mode: mode,
        turn_timeout_ms: None,
    }
}

fn request() -> AgentRequest {
    AgentRequest {
        instructions: compile_prompt(OriginKind::ArtifactComment, "Argue.", ""),
        input: vec![InputSection {
            tag: TAG_CURRENT_COMMENT.into(),
            attributes: Vec::new(),
            body: "What do you think?".into(),
            truncated: false,
            parts: Vec::new(),
        }],
        tools: Vec::new(),
        native_tools: Vec::new(),
        stable_head_sections: 0,
    }
}

/// Serve exactly one HTTP request on loopback with a refusal, and hand back what
/// was received (the request line, the headers, and the body).
fn refuse_once(
) -> (String, std::thread::JoinHandle<String>, std::sync::Arc<std::sync::atomic::AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let accepted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = accepted.clone();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
        stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut received = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let n = stream.read(&mut buffer).unwrap_or(0);
            if n == 0 {
                break;
            }
            received.extend_from_slice(&buffer[..n]);
            // The body can arrive after the headers, so read until the length
            // that the headers state is all there.
            if let Some(end) = received.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&received[..end]).to_lowercase();
                let length = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if received.len() >= end + 4 + length {
                    break;
                }
            }
        }
        let body = r#"{"error":{"message":"refused","type":"x","param":null,"code":null}}"#;
        let _ = write!(
            stream,
            "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        String::from_utf8_lossy(&received).to_string()
    });
    (url, handle, accepted)
}

/// The JSON body of a request that `refuse_once` received.
fn body_json(seen: &str) -> serde_json::Value {
    let (_, body) = seen.split_once("\r\n\r\n").expect("the request has a body");
    serde_json::from_str(body).expect("the body is JSON")
}

/// Make one call through the production seam and return what the server saw.
fn call_once(mode: Option<ModelMode>) -> (String, Result<ModelReply, CallFailure>) {
    call_once_with(|url| custom_endpoint(url, mode))
}

/// Make one call to the endpoint that `endpoint` builds for the loopback URL.
fn call_once_with(
    endpoint: impl FnOnce(&str) -> AiApiCall,
) -> (String, Result<ModelReply, CallFailure>) {
    let (url, server, accepted) = refuse_once();
    let endpoint = endpoint(&url);
    let request = request();
    let exchange = opening_exchange(&request, false);
    let outcome = RigCompletion.complete(&request, &exchange, &endpoint, Duration::from_secs(20));
    // A call refused before it is sent leaves the server waiting for a
    // connection. This empty one releases it, so that case fails and does not
    // hang. It is made only while the server still waits, so it cannot reach a
    // listener another test bound to the same port after this one closed.
    if !accepted.load(std::sync::atomic::Ordering::SeqCst) {
        drop(std::net::TcpStream::connect(url.trim_start_matches("http://")));
    }
    (server.join().unwrap(), outcome)
}

// AAP-FR-ADPX, CVL-FR-ZPGW: the adapter is given the gateway root with `/v1`
// added, the route its model's mode allows, and nothing else.
#[test]
fn the_adapter_configuration_follows_the_endpoint_and_the_model_mode() {
    let chat =
        openai_adapter_config(&custom_endpoint("https://gw.example.com", Some(ModelMode::Chat)));
    assert_eq!(chat.base_url, "https://gw.example.com/v1");
    assert_eq!(chat.route, OpenAiRoute::ChatCompletions);
    let responses = openai_adapter_config(&custom_endpoint(
        "https://gw.example.com",
        Some(ModelMode::Responses),
    ));
    assert_eq!(responses.route, OpenAiRoute::Responses);
    let both = openai_adapter_config(&custom_endpoint("https://gw.example.com", None));
    assert_eq!(both.route, OpenAiRoute::Unrestricted, "no preference is added");
    // CVL-FR-FHFM: every Custom route states the same limit.
    for config in [&chat, &responses, &both] {
        assert_eq!(config.max_output_tokens, Some(128_000), "{:?}", config.route);
    }

    // `openai` is carried exactly as before: its base URL is untouched and a
    // mode, which only a gateway model carries, has no effect.
    let mut openai = custom_endpoint("https://api.openai.com/v1", Some(ModelMode::Chat));
    openai.provider = "openai".into();
    let config = openai_adapter_config(&openai);
    assert_eq!(config.base_url, "https://api.openai.com/v1");
    assert_eq!(config.route, OpenAiRoute::Unrestricted);
    // CVL-FR-KXTQ: `openai` receives no limit from this module.
    assert_eq!(config.max_output_tokens, None);
}

// AAP-FR-ADPX: the call carries the secret as a bearer token and the model id
// the endpoint named, and a restricted chat model goes to Chat Completions.
// CVL-FR-FHFM: the call that the production carrier sends states the limit.
#[test]
fn a_chat_model_is_called_on_chat_completions_with_the_bearer_secret() {
    let (seen, outcome) = call_once(Some(ModelMode::Chat));
    let first = seen.lines().next().unwrap_or_default().to_string();
    assert!(first.starts_with("POST /v1/chat/completions"), "{first}");
    assert_eq!(body_json(&seen)["max_tokens"], 128_000, "{seen}");
    assert!(
        seen.to_lowercase().contains(&format!("authorization: bearer {}", SECRET.to_lowercase())),
        "the secret travels as a bearer token"
    );
    let failure = outcome.expect_err("the loopback server refuses");
    assert!(!format!("{failure:?}").contains(SECRET), "no secret in the failure");
}

// AAP-FR-RTMZ, CVL-FR-ZPGW: a responses model and an unrestricted model both
// take the adapter's own route, which this layer does not override or fall back
// from. CVL-FR-FHFM: the call on that route states the limit.
#[test]
fn a_responses_model_and_an_unrestricted_model_take_the_adapters_own_route() {
    for mode in [Some(ModelMode::Responses), None] {
        let (seen, outcome) = call_once(mode);
        let first = seen.lines().next().unwrap_or_default().to_string();
        assert!(first.starts_with("POST /v1/responses"), "{mode:?}: {first}");
        let body = body_json(&seen);
        assert_eq!(body["max_output_tokens"], 128_000, "{mode:?}: {seen}");
        assert!(body.get("max_tokens").is_none(), "{mode:?}: {seen}");
        assert!(outcome.is_err());
    }
}

// CVL-FR-ZPGW: the model list is not what the call is made against, but the
// request the loop assembles is the same for Custom as for any provider.
#[test]
fn the_completion_request_for_custom_carries_the_agents_model_and_no_reasoning() {
    let request = request();
    let exchange = opening_exchange(&request, false);
    let built = build_completion_request(
        &request,
        &exchange,
        &custom_endpoint("https://gw.example.com", Some(ModelMode::Chat)),
    );
    assert_eq!(built.model.as_deref(), Some("gw-model"));
    assert!(built.additional_params.is_none(), "a Custom model declares no reasoning");
    assert!(built.tools.is_empty());
}

// CVL-FR-KXTQ: an `openai` call that the production carrier sends states no
// output-token limit, also when it asks for reasoning.
#[test]
fn an_openai_call_states_no_output_token_limit() {
    let (seen, outcome) = call_once_with(|url| {
        let mut endpoint = custom_endpoint(&format!("{url}/v1"), None);
        endpoint.provider = "openai".into();
        endpoint.model_id = Some("gpt-6-sol".into());
        endpoint.reasoning = Some(crate::ai_api::ReasoningChoice::Effort { effort: "high".into() });
        endpoint
    });
    let first = seen.lines().next().unwrap_or_default().to_string();
    assert!(first.starts_with("POST /v1/responses"), "{first}");
    let body = body_json(&seen);
    for field in ["max_output_tokens", "max_completion_tokens", "max_tokens"] {
        assert!(body.get(field).is_none(), "{field}: {seen}");
    }
    assert!(outcome.is_err());
}

// AGC-FR-14, AAP-FR-APRV: an agent stores no provider, so the turn is carried
// by the provider the project resolves to, with the agent's own model and the
// model's route metadata.
#[test]
fn a_turn_is_carried_by_the_active_provider_with_the_agents_model_and_the_models_mode() {
    let h = Harness::new(vec![Ok("Answered.".into())]);
    h.mount();
    h.seed_second_provider("custom");
    let mut records = h.store().load_ai_api_registry().unwrap().0;
    let custom = records.iter_mut().find(|r| r.provider == "custom").unwrap();
    custom.models = vec![ModelOption::new("m", "m").with_mode(Some(ModelMode::Chat))];
    h.store().save_ai_api_registry(records, Some("custom".into())).unwrap();
    h.create_plain_agent("arch");

    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let (_, endpoint) = h.seam.requests().into_iter().next().expect("one call");
    assert_eq!(endpoint.provider, "custom");
    assert_eq!(endpoint.model_id.as_deref(), Some("m"));
    assert_eq!(endpoint.model_mode, Some(ModelMode::Chat));
    assert_eq!(endpoint.base_url, "https://provider.example/v1");
}

// AGC-FR-14, AGR-FR-18: after the active provider changes to one that does not
// serve the agent's model, the next dispatch is refused and nothing is
// registered.
#[test]
fn a_dispatch_after_a_provider_switch_to_one_without_the_model_is_refused() {
    let h = Harness::new(vec![Ok("Answered.".into())]);
    h.mount();
    h.create_plain_agent("arch");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");

    h.seed_second_provider("custom");
    let mut records = h.store().load_ai_api_registry().unwrap().0;
    records.iter_mut().find(|r| r.provider == "custom").unwrap().models =
        vec![ModelOption::new("other-model", "other-model")];
    h.store().save_ai_api_registry(records, Some("custom".into())).unwrap();

    let refused =
        h.dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id).unwrap_err();
    assert_eq!(refused, FAIL_AGENT_UNAVAILABLE);
    assert!(h.turns().in_flight(None).is_empty());
    assert!(h.seam.requests().is_empty(), "no call was made");
}

// CVL-FR-FHFM, AAP-FR-HFQZ: the provider's max output tokens replaces the
// default limit on both routes; with none set the default stands.
#[test]
fn the_providers_max_output_tokens_replaces_the_default_limit() {
    for mode in [Some(ModelMode::Chat), Some(ModelMode::Responses), None] {
        let mut endpoint = custom_endpoint("https://gw.example.com", mode);
        assert_eq!(openai_adapter_config(&endpoint).max_output_tokens, Some(128_000));
        endpoint.max_output_tokens = Some(4096);
        assert_eq!(openai_adapter_config(&endpoint).max_output_tokens, Some(4096), "{mode:?}");
    }
    let (seen, _) = call_once_with(|url| {
        let mut endpoint = custom_endpoint(url, Some(ModelMode::Chat));
        endpoint.max_output_tokens = Some(4096);
        endpoint
    });
    assert_eq!(body_json(&seen)["max_tokens"], 4096, "{seen}");
}

// CVL-FR-ZPGW, AAP-FR-ADPX: a Custom call carries its effective reasoning in
// the field of its route, and none where the reasoning is null.
#[test]
fn a_custom_call_carries_its_reasoning_in_the_field_of_its_route() {
    let effort = |e: &str| Some(crate::ai_api::ReasoningChoice::Effort { effort: e.into() });
    let request = request();
    let exchange = opening_exchange(&request, false);
    let built = |mode, reasoning| {
        let mut endpoint = custom_endpoint("https://gw.example.com", mode);
        endpoint.reasoning = reasoning;
        build_completion_request(&request, &exchange, &endpoint).additional_params
    };
    assert_eq!(
        built(Some(ModelMode::Chat), effort("low")),
        Some(serde_json::json!({ "reasoning_effort": "low" }))
    );
    for mode in [Some(ModelMode::Responses), None] {
        assert_eq!(
            built(mode, effort("none")),
            Some(serde_json::json!({ "reasoning": { "effort": "none" } })),
            "{mode:?}"
        );
    }
    assert_eq!(built(Some(ModelMode::Chat), None), None);

    // The wire the production carrier sends holds it.
    let (seen, _) = call_once_with(|url| {
        let mut endpoint = custom_endpoint(url, Some(ModelMode::Chat));
        endpoint.reasoning = effort("high");
        endpoint
    });
    assert_eq!(body_json(&seen)["reasoning_effort"], "high", "{seen}");
    let (seen, _) = call_once_with(|url| {
        let mut endpoint = custom_endpoint(url, None);
        endpoint.reasoning = effort("medium");
        endpoint
    });
    assert_eq!(body_json(&seen)["reasoning"]["effort"], "medium", "{seen}");
}
