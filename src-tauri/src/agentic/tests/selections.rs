//! Model and effort selections, and what a refresh prunes (AIC-FR-10).

use super::*;

// -- AIC-FR-10 / TS-12: selections ------------------------------------

#[test]
fn aic_ts11_selections_persist_and_an_unknown_id_is_refused() {
    let fs = FakeFs::with_executable(&["/usr/bin/claude"]);
    let h = harness(
        fs,
        FakeRunner::saying("/usr/bin/claude", "claude 2.1.4"),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    verify_integration_impl(&h.store, &h.ai, "claude_code", &cli_config_for("claude_code", "/usr/bin/claude"))
        .unwrap();

    let rec = set_model_impl(&h.store, &h.ai, "claude_code", None, Some("opus")).unwrap();
    assert_eq!(rec.selected_model.as_deref(), Some("opus"));

    assert_eq!(
        set_model_impl(&h.store, &h.ai, "claude_code", None, Some("nope")).unwrap_err(),
        ERR_UNKNOWN_MODEL
    );
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(
        find(&list, "claude_code").selected_model.as_deref(),
        Some("opus"),
        "a refused selection leaves the previous one alone"
    );

    // A null model under a null turn kind selects the backend's own default
    // (AIC-FR-10), the same way a null effort does.
    let cleared_model = set_model_impl(&h.store, &h.ai, "claude_code", None, None).unwrap();
    assert_eq!(cleared_model.selected_model, None);

    assert_eq!(
        set_effort_impl(&h.store, &h.ai, "claude_code", None, Some("nope")).unwrap_err(),
        ERR_UNKNOWN_EFFORT
    );
    let cleared = set_effort_impl(&h.store, &h.ai, "claude_code", None, None).unwrap();
    assert_eq!(cleared.selected_effort, None);
}

/// AIC-FR-10 (AIC-FR-10, AIC-FR-19): reasoning effort is held **per turn
/// kind, over one default**.
///
/// A kind with no override takes the default, which is what every record
/// written before kinds were distinguished resolves for every kind — so
/// nothing has to be migrated for a record to stay correct.
#[test]
fn aic_ts_mmhi_effort_is_held_per_turn_kind_over_one_default() {
    let h = claude_harness();
    verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(Some(SAMPLE_TOKEN)))
        .unwrap();
    set_active_impl(&h.store, &h.ai, "claude_code").unwrap();
    // Through the read path the requirement names, not through the private
    // helper behind it: a resolver that stopped threading the turn kind
    // would leave a helper-level assertion passing and every launch wrong.
    let resolve = |kind: &str| -> Option<String> {
        match resolve_agentic_invocation(&h.store, &h.ai, "/dev/acme", Some(kind)).unwrap() {
            AgenticInvocation::Cli { effort_id, .. } => effort_id,
            other => panic!("a CLI vendor resolved to {other:?}"),
        }
    };

    set_effort_impl(&h.store, &h.ai, "claude_code", None, Some("high")).unwrap();
    for kind in TURN_KINDS {
        assert_eq!(resolve(kind).as_deref(), Some("high"), "no override yet: {kind}");
    }

    let with_override =
        set_effort_impl(&h.store, &h.ai, "claude_code", Some("review"), Some("low")).unwrap();
    assert_eq!(with_override.effort_overrides.get("review").map(String::as_str), Some("low"));
    assert_eq!(resolve("review").as_deref(), Some("low"));
    for kind in ["work", "semantic_rebase"] {
        assert_eq!(resolve(kind).as_deref(), Some("high"), "untouched: {kind}");
    }

    // A null effort against a named kind **clears** rather than stores a
    // null, so the kind returns to the default.
    let back = set_effort_impl(&h.store, &h.ai, "claude_code", Some("review"), None).unwrap();
    assert!(back.effort_overrides.is_empty(), "{:?}", back.effort_overrides);
    assert_eq!(resolve("review").as_deref(), Some("high"));
}

