//! AAP-FR-01 .. AAP-FR-20: listing, verification, selection, activation, clearing, and the project override.

use super::*;

// -- AAP-FR-01, AAP-FR-02 / TS-02 -------------------------------------------------

#[test]
fn aap_ts01_a_fresh_machine_lists_four_unconfigured_providers() {
    let h = harness(Arc::new(FakeProber::default()), FakeKeychain::new());
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(
        list.iter().map(|i| i.provider.as_str()).collect::<Vec<_>>(),
        vec!["openrouter", "anthropic", "openai", "custom"]
    );
    for i in &list {
        assert_eq!(i.state, IntegrationState::Unconfigured);
        assert_eq!(i.base_url, None);
        assert_eq!(i.key_state, KeyState::Unset);
        assert_eq!(i.masked_hint, None);
        assert!(!i.active);
    }
    assert!(
        h.prober.calls.lock().unwrap().is_empty(),
        "listing makes no network request"
    );
}

#[test]
fn aap_ts02_only_custom_ships_without_a_default_url_and_a_gateway_url_is_stored() {
    for p in ["anthropic", "openai", "openrouter"] {
        let d = provider_descriptor(p).unwrap();
        assert!(d.default_base_url.is_some(), "{p} ships a default");
        assert!(d.key_required, "{p} requires a key");
    }
    let custom = provider_descriptor("custom").unwrap();
    assert_eq!(custom.default_base_url, None);
    // AAP-FR-04: the gateway secret is required.
    assert!(custom.key_required);
    assert_eq!(custom.models_path, "/v1/models");

    // A named provider reached through a gateway stores the gateway URL.
    let h = ok_harness();
    let rec = verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://gateway.internal/openai/v1",
        Some("sk-1234"),
    )
    .unwrap();
    assert_eq!(
        rec.base_url.as_deref(),
        Some("https://gateway.internal/openai/v1")
    );
}

// -- AAP-FR-04: the required key ---------------------------------------

#[test]
fn aap_ts03_custom_requires_a_key_like_every_named_provider_and_sends_no_request_without_one() {
    // AAP-FR-04: Custom with an empty secret is refused before any request.
    let h = harness(
        FakeProber::returning(&[("llama-3", "Llama 3")]),
        FakeKeychain::new(),
    );
    for secret in [None, Some("   ")] {
        assert_eq!(
            verify_integration_impl(&h.store, &h.ai, "custom", "https://gw.example", secret)
                .unwrap_err(),
            ERR_KEY_MISSING,
        );
    }
    assert!(h.prober.calls.lock().unwrap().is_empty(), "no request without a secret");
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(find(&list, "custom").state, IntegrationState::Unconfigured);

    let rec = verify_integration_impl(
        &h.store,
        &h.ai,
        "custom",
        "https://gw.example/",
        Some("gw-secret-1234"),
    )
    .unwrap();
    assert_eq!(rec.state, IntegrationState::Verified);
    assert_eq!(rec.key_state, KeyState::Set);
    assert_eq!(rec.masked_hint.as_deref(), Some("1234"));
    assert!(rec.key_required);
    // AAP-FR-KRVT: trailing slashes are removed from the stored root.
    assert_eq!(rec.base_url.as_deref(), Some("https://gw.example"));

    // A named provider with an empty key is refused before any request.
    let before = h.prober.calls.lock().unwrap().len();
    assert_eq!(
        verify_integration_impl(
            &h.store,
            &h.ai,
            "anthropic",
            "https://api.anthropic.com/v1",
            None
        )
        .unwrap_err(),
        ERR_KEY_MISSING
    );
    assert_eq!(
        h.prober.calls.lock().unwrap().len(),
        before,
        "a missing key costs no round trip"
    );
    // An all-whitespace key is an empty one.
    assert_eq!(
        verify_integration_impl(
            &h.store,
            &h.ai,
            "anthropic",
            "https://api.anthropic.com/v1",
            Some("   ")
        )
        .unwrap_err(),
        ERR_KEY_MISSING
    );
}

// -- AAP-FR-05: failure distinctions ------------------------------------

