//! AAP-FR-19, AAP-FR-23, AAP-FR-31, AAP-FR-33, AAP-FR-34: the caller-named read paths and the persistence of a choice.

use super::*;

// -- AAP-FR-33 / TS-34: the caller-named read paths ---------------------

#[test]
fn aap_ts33_a_callers_selection_is_validated_from_the_registry_alone() {
    // AAP-FR-33. The keychain is locked and the prober would panic if it
    // were reached, which is the whole claim: an agent can be validated
    // while the machine is offline and the credential store shut.
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(vec![with_ladder(
        "a/model",
        &["high", "medium", "low"],
        None,
        false,
    )]);
    let h = harness(prober, FakeKeychain::new());
    verify_openrouter(&h);
    let calls_before = h.prober.calls.lock().unwrap().len();
    h.keys.lock_it();

    assert!(validate_ai_api_selection(
        &h.store,
        "openrouter",
        "a/model",
        Some(&ReasoningChoice::Effort {
            effort: "medium".into()
        }),
    )
    .is_ok());
    assert_eq!(
        h.prober.calls.lock().unwrap().len(),
        calls_before,
        "validation makes no network request",
    );

    assert_eq!(
        validate_ai_api_selection(&h.store, "openrouter", "nope", None).unwrap_err(),
        ERR_UNKNOWN_MODEL,
    );
    assert_eq!(
        validate_ai_api_selection(
            &h.store,
            "openrouter",
            "a/model",
            Some(&ReasoningChoice::Effort {
                effort: "absurd".into()
            }),
        )
        .unwrap_err(),
        ERR_UNKNOWN_EFFORT,
    );
    // Configured but never verified, and never configured at all, are
    // different corrections and different answers.
    assert_eq!(
        validate_ai_api_selection(&h.store, "openai", "gpt-5", None).unwrap_err(),
        ERR_PROVIDER_UNCONFIGURED,
    );
}

#[test]
fn aap_ts33_a_configured_but_unverified_provider_is_not_verified_rather_than_unconfigured() {
    // AAP-FR-33's second distinction. A record with a base URL but no
    // successful check is exactly the hand-edited store `state_of` warns
    // about, and handing an agent an endpoint on that basis would turn a
    // configuration problem into a failed call at the moment of use.
    let h = ok_harness();
    h.store
        .save_ai_api_registry(
            vec![AiApiRecord {
                provider: "openai".into(),
                base_url: Some("https://api.openai.com/v1".into()),
                masked_hint: Some("1234".into()),
                verified_at: None,
                models: vec![ModelOption::new("gpt-5", "GPT-5")],
                ..Default::default()
            }],
            None,
        )
        .unwrap();
    assert_eq!(
        validate_ai_api_selection(&h.store, "openai", "gpt-5", None).unwrap_err(),
        ERR_NOT_VERIFIED,
    );
}

