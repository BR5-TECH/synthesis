//! AAP-FR-FGNK, AAP-FR-TXNM, GSS-FR-27: the turn timeout a provider stores.

use super::*;

/// A verified OpenRouter that offers `a/model`, so the read paths resolve.
fn verified_harness() -> Harness {
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(vec![with_ladder("a/model", &["high", "low"], None, false)]);
    let h = harness(prober, FakeKeychain::new());
    verify_openrouter(&h);
    set_model_impl(&h.store, &h.ai, "openrouter", Some("a/model")).unwrap();
    h
}

// AAP-FR-FGNK: a value in range is stored, returned, and read back later.
#[test]
fn a_turn_timeout_is_stored_and_read_back() {
    let h = verified_harness();

    let rec = set_turn_timeout_impl(&h.store, &h.ai, "openrouter", Some(600_000)).unwrap();
    assert_eq!(rec.turn_timeout_ms, Some(600_000));

    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(find(&list, "openrouter").turn_timeout_ms, Some(600_000));
}

// AAP-FR-FGNK: null clears the stored value.
#[test]
fn null_clears_the_turn_timeout() {
    let h = verified_harness();
    set_turn_timeout_impl(&h.store, &h.ai, "openrouter", Some(600_000)).unwrap();

    let rec = set_turn_timeout_impl(&h.store, &h.ai, "openrouter", None).unwrap();

    assert_eq!(rec.turn_timeout_ms, None);
    let (records, _) = h.store.load_ai_api_registry().unwrap();
    assert_eq!(records[0].turn_timeout_ms, None);
}

// AAP-FR-FGNK: the bounds are inclusive, and a value outside them is refused
// with the typed error and changes nothing.
#[test]
fn a_value_outside_the_range_is_refused_and_changes_nothing() {
    let h = verified_harness();
    set_turn_timeout_impl(&h.store, &h.ai, "openrouter", Some(120_000)).unwrap();
    let (before, _) = h.store.load_ai_api_registry().unwrap();

    for refused in [0, 29_999, 3_600_001, u64::MAX] {
        assert_eq!(
            set_turn_timeout_impl(&h.store, &h.ai, "openrouter", Some(refused)).unwrap_err(),
            ERR_TURN_TIMEOUT_OUT_OF_RANGE,
            "{refused}",
        );
    }
    let (after, _) = h.store.load_ai_api_registry().unwrap();
    assert_eq!(after, before, "a refusal writes nothing");

    for accepted in [TURN_TIMEOUT_MIN_MS, TURN_TIMEOUT_MAX_MS] {
        let rec = set_turn_timeout_impl(&h.store, &h.ai, "openrouter", Some(accepted)).unwrap();
        assert_eq!(rec.turn_timeout_ms, Some(accepted));
    }
    assert_eq!(ERR_TURN_TIMEOUT_OUT_OF_RANGE, "turn_timeout_out_of_range");
}

// AAP-FR-FGNK: setting one provider's value leaves every other provider, and
// the rest of its own record, untouched.
#[test]
fn setting_one_provider_leaves_the_others_and_its_own_selections_untouched() {
    let h = verified_harness();
    set_reasoning_impl(
        &h.store,
        &h.ai,
        "openrouter",
        Some(ReasoningChoice::Effort {
            effort: "low".into(),
        }),
    )
    .unwrap();
    set_turn_timeout_impl(&h.store, &h.ai, "anthropic", Some(90_000)).unwrap();
    let before = find(&list_integrations_impl(&h.store, &h.ai).unwrap(), "openrouter").clone();

    set_turn_timeout_impl(&h.store, &h.ai, "openrouter", Some(600_000)).unwrap();

    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    let after = find(&list, "openrouter");
    assert_eq!(
        AiApiIntegration {
            turn_timeout_ms: before.turn_timeout_ms,
            ..after.clone()
        },
        before,
        "nothing but the timeout changed",
    );
    assert_eq!(find(&list, "anthropic").turn_timeout_ms, Some(90_000));
    assert_eq!(find(&list, "openai").turn_timeout_ms, None);
}

// AAP-FR-FGNK: a provider with no record gets one holding the timeout alone,
// which configures, verifies, and activates nothing.
#[test]
fn a_provider_with_no_record_gets_one_that_holds_only_the_timeout() {
    let h = ok_harness();

    let rec = set_turn_timeout_impl(&h.store, &h.ai, "custom", Some(45_000)).unwrap();

    assert_eq!(rec.turn_timeout_ms, Some(45_000));
    assert_eq!(rec.state, IntegrationState::Unconfigured);
    assert!(!rec.active);
    assert_eq!(rec.base_url, None);
}

