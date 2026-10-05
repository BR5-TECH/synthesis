//! Claude Code's OAuth token (AIC-FR-26 … AIC-FR-29).

use super::*;

// -- Claude Code's OAuth token (AIC-FR-26..29) -------------------------

#[test]
fn aic_ts29_only_a_structurally_valid_token_is_accepted() {
    // AIC-FR-27: `^sk-ant-oat01-[A-Za-z0-9-]+$`, and nothing else. This is
    // the *pattern* — anchored at both ends. The surrounding whitespace
    // normalisation that precedes it is a separate layer, pinned by
    // `aic_ts27_surrounding_whitespace_is_normalised_before_the_pattern`.
    //
    // Checked as a unit as well as through the command because this is the
    // one rule implemented twice — here and in `CLAUDE_OAUTH_TOKEN_PATTERN`
    // on the frontend (AII-FR-50) — and a drift between the two is
    // invisible from either side alone. The tables are kept identical.
    for bad in [
        "",
        "oat01-abc",
        "sk-ant-oat01-",                 // the group needs one or more
        "sk-ant-oat01-abc_def",          // underscore is not in the class
        "sk-ant-oat01-abc def",          // nor is a space
        " sk-ant-oat01-abc",             // unanchored at the front
        "sk-ant-oat01-abc\n",            // nor at the back
        "xsk-ant-oat01-abc",
        "sk-ant-oat02-abc",
        "sk-ant-oat01-abc!",
        // `[A-Za-z0-9]` is ASCII. The likeliest future drift is someone
        // "simplifying" this predicate to `char::is_alphanumeric`, which
        // accepts these while the frontend's regex still refuses them.
        "sk-ant-oat01-abcé",
        "sk-ant-oat01-١٢٣",
        "sk-ant-oat01-Ω",
    ] {
        assert!(!is_valid_oauth_token(bad), "{bad:?} must not validate");
    }
    for good in [
        SAMPLE_TOKEN,
        "sk-ant-oat01-a",
        "sk-ant-oat01-A1-b2-C3",
        "sk-ant-oat01----",
    ] {
        assert!(is_valid_oauth_token(good), "{good:?} must validate");
    }
}

#[test]
fn aic_ts27_surrounding_whitespace_is_normalised_before_the_pattern() {
    // The pattern is anchored, so ` token\n` fails it outright — but a token
    // is a thing authors *paste*, and a trailing newline is not a different
    // token. `resolve_supplied_token` trims before matching, and the
    // frontend trims identically (AII-FR-50), so what the field accepts and
    // what the keychain will take never disagree.
    //
    // What is stored is the trimmed value, which is what keeps the masked
    // hint describing the token rather than the whitespace after it.
    let h = claude_harness();
    let padded = format!("  {SAMPLE_TOKEN}\n");
    assert!(!is_valid_oauth_token(&padded), "the pattern itself refuses it");

    let rec = verify_integration_impl(
        &h.store,
        &h.ai,
        "claude_code",
        &claude_config(Some(&padded)),
    )
    .unwrap();
    assert_eq!(rec.key_state, KeyState::Set);
    assert_eq!(rec.masked_hint.as_deref(), Some("ygAA"));
    assert_eq!(
        h.keys.get_raw("claude_code").as_deref(),
        Some(SAMPLE_TOKEN),
        "the trimmed value is what reaches the keychain"
    );

    // Whitespace is all that is forgiven: an inner space still fails.
    let h2 = claude_harness();
    assert_eq!(
        verify_integration_impl(
            &h2.store,
            &h2.ai,
            "claude_code",
            &claude_config(Some("sk-ant-oat01-abc def"))
        )
        .unwrap_err(),
        ERR_TOKEN_MALFORMED
    );
}