#[test]
fn aap_ts04_every_verification_failure_is_distinguishable() {
    for (probe_error, expected) in [
        (ProbeError::Unreachable("dns".into()), ERR_UNREACHABLE),
        (ProbeError::Rejected, ERR_REJECTED),
        (ProbeError::NotExpectedKind, ERR_NOT_AN_AI_ENDPOINT),
        (ProbeError::TimedOut, ERR_TIMED_OUT),
    ] {
        let h = harness(FakeProber::failing(probe_error), FakeKeychain::new());
        assert_eq!(
            verify_integration_impl(
                &h.store,
                &h.ai,
                "openai",
                "https://api.openai.com/v1",
                Some("sk-1234")
            )
            .unwrap_err(),
            expected
        );
        assert!(h.store.load_ai_api_registry().unwrap().0.is_empty());
        assert!(
            h.keys.get_raw("openai").is_none(),
            "a failed verification writes no key"
        );
    }

    let h = ok_harness();
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "openai", "", Some("k")).unwrap_err(),
        ERR_BASE_URL_EMPTY
    );
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "openai", "api.openai.com", Some("k"))
            .unwrap_err(),
        ERR_BASE_URL_INVALID
    );
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "nope", "https://x.com", Some("k"))
            .unwrap_err(),
        ERR_UNKNOWN_PROVIDER
    );
}

// AAP-FR-05, AAP-FR-HZTB, AAP-FR-PKWE
#[test]
fn a_refused_certificate_is_tls_untrusted_with_host_and_cause_and_commits_nothing() {
    for (cause, wire) in [
        (crate::tls::TlsCause::UnknownIssuer, "tls_untrusted:unknown_issuer:api.openai.com"),
        (crate::tls::TlsCause::Expired, "tls_untrusted:expired:api.openai.com"),
        (crate::tls::TlsCause::HostnameMismatch, "tls_untrusted:hostname_mismatch:api.openai.com"),
        (crate::tls::TlsCause::Other, "tls_untrusted:other:api.openai.com"),
    ] {
        let failure = crate::tls::TlsFailure::new("api.openai.com", cause);
        let h = harness(
            FakeProber::failing(ProbeError::TlsUntrusted(failure)),
            FakeKeychain::new(),
        );
        let error = verify_integration_impl(
            &h.store,
            &h.ai,
            "openai",
            "https://api.openai.com/v1",
            Some("sk-1234"),
        )
        .unwrap_err();
        assert_eq!(error, wire);
        assert_ne!(error, ERR_UNREACHABLE, "a TLS failure never reads as unreachable");
        assert!(!error.contains("sk-1234"), "the error holds no key");
        assert!(h.store.load_ai_api_registry().unwrap().0.is_empty());
        assert!(h.keys.get_raw("openai").is_none(), "a failed verification writes no key");
    }
}

// -- AAP-FR-06, AAP-FR-07, AAP-FR-08 / TS-06: what a verification commits ---------------------

#[test]
fn aap_ts05_a_successful_verification_commits_url_key_and_models() {
    let h = ok_harness();
    let rec = verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1/",
        Some("sk-proj-abcd3f9a"),
    )
    .unwrap();
    assert_eq!(rec.state, IntegrationState::Verified);
    assert_eq!(
        rec.base_url.as_deref(),
        Some("https://api.openai.com/v1"),
        "normalised on the way in"
    );
    assert_eq!(rec.masked_hint.as_deref(), Some("3f9a"));
    assert_eq!(rec.key_state, KeyState::Set);
    assert!(rec.verified_at.is_some());
    assert_eq!(rec.models_origin, ModelsOrigin::Probed);
    assert_eq!(
        h.keys.get_raw("openai").as_deref(),
        Some("sk-proj-abcd3f9a"),
        "the key rests in the keychain and only there"
    );

    // AAP-FR-08: the outbound record carries no key beyond the hint.
    let as_text = serde_json::to_string(&rec).unwrap();
    assert!(!as_text.contains("sk-proj-abcd"));
    assert!(as_text.contains("3f9a"));
}

#[test]
fn aap_ts06_a_failed_verification_never_degrades_a_working_one() {
    let prober = FakeProber::returning(&[("gpt-5", "GPT-5")]);
    let h = harness(prober.clone(), FakeKeychain::new());
    verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-good1234"),
    )
    .unwrap();

    *prober.error.lock().unwrap() = Some(ProbeError::Rejected);
    assert!(verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://elsewhere.example/v1",
        Some("sk-bad")
    )
    .is_err());

    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    let rec = find(&list, "openai");
    assert_eq!(rec.base_url.as_deref(), Some("https://api.openai.com/v1"));
    assert_eq!(rec.masked_hint.as_deref(), Some("1234"));
    assert_eq!(
        h.keys.get_raw("openai").as_deref(),
        Some("sk-good1234"),
        "the working key is untouched"
    );
}

