//! The cache key of an `openai` call and the cached split that an
//! `openrouter` call reports (`CVL-conversation-loop.md` CVL-FR-DNCX,
//! CVL-FR-IRYL).

use super::*;

fn endpoint(provider: &str) -> AiApiCall {
    AiApiCall {
        max_output_tokens: None,
        context_size: crate::ai_api::DEFAULT_CONTEXT_SIZE,
        turn_timeout_ms: None,
        provider: provider.into(),
        base_url: "https://example.invalid".into(),
        api_key: None,
        model_id: Some("m".into()),
        reasoning: None,
        accepts_image_input: false,
        model_mode: None,
    }
}

fn keyed_request() -> AgentRequest {
    AgentRequest {
        cache_key: Some(conversation_cache_key("agent-1", "discussion-1")),
        ..AgentRequest::default()
    }
}

// CVL-FR-DNCX: the key is 32 hexadecimal characters, the same for the same
// agent and discussion, different for another, and it holds neither id.
#[test]
fn the_cache_key_is_a_stable_hash_of_the_two_ids() {
    let key = conversation_cache_key("agent-1", "discussion-1");
    assert_eq!(key.len(), 32);
    assert!(key.chars().all(|c| c.is_ascii_hexdigit()), "{key}");
    assert_eq!(key, conversation_cache_key("agent-1", "discussion-1"));
    assert_ne!(key, conversation_cache_key("agent-2", "discussion-1"));
    assert_ne!(key, conversation_cache_key("agent-1", "discussion-2"));
    // The separator keeps two different pairs with the same joined text apart.
    assert_ne!(conversation_cache_key("ab", "c"), conversation_cache_key("a", "bc"));
    assert!(!key.contains("agent") && !key.contains("discussion"));
}

// CVL-FR-DNCX: an `openai` call holds the key beside its other parameters.
#[test]
fn an_openai_call_holds_the_prompt_cache_key() {
    let request = keyed_request();
    let mut openai = endpoint("openai");
    openai.reasoning = Some(crate::ai_api::ReasoningChoice::Effort { effort: "low".into() });
    let built = build_completion_request(&request, &opening_exchange(&request, false), &openai);
    let params = built.additional_params.expect("parameters");
    assert_eq!(params["prompt_cache_key"], serde_json::json!(request.cache_key.unwrap()));
    assert_eq!(params["reasoning"]["effort"], "low", "the reasoning stays");
}

// CVL-FR-DNCX: a call of another provider holds no key, and a request with no
// key gives none.
#[test]
fn no_other_provider_holds_the_key() {
    let request = keyed_request();
    for provider in ["anthropic", "custom", "openrouter"] {
        let built = build_completion_request(
            &request,
            &opening_exchange(&request, false),
            &endpoint(provider),
        );
        let has_key =
            built.additional_params.as_ref().is_some_and(|p| p.get("prompt_cache_key").is_some());
        assert!(!has_key, "{provider} holds no key");
    }
    let plain = AgentRequest::default();
    let built =
        build_completion_request(&plain, &opening_exchange(&plain, false), &endpoint("openai"));
    assert!(built.additional_params.is_none_or(|p| p.get("prompt_cache_key").is_none()));
}

fn usage(json: serde_json::Value) -> openrouter_rs::types::completion::ResponseUsage {
    serde_json::from_value(json).expect("usage")
}

// CVL-FR-IRYL: the cached count of the prompt details gives the split, and the
// rest of the prompt tokens is uncached.
#[test]
fn an_openrouter_usage_report_gives_the_cached_split() {
    let reported = usage(serde_json::json!({
        "prompt_tokens": 1000, "completion_tokens": 10, "total_tokens": 1010,
        "prompt_tokens_details": { "cached_tokens": 800, "cache_write_tokens": 150 }
    }));
    assert_eq!(input_split(&reported), Some(InputTokens { cached: 800, uncached: 200 }));
    assert_eq!(reported.prompt_tokens_details.unwrap().cache_write_tokens, Some(150));
}

// CVL-FR-IRYL: a report with no cached count gives no split, so the turn does
// not claim that caching did not land.
#[test]
fn a_report_with_no_cached_count_gives_no_split() {
    let none = usage(serde_json::json!({
        "prompt_tokens": 1000, "completion_tokens": 10, "total_tokens": 1010
    }));
    assert_eq!(input_split(&none), None);
    let no_cached = usage(serde_json::json!({
        "prompt_tokens": 1000, "completion_tokens": 10, "total_tokens": 1010,
        "prompt_tokens_details": { "audio_tokens": 0 }
    }));
    assert_eq!(input_split(&no_cached), None);
}