// AAP-FR-FGNK: an unknown provider is refused before the range is read, and
// no record is created.
#[test]
fn an_unknown_provider_is_refused_first() {
    let h = ok_harness();
    for value in [Some(60_000), Some(1), None] {
        assert_eq!(
            set_turn_timeout_impl(&h.store, &h.ai, "bedrock", value).unwrap_err(),
            ERR_UNKNOWN_PROVIDER,
        );
    }
    let (records, _) = h.store.load_ai_api_registry().unwrap();
    assert!(records.is_empty());
}

// AAP-FR-TXNM: the endpoint read path returns the stored value, or none.
#[test]
fn the_endpoint_carries_the_stored_turn_timeout() {
    let h = verified_harness();
    let none = resolve_ai_api_endpoint(&h.store, &h.ai, "openrouter", "a/model", None).unwrap();
    assert_eq!(none.turn_timeout_ms, None);

    set_turn_timeout_impl(&h.store, &h.ai, "openrouter", Some(600_000)).unwrap();
    let call = resolve_ai_api_endpoint(&h.store, &h.ai, "openrouter", "a/model", None).unwrap();
    assert_eq!(call.turn_timeout_ms, Some(600_000));
}

// AAP-FR-TXNM: no other read path returns it.
#[test]
fn the_graduation_read_path_never_carries_it() {
    let h = verified_harness();
    set_turn_timeout_impl(&h.store, &h.ai, "openrouter", Some(600_000)).unwrap();

    let run = resolve_ai_api_call(&h.store, &h.ai, "/dev/acme").unwrap();

    assert_eq!(run.provider, "openrouter");
    assert_eq!(run.turn_timeout_ms, None);
}

// GSS-FR-27: the value survives the TOML store beside a reasoning table and the
// model array, under its camel-case key, and a record written without it loads.
#[test]
fn the_turn_timeout_round_trips_through_the_toml_store() {
    let record = AiApiRecord {
        turn_timeout_ms: Some(600_000),
        provider: "openrouter".into(),
        base_url: Some("https://openrouter.ai/api/v1".into()),
        masked_hint: Some("1234".into()),
        verified_at: Some("2026-07-31T00:00:00Z".into()),
        selected_model: Some("a/model".into()),
        models_origin: ModelsOrigin::Probed,
        selected_reasoning: Some(ReasoningChoice::Effort {
            effort: "high".into(),
        }),
        models: vec![with_ladder("a/model", &["high"], Some("high"), false)],
    };
    let text = toml::to_string(&record).unwrap();
    assert!(text.contains("turnTimeoutMs = 600000"), "{text}");
    let back: AiApiRecord = toml::from_str(&text).unwrap();
    assert_eq!(back, record, "the record survives the store intact:\n{text}");

    let older = text.replace("turnTimeoutMs = 600000\n", "");
    let loaded: AiApiRecord = toml::from_str(&older).unwrap();
    assert_eq!(loaded.turn_timeout_ms, None);
    assert_eq!(loaded.selected_model.as_deref(), Some("a/model"));

    let cleared = toml::to_string(&AiApiRecord {
        turn_timeout_ms: None,
        ..record
    })
    .unwrap();
    assert!(!cleared.contains("turnTimeoutMs"), "{cleared}");
}

// AAP-FR-08: the outbound record carries the field under its IPC name.
#[test]
fn the_outbound_record_names_the_field_for_the_frontend() {
    let h = verified_harness();
    let rec = set_turn_timeout_impl(&h.store, &h.ai, "openrouter", Some(600_000)).unwrap();
    let json = serde_json::to_value(&rec).unwrap();
    assert_eq!(json["turnTimeoutMs"], 600_000);
}

// AAP-FR-TXNM: each provider's endpoint carries its own value.
#[test]
fn each_endpoint_carries_its_own_providers_value() {
    let h = verified_harness();
    set_turn_timeout_impl(&h.store, &h.ai, "openrouter", Some(600_000)).unwrap();
    set_turn_timeout_impl(&h.store, &h.ai, "anthropic", Some(90_000)).unwrap();

    let call = resolve_ai_api_endpoint(&h.store, &h.ai, "openrouter", "a/model", None).unwrap();

    assert_eq!(call.turn_timeout_ms, Some(600_000));
}
