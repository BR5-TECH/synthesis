//! The Custom LLM Gateway: address rules, model discovery, route metadata, and
//! the provider an agent is served by
//! (`AAP-ai-api-integrations.md` AAP-FR-KRVT, AAP-FR-MDLQ, AAP-FR-RTMZ,
//! AAP-FR-APRV).

use super::*;
use crate::ai_shared::{parse_gateway_models_payload, ModelMode, ModelsFormat};

const SECRET: &str = "gw-secret-ABCD1234";

fn gateway_models() -> Vec<ModelOption> {
    vec![
        ModelOption::new("chat-only", "chat-only").with_mode(Some(ModelMode::Chat)),
        ModelOption::new("resp-only", "resp-only").with_mode(Some(ModelMode::Responses)),
        ModelOption::new("both", "both"),
    ]
}

fn gateway_harness() -> Harness {
    let h = harness(FakeProber::returning(&[]), FakeKeychain::new());
    h.prober.set_rich_models(gateway_models());
    h
}

fn verify_gateway(h: &Harness, url: &str) -> Result<AiApiIntegration, String> {
    verify_integration_impl(&h.store, &h.ai, "custom", url, Some(SECRET))
}

// AAP-FR-KRVT, AAP-FR-MDLQ: discovery goes to `<base>/v1/models` with the
// bearer secret, over the normalised root.
#[test]
fn discovery_uses_the_v1_models_route_over_the_normalised_root_with_bearer_auth() {
    let h = gateway_harness();
    let rec = verify_gateway(&h, "  https://gw.example.com///  ").unwrap();

    assert_eq!(rec.base_url.as_deref(), Some("https://gw.example.com"));
    let calls = h.prober.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "https://gw.example.com/v1/models");
    assert_eq!(calls[0].1.as_deref(), Some(SECRET));
    assert_eq!(calls[0].2, AuthStyle::Bearer);
    let custom = provider_descriptor("custom").unwrap();
    assert_eq!(custom.models_format, ModelsFormat::Gateway);
    for named in ["openrouter", "anthropic", "openai"] {
        assert_eq!(provider_descriptor(named).unwrap().models_format, ModelsFormat::Lenient);
    }
}

// AAP-FR-KRVT: a root that already carries `/v1` is refused before any request.
#[test]
fn a_root_ending_in_v1_is_refused_without_a_request() {
    let h = gateway_harness();
    for url in [
        "https://gw.example.com/v1",
        "https://gw.example.com/v1/",
        "https://gw.example.com/team/V1//",
    ] {
        assert_eq!(verify_gateway(&h, url).unwrap_err(), ERR_BASE_URL_INVALID, "{url}");
    }
    assert_eq!(verify_gateway(&h, "").unwrap_err(), ERR_BASE_URL_EMPTY);
    assert_eq!(verify_gateway(&h, "ftp://gw.example.com").unwrap_err(), ERR_BASE_URL_INVALID);
    assert!(h.prober.calls.lock().unwrap().is_empty());
    // A path that merely starts with the letters is a different path.
    assert!(verify_gateway(&h, "https://gw.example.com/v10").is_ok());
    assert!(custom_gateway::has_v1_suffix("https://gw.example.com/v1"));
    assert!(!custom_gateway::has_v1_suffix("https://gw.example.com"));
    assert_eq!(
        custom_gateway::adapter_base_url("https://gw.example.com/"),
        "https://gw.example.com/v1"
    );
}

// AAP-FR-MDLQ, AAP-FR-RTMZ: a valid list with absent, null, and explicit `mode`.
#[test]
fn the_model_list_parses_absent_null_and_explicit_mode() {
    let body = r#"{
      "object": "list",
      "data": [
        {"id": "a", "object": "model"},
        {"id": "b", "object": "model", "mode": null},
        {"id": "c", "object": "model", "mode": "chat"},
        {"id": "d", "object": "model", "mode": "responses",
         "capabilities": {"reasoning": {"supported_efforts": ["low"]}},
         "input_modalities": ["image"]}
      ]
    }"#;
    let models = parse_gateway_models_payload(body).unwrap();
    let modes: Vec<_> = models.iter().map(|m| (m.id.as_str(), m.mode)).collect();
    assert_eq!(
        modes,
        vec![
            ("a", None),
            ("b", None),
            ("c", Some(ModelMode::Chat)),
            ("d", Some(ModelMode::Responses)),
        ]
    );
    // Nothing is inferred from anything but the standard fields.
    assert!(models.iter().all(|m| m.reasoning.is_none()));
    assert!(models.iter().all(|m| !m.accepts_image_input));
}