/// AIC-FR-10 (AIC-FR-10, AIC-FR-19): the model is held **per turn kind,
/// over one default**, and the whole of it survives a relaunch.
///
/// Read through `resolve_agentic_invocation`, which is the path a launch
/// takes: a resolver that stopped threading the turn kind into the model
/// would leave a record-level assertion passing and every launch wrong.
#[test]
fn aic_ts_kqvw_a_model_is_held_per_turn_kind_over_one_default() {
    let h = claude_harness();
    verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(Some(SAMPLE_TOKEN)))
        .unwrap();
    set_active_impl(&h.store, &h.ai, "claude_code").unwrap();
    let resolve = |kind: &str| -> Option<String> {
        match resolve_agentic_invocation(&h.store, &h.ai, "/dev/acme", Some(kind)).unwrap() {
            AgenticInvocation::Cli { model_id, .. } => model_id,
            other => panic!("a CLI vendor resolved to {other:?}"),
        }
    };

    set_model_impl(&h.store, &h.ai, "claude_code", None, Some("opus")).unwrap();
    for kind in TURN_KINDS {
        assert_eq!(resolve(kind).as_deref(), Some("opus"), "no override yet: {kind}");
    }

    let with_override =
        set_model_impl(&h.store, &h.ai, "claude_code", Some("review"), Some("sonnet")).unwrap();
    assert_eq!(with_override.selected_model.as_deref(), Some("opus"));
    assert_eq!(
        with_override.model_overrides.get("review").map(String::as_str),
        Some("sonnet"),
    );
    assert_eq!(resolve("review").as_deref(), Some("sonnet"));
    for kind in ["work", "semantic_rebase"] {
        assert_eq!(resolve(kind).as_deref(), Some("opus"), "untouched: {kind}");
    }

    // A relaunch reads the same store back: both the default and the one
    // override are the record's, not the session's.
    let relaunched = list_integrations_impl(&h.store, &h.ai).unwrap();
    let record = find(&relaunched, "claude_code");
    assert_eq!(record.selected_model.as_deref(), Some("opus"));
    assert_eq!(
        record.model_overrides.get("review").map(String::as_str),
        Some("sonnet"),
    );

    // A null model against a named kind **clears** rather than stores a
    // null, so the kind returns to the default.
    let back = set_model_impl(&h.store, &h.ai, "claude_code", Some("review"), None).unwrap();
    assert!(back.model_overrides.is_empty(), "{:?}", back.model_overrides);
    assert_eq!(resolve("review").as_deref(), Some("opus"));
}

/// AIC-FR-10 (AIC-FR-10): the default moves without touching either map,
/// and every rejection leaves all four selections exactly as they were.
#[test]
fn aic_ts_zbrf_a_rejected_model_update_changes_nothing_the_record_holds() {
    let h = claude_harness();
    verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(Some(SAMPLE_TOKEN)))
        .unwrap();
    set_model_impl(&h.store, &h.ai, "claude_code", None, Some("opus")).unwrap();
    set_model_impl(&h.store, &h.ai, "claude_code", Some("review"), Some("sonnet")).unwrap();
    set_effort_impl(&h.store, &h.ai, "claude_code", None, Some("high")).unwrap();
    set_effort_impl(&h.store, &h.ai, "claude_code", Some("work"), Some("low"))
        .unwrap();

    // The default moves; the two maps and the effort default stand still.
    let moved = set_model_impl(&h.store, &h.ai, "claude_code", None, Some("haiku")).unwrap();
    assert_eq!(moved.selected_model.as_deref(), Some("haiku"));
    assert_eq!(
        moved.model_overrides.get("review").map(String::as_str),
        Some("sonnet"),
        "setting the default touches neither map",
    );
    assert_eq!(moved.selected_effort.as_deref(), Some("high"));
    assert_eq!(
        moved.effort_overrides.get("work").map(String::as_str),
        Some("low"),
    );

    // Every shape of rejection, each read back against the same four values.
    let before = moved.clone();
    for (turn_kind, model_id, expected) in [
        (Some("nope"), Some("opus"), ERR_UNKNOWN_TURN_KIND),
        (Some("nope"), None, ERR_UNKNOWN_TURN_KIND),
        // Both wrong at once: the kind is checked first, so the caller is
        // told which of the two closed sets it left rather than being sent
        // after the model id when the kind was never one this module holds.
        // Pinned because it is the one case where the *order* of the two
        // checks is observable, and a surface that renders the typed error
        // renders whichever this returns.
        (Some("nope"), Some("gpt-5"), ERR_UNKNOWN_TURN_KIND),
        (None, Some("gpt-5"), ERR_UNKNOWN_MODEL),
        (Some("review"), Some("gpt-5"), ERR_UNKNOWN_MODEL),
    ] {
        assert_eq!(
            set_model_impl(&h.store, &h.ai, "claude_code", turn_kind, model_id).unwrap_err(),
            expected,
            "turn kind {turn_kind:?} with model {model_id:?}",
        );
        let list = list_integrations_impl(&h.store, &h.ai).unwrap();
        let after = find(&list, "claude_code");
        assert_eq!(after.selected_model, before.selected_model);
        assert_eq!(after.model_overrides, before.model_overrides);
        assert_eq!(after.selected_effort, before.selected_effort);
        assert_eq!(after.effort_overrides, before.effort_overrides);
    }
}