#[test]
fn aic_ts29_a_malformed_token_is_refused_before_anything_runs() {
    for bad in ["", "oat01-abc", "sk-ant-oat01-", "sk-ant-oat01-abc_def"] {
        let h = claude_harness();
        // Seeded first, so "the keychain was not touched" is a claim with
        // something to lose. Against an *empty* keychain the same assertion
        // holds for an implementation that deletes the entry on a malformed
        // token — which is precisely the regression worth catching, because
        // it is the case an author actually hits: a working token stored,
        // and a typo in the replacement.
        verify_integration_impl(
            &h.store,
            &h.ai,
            "claude_code",
            &claude_config(Some(SAMPLE_TOKEN)),
        )
        .unwrap();
        let (before, _) = h.store.load_agentic_registry().unwrap();
        h.runner.calls.lock().unwrap().clear();
        h.keys.forget_calls();

        assert_eq!(
            verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(Some(bad)))
                .unwrap_err(),
            ERR_TOKEN_MALFORMED,
            "{bad:?}"
        );
        // AIC-FR-27: before the binary is run and before the keychain is
        // touched — so a value that is not a token costs neither.
        assert!(
            h.runner.calls.lock().unwrap().is_empty(),
            "{bad:?}: no binary was executed"
        );
        assert!(
            h.keys.calls_for("claude_code").is_empty(),
            "{bad:?}: the keychain was not touched at all"
        );
        assert_eq!(
            h.keys.get_raw("claude_code").as_deref(),
            Some(SAMPLE_TOKEN),
            "{bad:?}: the token that was working is still stored"
        );
        let (after, _) = h.store.load_agentic_registry().unwrap();
        assert_eq!(before, after, "{bad:?}: the registry is unchanged");
    }
}

#[test]
fn aic_ts29_a_malformed_token_against_an_empty_keychain_stores_nothing() {
    let h = claude_harness();
    assert_eq!(
        verify_integration_impl(
            &h.store,
            &h.ai,
            "claude_code",
            &claude_config(Some("sk-ant-oat01-abc_def"))
        )
        .unwrap_err(),
        ERR_TOKEN_MALFORMED
    );
    assert!(h.keys.get_raw("claude_code").is_none());
    let (records, _) = h.store.load_agentic_registry().unwrap();
    assert!(records.is_empty(), "nothing was persisted");
    assert_eq!(
        find(&list_integrations_impl(&h.store, &h.ai).unwrap(), "claude_code").state,
        IntegrationState::Unconfigured
    );
}

#[test]
fn aic_ts30_replacing_a_token_is_one_write_and_never_a_delete() {
    // AIC-FR-28: the replacement must be a single overwrite. A
    // delete-then-set reaches an identical final state through an
    // observable moment in which the vendor holds *no* token, which is what
    // this asserts against and what final-state assertions cannot see.
    let h = claude_harness();
    verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(Some(SAMPLE_TOKEN)))
        .unwrap();
    assert_eq!(h.keys.calls_for("claude_code"), vec!["set"]);
    h.keys.forget_calls();

    verify_integration_impl(
        &h.store,
        &h.ai,
        "claude_code",
        &claude_config(Some("sk-ant-oat01-second-token-value-9f2b")),
    )
    .unwrap();
    assert_eq!(
        h.keys.calls_for("claude_code"),
        vec!["set"],
        "replacing a token is exactly one set and no delete"
    );

    // And a re-verification keeping the stored token touches the keychain
    // not at all, which is what makes `{ path }` safe to send repeatedly.
    h.keys.forget_calls();
    verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(None)).unwrap();
    assert!(h.keys.calls_for("claude_code").is_empty());
}

#[test]
fn aic_ts29_a_malformed_token_error_never_quotes_the_value() {
    // AIC-FR-20: no error payload may carry credential material. A rejection
    // that echoed what it rejected would put a near-miss token — one stray
    // character from the real thing — into the Logs panel.
    let h = claude_harness();
    let attempt = "sk-ant-oat01-nearly_valid_but_not";
    let err =
        verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(Some(attempt)))
            .unwrap_err();
    assert_eq!(err, ERR_TOKEN_MALFORMED);
    assert!(!err.contains(attempt));
    assert!(!err.contains("nearly"));
}

#[test]
fn aic_ts28_a_path_only_payload_needs_a_stored_token() {
    // AIC-FR-26: `{ path }` is valid only where a token is already held.
    let h = claude_harness();
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(None))
            .unwrap_err(),
        ERR_TOKEN_MISSING
    );
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(find(&list, "claude_code").state, IntegrationState::Unconfigured);
    assert!(h.keys.get_raw("claude_code").is_none());

    // With a token stored, the same payload verifies and is indistinguishable
    // from the first result but for its timestamp.
    let first =
        verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(Some(SAMPLE_TOKEN)))
            .unwrap();
    let second =
        verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(None)).unwrap();
    assert_eq!(second.state, IntegrationState::Verified);
    assert_eq!(second.key_state, KeyState::Set);
    assert_eq!(second.masked_hint, first.masked_hint);
    assert_eq!(
        AgenticIntegration { verified_at: None, ..second },
        AgenticIntegration { verified_at: None, ..first }
    );
    // The stored token is untouched by a re-verification that carried none.
    assert_eq!(h.keys.get_raw("claude_code").as_deref(), Some(SAMPLE_TOKEN));
}

