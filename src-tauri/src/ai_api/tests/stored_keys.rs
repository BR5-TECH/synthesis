//! AAP-FR-07, AAP-FR-08, AAP-FR-32: what happens when the store holds a key, or holds none.

use super::*;

// -- AAP-FR-07, AAP-FR-08: no key in the store -------------------------------------

#[test]
fn aap_ts20_no_key_reaches_the_registry() {
    let h = ok_harness();
    verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-proj-supersecret3f9a"),
    )
    .unwrap();
    let (records, _) = h.store.load_ai_api_registry().unwrap();
    let as_text = serde_json::to_string(&records).unwrap();
    assert!(!as_text.contains("supersecret"));
    assert!(as_text.contains("3f9a"));
}

#[test]
fn a_record_never_carries_a_hint_without_the_key_behind_it() {
    // The invariant: a hint and a keychain entry exist together or not at
    // all. A hintless record with an entry behind it would leave a key
    // readable with nothing describing it; a hint with no entry would
    // promise a credential that is not there.
    let h = harness(FakeProber::returning(&[]), FakeKeychain::new());
    let agree = |label: &str| {
        let list = list_integrations_impl(&h.store, &h.ai).unwrap();
        let hint = find(&list, "custom").masked_hint.is_some();
        assert_eq!(
            hint,
            h.keys.get_raw("custom").is_some(),
            "hint and keychain entry disagree {label}"
        );
        hint
    };

    // AAP-FR-04: a Custom verification with no key at all is refused, so
    // neither exists.
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "custom", "http://localhost:1234", None)
            .unwrap_err(),
        ERR_KEY_MISSING,
    );
    assert!(!agree("for a refused keyless verification"));

    // A key supplied: both exist.
    verify_integration_impl(
        &h.store,
        &h.ai,
        "custom",
        "http://localhost:1234",
        Some("sk-old1234"),
    )
    .unwrap();
    assert!(agree("once a key is supplied"));

    // AAP-FR-32: re-verified with the field empty, the stored key is
    // re-presented rather than dropped — so both still exist, and an author
    // checking a working endpoint cannot lose their credential by leaving
    // the field alone.
    verify_integration_impl(&h.store, &h.ai, "custom", "http://localhost:1234", None)
        .unwrap();
    assert!(agree("after a re-verification with an empty field"));
    assert_eq!(h.keys.get_raw("custom").as_deref(), Some("sk-old1234"));

    // AAP-FR-15: withdrawing the key is what Clear is for, and it takes
    // both together.
    clear_integration_impl(&h.store, &h.ai, "custom").unwrap();
    assert!(!agree("after Clear"));
}

// -- AAP-FR-32, AAP-FR-04, AAP-FR-06: verifying a provider whose key is already stored ---------

#[test]
fn aap_ts32_an_empty_key_field_re_presents_the_stored_key() {
    let prober = FakeProber::returning(&[("gpt-5", "GPT-5")]);
    let h = harness(prober.clone(), FakeKeychain::new());
    verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-stored1234"),
    )
    .unwrap();
    let before = find(&list_integrations_impl(&h.store, &h.ai).unwrap(), "openai")
        .masked_hint
        .clone();

    // The whole point: no key typed, and it verifies anyway.
    let calls_before = prober.calls.lock().unwrap().len();
    let rec = verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1",
        None,
    )
    .unwrap();
    assert_eq!(rec.state, IntegrationState::Verified);
    assert_eq!(rec.masked_hint, before, "the hint is unchanged");
    assert_eq!(
        h.keys.get_raw("openai").as_deref(),
        Some("sk-stored1234"),
        "the stored key is untouched"
    );

    // ...and the stored key is what actually went to the endpoint, rather
    // than the request being made unauthenticated.
    let calls = prober.calls.lock().unwrap();
    assert_eq!(calls.len(), calls_before + 1);
    assert_eq!(calls.last().unwrap().1.as_deref(), Some("sk-stored1234"));
    drop(calls);

    // A key the author does type still replaces the stored one.
    verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-new5678"),
    )
    .unwrap();
    assert_eq!(h.keys.get_raw("openai").as_deref(), Some("sk-new5678"));

    // A record whose entry was wiped out from under the application has no
    // key to re-present, so a named provider refuses rather than probing
    // unauthenticated.
    h.keys.wipe("openai");
    let calls_before = prober.calls.lock().unwrap().len();
    assert_eq!(
        verify_integration_impl(
            &h.store,
            &h.ai,
            "openai",
            "https://api.openai.com/v1",
            None
        )
        .unwrap_err(),
        ERR_KEY_MISSING
    );
    assert_eq!(
        prober.calls.lock().unwrap().len(),
        calls_before,
        "a missing key still costs no round trip"
    );
    assert!(h.keys.get_raw("openai").is_none());
}

#[test]
fn no_debug_output_of_the_call_shape_contains_the_key() {
    // AAP-FR-07: the same reasoning as the agentic level's — `Debug` is the
    // accidental channel a key would escape through, so it is redacted.
    let call = AiApiCall {
        turn_timeout_ms: None,
        provider: "openai".into(),
        base_url: "https://api.openai.com/v1".into(),
        api_key: Some("sk-proj-supersecret".into()),
        model_id: Some("gpt-5".into()),
        reasoning: None,
        accepts_image_input: false,
                    model_mode: None,
    };
    let rendered = format!("{call:?}");
    assert!(!rendered.contains("supersecret"), "leaked via Debug: {rendered}");
    assert!(rendered.contains("<redacted>"));
    // The rest stays legible.
    assert!(rendered.contains("openai"));
    assert!(rendered.contains("gpt-5"));

    let keyless = AiApiCall {
        api_key: None,
        ..call
    };
    assert!(format!("{keyless:?}").contains("api_key: \"None\""));
}

