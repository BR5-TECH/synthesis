//! EAC-FR-01 … EAC-FR-06 — the executor is unreachable from the frontend; the generated invocations, integration resolution, credentials, and the execution directory.

use super::*;

// ---------------------------------------------------------------------------
// EAC-FR-01 — unreachable from the frontend
// ---------------------------------------------------------------------------

/// The executor is a loop-facing tool: no Tauri command, no event, nothing the
/// frontend can `invoke()` (EAC-FR-01, TLC-FR-20).
///
/// Asserted against the source rather than against a registry handle, because
/// what must stay true is that nobody *adds* it — a runtime registry read would
/// pass right up until someone did.
#[test]
fn the_executor_is_not_reachable_from_the_frontend() {
    let lib = include_str!("../../../lib.rs");
    assert!(
        !lib.contains("execute_agent_cli"),
        "execute_agent_cli must not be registered in generate_handler!"
    );

    // Every file in the module, not just the entry point: a command added to
    // `runtime.rs` or `descriptor.rs` would be just as reachable.
    for source in module_sources() {
    for declaration in [
        "#[tauri::command]",
        "impl rig::tool::PortableTool",
        "fn parameters(",
        "const NAME",
        "fn description(",
    ] {
        assert!(
            !source
                .lines()
                .any(|line| line.trim_start().starts_with(declaration)),
            "agent_exec declares {declaration}"
        );
    }
    }
}

/// Every source file of this module. Both structural scans below cover all of
/// them: `agent_exec.rs` delegates argv building to `descriptor.rs` and process
/// handling to `runtime.rs`, so scanning it alone checks the one file that
/// architecturally cannot hold what is being looked for.
fn module_sources() -> [&'static str; 4] {
    [
        include_str!("../../agent_exec.rs"),
        include_str!("../descriptor.rs"),
        include_str!("../protocol.rs"),
        include_str!("../runtime.rs"),
    ]
}

// ---------------------------------------------------------------------------
// EAC-FR-35, EAC-FR-10, EAC-FR-11, EAC-FR-13, EAC-FR-22 / EAC-FR-12 — the generated invocations, exactly
// ---------------------------------------------------------------------------

/// EAC-FR-35, EAC-FR-10, EAC-FR-11, EAC-FR-13, EAC-FR-22 — Claude Code's complete Docker and vendor vectors, and the task
/// on stdin.
#[test]
fn claude_codes_generated_invocation_is_exactly_the_descriptor() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let outcome = run(&harness, runtime.clone(), task("revise the draft")).expect("runs");

    let recorded = runtime.only_run();
    let (uid, gid) = descriptor::host_ids();
    let image = test_image_reference("claude_code");
    // The canonical spelling: EAC-FR-02 gives the execution directory in that
    // form, and EAC-FR-ZKMR mounts it under the same one on both sides.
    let source = harness.workspace_target();

    // The container name and the assigned session id must differ per launch, so
    // each is matched by shape and the rest is compared literally.
    let name = container_name(&recorded.argv);
    assert!(name.starts_with("synthesis-agent-"));
    let session_id = assigned_session_id(&recorded.argv).expect("--session-id");
    assert_eq!(session_id.len(), 36, "the CLI requires a UUID");

    // EAC-FR-31: the session-state mount and the variable that addresses it.
    let sessions = harness.session_state_dir("claude_code");

    assert_eq!(
        recorded.argv,
        vec![
            "run".to_string(),
            "--rm".into(),
            "--interactive".into(),
            "--name".into(),
            name,
            "--network".into(),
            "bridge".into(),
            "--user".into(),
            format!("{uid}:{gid}"),
            "--workdir".into(),
            harness.workspace_target().into(),
            "--cap-drop".into(),
            "ALL".into(),
            "--security-opt".into(),
            "no-new-privileges".into(),
            "--mount".into(),
            format!("type=bind,source={source},target={source}"),
            "--mount".into(),
            format!("type=bind,source={sessions},target=/home/agent/.claude"),
            "--env".into(),
            "CLAUDE_CODE_OAUTH_TOKEN".into(),
            "--env".into(),
            "CLAUDE_CONFIG_DIR=/home/agent/.claude".into(),
            image,
            "-p".into(),
            "--output-format".into(),
            "stream-json".into(),
            "--verbose".into(),
            "--json-schema".into(),
            descriptor::claude_code::ENVELOPE_SCHEMA.into(),
            "--permission-mode".into(),
            "bypassPermissions".into(),
            "--session-id".into(),
            session_id,
        ]
    );

    // EAC-FR-09: the serialized task, on stdin and nowhere else — carrying the
    // executor's response contract beside it (EAC-FR-35).
    let (sent, contract) = split_task_document(&recorded.stdin);
    assert_eq!(sent.instruction, "revise the draft");
    assert_eq!(contract, *RESPONSE_CONTRACT);

    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);
    let response = outcome.response.expect("an envelope");
    assert_eq!(response.outcome, AgentOutcome::Success);
    assert_eq!(response.summary, "did the thing");
}

