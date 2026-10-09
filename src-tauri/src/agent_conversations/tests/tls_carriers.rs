//! A refused certificate through each production client (`AAP-ai-api-integrations.md`
//! AAP-FR-WMCX, AAP-FR-HZTB, AAP-FR-BDKS; `CVL-conversation-loop.md` CVL-FR-21).
//!
//! A loopback HTTPS server holds a certificate from a private root, which the
//! application does not trust. Each provider's client must report the refusal
//! as the typed TLS failure, with the host and the cause.

use super::*;
use crate::tls::tests::{private_ca_for_tests, serve_for_tests};

fn endpoint(provider: &str, base_url: &str) -> AiApiCall {
    AiApiCall {
        provider: provider.into(),
        base_url: base_url.into(),
        api_key: Some("sk-secret-key-123".into()),
        model_id: Some("some-model".into()),
        reasoning: None,
        accepts_image_input: false,
        model_mode: None,
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

// AAP-FR-WMCX, AAP-FR-HZTB, AAP-FR-BDKS, CVL-FR-21, AGC-FR-RWPT
#[test]
fn every_provider_client_reports_an_untrusted_certificate_as_the_typed_failure() {
    let addr = serve_for_tests(&private_ca_for_tests(), &["localhost"]);
    let base_url = format!("https://localhost:{}", addr.port());
    for provider in ["anthropic", "openai", "custom", "openrouter"] {
        let endpoint = endpoint(provider, &base_url);
        let request = request();
        let exchange = opening_exchange(&request, false);
        let failure = RigCompletion
            .complete(&request, &exchange, &endpoint, Duration::from_secs(30))
            .expect_err("the certificate is from no trusted root");
        assert_eq!(failure.failure, FAIL_TLS_UNTRUSTED, "{provider}");
        assert_eq!(failure.class, class::TLS, "{provider}");
        assert_eq!(
            failure.tls,
            Some(crate::tls::TlsFailure::new(
                "localhost",
                crate::tls::TlsCause::UnknownIssuer
            )),
            "{provider}",
        );
        assert!(failure.recoverable(), "{provider}: the author can fix the trust and retry");
        assert!(!failure.repeatable(), "{provider}: the same certificate is not asked for again");
    }
}