#[test]
fn a_key_required_provider_with_no_key_is_never_verified() {
    // AAP-FR-09 / AAP-FR-13. A half-written or hand-edited store can give a
    // key-required provider a base URL and no hint. Calling that `verified`
    // would let it be activated and then hand `resolve_ai_api_call` an
    // endpoint with no credential — a configuration problem surfacing as a
    // 401 at the moment of use, far from where it can be fixed.
    let h = ok_harness();
    h.store
        .save_ai_api_registry(
            vec![AiApiRecord {
                provider: "openai".into(),
                base_url: Some("https://api.openai.com/v1".into()),
                masked_hint: None,
                ..Default::default()
            }],
            None,
        )
        .unwrap();

    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    let rec = find(&list, "openai");
    assert_eq!(rec.state, IntegrationState::KeyUnavailable);
    assert_eq!(
        rec.base_url.as_deref(),
        Some("https://api.openai.com/v1"),
        "the row is kept so the author can re-verify it"
    );
    assert_eq!(
        set_active_impl(&h.store, &h.ai, "openai").unwrap_err(),
        ERR_NOT_VERIFIED
    );
    assert_eq!(
        get_project_impl(&h.store, &h.ai, "/dev/acme")
            .unwrap()
            .resolution,
        ProjectResolution::NoneSelected,
        "configured but unusable is not the same as nothing configured"
    );

    // AAP-FR-04: `custom` requires a key too, so the same shape is condemned
    // for it as well.
    let h2 = ok_harness();
    h2.store
        .save_ai_api_registry(
            vec![AiApiRecord {
                provider: "custom".into(),
                base_url: Some("http://localhost:1234".into()),
                masked_hint: None,
                ..Default::default()
            }],
            None,
        )
        .unwrap();
    let list = list_integrations_impl(&h2.store, &h2.ai).unwrap();
    assert_eq!(find(&list, "custom").state, IntegrationState::KeyUnavailable);
}

#[test]
fn an_explicit_choice_that_degrades_resolves_to_nothing_rather_than_falling_back() {
    // AAP-FR-14. The author chose that provider. Quietly calling a different
    // endpoint — and spending against a different account — is worse than
    // calling none, so a degraded explicit choice must resolve to nothing
    // rather than silently handing the role to the other verified provider.
    let h = ok_harness();
    verify_integration_impl(
        &h.store,
        &h.ai,
        "anthropic",
        "https://api.anthropic.com/v1",
        Some("sk-ant-1111"),
    )
    .unwrap();
    verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-2222"),
    )
    .unwrap();
    set_active_impl(&h.store, &h.ai, "openai").unwrap();

    // The activated provider's key goes missing; Anthropic is still fine.
    h.keys.wipe("openai");

    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(
        list.iter().filter(|i| i.active).count(),
        0,
        "no record may inherit the role the author gave to another"
    );
    assert!(!find(&list, "anthropic").active);

    let resolved = get_project_impl(&h.store, &h.ai, "/dev/acme").unwrap();
    assert_eq!(resolved.resolution, ProjectResolution::NoneSelected);
    assert_eq!(resolved.provider, None);
    assert_eq!(
        resolve_ai_api_call(&h.store, &h.ai, "/dev/acme").unwrap_err(),
        ERR_NONE_SELECTED
    );
}

#[test]
fn aap_ts17_an_override_is_one_fact_for_the_project_across_worktrees() {
    // AAP-FR-18. The override is keyed by project, so every
    // worktree of it reads the same value and no other project sees it. The
    // claim that nothing reaches a worktree's `.synthesis/` is the storage
    // layer's (GSS-FR-26, GSS-FR-18); what this pins is that the module never takes a
    // worktree into account at all.
    let h = ok_harness();
    verify_integration_impl(
        &h.store,
        &h.ai,
        "anthropic",
        "https://api.anthropic.com/v1",
        Some("sk-ant-1111"),
    )
    .unwrap();
    verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-2222"),
    )
    .unwrap();
    set_active_impl(&h.store, &h.ai, "anthropic").unwrap();
    set_project_impl(&h.store, &h.ai, "/dev/acme", Some("openai")).unwrap();

    // Reading again under the same project key — which is what a change of
    // active worktree amounts to — returns the same answer.
    let again = get_project_impl(&h.store, &h.ai, "/dev/acme").unwrap();
    assert_eq!(again.resolution, ProjectResolution::Overridden);
    assert_eq!(again.provider.as_deref(), Some("openai"));
    // And a different project is untouched by it.
    let other = get_project_impl(&h.store, &h.ai, "/dev/other").unwrap();
    assert_eq!(other.resolution, ProjectResolution::Inherited);
    assert_eq!(other.provider.as_deref(), Some("anthropic"));
}

#[test]
fn a_url_carrying_a_credential_is_refused_and_never_persisted() {
    // AAP-FR-07: a key is never written into a URL. The base URL is stored
    // verbatim, so this is the one path by which a pasted key could reach
    // `synthesis.toml` without ever being passed as `api_key`.
    let h = ok_harness();
    assert_eq!(
        verify_integration_impl(
            &h.store,
            &h.ai,
            "openai",
            "https://sk-proj-supersecret@api.openai.com/v1",
            Some("sk-1234"),
        )
        .unwrap_err(),
        ERR_BASE_URL_INVALID
    );
    let (records, _) = h.store.load_ai_api_registry().unwrap();
    assert!(records.is_empty());
    let as_text = serde_json::to_string(&records).unwrap();
    assert!(!as_text.contains("supersecret"));
}