/// EAC-FR-12, EAC-FR-22 — Codex's distinct contract, and the identical result shape.
#[test]
fn codexs_generated_invocation_carries_its_own_contract() {
    let harness = harness_for("codex");
    // Codex declares reasoning efforts, so a resolved one becomes a config
    // override; a model becomes `--model`.
    agentic::set_model_impl(&harness.store, &harness.ai, "codex", None, Some("gpt-5.1")).expect("model");
    agentic::set_effort_impl(&harness.store, &harness.ai, "codex", None, Some("high")).expect("effort");

    let runtime = RecordingRuntime::replying(&codex_stdout(&envelope_json("success")));
    let outcome = run(&harness, runtime.clone(), task("run the suite")).expect("runs");

    let recorded = runtime.only_run();
    let image = test_image_reference("codex");
    let vendor_tail = &recorded.argv[recorded.argv.iter().position(|a| *a == image).unwrap() + 1..];

    assert_eq!(
        vendor_tail,
        [
            "exec",
            "--json",
            "--sandbox",
            "danger-full-access",
            "--skip-git-repo-check",
            "--model",
            "gpt-5.1",
            "-c",
            "model_reasoning_effort=\"high\"",
            "-",
        ]
    );

    // Codex authenticates through a mounted login directory, never an
    // environment variable carrying a secret — the Claude token variable is
    // absent entirely, and the only variable set names the mount itself.
    assert!(recorded.env.is_empty());
    assert!(!recorded.argv.contains(&"CLAUDE_CODE_OAUTH_TOKEN".to_string()));
    assert!(recorded
        .argv
        .iter()
        .any(|a| a.starts_with("CODEX_HOME=")));
    // CDX-FR-23 / EAC-FR-16: the login directory is also the session directory,
    // so its mount is read/write rather than read-only.
    let mount = recorded
        .argv
        .iter()
        .find(|a| a.contains("target=") && a.contains(".codex"))
        .expect("the codex config mount");
    assert!(
        !mount.contains("readonly"),
        "a read-only mount leaves the CLI unable to persist a session: {mount}"
    );
    // And this vendor gets no session-state mount of its own — the credential
    // mount is it.
    assert_eq!(
        recorded.argv.iter().filter(|a| *a == "--mount").count(),
        2,
        "workspace and the login directory, and nothing else"
    );

    // The caller sees the same shape it saw for Claude Code.
    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);
    assert_eq!(
        outcome.response.expect("envelope").outcome,
        AgentOutcome::Success
    );
}

// ---------------------------------------------------------------------------
// EAC-FR-02, EAC-FR-03 / EAC-FR-04 — resolution is the executor's, and only two vendors
// ---------------------------------------------------------------------------

