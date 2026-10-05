//! The project override (AIC-FR-16, AIC-FR-17).

use super::*;

// -- AIC-FR-16, AIC-FR-17 / TS-18 / TS-19 / TS-20: the project override ----------

#[test]
fn aic_ts17_an_override_belongs_to_one_project() {
    let fs = FakeFs::with_executable(&["/usr/bin/claude", "/usr/bin/codex"]);
    let runner = Arc::new(FakeRunner::default());
    for (p, banner) in [
        ("/usr/bin/claude", "claude 2.1.4"),
        ("/usr/bin/codex", "codex 0.4.0"),
    ] {
        runner.responses.lock().unwrap().insert(
            p.into(),
            CliOutput {
                stdout: banner.into(),
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
    verify_integration_impl(&h.store, &h.ai, "claude_code", &cli_config_for("claude_code", "/usr/bin/claude"))
        .unwrap();
    verify_integration_impl(&h.store, &h.ai, "codex", &cli_config_for("codex", "/usr/bin/codex")).unwrap();
    set_active_impl(&h.store, &h.ai, "claude_code").unwrap();

    let overridden =
        set_project_impl(&h.store, &h.ai, "/dev/acme", Some("codex")).unwrap();
    assert_eq!(overridden.resolution, ProjectResolution::Overridden);
    assert_eq!(overridden.vendor.as_deref(), Some("codex"));

    let other = get_project_impl(&h.store, &h.ai, "/dev/other").unwrap();
    assert_eq!(other.resolution, ProjectResolution::Inherited);
    assert_eq!(other.vendor.as_deref(), Some("claude_code"));
}

#[test]
fn aic_ts18_a_cleared_integration_leaves_its_override_recorded_but_unresolving() {
    let fs = FakeFs::with_executable(&["/usr/bin/claude", "/usr/bin/codex"]);
    let runner = Arc::new(FakeRunner::default());
    for (p, banner) in [
        ("/usr/bin/claude", "claude 2.1.4"),
        ("/usr/bin/codex", "codex 0.4.0"),
    ] {
        runner.responses.lock().unwrap().insert(
            p.into(),
            CliOutput {
                stdout: banner.into(),
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
    verify_integration_impl(&h.store, &h.ai, "claude_code", &cli_config_for("claude_code", "/usr/bin/claude"))
        .unwrap();
    verify_integration_impl(&h.store, &h.ai, "codex", &cli_config_for("codex", "/usr/bin/codex")).unwrap();
    set_active_impl(&h.store, &h.ai, "claude_code").unwrap();
    set_project_impl(&h.store, &h.ai, "/dev/acme", Some("codex")).unwrap();

    clear_integration_impl(&h.store, &h.ai, "codex").unwrap();
    let resolved = get_project_impl(&h.store, &h.ai, "/dev/acme").unwrap();
    assert_eq!(resolved.resolution, ProjectResolution::OverrideUnavailable);
    assert_eq!(
        resolved.override_vendor.as_deref(),
        Some("codex"),
        "the author's choice is kept so re-verifying restores it"
    );
    assert_eq!(resolved.vendor.as_deref(), Some("claude_code"));
}

#[test]
fn aic_ts19_an_override_may_only_name_a_verified_integration() {
    let fs = FakeFs::with_executable(&["/usr/bin/claude"]);
    let h = harness(
        fs,
        FakeRunner::saying("/usr/bin/claude", "claude 2.1.4"),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    verify_integration_impl(&h.store, &h.ai, "claude_code", &cli_config_for("claude_code", "/usr/bin/claude"))
        .unwrap();

    assert_eq!(
        set_project_impl(&h.store, &h.ai, "/dev/acme", Some("codex")).unwrap_err(),
        ERR_NOT_VERIFIED
    );
    assert_eq!(h.store.load_agentic_override("/dev/acme").unwrap(), None);

    set_project_impl(&h.store, &h.ai, "/dev/acme", Some("claude_code")).unwrap();
    let back = set_project_impl(&h.store, &h.ai, "/dev/acme", None).unwrap();
    assert_eq!(back.resolution, ProjectResolution::Inherited);
    assert_eq!(back.override_vendor, None);
}

#[test]
fn an_override_needs_an_open_project() {
    let h = harness(
        FakeFs::with_executable(&[]),
        Arc::new(FakeRunner::default()),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    // An empty slot key is "no project open"; writing an override there
    // would belong to no project in particular.
    assert_eq!(
        set_project_impl(&h.store, &h.ai, "", Some("claude_code")).unwrap_err(),
        ERR_NO_PROJECT
    );
    let resolved = get_project_impl(&h.store, &h.ai, "").unwrap();
    assert_eq!(resolved.resolution, ProjectResolution::NoneConfigured);
}

#[test]
fn aic_ts20_an_override_is_one_fact_for_the_project_across_worktrees() {
    // AIC-FR-18. Keyed by project, so every worktree of it reads
    // the same value and no other project sees it. That nothing reaches a
    // worktree's `.synthesis/` is the storage layer's claim (GSS-FR-26, GSS-FR-18);
    // what this pins is that the module never takes a worktree into account.
    let fs = FakeFs::with_executable(&["/usr/bin/claude", "/usr/bin/codex"]);
    let runner = Arc::new(FakeRunner::default());
    for (p, banner) in [
        ("/usr/bin/claude", "claude 2.1.4"),
        ("/usr/bin/codex", "codex 0.4.0"),
    ] {
        runner.responses.lock().unwrap().insert(
            p.into(),
            CliOutput {
                stdout: banner.into(),
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
    verify_integration_impl(&h.store, &h.ai, "claude_code", &cli_config_for("claude_code", "/usr/bin/claude"))
        .unwrap();
    verify_integration_impl(&h.store, &h.ai, "codex", &cli_config_for("codex", "/usr/bin/codex")).unwrap();
    set_active_impl(&h.store, &h.ai, "claude_code").unwrap();
    set_project_impl(&h.store, &h.ai, "/dev/acme", Some("codex")).unwrap();

    let again = get_project_impl(&h.store, &h.ai, "/dev/acme").unwrap();
    assert_eq!(again.resolution, ProjectResolution::Overridden);
    assert_eq!(again.vendor.as_deref(), Some("codex"));
    let other = get_project_impl(&h.store, &h.ai, "/dev/other").unwrap();
    assert_eq!(other.resolution, ProjectResolution::Inherited);
    assert_eq!(other.vendor.as_deref(), Some("claude_code"));
}

#[test]
fn an_explicit_choice_that_degrades_resolves_to_nothing_rather_than_falling_back() {
    // AIC-FR-13. The author chose that backend; quietly driving a different
    // agent is worse than driving none, so a degraded explicit choice must
    // resolve to nothing rather than handing the role to the survivor.
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
    set_active_impl(&h.store, &h.ai, "claude_agent_api").unwrap();

    // The activated backend's key goes missing; the CLI is still fine.
    h.keys.wipe("claude_agent_api");

    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(
        list.iter().filter(|i| i.active).count(),
        0,
        "no record may inherit the role the author gave to another"
    );
    assert!(!find(&list, "claude_code").active);

    let resolved = get_project_impl(&h.store, &h.ai, "/dev/acme").unwrap();
    assert_eq!(resolved.resolution, ProjectResolution::NoneSelected);
    assert_eq!(
        resolve_agentic_invocation(&h.store, &h.ai, "/dev/acme", None).unwrap_err(),
        ERR_NONE_SELECTED
    );
}

#[test]
fn a_key_required_api_agent_with_no_key_is_never_verified() {
    // AIC-FR-15. `claude_agent_api` requires a key; a record giving it a
    // base URL and no hint cannot work, and calling it verified would let it
    // be activated and then resolve to an endpoint with no credential.
    let h = harness(
        FakeFs::with_executable(&[]),
        Arc::new(FakeRunner::default()),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    h.store
        .save_agentic_registry(
            vec![AgenticRecord {
                vendor: "claude_agent_api".into(),
                base_url: Some("https://api.anthropic.com/v1".into()),
                masked_hint: None,
                ..Default::default()
            }],
            None,
        )
        .unwrap();
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(
        find(&list, "claude_agent_api").state,
        IntegrationState::KeyUnavailable
    );
    assert_eq!(
        set_active_impl(&h.store, &h.ai, "claude_agent_api").unwrap_err(),
        ERR_NOT_VERIFIED
    );

    // `custom_agent_api`'s key is optional, so the same shape is verified.
    let h2 = harness(
        FakeFs::with_executable(&[]),
        Arc::new(FakeRunner::default()),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    h2.store
        .save_agentic_registry(
            vec![AgenticRecord {
                vendor: "custom_agent_api".into(),
                base_url: Some("http://localhost:8080/v1".into()),
                masked_hint: None,
                ..Default::default()
            }],
            None,
        )
        .unwrap();
    let list = list_integrations_impl(&h2.store, &h2.ai).unwrap();
    assert_eq!(
        find(&list, "custom_agent_api").state,
        IntegrationState::Verified
    );
}
