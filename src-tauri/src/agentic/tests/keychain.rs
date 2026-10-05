//! The keychain (AIC-FR-24).

use super::*;

// -- AIC-FR-24: the keychain ------------------------------------------

#[test]
fn aic_ts26_a_locked_keychain_fails_the_write_but_never_the_listing() {
    let keys = FakeKeychain::new();
    let h = harness(
        FakeFs::with_executable(&[]),
        Arc::new(FakeRunner::default()),
        FakeProber::returning(&[("claude-opus-5", "Claude Opus 5")]),
        keys.clone(),
    );
    keys.lock_it();

    assert_eq!(
        verify_integration_impl(
            &h.store,
            &h.ai,
            "claude_agent_api",
            &api_config("https://api.anthropic.com/v1", Some("k-1234")),
        )
        .unwrap_err(),
        ERR_KEYCHAIN_UNAVAILABLE
    );
    assert!(
        h.store.load_agentic_registry().unwrap().0.is_empty(),
        "a keychain that refuses leaves the registry exactly as it was"
    );

    // Listing keeps working: it probes for presence and downgrades rather
    // than failing.
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(list.len(), 5);
}

#[test]
fn re_verifying_an_api_agent_without_a_key_does_not_leave_the_old_one_readable() {
    // AIC-FR-14 / AIC-FR-20. `custom_agent_api` is the one agentic vendor
    // whose key is optional, so it is the one that can be reconfigured from
    // keyed to keyless. The record then carries no hint — and an entry left
    // behind in the credential store would be a key nothing describes and
    // nothing can clear.
    let h = harness(
        FakeFs::with_executable(&[]),
        Arc::new(FakeRunner::default()),
        FakeProber::returning(&[("local-agent", "Local agent")]),
        FakeKeychain::new(),
    );
    verify_integration_impl(
        &h.store,
        &h.ai,
        "custom_agent_api",
        &api_config("http://localhost:8080/v1", Some("sk-old-1234")),
    )
    .unwrap();
    assert_eq!(h.keys.get_raw("custom_agent_api").as_deref(), Some("sk-old-1234"));

    let rec = verify_integration_impl(
        &h.store,
        &h.ai,
        "custom_agent_api",
        &api_config("http://localhost:8080/v1", None),
    )
    .unwrap();
    assert_eq!(rec.masked_hint, None);
    assert_eq!(rec.key_state, KeyState::Unset);
    assert!(
        h.keys.get_raw("custom_agent_api").is_none(),
        "no orphaned key may survive behind a hintless record"
    );
}

#[test]
fn verifying_a_credential_free_cli_never_touches_the_credential_store() {
    // AIC-FR-20 / AIC-FR-25: Codex and OpenCode authenticate themselves, so
    // their verification must not reach for the keychain even to clear it.
    // The guard matters because a locked keychain would otherwise fail a
    // verification that needs no credential at all — which is exactly what a
    // naive "delete whenever no credential arrived" would cause, and what
    // adding Claude Code's token could easily have generalised into.
    let keys = FakeKeychain::new();
    let fs = FakeFs::with_executable(&["/usr/bin/codex"]);
    let h = harness(
        fs,
        FakeRunner::saying("/usr/bin/codex", "codex-cli 0.4.0"),
        Arc::new(FakeProber::default()),
        keys.clone(),
    );
    keys.lock_it();

    let rec =
        verify_integration_impl(&h.store, &h.ai, "codex", &cli_config_for("codex", "/usr/bin/codex"))
            .unwrap();
    assert_eq!(
        rec.state,
        IntegrationState::Verified,
        "a locked keychain is irrelevant to a credential-free CLI verification"
    );
    assert_eq!(rec.key_state, KeyState::Unset);
}

#[test]
fn no_debug_output_of_a_key_bearing_shape_contains_the_key() {
    // AIC-FR-20: no log line, error payload, or panic message may carry a
    // key. `Debug` is the channel that would leak one by accident — a stray
    // `{:?}`, an `assert_eq!` failure, an `unwrap` panic — so the two shapes
    // that hold cleartext redact it rather than deriving the obvious impl.
    let invocation = AgenticInvocation::Api {
        vendor: "claude_agent_api".into(),
        base_url: "https://api.anthropic.com/v1".into(),
        api_key: Some("sk-ant-supersecret".into()),
        model_id: None,
        effort_id: None,
    };
    let rendered = format!("{invocation:?}");
    assert!(!rendered.contains("supersecret"), "leaked via Debug: {rendered}");
    assert!(rendered.contains("<redacted>"));
    // The rest of the shape stays legible — redaction must not cost the
    // diagnostics the type exists to provide.
    assert!(rendered.contains("claude_agent_api"));
    assert!(rendered.contains("https://api.anthropic.com/v1"));

    let config = VerifyConfig {
        base_url: Some("https://api.anthropic.com/v1".into()),
        api_key: Some("sk-ant-supersecret".into()),
        ..Default::default()
    };
    let rendered = format!("{config:?}");
    assert!(!rendered.contains("supersecret"), "leaked via Debug: {rendered}");
    assert!(rendered.contains("<redacted>"));

    // A CLI invocation has no key to redact and says so plainly.
    let cli = AgenticInvocation::Cli {
        vendor: "claude_code".into(),
        binary_path: "/usr/bin/claude".into(),
        model_id: Some("opus".into()),
        effort_id: None,
    };
    assert!(format!("{cli:?}").contains("/usr/bin/claude"));
}