/// AIC-FR-10 (AIC-FR-10, AIC-FR-19): the two maps are independent of each
/// other and of the defaults above them.
///
/// A kind may take a model of its own and the default effort, an effort of
/// its own and the default model, both, or neither — and clearing one of a
/// kind's two overrides leaves the other where it stands.
#[test]
fn aic_ts_pnxd_the_model_and_the_effort_of_one_kind_move_independently() {
    let h = claude_harness();
    verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(Some(SAMPLE_TOKEN)))
        .unwrap();
    set_active_impl(&h.store, &h.ai, "claude_code").unwrap();
    let resolve = |kind: &str| -> (Option<String>, Option<String>) {
        match resolve_agentic_invocation(&h.store, &h.ai, "/dev/acme", Some(kind)).unwrap() {
            AgenticInvocation::Cli { model_id, effort_id, .. } => (model_id, effort_id),
            other => panic!("a CLI vendor resolved to {other:?}"),
        }
    };

    set_model_impl(&h.store, &h.ai, "claude_code", None, Some("opus")).unwrap();
    set_effort_impl(&h.store, &h.ai, "claude_code", None, Some("high")).unwrap();
    set_model_impl(&h.store, &h.ai, "claude_code", Some("work"), Some("sonnet"))
        .unwrap();
    set_effort_impl(&h.store, &h.ai, "claude_code", Some("work"), Some("medium"))
        .unwrap();

    assert_eq!(
        resolve("work"),
        (Some("sonnet".into()), Some("medium".into())),
    );
    for kind in ["review", "semantic_rebase"] {
        assert_eq!(
            resolve(kind),
            (Some("opus".into()), Some("high".into())),
            "the kinds that were given nothing: {kind}",
        );
    }

    // The model override alone is cleared: that kind reasons at its own
    // level still, on the record's default model.
    set_model_impl(&h.store, &h.ai, "claude_code", Some("work"), None).unwrap();
    assert_eq!(
        resolve("work"),
        (Some("opus".into()), Some("medium".into())),
    );
    for kind in ["review", "semantic_rebase"] {
        assert_eq!(
            resolve(kind),
            (Some("opus".into()), Some("high".into())),
            "still untouched: {kind}",
        );
    }
}

