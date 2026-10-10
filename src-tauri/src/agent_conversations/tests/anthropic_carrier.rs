//! The Anthropic carrier (`CVL-conversation-loop.md` CVL-FR-KXTQ, CVL-FR-HBNW).
//!
//! The model-level tests assert what the carrier configures without a network
//! call. The wire test makes one call through the production seam to a loopback
//! server and reads the body that the Messages API would receive.

use super::*;
use crate::ai_api::ReasoningChoice;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

const SECRET: &str = "sk-ant-not-real-QWER1234";

fn anthropic_endpoint(
    base_url: &str,
    model: &str,
    reasoning: Option<ReasoningChoice>,
) -> AiApiCall {
    AiApiCall {
        turn_timeout_ms: None,
        provider: "anthropic".into(),
        base_url: base_url.into(),
        api_key: Some(SECRET.into()),
        model_id: Some(model.into()),
        reasoning,
        accepts_image_input: false,
        model_mode: None,
    }
}

fn every_reasoning_choice() -> Vec<Option<ReasoningChoice>> {
    vec![
        None,
        Some(ReasoningChoice::Off),
        Some(ReasoningChoice::On),
        Some(ReasoningChoice::Effort {
            effort: "high".into(),
        }),
    ]
}

// CVL-FR-KXTQ: every model gets the limit of this module, also a model id that
// `rig` has no default for and a model id that it has a different default for.
#[test]
fn every_anthropic_model_carries_the_fixed_output_token_limit() {
    let client = rig::providers::anthropic::AnthropicConfig::new("not-a-real-key")
        .connect(rig_reqwest::ReqwestClient::from(reqwest013::Client::new()));
    for model_id in [
        "claude-opus-5-5",
        "claude-sonnet-5-5",
        "claude-fable-5-1",
        "claude-haiku-5-5",
        "claude-haiku-4-5",
        "m",
        "",
    ] {
        let endpoint = anthropic_endpoint("https://example.invalid", model_id, None);
        let model = anthropic_model(&client, &endpoint);
        assert_eq!(model.wire.default_max_tokens, Some(32_000), "{model_id:?}");
    }
}

// CVL-FR-23: an Anthropic request asks for the provider's short cache
// retention, which the provider applies at the boundary it advances itself.
// The limit of CVL-FR-KXTQ does not remove it.
#[test]
fn an_anthropic_request_asks_for_the_short_cache_retention() {
    let request = AgentRequest::default();
    let exchange = opening_exchange(&request, false);
    let endpoint = anthropic_endpoint("https://example.invalid", "claude-opus-5-5", None);
    let built = anthropic_request(build_completion_request(&request, &exchange, &endpoint));
    assert_eq!(
        built.options.cache,
        Some(rig::completion::CacheRetention::Short)
    );
}

// CVL-FR-HBNW: no reasoning choice puts a parameter on an Anthropic request.
// The same choices still reach an OpenAI-compatible request, so the rule is
// about the provider and not about the choice.
#[test]
fn an_anthropic_request_carries_no_reasoning_parameter_for_any_choice() {
    let request = AgentRequest::default();
    let exchange = opening_exchange(&request, false);
    for choice in every_reasoning_choice() {
        let endpoint =
            anthropic_endpoint("https://example.invalid", "claude-opus-5-5", choice.clone());
        let built = build_completion_request(&request, &exchange, &endpoint);
        assert_eq!(built.additional_params, None, "{choice:?}");
        assert_eq!(
            built.max_tokens, None,
            "the limit belongs to the model, not to the shared request"
        );

        let mut openai = endpoint.clone();
        openai.provider = "openai".into();
        let built = build_completion_request(&request, &exchange, &openai);
        assert_eq!(
            built.additional_params.is_some(),
            choice.is_some(),
            "{choice:?}"
        );
    }
}

/// Serve exactly one HTTP request on loopback with a refusal, and hand back the
/// whole request: the request line, the headers, and the body. A connection
/// that sends nothing gives an empty string.
fn refuse_once() -> (
    std::net::SocketAddr,
    std::thread::JoinHandle<String>,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
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
        let body =
            r#"{"type":"error","error":{"type":"invalid_request_error","message":"refused"}}"#;
        let _ = write!(
            stream,
            "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        String::from_utf8_lossy(&received).to_string()
    });
    (addr, handle, accepted)
}

// CVL-FR-KXTQ, CVL-FR-HBNW: a turn on a model that `rig` does not know, with a
// reasoning choice, reaches the Messages API. The body states the limit and
// holds no reasoning field. The base URL is the provider default shape, which
// ends in `/v1`.
#[test]
fn an_anthropic_call_reaches_the_messages_api_with_the_limit_and_no_reasoning() {
    let (addr, server, accepted) = refuse_once();
    let endpoint = anthropic_endpoint(
        &format!("http://{addr}/v1"),
        "claude-opus-5-5",
        Some(ReasoningChoice::Effort {
            effort: "high".into(),
        }),
    );
    let request = wire_request();
    let exchange = opening_exchange(&request, false);
    let outcome = RigCompletion.complete(&request, &exchange, &endpoint, Duration::from_secs(20));
    // A call refused before it is sent leaves the server waiting for a
    // connection. This empty one releases it, so that case fails below and does
    // not hang. It is made only while the server still waits, so it cannot reach
    // a listener another test bound to the same port after this one closed.
    if !accepted.load(std::sync::atomic::Ordering::SeqCst) {
        drop(TcpStream::connect(addr));
    }
    let seen = server.join().unwrap();

    let first = seen.lines().next().unwrap_or_default();
    assert!(
        first.starts_with("POST /v1/messages "),
        "no request reached the server: {outcome:?}"
    );
    assert!(
        seen.to_lowercase()
            .contains(&format!("x-api-key: {}", SECRET.to_lowercase())),
        "the key travels in the Anthropic header",
    );
    let body = seen.split("\r\n\r\n").nth(1).unwrap_or_default();
    let json: serde_json::Value = serde_json::from_str(body).expect("the body is JSON");
    assert_eq!(json["model"], "claude-opus-5-5");
    assert_eq!(json["max_tokens"], 32_000);
    assert!(json.get("reasoning").is_none(), "{body}");
    // CVL-FR-23: the short retention, at the boundary the provider advances.
    assert_eq!(
        json["cache_control"],
        serde_json::json!({ "type": "ephemeral" }),
        "{body}"
    );
    // CVL-FR-HBNW: the framework's default for a model its catalog marks for
    // it is adaptive thinking.
    assert_eq!(json["thinking"]["type"], "adaptive", "{body}");

    // CVL-FR-35: the refusal is recorded in the provider's own terms.
    let failure = outcome.expect_err("the loopback server refuses");
    assert_eq!(failure.status, Some(400));
    assert_eq!(failure.provider_code.as_deref(), Some("invalid_request_error"));
    assert_eq!(failure.provider_message.as_deref(), Some("refused"));
    assert!(
        !format!("{failure:?}").contains(SECRET),
        "no secret in the failure"
    );
}