#[test]
fn aap_ts34_the_endpoint_carries_the_callers_model_not_the_records_and_no_override_redirects_it()
{
    // AAP-FR-34. The record selects `a/model` at `low`; the caller asks for
    // `b/model` at `high`, and the project is overridden to Anthropic.
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(vec![
        with_ladder("a/model", &["high", "low"], None, false),
        with_ladder("b/model", &["high", "low"], None, false),
    ]);
    let h = harness(prober, FakeKeychain::new());
    verify_openrouter(&h);
    set_model_impl(&h.store, &h.ai, "openrouter", Some("a/model")).unwrap();
    set_reasoning_impl(
        &h.store,
        &h.ai,
        "openrouter",
        Some(ReasoningChoice::Effort {
            effort: "low".into(),
        }),
    )
    .unwrap();
    h.store
        .save_ai_api_override("/dev/acme", "anthropic")
        .unwrap();

    let call = resolve_ai_api_endpoint(
        &h.store,
        &h.ai,
        "openrouter",
        "b/model",
        Some(ReasoningChoice::Effort {
            effort: "high".into(),
        }),
    )
    .unwrap();
    assert_eq!(call.provider, "openrouter");
    assert_eq!(call.base_url, "https://openrouter.ai/api/v1");
    assert_eq!(call.api_key.as_deref(), Some("sk-or-1234"));
    assert_eq!(call.model_id.as_deref(), Some("b/model"));
    assert_eq!(
        call.reasoning,
        Some(ReasoningChoice::Effort {
            effort: "high".into()
        }),
    );

    // A model the record does not offer is refused, and no key was read for
    // it — validation runs before the keychain is touched.
    assert_eq!(
        resolve_ai_api_endpoint(&h.store, &h.ai, "openrouter", "c/model", None).unwrap_err(),
        ERR_UNKNOWN_MODEL,
    );

    // The keychain entry deleted out from under the record.
    h.keys.wipe("openrouter");
    assert_eq!(
        resolve_ai_api_endpoint(&h.store, &h.ai, "openrouter", "b/model", None).unwrap_err(),
        ERR_KEY_UNAVAILABLE,
    );
    // And a store that will not answer at all is a different correction.
    h.keys.set("openrouter", "sk-or-1234").unwrap();
    h.keys.lock_it();
    assert_eq!(
        resolve_ai_api_endpoint(&h.store, &h.ai, "openrouter", "b/model", None).unwrap_err(),
        ERR_KEYCHAIN_UNAVAILABLE,
    );
}

// -- AAP-FR-19, AAP-FR-31, AAP-FR-34: the two read paths never borrow each other's selections --

#[test]
fn aap_ts35_a_stored_selection_serves_the_graduation_run_and_never_an_agents_turn() {
    // AAP-FR-19 / AAP-FR-31 / AAP-FR-34. The two consumers are settled: a
    // graduation run takes what the resolved integration stores, an agent
    // takes what its own description carries, and neither reads the other's.
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(vec![
        with_ladder("a/model", &["high", "low"], None, false),
        with_ladder("b/model", &["high", "low"], None, false),
    ]);
    let h = harness(prober, FakeKeychain::new());
    verify_openrouter(&h);
    set_model_impl(&h.store, &h.ai, "openrouter", Some("a/model")).unwrap();
    set_reasoning_impl(
        &h.store,
        &h.ai,
        "openrouter",
        Some(ReasoningChoice::Effort {
            effort: "low".into(),
        }),
    )
    .unwrap();

    // The graduation run's path: the integration the project resolves to,
    // carrying that integration's own stored model and reasoning.
    let run = resolve_ai_api_call(&h.store, &h.ai, "/dev/acme").unwrap();
    assert_eq!(run.provider, "openrouter");
    assert_eq!(run.base_url, "https://openrouter.ai/api/v1");
    assert_eq!(run.api_key.as_deref(), Some("sk-or-1234"));
    assert_eq!(run.model_id.as_deref(), Some("a/model"));
    assert_eq!(
        run.reasoning,
        Some(ReasoningChoice::Effort {
            effort: "low".into()
        }),
    );

    // The agent's path: the same endpoint and key, and nothing else in
    // common. The stored `a/model` and `low` had no effect on it.
    let agent = resolve_ai_api_endpoint(
        &h.store,
        &h.ai,
        "openrouter",
        "b/model",
        Some(ReasoningChoice::Effort {
            effort: "high".into(),
        }),
    )
    .unwrap();
    assert_eq!(agent.base_url, run.base_url);
    assert_eq!(agent.api_key, run.api_key);
    assert_eq!(agent.model_id.as_deref(), Some("b/model"));
    assert_eq!(
        agent.reasoning,
        Some(ReasoningChoice::Effort {
            effort: "high".into()
        }),
    );

    // AAP-FR-34: an agent that asked for **no** reasoning gets none — the
    // record's stored `low` must not fill the caller's `None`. This is the
    // leak the two-path rule exists to forbid, and the only assertion in the
    // suite that would turn red if the resolver ever fell back to the store.
    let none = resolve_ai_api_endpoint(&h.store, &h.ai, "openrouter", "b/model", None).unwrap();
    assert_eq!(none.model_id.as_deref(), Some("b/model"));
    assert_eq!(
        none.reasoning, None,
        "the record's stored choice must not fill a caller's None",
    );

    // And the two paths move independently. The record is moved somewhere
    // the agent is *not* — provider default, no reasoning — so a resolver
    // that substituted the stored selection could no longer pass by
    // coincidence of the two happening to agree.
    set_model_impl(&h.store, &h.ai, "openrouter", None).unwrap();
    set_reasoning_impl(&h.store, &h.ai, "openrouter", None).unwrap();

    let again = resolve_ai_api_endpoint(
        &h.store,
        &h.ai,
        "openrouter",
        "b/model",
        Some(ReasoningChoice::Effort {
            effort: "high".into(),
        }),
    )
    .unwrap();
    assert_eq!(again.model_id.as_deref(), Some("b/model"));
    assert_eq!(
        again.reasoning,
        Some(ReasoningChoice::Effort {
            effort: "high".into()
        }),
        "an agent's turn resolves to its own selections exactly as before",
    );

    // The run's path moved with the record, which is the other half of it.
    let run_after = resolve_ai_api_call(&h.store, &h.ai, "/dev/acme").unwrap();
    assert_eq!(run_after.model_id, None);
    assert_eq!(run_after.reasoning, None);
}