// -- AAP-FR-09, AAP-FR-13: the keychain wiped out from under us --------------------

#[test]
fn aap_ts07_an_unreadable_key_degrades_the_record_without_losing_it() {
    let h = ok_harness();
    verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-1234"),
    )
    .unwrap();
    set_model_impl(&h.store, &h.ai, "openai", Some("gpt-5")).unwrap();

    h.keys.wipe("openai");

    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    let rec = find(&list, "openai");
    assert_eq!(rec.state, IntegrationState::KeyUnavailable);
    assert_eq!(rec.key_state, KeyState::Unavailable);
    assert_eq!(
        rec.base_url.as_deref(),
        Some("https://api.openai.com/v1"),
        "retained rather than pruned — the author can re-verify it"
    );
    assert_eq!(rec.selected_model.as_deref(), Some("gpt-5"));
    assert_eq!(
        set_active_impl(&h.store, &h.ai, "openai").unwrap_err(),
        ERR_NOT_VERIFIED
    );
}

// -- AAP-FR-10: probed vs catalog ---------------------------------------

#[test]
fn aap_ts08_an_endpoint_that_lists_nothing_falls_back_to_the_bundled_catalog() {
    let h = harness(FakeProber::returning(&[]), FakeKeychain::new());
    let rec = verify_integration_impl(
        &h.store,
        &h.ai,
        "anthropic",
        "https://api.anthropic.com/v1",
        Some("sk-ant-1234"),
    )
    .unwrap();
    assert_eq!(
        rec.state,
        IntegrationState::Verified,
        "an empty list degrades the models, not the verification"
    );
    assert_eq!(rec.models_origin, ModelsOrigin::Catalog);
    assert!(rec.models.iter().any(|m| m.id == "claude-opus-5-5"));

    let h2 = ok_harness();
    let rec2 = verify_integration_impl(
        &h2.store,
        &h2.ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-1234"),
    )
    .unwrap();
    assert_eq!(rec2.models_origin, ModelsOrigin::Probed);
}

// -- AAP-FR-11 / TS-10: model selection ---------------------------------

#[test]
fn aap_ts09_a_selection_persists_and_an_unknown_id_is_refused() {
    let h = ok_harness();
    verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-1234"),
    )
    .unwrap();

    let rec = set_model_impl(&h.store, &h.ai, "openai", Some("gpt-5")).unwrap();
    assert_eq!(rec.selected_model.as_deref(), Some("gpt-5"));

    assert_eq!(
        set_model_impl(&h.store, &h.ai, "openai", Some("nope")).unwrap_err(),
        ERR_UNKNOWN_MODEL
    );
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(
        find(&list, "openai").selected_model.as_deref(),
        Some("gpt-5"),
        "a refused selection leaves the previous one alone"
    );

    let cleared = set_model_impl(&h.store, &h.ai, "openai", None).unwrap();
    assert_eq!(cleared.selected_model, None);
}

#[test]
fn aap_ts10_a_model_the_endpoint_stops_offering_falls_back_to_the_default() {
    let prober = FakeProber::returning(&[("gpt-5", "GPT-5"), ("gpt-4o", "GPT-4o")]);
    let h = harness(prober.clone(), FakeKeychain::new());
    verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-1234"),
    )
    .unwrap();
    set_model_impl(&h.store, &h.ai, "openai", Some("gpt-4o")).unwrap();

    prober.set_models(&[("gpt-5", "GPT-5"), ("gpt-5.5", "GPT-5.5")]);
    let rec = verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-1234"),
    )
    .unwrap();
    assert_eq!(rec.selected_model, None);
}

// -- AAP-FR-13 / TS-12: the single active choice ------------------------

