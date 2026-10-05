//! Model probing and reasoning efforts (AIC-FR-08, AIC-FR-09).

use super::*;

// -- AIC-FR-08: model probing -----------------------------------------

#[test]
fn aic_ts09_a_probe_enriches_the_list_and_never_fails_the_verification() {
    // OpenCode declares a probe; a good answer becomes `probed`.
    let fs = FakeFs::with_executable(&["/usr/bin/opencode"]);
    let runner = Arc::new(FakeRunner::default());
    runner.responses.lock().unwrap().insert(
        "/usr/bin/opencode".to_string(),
        CliOutput {
            stdout: "opencode 0.4.0\nanthropic/claude-opus-5\nopenai/gpt-5".into(),
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
    let rec = verify_integration_impl(
        &h.store,
        &h.ai,
        "opencode",
        &cli_config_for("opencode", "/usr/bin/opencode"),
    )
    .unwrap();
    assert_eq!(rec.models_origin, ModelsOrigin::Probed);
    assert!(rec.models.iter().any(|m| m.id == "anthropic/claude-opus-5"));

    // Claude Code declares no probe at all: the catalog is what it carries,
    // and the verification still succeeds.
    let fs2 = FakeFs::with_executable(&["/usr/bin/claude"]);
    let h2 = harness(
        fs2,
        FakeRunner::saying("/usr/bin/claude", "claude 2.1.4"),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    let rec2 =
        verify_integration_impl(&h2.store, &h2.ai, "claude_code", &cli_config_for("claude_code", "/usr/bin/claude"))
            .unwrap();
    assert_eq!(rec2.models_origin, ModelsOrigin::Catalog);
    assert_eq!(rec2.state, IntegrationState::Verified);
}

// -- AIC-FR-09: efforts ------------------------------------------------

#[test]
fn aic_ts10_effort_levels_come_from_the_descriptor_and_may_be_empty() {
    let h = harness(
        FakeFs::with_executable(&[]),
        Arc::new(FakeRunner::default()),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(
        find(&list, "claude_code")
            .reasoning_efforts
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        // AIC-FR-09: every level the pinned CLI accepts, as CCP-FR-05
        // states them. Declaring fewer would put a level the executor can
        // generate beyond the author's reach.
        vec!["low", "medium", "high", "xhigh", "max"]
    );
    // OpenCode declares none, which renders no effort selector at all.
    assert!(find(&list, "opencode").reasoning_efforts.is_empty());
    assert_eq!(find(&list, "opencode").selected_effort, None);
}