#[test]
fn aic_ts30_a_new_token_replaces_the_old_one_only_on_success() {
    // AIC-FR-28: the old token survives every failure, and the new one is
    // the only one left after a success — never both, never neither.
    const NEXT: &str = "sk-ant-oat01-second-token-value-9f2b";
    let h = claude_harness();
    verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(Some(SAMPLE_TOKEN)))
        .unwrap();

    // A well-formed new token against a path that fails.
    let bad_path = VerifyConfig {
        path: Some("/nope/claude".into()),
        oauth_token: Some(NEXT.into()),
        ..Default::default()
    };
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "claude_code", &bad_path).unwrap_err(),
        ERR_NOT_FOUND
    );
    assert_eq!(
        h.keys.get_raw("claude_code").as_deref(),
        Some(SAMPLE_TOKEN),
        "the token that was working is still the one stored"
    );
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(find(&list, "claude_code").masked_hint.as_deref(), Some("ygAA"));

    // The same token against a path that verifies.
    verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(Some(NEXT)))
        .unwrap();
    assert_eq!(h.keys.get_raw("claude_code").as_deref(), Some(NEXT));
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(find(&list, "claude_code").masked_hint.as_deref(), Some("9f2b"));
}

#[test]
fn aic_ts30_the_token_is_written_only_after_the_path_verifies() {
    // AIC-FR-28: a token is worth storing only for an installation that has
    // proved to exist, which is what makes this ordering — unlike the API
    // level's — key-after rather than key-first.
    let h = harness(
        FakeFs::with_executable(&["/usr/bin/other"]),
        FakeRunner::saying("/usr/bin/other", "GNU coreutils 9.1"),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    let wrong_binary = VerifyConfig {
        path: Some("/usr/bin/other".into()),
        oauth_token: Some(SAMPLE_TOKEN.into()),
        ..Default::default()
    };
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "claude_code", &wrong_binary).unwrap_err(),
        ERR_NOT_THE_EXPECTED_CLI
    );
    assert!(
        h.keys.get_raw("claude_code").is_none(),
        "a token must not outlive the verification that carried it"
    );
}

#[test]
fn aic_ts24_an_oauth_token_belongs_to_claude_code_alone() {
    // AIC-FR-26: sending one to Codex or OpenCode is the same class of
    // mistake as sending a base URL to a CLI, and is refused the same way
    // rather than being silently dropped.
    let h = harness(
        FakeFs::with_executable(&["/usr/bin/codex", "/usr/bin/opencode"]),
        Arc::new(FakeRunner::default()),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    for vendor in ["codex", "opencode"] {
        let config = VerifyConfig {
            path: Some(format!("/usr/bin/{vendor}")),
            oauth_token: Some(SAMPLE_TOKEN.into()),
            ..Default::default()
        };
        assert_eq!(
            verify_integration_impl(&h.store, &h.ai, vendor, &config).unwrap_err(),
            ERR_WRONG_CONFIG_KIND,
            "{vendor}"
        );
        assert!(h.keys.get_raw(vendor).is_none(), "{vendor}");
    }
    // And to an API-kind vendor, whose credential is its key.
    let api = VerifyConfig {
        base_url: Some("https://api.anthropic.com/v1".into()),
        oauth_token: Some(SAMPLE_TOKEN.into()),
        ..Default::default()
    };
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "claude_agent_api", &api).unwrap_err(),
        ERR_WRONG_CONFIG_KIND
    );
    let (records, _) = h.store.load_agentic_registry().unwrap();
    assert!(records.is_empty());
}