#[test]
fn aap_ts11_activation_requires_verification_and_only_one_is_ever_active() {
    let h = ok_harness();
    assert_eq!(
        set_active_impl(&h.store, &h.ai, "openai").unwrap_err(),
        ERR_NOT_VERIFIED
    );

    verify_integration_impl(
        &h.store,
        &h.ai,
        "anthropic",
        "https://api.anthropic.com/v1",
        Some("sk-ant-1234"),
    )
    .unwrap();
    verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-5678"),
    )
    .unwrap();

    let list = set_active_impl(&h.store, &h.ai, "anthropic").unwrap();
    assert!(find(&list, "anthropic").active);
    let list = set_active_impl(&h.store, &h.ai, "openai").unwrap();
    assert_eq!(list.iter().filter(|i| i.active).count(), 1);
    assert!(find(&list, "openai").active);
}

#[test]
fn aap_ts12_a_sole_verified_provider_is_active_without_being_activated() {
    let h = ok_harness();
    verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-1234"),
    )
    .unwrap();
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert!(find(&list, "openai").active);

    verify_integration_impl(
        &h.store,
        &h.ai,
        "anthropic",
        "https://api.anthropic.com/v1",
        Some("sk-ant-5678"),
    )
    .unwrap();
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(
        list.iter().filter(|i| i.active).count(),
        0,
        "the implicit resolution applies only to a set of one"
    );

    set_active_impl(&h.store, &h.ai, "anthropic").unwrap();
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert!(find(&list, "anthropic").active);
}

// -- AAP-FR-15: clearing ------------------------------------------------

#[test]
fn aap_ts13_clearing_removes_the_key_and_the_record_and_is_idempotent() {
    let h = ok_harness();
    verify_integration_impl(
        &h.store,
        &h.ai,
        "openrouter",
        "https://openrouter.ai/api/v1",
        Some("sk-or-3f9a"),
    )
    .unwrap();
    set_model_impl(&h.store, &h.ai, "openrouter", Some("gpt-5")).unwrap();
    set_active_impl(&h.store, &h.ai, "openrouter").unwrap();
    assert!(h.keys.get_raw("openrouter").is_some());

    for _ in 0..2 {
        let list = clear_integration_impl(&h.store, &h.ai, "openrouter").unwrap();
        let rec = find(&list, "openrouter");
        assert_eq!(rec.state, IntegrationState::Unconfigured);
        assert_eq!(rec.base_url, None);
        assert_eq!(rec.masked_hint, None);
        assert_eq!(rec.selected_model, None);
        assert_eq!(list.iter().filter(|i| i.active).count(), 0);
    }
    assert!(h.keys.get_raw("openrouter").is_none());
}

// -- AAP-FR-16, AAP-FR-17 .. TS-17: the project override ---------------------------

#[test]
fn aap_ts14_an_override_belongs_to_one_project() {
    let h = ok_harness();
    verify_integration_impl(
        &h.store,
        &h.ai,
        "anthropic",
        "https://api.anthropic.com/v1",
        Some("sk-ant-1234"),
    )
    .unwrap();
    verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-5678"),
    )
    .unwrap();
    set_active_impl(&h.store, &h.ai, "anthropic").unwrap();

    let overridden = set_project_impl(&h.store, &h.ai, "/dev/acme", Some("openai")).unwrap();
    assert_eq!(overridden.resolution, ProjectResolution::Overridden);
    assert_eq!(overridden.provider.as_deref(), Some("openai"));

    let other = get_project_impl(&h.store, &h.ai, "/dev/other").unwrap();
    assert_eq!(other.resolution, ProjectResolution::Inherited);
    assert_eq!(other.provider.as_deref(), Some("anthropic"));
}

#[test]
fn aap_ts15_a_cleared_provider_leaves_its_override_recorded_but_unresolving() {
    let h = ok_harness();
    verify_integration_impl(
        &h.store,
        &h.ai,
        "anthropic",
        "https://api.anthropic.com/v1",
        Some("sk-ant-1234"),
    )
    .unwrap();
    verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-5678"),
    )
    .unwrap();
    set_active_impl(&h.store, &h.ai, "anthropic").unwrap();
    set_project_impl(&h.store, &h.ai, "/dev/acme", Some("openai")).unwrap();

    clear_integration_impl(&h.store, &h.ai, "openai").unwrap();
    let resolved = get_project_impl(&h.store, &h.ai, "/dev/acme").unwrap();
    assert_eq!(resolved.resolution, ProjectResolution::OverrideUnavailable);
    assert_eq!(resolved.override_provider.as_deref(), Some("openai"));
    assert_eq!(resolved.provider.as_deref(), Some("anthropic"));
}