// -- AAP-FR-23: the dependency's blast radius ---------------------------

#[test]
fn aap_ts31_only_openrouter_routes_through_its_own_client() {
    // AAP-FR-23: a client that fails outright for OpenRouter must leave the
    // other three providers verifying normally. The generic prober here
    // stands in for the `ureq` path and the failing one for the SDK.
    let generic = FakeProber::returning(&[("gpt-5", "GPT-5")]);
    let openrouter = FakeProber::failing(ProbeError::Unreachable("sdk down".into()));
    let keys = FakeKeychain::new();
    let h = Harness {
        store: GlobalSettingsStore::in_memory(),
        ai: AiApiIntegrations::new(Box::new(generic.clone()), Box::new(keys.clone()))
            .with_openrouter_prober(Box::new(openrouter.clone())),
        keys,
        prober: generic.clone(),
    };

    assert_eq!(
        verify_integration_impl(
            &h.store,
            &h.ai,
            "openrouter",
            "https://openrouter.ai/api/v1",
            Some("sk-or-1234")
        )
        .unwrap_err(),
        ERR_UNREACHABLE
    );
    assert_eq!(
        openrouter.calls.lock().unwrap().len(),
        1,
        "openrouter went to its own client"
    );

    for provider in ["anthropic", "openai", "custom"] {
        // AAP-FR-KRVT: the Custom gateway root carries no `/v1`.
        let url = if provider == "custom" {
            "https://custom.example".to_string()
        } else {
            format!("https://{provider}.example/v1")
        };
        assert_eq!(
            verify_integration_impl(&h.store, &h.ai, provider, &url, Some("sk-1234"))
                .unwrap()
                .state,
            IntegrationState::Verified,
            "{provider} still verifies through the generic path"
        );
    }
    assert_eq!(
        openrouter.calls.lock().unwrap().len(),
        1,
        "and none of them touched the openrouter client"
    );
}

// -- Persistence of the choice ------------------------------------------

#[test]
fn a_reasoning_choice_round_trips_through_the_toml_store() {
    // The record's field order is load-bearing: `selected_reasoning`
    // serialises as a table, so a scalar declared after it would be parsed
    // as belonging to it. A round trip is what proves the ordering holds.
    let record = AiApiRecord {
        turn_timeout_ms: None,
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
    let back: AiApiRecord = toml::from_str(&text).unwrap();
    assert_eq!(back, record, "the record survives the store intact:\n{text}");
}