/// AIC-FR-16 (AIC-FR-16, AIC-FR-19): which integration answers and what
/// that integration is set to are two separate questions.
///
/// The project override selects the integration; the model and the effort
/// then come from that integration's own default and its own overrides, so
/// moving a project from one integration to another moves it to the
/// selections the author made for the one it moved to — and changes neither
/// integration's selections in the moving.
#[test]
fn aic_ts_gvwt_a_project_override_carries_no_selections_of_its_own() {
    let h = all_clis_harness();
    for (vendor, path) in CLI_BINARIES.iter().take(2) {
        verify_integration_impl(&h.store, &h.ai, vendor, &cli_config_for(vendor, path)).unwrap();
    }
    set_active_impl(&h.store, &h.ai, "claude_code").unwrap();

    set_model_impl(&h.store, &h.ai, "claude_code", None, Some("opus")).unwrap();
    set_effort_impl(&h.store, &h.ai, "claude_code", None, Some("high")).unwrap();
    set_model_impl(&h.store, &h.ai, "codex", None, Some("gpt-5")).unwrap();
    set_model_impl(&h.store, &h.ai, "codex", Some("work"), Some("gpt-5.1"))
        .unwrap();
    set_effort_impl(&h.store, &h.ai, "codex", None, Some("low")).unwrap();

    let resolve = || match resolve_agentic_invocation(
        &h.store,
        &h.ai,
        "/dev/acme",
        Some("work"),
    )
    .unwrap()
    {
        AgenticInvocation::Cli { vendor, model_id, effort_id, .. } => {
            (vendor, model_id, effort_id)
        }
        other => panic!("a CLI vendor resolved to {other:?}"),
    };

    set_project_impl(&h.store, &h.ai, "/dev/acme", Some("codex")).unwrap();
    assert_eq!(
        resolve(),
        ("codex".to_string(), Some("gpt-5.1".into()), Some("low".into())),
        "the overridden integration's own override and default, and nothing of the other's",
    );

    set_project_impl(&h.store, &h.ai, "/dev/acme", None).unwrap();
    assert_eq!(
        resolve(),
        ("claude_code".to_string(), Some("opus".into()), Some("high".into())),
    );

    set_project_impl(&h.store, &h.ai, "/dev/acme", Some("codex")).unwrap();
    assert_eq!(
        resolve(),
        ("codex".to_string(), Some("gpt-5.1".into()), Some("low".into())),
    );
    // Neither record was written by the moving of the project between them.
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    let claude = find(&list, "claude_code");
    assert_eq!(claude.selected_model.as_deref(), Some("opus"));
    assert!(claude.model_overrides.is_empty(), "{:?}", claude.model_overrides);
    assert_eq!(claude.selected_effort.as_deref(), Some("high"));
    let codex = find(&list, "codex");
    assert_eq!(codex.selected_model.as_deref(), Some("gpt-5"));
    assert_eq!(
        codex.model_overrides.get("work").map(String::as_str),
        Some("gpt-5.1"),
    );
    assert_eq!(codex.selected_effort.as_deref(), Some("low"));
}

/// CCP-FR-05 (AIC-FR-09, AIC-FR-10): a CLI vendor declares every level its
/// pinned CLI accepts, and neither an undeclared level nor an unknown turn
/// kind is ever stored.
#[test]
fn aic_ts_dcpe_every_level_the_cli_accepts_and_nothing_outside_the_sets() {
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
        // CCP-FR-05's five. `xhigh` and `max` are levels the executor can
        // generate and the CLI honours, so a shorter list would put them
        // beyond the author's reach.
        vec!["low", "medium", "high", "xhigh", "max"],
    );
    // The two that were unreachable are selectable, and per kind.
    for level in ["xhigh", "max"] {
        let updated =
            set_effort_impl(&h.store, &h.ai, "claude_code", Some("work"), Some(level))
                .unwrap();
        assert_eq!(
            updated.effort_overrides.get("work").map(String::as_str),
            Some(level),
        );
    }
    assert_eq!(
        set_effort_impl(&h.store, &h.ai, "claude_code", Some("review"), Some("nope"))
            .unwrap_err(),
        ERR_UNKNOWN_EFFORT,
    );
    assert_eq!(
        set_effort_impl(&h.store, &h.ai, "claude_code", Some("nope"), Some("low"))
            .unwrap_err(),
        ERR_UNKNOWN_TURN_KIND,
    );
    // Both wrong at once, on the same terms the model setter is held to:
    // the kind is checked first, so a caller that named neither set
    // correctly is told about the kind rather than about the effort.
    assert_eq!(
        set_effort_impl(&h.store, &h.ai, "claude_code", Some("nope"), Some("nope"))
            .unwrap_err(),
        ERR_UNKNOWN_TURN_KIND,
    );
    // No refusal stored anything.
    let (records, _) = h.store.load_agentic_registry().unwrap();
    let record = records.iter().find(|r| r.vendor == "claude_code").unwrap();
    assert_eq!(record.effort_overrides.len(), 1, "{:?}", record.effort_overrides);
    assert!(record.effort_overrides.contains_key("work"));
}