// AAP-FR-MDLQ: every malformed shape is rejected whole.
#[test]
fn a_malformed_model_list_is_rejected_whole() {
    for (label, body) in [
        ("not json", "<html>"),
        ("wrong object", r#"{"object":"models","data":[]}"#),
        ("missing object", r#"{"data":[]}"#),
        ("data not an array", r#"{"object":"list","data":{}}"#),
        ("missing data", r#"{"object":"list"}"#),
        (
            "entry without id",
            r#"{"object":"list","data":[{"id":"a","object":"model"},{"object":"model"}]}"#,
        ),
        ("numeric id", r#"{"object":"list","data":[{"id":1,"object":"model"}]}"#),
        ("entry without object", r#"{"object":"list","data":[{"id":"a"}]}"#),
        (
            "entry with another object",
            r#"{"object":"list","data":[{"id":"a","object":"thing"}]}"#,
        ),
        (
            "unknown mode",
            r#"{"object":"list","data":[{"id":"a","object":"model","mode":"both"}]}"#,
        ),
        (
            "numeric mode",
            r#"{"object":"list","data":[{"id":"a","object":"model","mode":1}]}"#,
        ),
    ] {
        assert!(parse_gateway_models_payload(body).is_none(), "{label}");
    }
    assert_eq!(
        parse_gateway_models_payload(r#"{"object":"list","data":[]}"#),
        Some(vec![]),
        "an empty list is a valid list"
    );
}

// AAP-FR-MDLQ, AAP-FR-06: a failed verification leaves the last verified
// configuration, its models, and its secret exactly as they were.
#[test]
fn a_malformed_response_leaves_the_last_verified_configuration_unchanged() {
    let h = gateway_harness();
    verify_gateway(&h, "https://gw.example.com").unwrap();
    let before = list_integrations_impl(&h.store, &h.ai).unwrap();

    *h.prober.error.lock().unwrap() = Some(ProbeError::NotExpectedKind);
    let err = verify_integration_impl(
        &h.store,
        &h.ai,
        "custom",
        "https://other.example.com",
        Some("another-secret-9999"),
    )
    .unwrap_err();
    assert_eq!(err, ERR_NOT_AN_AI_ENDPOINT);
    assert!(!err.contains("another-secret"), "no secret in the failure");

    let after = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(before, after);
    assert_eq!(h.keys.get_raw("custom").as_deref(), Some(SECRET));
}

// AAP-FR-RTMZ: the route metadata is stored with the model list and returned by
// both read paths for the resolved model, and survives the TOML store.
#[test]
fn mode_is_stored_listed_and_returned_by_both_read_paths() {
    let h = gateway_harness();
    let rec = verify_gateway(&h, "https://gw.example.com").unwrap();
    let modes: Vec<_> = rec.models.iter().map(|m| m.mode).collect();
    assert_eq!(modes, vec![Some(ModelMode::Chat), Some(ModelMode::Responses), None]);
    set_active_impl(&h.store, &h.ai, "custom").unwrap();

    for (model, expected) in [
        ("chat-only", Some(ModelMode::Chat)),
        ("resp-only", Some(ModelMode::Responses)),
        ("both", None),
    ] {
        let endpoint =
            resolve_ai_api_endpoint(&h.store, &h.ai, "custom", model, None).unwrap();
        assert_eq!(endpoint.model_mode, expected, "{model}");
        assert_eq!(endpoint.base_url, "https://gw.example.com");
        assert_eq!(endpoint.api_key.as_deref(), Some(SECRET));
        set_model_impl(&h.store, &h.ai, "custom", Some(model)).unwrap();
        let call = resolve_ai_api_call(&h.store, &h.ai, "proj").unwrap();
        assert_eq!(call.model_mode, expected, "{model}");
    }

    let (records, _) = h.store.load_ai_api_registry().unwrap();
    let text = toml::to_string(&records[0]).unwrap();
    let back: AiApiRecord = toml::from_str(&text).unwrap();
    assert_eq!(back, records[0], "{text}");
    assert!(text.contains("mode = \"chat\""));
    assert!(!text.contains(SECRET));
}

// AAP-FR-25: a Custom model declares no reasoning, so an agent cannot ask for it.
#[test]
fn a_custom_model_offers_no_reasoning() {
    let h = gateway_harness();
    verify_gateway(&h, "https://gw.example.com").unwrap();
    assert!(h.store.load_ai_api_registry().unwrap().0[0]
        .models
        .iter()
        .all(|m| m.reasoning.is_none()));
    assert_eq!(
        resolve_ai_api_endpoint(
            &h.store,
            &h.ai,
            "custom",
            "both",
            Some(ReasoningChoice::On)
        )
        .unwrap_err(),
        ERR_REASONING_UNSUPPORTED,
    );
}

// AAP-FR-07, AAP-FR-08: the secret reaches no stored record, no listing, no
// debug output, and no failure.
#[test]
fn the_secret_is_redacted_everywhere_but_the_vault_and_the_call() {
    let h = gateway_harness();
    verify_gateway(&h, "https://gw.example.com").unwrap();
    set_active_impl(&h.store, &h.ai, "custom").unwrap();

    let listing = serde_json::to_string(&list_integrations_impl(&h.store, &h.ai).unwrap()).unwrap();
    assert!(!listing.contains(SECRET));
    assert!(listing.contains("1234"), "only the masked hint");
    let stored = toml::to_string(&h.store.load_ai_api_registry().unwrap().0[0]).unwrap();
    assert!(!stored.contains(SECRET));
    let catalogs = serde_json::to_string(&list_catalogs_impl(&h.store).unwrap()).unwrap();
    assert!(!catalogs.contains(SECRET));

    let endpoint = resolve_ai_api_endpoint(&h.store, &h.ai, "custom", "both", None).unwrap();
    assert_eq!(endpoint.api_key.as_deref(), Some(SECRET));
    assert!(!format!("{endpoint:?}").contains(SECRET));
    assert!(!format!("{:?}", resolve_ai_api_call(&h.store, &h.ai, "p")).contains(SECRET));

    h.prober.error.lock().unwrap().replace(ProbeError::Rejected);
    let err = verify_gateway(&h, "https://gw.example.com").unwrap_err();
    assert!(!err.contains(SECRET));
}

// AAP-FR-APRV: the provider an agent is served by comes from the registry alone.
#[test]
fn the_agent_provider_follows_the_project_resolution_without_touching_the_keychain() {
    let h = gateway_harness();
    // Nothing configured.
    assert_eq!(resolve_agent_provider(&h.store, "").unwrap_err(), ERR_PROVIDER_UNCONFIGURED);
    let none = active_catalog_impl(&h.store, "").unwrap();
    assert_eq!(none.resolution, ProjectResolution::NoneConfigured);
    assert!(none.catalog.is_none());

    // One verified provider resolves implicitly.
    verify_gateway(&h, "https://gw.example.com").unwrap();
    let reads = h.keys.reads();
    assert_eq!(resolve_agent_provider(&h.store, "").unwrap(), "custom");
    let active = active_catalog_impl(&h.store, "").unwrap();
    assert_eq!(active.resolution, ProjectResolution::Inherited);
    let catalog = active.catalog.unwrap();
    assert_eq!(catalog.provider, "custom");
    assert_eq!(catalog.models.len(), 3);
    assert_eq!(h.keys.reads(), reads, "no key was read");

    // A second verified provider with no choice leaves nothing resolved.
    verify_integration_impl(&h.store, &h.ai, "openai", "https://api.openai.com/v1", Some("sk-1234"))
        .unwrap();
    assert_eq!(resolve_agent_provider(&h.store, "").unwrap_err(), ERR_NOT_VERIFIED);
    assert_eq!(
        active_catalog_impl(&h.store, "").unwrap().resolution,
        ProjectResolution::NoneSelected
    );

    // An explicit activation, then a project override, move the provider.
    h.store
        .save_ai_api_registry(h.store.load_ai_api_registry().unwrap().0, Some("openai".into()))
        .unwrap();
    assert_eq!(resolve_agent_provider(&h.store, "").unwrap(), "openai");
    h.store.save_ai_api_override("proj", "custom").unwrap();
    assert_eq!(resolve_agent_provider(&h.store, "proj").unwrap(), "custom");
    assert_eq!(
        active_catalog_impl(&h.store, "proj").unwrap().resolution,
        ProjectResolution::Overridden
    );
    assert_eq!(resolve_agent_provider(&h.store, "other").unwrap(), "openai");

    // Even with the keychain shut, an agent can still be edited.
    h.keys.lock_it();
    let reads_after_lock = h.keys.reads();
    assert_eq!(resolve_agent_provider(&h.store, "other").unwrap(), "openai");
    assert_eq!(h.keys.reads(), reads_after_lock);
}

// AAP-FR-MDLQ, AAP-FR-ADPX: the production prober, against a loopback server,
// asks for `GET /v1/models` with the bearer secret and reads the answer
// strictly.
#[test]
fn the_production_prober_sends_the_bearer_secret_to_v1_models_and_reads_strictly() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use crate::ai_shared::HttpEndpointProber;

    fn serve(body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut received = vec![0u8; 4096];
            let n = stream.read(&mut received).unwrap();
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            String::from_utf8_lossy(&received[..n]).to_string()
        });
        (url, handle)
    }
    let probe = |url: &str| {
        HttpEndpointProber.probe(&ProbeRequest {
            base_url: url,
            api_key: Some(SECRET),
            auth: AuthStyle::Bearer,
            models_path: "/v1/models",
            models_format: ModelsFormat::Gateway,
        })
    };

    let (url, server) = serve(r#"{"object":"list","data":[{"id":"a","object":"model","mode":"chat"},{"id":"b","object":"model"}]}"#);
    let models = probe(&url).unwrap();
    let seen = server.join().unwrap();
    assert!(seen.starts_with("GET /v1/models "), "{seen}");
    assert!(
        seen.to_lowercase().contains(&format!("authorization: bearer {}", SECRET.to_lowercase())),
        "{seen}"
    );
    assert_eq!(models.iter().map(|m| m.mode).collect::<Vec<_>>(), vec![Some(ModelMode::Chat), None]);

    let (url, server) = serve(r#"{"object":"list","data":[{"id":"a"}]}"#);
    assert_eq!(probe(&url).unwrap_err(), ProbeError::NotExpectedKind);
    server.join().unwrap();
}