#[test]
fn aap_ts16_an_override_may_only_name_a_verified_provider_and_null_clears_it() {
    let h = ok_harness();
    verify_integration_impl(
        &h.store,
        &h.ai,
        "anthropic",
        "https://api.anthropic.com/v1",
        Some("sk-ant-1234"),
    )
    .unwrap();

    assert_eq!(
        set_project_impl(&h.store, &h.ai, "/dev/acme", Some("openai")).unwrap_err(),
        ERR_NOT_VERIFIED
    );
    assert_eq!(h.store.load_ai_api_override("/dev/acme").unwrap(), None);

    set_project_impl(&h.store, &h.ai, "/dev/acme", Some("anthropic")).unwrap();
    let back = set_project_impl(&h.store, &h.ai, "/dev/acme", None).unwrap();
    assert_eq!(back.resolution, ProjectResolution::Inherited);
    assert_eq!(back.override_provider, None);

    // No project open: an override would belong to no project in particular.
    assert_eq!(
        set_project_impl(&h.store, &h.ai, "", Some("anthropic")).unwrap_err(),
        ERR_NO_PROJECT
    );
}

#[test]
fn aap_ts17_nothing_configured_and_nothing_selected_are_different_answers() {
    let h = ok_harness();
    assert_eq!(
        get_project_impl(&h.store, &h.ai, "/dev/acme")
            .unwrap()
            .resolution,
        ProjectResolution::NoneConfigured
    );

    verify_integration_impl(
        &h.store,
        &h.ai,
        "anthropic",
        "https://api.anthropic.com/v1",
        Some("sk-ant-1234"),
    )
    .unwrap();
    verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-5678"),
    )
    .unwrap();
    assert_eq!(
        get_project_impl(&h.store, &h.ai, "/dev/acme")
            .unwrap()
            .resolution,
        ProjectResolution::NoneSelected,
        "two verified providers and no choice is a different problem"
    );
}

// -- AAP-FR-16, AAP-FR-19: the call read path --------------------------------------

#[test]
fn aap_ts18_the_call_path_refuses_rather_than_returning_a_key() {
    let h = ok_harness();
    assert_eq!(
        resolve_ai_api_call(&h.store, &h.ai, "/dev/acme").unwrap_err(),
        ERR_NONE_CONFIGURED
    );

    verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-1234"),
    )
    .unwrap();
    set_model_impl(&h.store, &h.ai, "openai", Some("gpt-5")).unwrap();

    let call = resolve_ai_api_call(&h.store, &h.ai, "/dev/acme").unwrap();
    assert_eq!(call.provider, "openai");
    assert_eq!(call.base_url, "https://api.openai.com/v1");
    assert_eq!(call.api_key.as_deref(), Some("sk-1234"));
    assert_eq!(call.model_id.as_deref(), Some("gpt-5"));

    // AAP-FR-04: Custom resolves with its secret, never without one.
    let h2 = harness(FakeProber::returning(&[]), FakeKeychain::new());
    verify_integration_impl(&h2.store, &h2.ai, "custom", "http://gw.local:1234", Some("gw-secret"))
        .unwrap();
    let call = resolve_ai_api_call(&h2.store, &h2.ai, "/dev/acme").unwrap();
    assert_eq!(call.api_key.as_deref(), Some("gw-secret"));
    assert_eq!(call.base_url, "http://gw.local:1234");
}

// -- AAP-FR-20: the keychain --------------------------------------------

#[test]
fn aap_ts19_a_locked_keychain_fails_the_write_but_never_the_listing() {
    let keys = FakeKeychain::new();
    let h = harness(FakeProber::returning(&[("gpt-5", "GPT-5")]), keys.clone());
    keys.lock_it();

    assert_eq!(
        verify_integration_impl(
            &h.store,
            &h.ai,
            "openai",
            "https://api.openai.com/v1",
            Some("sk-1234")
        )
        .unwrap_err(),
        ERR_KEYCHAIN_UNAVAILABLE
    );
    assert!(h.store.load_ai_api_registry().unwrap().0.is_empty());

    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(list.len(), 4, "listing still answers");
}

