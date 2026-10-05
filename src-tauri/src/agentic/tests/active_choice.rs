//! The single active choice, clearing, and the degraded states
//! (AIC-FR-12, AIC-FR-14, AIC-FR-15, AIC-FR-24).

use super::*;

// -- AIC-FR-12 / TS-14: the single active choice ----------------------

#[test]
fn aic_ts13_one_active_choice_spans_both_kinds() {
    let fs = FakeFs::with_executable(&["/usr/bin/claude"]);
    let h = harness(
        fs,
        FakeRunner::saying("/usr/bin/claude", "claude 2.1.4"),
        FakeProber::returning(&[("claude-opus-5", "Claude Opus 5")]),
        FakeKeychain::new(),
    );
    verify_integration_impl(&h.store, &h.ai, "claude_code", &cli_config_for("claude_code", "/usr/bin/claude"))
        .unwrap();

    // Not verified yet, so it cannot be activated.
    assert_eq!(
        set_active_impl(&h.store, &h.ai, "claude_agent_api").unwrap_err(),
        ERR_NOT_VERIFIED
    );

    let list = set_active_impl(&h.store, &h.ai, "claude_code").unwrap();
    assert!(find(&list, "claude_code").active);

    // Verify the API agent and activate it: the CLI must give the role up.
    verify_integration_impl(
        &h.store,
        &h.ai,
        "claude_agent_api",
        &api_config("https://api.anthropic.com/v1", Some("sk-ant-a71c")),
    )
    .unwrap();
    let list = set_active_impl(&h.store, &h.ai, "claude_agent_api").unwrap();
    assert_eq!(
        list.iter().filter(|i| i.active).count(),
        1,
        "the registry cannot hold two active integrations"
    );
    assert!(find(&list, "claude_agent_api").active);
    assert!(!find(&list, "claude_code").active);
}

#[test]
fn aic_ts14_a_sole_verified_backend_is_active_without_being_activated() {
    let fs = FakeFs::with_executable(&["/usr/bin/claude", "/usr/bin/codex"]);
    let runner = Arc::new(FakeRunner::default());
    runner.responses.lock().unwrap().insert(
        "/usr/bin/claude".into(),
        CliOutput {
            stdout: "claude 2.1.4".into(),
            stderr: String::new(),
            success: true,
        },
    );
    runner.responses.lock().unwrap().insert(
        "/usr/bin/codex".into(),
        CliOutput {
            stdout: "codex 0.4.0".into(),
            stderr: String::new(),
            success: true,
        },
    );
    let h = harness(
        fs,
        runner,
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );

    verify_integration_impl(&h.store, &h.ai, "claude_code", &cli_config_for("claude_code", "/usr/bin/claude"))
        .unwrap();
    let resolved = get_project_impl(&h.store, &h.ai, "/dev/acme").unwrap();
    assert_eq!(resolved.resolution, ProjectResolution::Inherited);
    assert_eq!(resolved.vendor.as_deref(), Some("claude_code"));
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert!(find(&list, "claude_code").active);

    // A second verified backend makes the set larger than one, so the
    // implicit resolution stops applying.
    verify_integration_impl(&h.store, &h.ai, "codex", &cli_config_for("codex", "/usr/bin/codex")).unwrap();
    let resolved = get_project_impl(&h.store, &h.ai, "/dev/acme").unwrap();
    assert_eq!(resolved.resolution, ProjectResolution::NoneSelected);
    assert_eq!(resolved.vendor, None);
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(list.iter().filter(|i| i.active).count(), 0);

    // An explicit activation settles it.
    set_active_impl(&h.store, &h.ai, "codex").unwrap();
    let resolved = get_project_impl(&h.store, &h.ai, "/dev/acme").unwrap();
    assert_eq!(resolved.resolution, ProjectResolution::Inherited);
    assert_eq!(resolved.vendor.as_deref(), Some("codex"));
}

// -- AIC-FR-14: clearing ----------------------------------------------

#[test]
fn aic_ts15_clearing_an_api_agent_removes_its_key_and_is_idempotent() {
    let h = harness(
        FakeFs::with_executable(&[]),
        Arc::new(FakeRunner::default()),
        FakeProber::returning(&[("claude-opus-5", "Claude Opus 5")]),
        FakeKeychain::new(),
    );
    verify_integration_impl(
        &h.store,
        &h.ai,
        "claude_agent_api",
        &api_config("https://api.anthropic.com/v1", Some("sk-ant-a71c")),
    )
    .unwrap();
    set_model_impl(&h.store, &h.ai, "claude_agent_api", None, Some("claude-opus-5")).unwrap();
    set_active_impl(&h.store, &h.ai, "claude_agent_api").unwrap();
    assert!(h.keys.get_raw("claude_agent_api").is_some());

    for _ in 0..2 {
        let list = clear_integration_impl(&h.store, &h.ai, "claude_agent_api").unwrap();
        let rec = find(&list, "claude_agent_api");
        assert_eq!(rec.state, IntegrationState::Unconfigured);
        assert_eq!(rec.base_url, None);
        assert_eq!(rec.masked_hint, None);
        assert_eq!(rec.selected_model, None);
        assert_eq!(list.iter().filter(|i| i.active).count(), 0);
    }
    assert!(
        h.keys.get_raw("claude_agent_api").is_none(),
        "the keychain entry goes with the record"
    );
}

// -- AIC-FR-15, AIC-FR-12, AIC-FR-24: degraded states ---------------------------------------

#[test]
fn aic_ts16_each_kind_degrades_in_its_own_way_and_keeps_its_configuration() {
    let fs = FakeFs::with_executable(&["/usr/bin/claude"]);
    let h = harness(
        fs.clone(),
        FakeRunner::saying("/usr/bin/claude", "claude 2.1.4"),
        FakeProber::returning(&[("claude-opus-5", "Claude Opus 5")]),
        FakeKeychain::new(),
    );
    verify_integration_impl(&h.store, &h.ai, "claude_code", &cli_config_for("claude_code", "/usr/bin/claude"))
        .unwrap();
    verify_integration_impl(
        &h.store,
        &h.ai,
        "claude_agent_api",
        &api_config("https://api.anthropic.com/v1", Some("sk-ant-a71c")),
    )
    .unwrap();

    // The binary is deleted; the keychain entry is wiped.
    fs.remove("/usr/bin/claude");
    h.keys.wipe("claude_agent_api");

    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    let cli = find(&list, "claude_code");
    assert_eq!(cli.state, IntegrationState::Missing);
    assert_eq!(cli.binary_path.as_deref(), Some("/usr/bin/claude"));
    assert_eq!(cli.version.as_deref(), Some("2.1.4"));

    let api = find(&list, "claude_agent_api");
    assert_eq!(api.state, IntegrationState::KeyUnavailable);
    assert_eq!(api.key_state, KeyState::Unavailable);
    assert_eq!(
        api.base_url.as_deref(),
        Some("https://api.anthropic.com/v1"),
        "a degraded record keeps what it had"
    );

    // Neither can be activated while degraded.
    assert_eq!(
        set_active_impl(&h.store, &h.ai, "claude_code").unwrap_err(),
        ERR_NOT_VERIFIED
    );
    assert_eq!(
        set_active_impl(&h.store, &h.ai, "claude_agent_api").unwrap_err(),
        ERR_NOT_VERIFIED
    );
}
