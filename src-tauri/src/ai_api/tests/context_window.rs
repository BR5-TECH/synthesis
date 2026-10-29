//! The context window of a model (AAP-FR-RANS, AAP-FR-LADB, AAP-FR-RICY,
//! AAP-FR-BKBF).

use super::*;
use crate::ai_shared::parse_models_payload;

fn sized(id: &str, size: Option<u64>) -> ModelOption {
    ModelOption { detected_context_size: size, ..ModelOption::new(id, id) }
}

// AAP-FR-LADB: an Anthropic- or OpenAI-shaped entry gives its window where it
// carries a context-window field, and no window where it does not.
#[test]
fn a_models_payload_gives_the_reported_window() {
    let models = parse_models_payload(
        r#"{"data":[
            {"id":"a","max_input_tokens":200000},
            {"id":"b","context_window":128000},
            {"id":"c","context_length":32768},
            {"id":"d"},
            {"id":"e","max_input_tokens":0},
            {"id":"f","context_window":"lots"},
            {"id":"g","context_window":-5}
        ]}"#,
    )
    .expect("a list");
    let size = |id: &str| models.iter().find(|m| m.id == id).unwrap().detected_context_size;
    assert_eq!(size("a"), Some(200_000));
    assert_eq!(size("b"), Some(128_000));
    assert_eq!(size("c"), Some(32_768));
    for none in ["d", "e", "f", "g"] {
        assert_eq!(size(none), None, "{none} reports no window");
    }
}

// AAP-FR-RANS, AAP-FR-BKBF: the window of a model of a non-Custom provider is
// its detected size, read from the registry alone.
#[test]
fn the_window_of_a_listed_model_is_its_detected_size() {
    let prober = Arc::new(FakeProber::default());
    prober.set_rich_models(vec![sized("big/model", Some(1_000_000)), sized("plain/model", None)]);
    let h = harness(prober, FakeKeychain::new());
    verify_openrouter(&h);
    let reads_before = h.keys.reads();

    assert_eq!(resolve_context_window(&h.store, "openrouter", "big/model"), Some(1_000_000));
    // AAP-FR-BKBF: no effective size, an unlisted model, and an unconfigured
    // provider all answer null and refuse nothing.
    assert_eq!(resolve_context_window(&h.store, "openrouter", "plain/model"), None);
    assert_eq!(resolve_context_window(&h.store, "openrouter", "no/such"), None);
    assert_eq!(resolve_context_window(&h.store, "anthropic", "claude"), None);
    assert_eq!(resolve_context_window(&h.store, "nonsense", "x"), None);
    // No key is read on any of these paths.
    assert_eq!(h.keys.reads(), reads_before);
}

// AAP-FR-RANS: the endpoint of an agent's turn carries the same window for a
// non-Custom provider.
#[test]
fn an_agent_endpoint_carries_the_detected_window_of_any_provider() {
    let prober = Arc::new(FakeProber::default());
    prober.set_rich_models(vec![sized("big/model", Some(400_000))]);
    let h = harness(prober, FakeKeychain::new());
    verify_openrouter(&h);
    let call = resolve_ai_api_endpoint(&h.store, &h.ai, "openrouter", "big/model", None).unwrap();
    assert_eq!(call.context_size, Some(400_000));
}

// AAP-FR-RICY: each verification replaces the detected size of every model.
#[test]
fn each_verification_replaces_the_detected_window() {
    let prober = Arc::new(FakeProber::default());
    prober.set_rich_models(vec![sized("m", Some(8_192))]);
    let h = harness(prober.clone(), FakeKeychain::new());
    verify_openrouter(&h);
    assert_eq!(resolve_context_window(&h.store, "openrouter", "m"), Some(8_192));

    // A probe that reports no window is not an error, and the model list is
    // otherwise as the probe gave it.
    prober.set_rich_models(vec![sized("m", None), sized("n", None)]);
    verify_openrouter(&h);
    assert_eq!(resolve_context_window(&h.store, "openrouter", "m"), None);
    let (records, _) = h.store.load_ai_api_registry().unwrap();
    let record = records.iter().find(|r| r.provider == "openrouter").unwrap();
    let listed: Vec<(&str, &str)> =
        record.models.iter().map(|m| (m.id.as_str(), m.label.as_str())).collect();
    assert_eq!(listed, [("m", "m"), ("n", "n")]);
}

// AAP-FR-BKBF: a provider that is not configured answers null, also for a
// model it lists with a detected size.
#[test]
fn an_unconfigured_provider_has_no_window_for_a_listed_model() {
    let prober = Arc::new(FakeProber::default());
    prober.set_rich_models(vec![sized("big/model", Some(1_000_000))]);
    let h = harness(prober, FakeKeychain::new());
    verify_openrouter(&h);
    let (mut records, active) = h.store.load_ai_api_registry().unwrap();
    let record = records.iter_mut().find(|r| r.provider == "openrouter").unwrap();
    record.masked_hint = None;
    record.base_url = None;
    record.verified_at = None;
    assert_eq!(state_assuming_key(record, true), IntegrationState::Unconfigured);
    assert_eq!(record.models[0].detected_context_size, Some(1_000_000), "still listed");
    h.store.save_ai_api_registry(records, active).unwrap();
    assert_eq!(resolve_context_window(&h.store, "openrouter", "big/model"), None);
}

// AAP-FR-RICY: a model from the bundled catalog carries no window.
#[test]
fn a_catalog_model_carries_no_window() {
    let mut checked = 0;
    for provider in ["anthropic", "openai", "openrouter"] {
        let descriptor = provider_descriptor(provider).expect("a known provider");
        for model in catalog_models(descriptor) {
            assert_eq!(model.detected_context_size, None, "{provider}/{}", model.id);
            checked += 1;
        }
    }
    assert!(checked > 0, "the bundled catalog holds models to check");
}

// AAP-FR-RANS: a Custom model falls back to the provider's max context size,
// and no other provider does.
#[test]
fn only_custom_falls_back_to_the_max_context_size() {
    let mut record = AiApiRecord::empty("openai");
    record.max_context_size = Some(9_999);
    assert_eq!(effective_context_size(&record, Some(&sized("m", None))), None);
    assert_eq!(effective_context_size(&record, Some(&sized("m", Some(5)))), Some(5));

    let mut custom = AiApiRecord::empty(CUSTOM);
    custom.max_context_size = Some(9_999);
    assert_eq!(effective_context_size(&custom, Some(&sized("m", None))), Some(9_999));
    assert_eq!(effective_context_size(&custom, Some(&sized("m", Some(5)))), Some(5));
    assert_eq!(effective_context_size(&custom, None), Some(9_999));
}