#[test]
fn aic_ts31_clearing_claude_code_destroys_its_token_and_is_idempotent() {
    let h = claude_harness();
    verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(Some(SAMPLE_TOKEN)))
        .unwrap();
    set_model_impl(&h.store, &h.ai, "claude_code", None, Some("opus")).unwrap();
    set_active_impl(&h.store, &h.ai, "claude_code").unwrap();

    for _ in 0..2 {
        let list = clear_integration_impl(&h.store, &h.ai, "claude_code").unwrap();
        let rec = find(&list, "claude_code");
        assert_eq!(rec.state, IntegrationState::Unconfigured);
        assert_eq!(rec.binary_path, None);
        assert_eq!(rec.key_state, KeyState::Unset);
        assert_eq!(rec.masked_hint, None);
        assert_eq!(rec.version, None);
        assert_eq!(rec.selected_model, None);
        assert!(!rec.active);
        assert!(h.keys.get_raw("claude_code").is_none());
        // AIC-FR-14: no token-derived value survives in what is returned.
        let wire = serde_json::to_string(&list).unwrap();
        assert!(!wire.contains("ygAA") && !wire.contains("sk-ant-oat01-"));
    }
}

#[test]
fn aic_ts32_a_locked_keychain_commits_nothing_for_claude_code() {
    // AIC-FR-24: the typed error, and no partial update on either side.
    let h = claude_harness();
    verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(Some(SAMPLE_TOKEN)))
        .unwrap();
    let (before, _) = h.store.load_agentic_registry().unwrap();
    h.keys.lock_it();

    assert_eq!(
        verify_integration_impl(
            &h.store,
            &h.ai,
            "claude_code",
            &claude_config(Some("sk-ant-oat01-another-token-c0de"))
        )
        .unwrap_err(),
        ERR_KEYCHAIN_UNAVAILABLE
    );
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(None))
            .unwrap_err(),
        ERR_KEYCHAIN_UNAVAILABLE,
        "a presence probe that cannot answer is not a missing token"
    );
    assert_eq!(
        clear_integration_impl(&h.store, &h.ai, "claude_code").unwrap_err(),
        ERR_KEYCHAIN_UNAVAILABLE
    );
    let (after, _) = h.store.load_agentic_registry().unwrap();
    assert_eq!(before, after, "the registry is exactly as it was");
    // AIC-FR-15 / FR-24: listing keeps working while the keychain will not
    // answer, and the record it returns says so rather than still claiming
    // to be verified. `key_presence` swallows the error into `false`; a
    // regression to `unwrap_or(true)` would leave this reading `verified`
    // and would otherwise be caught by nothing.
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(list.len(), VENDORS.len());
    let claude = find(&list, "claude_code");
    assert_eq!(claude.state, IntegrationState::KeyUnavailable);
    assert_eq!(claude.key_state, KeyState::Unavailable);
    assert_eq!(
        claude.binary_path.as_deref(),
        Some("/usr/bin/claude"),
        "a degraded record keeps its configuration"
    );
}

#[test]
fn aic_ts33_claude_code_degrades_by_token_and_by_binary() {
    let h = claude_harness();
    verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(Some(SAMPLE_TOKEN)))
        .unwrap();

    // The token is deleted out from under the application.
    h.keys.wipe("claude_code");
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    let rec = find(&list, "claude_code");
    assert_eq!(rec.state, IntegrationState::KeyUnavailable);
    assert_eq!(rec.key_state, KeyState::Unavailable);
    assert_eq!(
        rec.binary_path.as_deref(),
        Some("/usr/bin/claude"),
        "a degraded record keeps its configuration"
    );
    assert!(rec.version.is_some());
    assert_eq!(
        set_active_impl(&h.store, &h.ai, "claude_code").unwrap_err(),
        ERR_NOT_VERIFIED
    );

    // And then the binary goes too. AIC-FR-15: `missing` wins, because a
    // path that is not there is the first thing to correct.
    h.fs.remove("/usr/bin/claude");
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(find(&list, "claude_code").state, IntegrationState::Missing);
    assert_eq!(
        set_active_impl(&h.store, &h.ai, "claude_code").unwrap_err(),
        ERR_NOT_VERIFIED
    );
}

#[test]
fn aic_ts34_an_invocation_carries_no_token() {
    // AIC-FR-29: the token's whole observable effect outside this module is
    // that the record reads `key_state = "set"`. Nothing transports it.
    let h = claude_harness();
    verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(Some(SAMPLE_TOKEN)))
        .unwrap();
    set_active_impl(&h.store, &h.ai, "claude_code").unwrap();

    let invocation = resolve_agentic_invocation(&h.store, &h.ai, "/dev/acme", None).unwrap();
    match &invocation {
        AgenticInvocation::Cli { vendor, binary_path, .. } => {
            assert_eq!(vendor, "claude_code");
            assert_eq!(binary_path, "/usr/bin/claude");
        }
        other => panic!("expected a CLI invocation, got {other:?}"),
    }
    // The whole shape, not a field-by-field check: a token smuggled into a
    // variant added later would show up here.
    let rendered = format!("{invocation:?}");
    assert!(!rendered.contains(SAMPLE_TOKEN));
    assert!(!rendered.contains("sk-ant-oat01-"));
    assert!(!rendered.contains("ygAA"));
}