/// EAC-FR-02, EAC-FR-03 — the request type cannot express a vendor, and the executor
/// resolves one itself on every launch.
#[test]
fn the_request_cannot_select_an_integration_and_the_executor_resolves_one() {
    // The shape claim, asserted against the source: there is no field a caller
    // could set, which is what makes EAC-FR-02 enforceable rather than a
    // convention.
    let source = include_str!("../../agent_exec.rs");
    let request_struct = source
        .split("pub struct AgentExecutionRequest {")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .expect("the request struct");
    for forbidden in [
        "vendor", "model", "effort", "image", "token", "credential", "digest",
    ] {
        assert!(
            !request_struct.contains(forbidden),
            "AgentExecutionRequest must not carry a {forbidden} field"
        );
    }

    // And the behaviour: a launch resolves through AIC, so changing the
    // project's selection changes the image without the caller saying anything.
    let harness = harness_for("codex");
    let runtime = RecordingRuntime::replying(&codex_stdout(&envelope_json("success")));
    run(&harness, runtime.clone(), task("go")).expect("runs");
    assert_eq!(
        runtime.images_asked.lock().unwrap().as_slice(),
        [test_image_reference("codex")]
    );
}

/// EAC-FR-03, EAC-FR-04 — an unresolved project, OpenCode, and each API-kind vendor all
/// refuse before the seam is ever touched.
#[test]
fn an_unresolvable_or_unsupported_integration_never_reaches_the_runtime() {
    let harness = empty_harness();
    let runtime = RecordingRuntime::replying("");
    let error = run(&harness, runtime.clone(), task("go")).unwrap_err();
    assert!(matches!(
        error,
        AgentExecutionError::IntegrationUnresolved(ref reason) if reason == agentic::ERR_NONE_CONFIGURED
    ));
    assert_eq!(runtime.launched(), 0);
    assert!(runtime.images_asked.lock().unwrap().is_empty());

    // OpenCode is CLI-kind and still unsupported: no execution protocol is
    // pinned for it.
    let harness = harness_for("opencode");
    let runtime = RecordingRuntime::replying("");
    let error = run(&harness, runtime.clone(), task("go")).unwrap_err();
    assert!(
        matches!(error, AgentExecutionError::UnsupportedIntegration(ref v) if v == "opencode"),
        "got {error:?}"
    );
    assert_eq!(runtime.launched(), 0);
}

// ---------------------------------------------------------------------------
// EAC-FR-05 / EAC-FR-06 — credentials and the execution directory
// ---------------------------------------------------------------------------