#[test]
fn aic_ts12_a_selection_the_refreshed_list_drops_falls_back_to_the_default() {
    let fs = FakeFs::with_executable(&["/usr/bin/opencode"]);
    let runner = Arc::new(FakeRunner::default());
    runner.responses.lock().unwrap().insert(
        "/usr/bin/opencode".to_string(),
        CliOutput {
            stdout: "opencode 0.4.0\nmodel-a\nmodel-b".into(),
            stderr: String::new(),
            success: true,
        },
    );
    let h = harness(
        fs,
        runner.clone(),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    verify_integration_impl(&h.store, &h.ai, "opencode", &cli_config_for("opencode", "/usr/bin/opencode"))
        .unwrap();
    set_model_impl(&h.store, &h.ai, "opencode", None, Some("model-a")).unwrap();
    set_effort_impl(&h.store, &h.ai, "opencode", None, None).unwrap();

    // The installation is upgraded and no longer offers `model-a`.
    runner.responses.lock().unwrap().insert(
        "/usr/bin/opencode".to_string(),
        CliOutput {
            stdout: "opencode 0.5.0\nmodel-b\nmodel-c".into(),
            stderr: String::new(),
            success: true,
        },
    );
    let rec = verify_integration_impl(
        &h.store,
        &h.ai,
        "opencode",
        &cli_config_for("opencode", "/usr/bin/opencode"),
    )
    .unwrap();
    assert_eq!(
        rec.selected_model, None,
        "a model this installation no longer has falls back to its own default"
    );
}

/// AIC-FR-11 (AIC-FR-11): the clearing a refreshed model list causes reaches
/// **every** model selection the record holds, and reaches neither effort
/// selection.
///
/// An API-kind vendor, because it is the kind that both probes its models
/// and declares effort levels — an integration whose overrides can be pruned
/// while its efforts are watched for movement they must never make.
#[test]
fn aic_ts12_a_refresh_prunes_the_default_and_every_override_it_dropped() {
    let prober = FakeProber::returning(&[("m", "M"), ("n", "N")]);
    let h = harness(
        FakeFs::with_executable(&[]),
        Arc::new(FakeRunner::default()),
        prober.clone(),
        FakeKeychain::new(),
    );
    let config = api_config("https://api.anthropic.com/v1", Some("sk-ant-a71c"));
    verify_integration_impl(&h.store, &h.ai, "claude_agent_api", &config).unwrap();

    set_model_impl(&h.store, &h.ai, "claude_agent_api", None, Some("m")).unwrap();
    set_model_impl(&h.store, &h.ai, "claude_agent_api", Some("review"), Some("m")).unwrap();
    set_model_impl(
        &h.store,
        &h.ai,
        "claude_agent_api",
        Some("work"),
        Some("n"),
    )
    .unwrap();
    set_effort_impl(&h.store, &h.ai, "claude_agent_api", None, Some("high")).unwrap();
    set_effort_impl(
        &h.store,
        &h.ai,
        "claude_agent_api",
        Some("review"),
        Some("low"),
    )
    .unwrap();

    // The deployment is upgraded: it still offers `n`, and no longer `m`.
    *prober.models.lock().unwrap() = vec![ModelOption::new("n", "N")];
    let rec = verify_integration_impl(&h.store, &h.ai, "claude_agent_api", &config).unwrap();

    assert_eq!(
        rec.selected_model, None,
        "a default this deployment no longer offers falls back to its own"
    );
    assert_eq!(
        rec.model_overrides.get("review"),
        None,
        "an override naming a dropped model returns that kind to the default"
    );
    assert_eq!(
        rec.model_overrides.get("work").map(String::as_str),
        Some("n"),
        "an override the refreshed list still offers is kept exactly as it was"
    );
    // AIC-FR-11: effort levels come from the descriptor, so a refreshed model
    // list moves neither the default effort nor any effort override.
    assert_eq!(rec.selected_effort.as_deref(), Some("high"));
    assert_eq!(
        rec.effort_overrides.get("review").map(String::as_str),
        Some("low"),
    );
}