#[test]
fn no_log_record_this_module_emits_can_carry_a_credential() {
    // AIC-FR-20 and the conventions' logging rule. The facility redacts
    // nothing (LGC-FR-16), so the emit site is the only place this can be
    // enforced — and these four emitters are the whole of this module's
    // logging. The buffer is exported as text rather than inspected field
    // by field, because a leak added later would arrive in a *new* field
    // that a field-by-field assertion would not know to look at.
    use crate::logging::{LogBuffer, LogFilter, LogSink};

    #[derive(Clone, Default)]
    struct Silent;
    impl LogSink for Silent {
        fn publish(&self, _state: &crate::logging::BufferState) {}
    }

    static TEST_BUFFER: LogBuffer = LogBuffer::new();
    let sink = Silent;
    let h = claude_harness();

    // A successful verification carrying a brand-new token…
    let config = claude_config(Some(SAMPLE_TOKEN));
    log_verify_attempt(&sink, &TEST_BUFFER, "claude_code", &config);
    let ok = verify_integration_impl(&h.store, &h.ai, "claude_code", &config);
    log_verify_outcome(&sink, &TEST_BUFFER, "claude_code", &ok, 12);
    assert!(ok.is_ok());

    // …a rejection provoked by a near-miss value, which is the record most
    // likely to quote what it rejected…
    let near_miss = "sk-ant-oat01-nearly_valid_but_not";
    let bad = claude_config(Some(near_miss));
    log_verify_attempt(&sink, &TEST_BUFFER, "claude_code", &bad);
    let err = verify_integration_impl(&h.store, &h.ai, "claude_code", &bad);
    log_verify_outcome(&sink, &TEST_BUFFER, "claude_code", &err, 3);
    assert!(err.is_err());

    // …a refused selection, which is the one record that could quote a
    // value the author typed into a selector…
    log_selection_refused(
        &sink,
        &TEST_BUFFER,
        "claude_code",
        "model",
        Some("review"),
        ERR_UNKNOWN_MODEL,
    );

    // …and a clear, which destroys the credential.
    let cleared = clear_integration_impl(&h.store, &h.ai, "claude_code");
    log_clear_outcome(&sink, &TEST_BUFFER, "claude_code", &cleared);

    // Flush the batch the emitters queued, then read everything back.
    let _ = TEST_BUFFER.take_pending_flush(Instant::now() + Duration::from_secs(1));
    let (text, count) = TEST_BUFFER.export_text(&LogFilter::default()).unwrap();
    assert!(count >= 5, "the emitters produced records to inspect");
    for forbidden in [
        SAMPLE_TOKEN,
        near_miss,
        "sk-ant-oat01-",
        "nearly_valid",
        // Not even the masked hint: the module's own convention is that a
        // log names a credential rather than describing it.
        "ygAA",
    ] {
        assert!(
            !text.contains(forbidden),
            "a log record carried {forbidden:?}"
        );
    }
    // And what it *does* carry is enough to debug from: the vendor, whether
    // a new token came with the submission, and the typed failure.
    assert!(text.contains("claude_code"));
    assert!(text.contains("newOauthToken"));
    assert!(text.contains(ERR_TOKEN_MALFORMED));
    // A refusal names which selection and which turn kind it was, and the
    // typed reason — never the identifier the author asked for.
    assert!(text.contains(ERR_UNKNOWN_MODEL));
    assert!(text.contains("review"));
}

#[test]
fn a_token_is_never_serialisable_back_out_of_the_verify_config() {
    // AIC-FR-20: `VerifyConfig` is the one shape a cleartext token enters
    // through, so its `Debug` is hand-rolled around `redacted` — a derived
    // one would put the token one stray `{:?}` or `unwrap` panic away from
    // a log line.
    let config = claude_config(Some(SAMPLE_TOKEN));
    let rendered = format!("{config:?}");
    assert!(!rendered.contains(SAMPLE_TOKEN));
    assert!(!rendered.contains("sk-ant-oat01-"));
    assert!(rendered.contains("<redacted>"));
}