/// EAC-FR-05 — a credential the handoff will not produce stops the launch
/// before the runtime is touched, and no error payload carries anything.
///
/// The realistic case is Codex: its record verifies without a keychain entry,
/// so it resolves as usable and the handoff is the first thing to notice that
/// the author never signed in to Codex on this machine. Claude Code cannot
/// reach here by the same route — AIC degrades a record whose token it cannot
/// read to `key_unavailable` (AIC-FR-15), so an absent token surfaces one step
/// earlier as an unresolved integration, which is asserted below too.
#[test]
fn an_unavailable_credential_prevents_the_launch_without_disclosing_anything() {
    // Codex verified, with no login directory anywhere under its home.
    let empty_home = tempfile::tempdir().expect("home");
    let workspace = tempfile::tempdir().expect("workspace");
    let ai = AgenticIntegrations::new(
        Box::new(FakeProbe {
            executables: vec!["/usr/bin/codex".into()],
            directories: vec![],
        }),
        Box::new(FakeRunner {
            banners: HashMap::from([("/usr/bin/codex".to_string(), "codex 0.147.0".to_string())]),
        }),
        Box::new(NoProber),
        Box::new(Arc::new(FakeKeys::default())),
    )
    .with_home(empty_home.path());

    let store = GlobalSettingsStore::in_memory();
    verify_integration_impl(
        &store,
        &ai,
        "codex",
        &VerifyConfig {
            path: Some("/usr/bin/codex".into()),
            ..Default::default()
        },
    )
    .expect("codex verifies");
    agentic::set_active_impl(&store, &ai, "codex").expect("activates");

    let fs = FsAccess::builder()
        .allow_root(workspace.path())
        .build()
        .expect("fs");
    let context = LaunchContext {
        store: &store,
        integrations: &ai,
        project_key: "/dev/acme",
        fs: &fs,
        images: &TEST_IMAGES,
    };

    let runtime = RecordingRuntime::replying("");
    let executor = AgentCliExecutor::new(runtime.clone());
    let error = block_on(executor.execute_agent_cli(
        &Silent,
        &context,
        AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
            execution_directory: workspace.path().to_path_buf(),
            task: task("go"),
            cancellation: CancellationToken::new(),
            activity: None,
            supplementary_mount: None,
        },
    ))
    .unwrap_err();

    assert_eq!(
        error,
        AgentExecutionError::CredentialUnavailable(
            agentic::ERR_CODEX_CONFIG_MISSING.to_string()
        )
    );
    assert_eq!(runtime.launched(), 0, "the runtime was never touched");
    assert!(runtime.images_asked.lock().unwrap().is_empty());

    // No refusal carries a credential, or the home path that names the user.
    let rendered = format!("{error:?}");
    assert!(!rendered.contains(SAMPLE_TOKEN));
    assert!(!rendered.contains("sk-ant-oat01-"));
    assert!(!rendered.contains(&*empty_home.path().to_string_lossy()));

    // Claude Code with no readable token never gets this far: the integration
    // degrades and stops resolving, one step earlier.
    let claude_ai = AgenticIntegrations::new(
        Box::new(FakeProbe {
            executables: vec!["/usr/bin/claude".into()],
            directories: vec![],
        }),
        Box::new(FakeRunner {
            banners: HashMap::from([("/usr/bin/claude".to_string(), "claude 2.1.232".to_string())]),
        }),
        Box::new(NoProber),
        Box::new(Arc::new(FakeKeys::default())),
    );
    let harness = harness_for("claude_code");
    let context = LaunchContext {
        store: &harness.store,
        integrations: &claude_ai,
        project_key: "/dev/acme",
        fs: &harness.fs,
        images: &TEST_IMAGES,
    };
    let runtime = RecordingRuntime::replying("");
    let executor = AgentCliExecutor::new(runtime.clone())
        .with_session_state_root(harness.sessions_root());
    let error = block_on(executor.execute_agent_cli(
        &Silent,
        &context,
        AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
            execution_directory: harness.workspace(),
            task: task("go"),
            cancellation: CancellationToken::new(),
            activity: None,
            supplementary_mount: None,
        },
    ))
    .unwrap_err();
    assert!(matches!(
        error,
        AgentExecutionError::IntegrationUnresolved(_)
    ));
    assert_eq!(runtime.launched(), 0);
}

/// EAC-FR-06 — a directory that is missing, is a file, or cannot be mounted.
#[test]
fn an_unusable_execution_directory_is_refused_before_launch() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying("");
    let executor = AgentCliExecutor::new(runtime.clone())
        .with_session_state_root(harness.sessions_root());

    let missing = harness.workspace().join("not-here");
    let file = harness.workspace().join("a-file");
    std::fs::write(&file, b"x").expect("write");

    for (path, expected) in [
        (missing, DirectoryProblem::Missing),
        (file, DirectoryProblem::NotADirectory),
    ] {
        let error = block_on(executor.execute_agent_cli(
            &Silent,
            &harness.context(),
            AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
                execution_directory: path,
                task: task("go"),
                cancellation: CancellationToken::new(),
                activity: None,
                supplementary_mount: None,
            },
        ))
        .unwrap_err();
        assert_eq!(
            error,
            AgentExecutionError::ExecutionDirectoryInvalid(expected),
            "unexpected refusal"
        );
    }
    assert_eq!(runtime.launched(), 0);
    // Nothing was created on the way to refusing.
    assert!(!harness.workspace().join("not-here").exists());
}
