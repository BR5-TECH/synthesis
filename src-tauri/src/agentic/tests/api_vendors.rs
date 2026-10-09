//! API verification failures and the two API vendors (AIC-FR-22,
//! AIC-FR-23, AIC-FR-26).

use super::*;

// -- AIC-FR-22, AIC-FR-26: API verification failures -----------------------------

// AIC-FR-22, AAP-FR-HZTB
#[test]
fn a_refused_certificate_fails_api_verification_with_host_and_cause() {
    let failure = crate::tls::TlsFailure::new("agents.corp", crate::tls::TlsCause::HostnameMismatch);
    let h = harness(
        FakeFs::with_executable(&[]),
        Arc::new(FakeRunner::default()),
        FakeProber::failing(ProbeError::TlsUntrusted(failure)),
        FakeKeychain::new(),
    );
    let error = verify_integration_impl(
        &h.store,
        &h.ai,
        "claude_agent_api",
        &api_config("https://agents.corp/v1", Some("k")),
    )
    .unwrap_err();
    assert_eq!(error, "tls_untrusted:hostname_mismatch:agents.corp");
    assert_ne!(error, ERR_UNREACHABLE);
    assert!(h.store.load_agentic_registry().unwrap().0.is_empty());
}

#[test]
fn aic_ts24_each_api_failure_is_distinguishable() {
    let cases: Vec<(ProbeError, &str)> = vec![
        (ProbeError::Unreachable("dns".into()), ERR_UNREACHABLE),
        (ProbeError::Rejected, ERR_REJECTED),
        (ProbeError::NotExpectedKind, ERR_NOT_AN_AGENT_ENDPOINT),
        (ProbeError::TimedOut, ERR_TIMED_OUT),
    ];
    for (probe_error, expected) in cases {
        let h = harness(
            FakeFs::with_executable(&[]),
            Arc::new(FakeRunner::default()),
            FakeProber::failing(probe_error),
            FakeKeychain::new(),
        );
        assert_eq!(
            verify_integration_impl(
                &h.store,
                &h.ai,
                "claude_agent_api",
                &api_config("https://api.anthropic.com/v1", Some("k")),
            )
            .unwrap_err(),
            expected
        );
        assert!(
            h.store.load_agentic_registry().unwrap().0.is_empty(),
            "a failed verification persists nothing"
        );
    }

    // The URL checks happen before any request at all.
    let h = harness(
        FakeFs::with_executable(&[]),
        Arc::new(FakeRunner::default()),
        FakeProber::returning(&[]),
        FakeKeychain::new(),
    );
    assert_eq!(
        verify_integration_impl(
            &h.store,
            &h.ai,
            "claude_agent_api",
            &api_config("", Some("k"))
        )
        .unwrap_err(),
        ERR_BASE_URL_EMPTY
    );
    assert_eq!(
        verify_integration_impl(
            &h.store,
            &h.ai,
            "claude_agent_api",
            &api_config("not-a-url", Some("k"))
        )
        .unwrap_err(),
        ERR_BASE_URL_INVALID
    );
    assert_eq!(
        verify_integration_impl(
            &h.store,
            &h.ai,
            "claude_agent_api",
            &api_config("https://api.anthropic.com/v1", None)
        )
        .unwrap_err(),
        ERR_KEY_MISSING
    );

    // A config of the wrong kind is refused rather than half-honoured.
    assert_eq!(
        verify_integration_impl(
            &h.store,
            &h.ai,
            "claude_code",
            &api_config("https://example.com", Some("k"))
        )
        .unwrap_err(),
        ERR_WRONG_CONFIG_KIND
    );
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "claude_agent_api", &cli_config("/usr/bin/x"))
            .unwrap_err(),
        ERR_WRONG_CONFIG_KIND
    );
}

// -- AIC-FR-23: the two API vendors -----------------------------------

#[test]
fn aic_ts25_only_the_named_api_vendor_ships_a_default_url() {
    assert_eq!(
        vendor_descriptor("claude_agent_api").unwrap().default_base_url,
        Some("https://api.anthropic.com/v1")
    );
    assert_eq!(
        vendor_descriptor("custom_agent_api").unwrap().default_base_url,
        None,
        "the custom vendor exists for a URL only the author knows"
    );

    // A self-hosted deployment verifies through the custom vendor, with no
    // key, and the URL it was verified against is what gets stored.
    let prober = FakeProber::returning(&[("local-agent", "Local agent")]);
    let h = harness(
        FakeFs::with_executable(&[]),
        Arc::new(FakeRunner::default()),
        prober.clone(),
        FakeKeychain::new(),
    );
    let rec = verify_integration_impl(
        &h.store,
        &h.ai,
        "custom_agent_api",
        &api_config("http://localhost:8080/v1/", None),
    )
    .unwrap();
    assert_eq!(rec.state, IntegrationState::Verified);
    assert_eq!(
        rec.base_url.as_deref(),
        Some("http://localhost:8080/v1"),
        "normalised, so one stored value produces one request path"
    );
    assert_eq!(rec.masked_hint, None);
    assert_eq!(rec.key_state, KeyState::Unset);

    // A named vendor reached through a gateway stores the gateway URL.
    let rec = verify_integration_impl(
        &h.store,
        &h.ai,
        "claude_agent_api",
        &api_config("https://gateway.internal/anthropic/v1", Some("k-1234")),
    )
    .unwrap();
    assert_eq!(
        rec.base_url.as_deref(),
        Some("https://gateway.internal/anthropic/v1")
    );
    let calls = prober.calls.lock().unwrap();
    assert!(calls
        .iter()
        .any(|(url, _, _)| url == "https://gateway.internal/anthropic/v1/models"));
}
