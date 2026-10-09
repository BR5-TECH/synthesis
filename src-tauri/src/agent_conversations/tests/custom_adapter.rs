//! The Custom gateway as an integration of the existing Rig-backed adapter
//! (`AAP-ai-api-integrations.md` AAP-FR-ADPX, AAP-FR-RTMZ; `CVL-conversation-loop.md`
//! CVL-FR-ZPGW).
//!
//! The completion wire format is the framework's, so nothing here fixes a
//! request or a response body. What is asserted is what this application
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
/// was received (the request line and the headers).
fn refuse_once() -> (
    String,
    std::thread::JoinHandle<String>,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let accepted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = accepted.clone();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
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
            if received.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
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

/// Make one call through the production seam and return what the server saw.
fn call_once(mode: Option<ModelMode>) -> (String, Result<ModelReply, CallFailure>) {
    let (url, server, accepted) = refuse_once();
    let endpoint = custom_endpoint(&url, mode);
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
    let chat = openai_adapter_config(&custom_endpoint("https://gw.example.com", Some(ModelMode::Chat)));
    assert_eq!(chat.base_url, "https://gw.example.com/v1");
    assert_eq!(chat.route, OpenAiRoute::ChatCompletions);
    let responses =
        openai_adapter_config(&custom_endpoint("https://gw.example.com", Some(ModelMode::Responses)));
    assert_eq!(responses.route, OpenAiRoute::Responses);
    let both = openai_adapter_config(&custom_endpoint("https://gw.example.com", None));
    assert_eq!(both.route, OpenAiRoute::Unrestricted, "no preference is added");

    // `openai` is carried exactly as before: its base URL is untouched and a
    // mode, which only a gateway model carries, has no effect.
    let mut openai = custom_endpoint("https://api.openai.com/v1", Some(ModelMode::Chat));
    openai.provider = "openai".into();
    let config = openai_adapter_config(&openai);
    assert_eq!(config.base_url, "https://api.openai.com/v1");
    assert_eq!(config.route, OpenAiRoute::Unrestricted);
}

// AAP-FR-ADPX: the call carries the secret as a bearer token and the model id
// the endpoint named, and a restricted chat model goes to Chat Completions.
#[test]
fn a_chat_model_is_called_on_chat_completions_with_the_bearer_secret() {
    let (seen, outcome) = call_once(Some(ModelMode::Chat));
    let first = seen.lines().next().unwrap_or_default().to_string();
    assert!(first.starts_with("POST /v1/chat/completions"), "{first}");
    assert!(
        seen.to_lowercase()
            .contains(&format!("authorization: bearer {}", SECRET.to_lowercase())),
        "the secret travels as a bearer token"
    );
    let failure = outcome.expect_err("the loopback server refuses");
    assert!(!format!("{failure:?}").contains(SECRET), "no secret in the failure");
}

// AAP-FR-RTMZ, CVL-FR-ZPGW: a responses model and an unrestricted model both
// take the adapter's own route, which this layer does not override or fall back
// from.
#[test]
fn a_responses_model_and_an_unrestricted_model_take_the_adapters_own_route() {
    for mode in [Some(ModelMode::Responses), None] {
        let (seen, outcome) = call_once(mode);
        let first = seen.lines().next().unwrap_or_default().to_string();
        assert!(first.starts_with("POST /v1/responses"), "{mode:?}: {first}");
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

    let refused = h
        .dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id)
        .unwrap_err();
    assert_eq!(refused, FAIL_AGENT_UNAVAILABLE);
    assert!(h.turns().in_flight(None).is_empty());
    assert!(h.seam.requests().is_empty(), "no call was made");
}
