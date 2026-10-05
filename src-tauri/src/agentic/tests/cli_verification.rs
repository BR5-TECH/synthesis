//! CLI verification and the origin of a path (AIC-FR-04, AIC-FR-06,
//! AIC-FR-07, AIC-FR-26).

use super::*;

// -- AIC-FR-04, AIC-FR-06, AIC-FR-26 / TS-05 / TS-06 / TS-07: CLI verification --------------

#[test]
fn aic_ts04_a_good_cli_verifies_and_commits_its_path() {
    let fs = FakeFs::with_executable(&["/usr/bin/claude"]);
    let h = harness(
        fs,
        FakeRunner::saying("/usr/bin/claude", "claude 2.1.4"),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    let rec =
        verify_integration_impl(&h.store, &h.ai, "claude_code", &cli_config_for("claude_code", "/usr/bin/claude"))
            .unwrap();
    assert_eq!(rec.state, IntegrationState::Verified);
    assert_eq!(rec.version.as_deref(), Some("2.1.4"));
    assert!(rec.verified_at.is_some());
    assert_eq!(rec.binary_path.as_deref(), Some("/usr/bin/claude"));
    // AIC-FR-25: a CLI record carries no API fields.
    assert_eq!(rec.base_url, None);
    // AIC-FR-04, AIC-FR-06, AIC-FR-26: Claude Code's token is stored, described by a masked hint,
    // and carried nowhere else. The serialised record is inspected because
    // that is the shape that actually crosses the IPC boundary — a field
    // added later that echoed the token would show up here and nowhere in a
    // field-by-field assertion.
    assert_eq!(rec.key_state, KeyState::Set);
    assert_eq!(rec.masked_hint.as_deref(), Some("ygAA"));
    let wire = serde_json::to_string(&rec).unwrap();
    assert!(
        !wire.contains(SAMPLE_TOKEN),
        "no part of the token beyond its masked hint may cross the boundary"
    );
    assert!(!wire.contains("sk-ant-oat01-"));
    assert_eq!(h.keys.get_raw("claude_code").as_deref(), Some(SAMPLE_TOKEN));
}

#[test]
fn aic_ts05_each_cli_failure_is_distinguishable_and_persists_nothing() {
    let fs = FakeFs::with_executable(&[]);
    fs.add_file("/usr/bin/nonexec"); // exists, not executable
    let h = harness(
        fs.clone(),
        FakeRunner::saying("/usr/bin/other", "GNU coreutils 9.1"),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );

    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "claude_code", &cli_config_for("claude_code", "")).unwrap_err(),
        ERR_PATH_EMPTY
    );
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "claude_code", &cli_config_for("claude_code", "/nope"))
            .unwrap_err(),
        ERR_NOT_FOUND
    );
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "claude_code", &cli_config_for("claude_code", "/usr/bin/nonexec"))
            .unwrap_err(),
        ERR_NOT_EXECUTABLE
    );
    fs.add_executable("/usr/bin/other");
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "claude_code", &cli_config_for("claude_code", "/usr/bin/other"))
            .unwrap_err(),
        ERR_NOT_THE_EXPECTED_CLI
    );

    assert!(
        h.store.load_agentic_registry().unwrap().0.is_empty(),
        "not one of those failures may leave a record behind"
    );
}

#[test]
fn aic_ts06_a_failed_verification_never_degrades_a_working_one() {
    let fs = FakeFs::with_executable(&["/a/claude"]);
    let h = harness(
        fs,
        FakeRunner::saying("/a/claude", "claude 2.1.4"),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    verify_integration_impl(&h.store, &h.ai, "claude_code", &cli_config_for("claude_code", "/a/claude")).unwrap();
    assert!(
        verify_integration_impl(&h.store, &h.ai, "claude_code", &cli_config_for("claude_code", "/b/claude"))
            .is_err()
    );
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    let rec = find(&list, "claude_code");
    assert_eq!(rec.binary_path.as_deref(), Some("/a/claude"));
    assert_eq!(rec.version.as_deref(), Some("2.1.4"));
    assert_eq!(rec.state, IntegrationState::Verified);
    // AIC-FR-28: and the credential the working configuration depends on.
    assert_eq!(rec.masked_hint.as_deref(), Some("ygAA"));
    assert_eq!(rec.key_state, KeyState::Set);
    assert_eq!(h.keys.get_raw("claude_code").as_deref(), Some(SAMPLE_TOKEN));
}

#[test]
fn aic_ts07_a_cli_that_never_returns_times_out() {
    let fs = FakeFs::with_executable(&["/usr/bin/claude"]);
    let h = harness(
        fs,
        FakeRunner::failing("timeout"),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "claude_code", &cli_config_for("claude_code", "/usr/bin/claude"))
            .unwrap_err(),
        ERR_TIMED_OUT
    );
}

// -- AIC-FR-07: path origin -------------------------------------------

#[test]
fn aic_ts08_a_path_the_author_supplied_is_never_replaced_by_detection() {
    // `/opt/homebrew/bin/claude` is a conventional location, so detection
    // finds it; `/custom/claude` is not, so it reads as user-supplied.
    let fs = FakeFs::with_executable(&["/opt/homebrew/bin/claude", "/custom/claude"]);
    let runner = Arc::new(FakeRunner::default());
    for p in ["/opt/homebrew/bin/claude", "/custom/claude"] {
        runner.responses.lock().unwrap().insert(
            p.to_string(),
            CliOutput {
                stdout: "claude 2.1.4".into(),
                stderr: String::new(),
                success: true,
            },
        );
    }
    let h = harness(
        fs,
        runner,
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );

    let detected = verify_integration_impl(
        &h.store,
        &h.ai,
        "claude_code",
        &cli_config_for("claude_code", "/opt/homebrew/bin/claude"),
    )
    .unwrap();
    assert_eq!(detected.path_origin, PathOrigin::Detected);

    let supplied =
        verify_integration_impl(&h.store, &h.ai, "claude_code", &cli_config_for("claude_code", "/custom/claude"))
            .unwrap();
    assert_eq!(supplied.path_origin, PathOrigin::UserSupplied);
    // Detection still finds the homebrew path, but the stored one is the
    // author's and stays that way.
    assert_eq!(
        detect_binary_impl(&h.ai, "claude_code").unwrap().path.as_deref(),
        Some("/opt/homebrew/bin/claude")
    );
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(
        find(&list, "claude_code").binary_path.as_deref(),
        Some("/custom/claude")
    );
}
