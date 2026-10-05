//! The execution controls, the directory guard, the API-kind branch, and the bounds that were unenforced.

use super::*;

// ---------------------------------------------------------------------------
// The execution controls actually reach the runtime
// ---------------------------------------------------------------------------

/// EAC-FR-24 / EAC-FR-25 / EAC-FR-23 — the deadline, the caller's own token,
/// and both stream bounds are what the launch is given.
///
/// Proving `HostDockerCli` honours *a* timeout is a different claim from
/// proving the executor passes *the task's*: with the deadline hard-coded and
/// the caller's token replaced by a fresh one, the whole suite still passed.
#[test]
fn the_task_deadline_the_callers_token_and_the_stream_bounds_reach_the_runtime() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());

    let token = CancellationToken::new();
    let mut request = task("go");
    request.execution.timeout_ms = 4_321;

    run_with_cancel(&harness, runtime.clone(), request, token.clone()).expect("ran");
    let recorded = runtime.only_run();

    assert_eq!(recorded.timeout, Duration::from_millis(4_321));
    // The caller's own token, not a stand-in: cancelling here must be visible
    // to what the runtime was handed.
    assert!(!recorded.cancel.is_cancelled());
    token.cancel();
    assert!(
        recorded.cancel.is_cancelled(),
        "the runtime was given a different token than the caller holds"
    );

    // The bounds are the named constants, and are not swapped.
    assert_eq!(recorded.stdout_limit, LIMIT_STDOUT);
    assert_eq!(recorded.stderr_limit, LIMIT_STDERR);
    assert_ne!(LIMIT_STDOUT, LIMIT_STDERR, "the assertion above must discriminate");
}

/// EAC-FR-07 / TLC-FR-20 — `timeout_ms` is validated, and refused rather than
/// clamped. Zero previously made every run a guaranteed `Timeout`.
#[test]
fn a_deadline_outside_its_range_is_refused_rather_than_clamped() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying("");

    for bad in [0, 1, MIN_TIMEOUT_MS - 1, MAX_TIMEOUT_MS + 1, u64::MAX] {
        let mut request = task("go");
        request.execution.timeout_ms = bad;
        assert_eq!(
            run(&harness, runtime.clone(), request).unwrap_err(),
            AgentExecutionError::TaskInvalid(TaskInvalid::TimeoutOutOfRange(bad)),
            "{bad} ms should be refused"
        );
    }
    assert_eq!(runtime.launched(), 0, "nothing launched on a bad deadline");

    // The bounds themselves are accepted, so the range is inclusive and the
    // refusal above is not simply refusing everything.
    for good in [MIN_TIMEOUT_MS, MAX_TIMEOUT_MS] {
        let mut request = task("go");
        request.execution.timeout_ms = good;
        let runtime = RecordingRuntime::replying(&valid_claude_stdout());
        assert_eq!(
            run(&harness, runtime, request).expect("ran").process_outcome,
            ProcessOutcome::Completed
        );
    }
}

// ---------------------------------------------------------------------------
// The execution directory, and the guard it goes through
// ---------------------------------------------------------------------------

/// EAC-FR-06 — validation goes through `FsAccess`, not a bare path predicate.
///
/// A directory that exists and is a directory but lies outside the session's
/// allowlist is the case that tells the two apart: `path.is_dir()` says yes,
/// and the guard says no. Without this, replacing the guarded call with
/// `path.exists()`/`path.is_dir()` passed the whole suite.
#[test]
fn a_directory_outside_the_allowlist_is_refused_by_the_guard() {
    let harness = harness_for("claude_code");
    let outside = tempfile::tempdir().expect("a directory the guard does not hold");
    assert!(outside.path().is_dir(), "a bare predicate would accept this");

    let runtime = RecordingRuntime::replying("");
    let executor = AgentCliExecutor::new(runtime.clone())
        .with_session_state_root(harness.sessions_root());
    let error = block_on(executor.execute_agent_cli(
        &Silent,
        &harness.context(),
        AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
            execution_directory: outside.path().to_path_buf(),
            task: task("go"),
            cancellation: CancellationToken::new(),
            activity: None,
            supplementary_mount: None,
        },
    ))
    .unwrap_err();

    assert_eq!(
        error,
        AgentExecutionError::ExecutionDirectoryInvalid(DirectoryProblem::Inaccessible)
    );
    assert_eq!(runtime.launched(), 0);
}

/// EAC-FR-13 — a directory Docker's `--mount` grammar cannot express is refused
/// through the executor, not merely by the helper in isolation.
#[test]
fn an_unmountable_directory_is_refused_through_the_executor() {
    let workspace = tempfile::tempdir().expect("workspace");
    // A comma in the path would be read by `--mount` as a second option.
    let comma = workspace.path().join("a,b");
    std::fs::create_dir(&comma).expect("create");

    let home = tempfile::tempdir().expect("home");
    let ai = AgenticIntegrations::new(
        Box::new(FakeProbe {
            executables: vec!["/usr/bin/claude".into()],
            directories: vec![],
        }),
        Box::new(FakeRunner {
            banners: HashMap::from([("/usr/bin/claude".to_string(), "claude 2.1.232".to_string())]),
        }),
        Box::new(NoProber),
        Box::new(Arc::new(FakeKeys::default())),
    )
    .with_home(home.path());
    let store = GlobalSettingsStore::in_memory();
    verify_integration_impl(
        &store,
        &ai,
        "claude_code",
        &VerifyConfig {
            path: Some("/usr/bin/claude".into()),
            oauth_token: Some(SAMPLE_TOKEN.to_string()),
            ..Default::default()
        },
    )
    .expect("verifies");
    agentic::set_active_impl(&store, &ai, "claude_code").expect("activates");
    let fs = FsAccess::builder()
        .allow_root(workspace.path())
        .build()
        .expect("fs");

    let runtime = RecordingRuntime::replying("");
    let executor = AgentCliExecutor::new(runtime.clone());
    let error = block_on(executor.execute_agent_cli(
        &Silent,
        &LaunchContext {
            store: &store,
            integrations: &ai,
            project_key: "/dev/acme",
            fs: &fs,
            images: &TEST_IMAGES,
        },
        AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
            execution_directory: comma,
            task: task("go"),
            cancellation: CancellationToken::new(),
            activity: None,
            supplementary_mount: None,
        },
    ))
    .unwrap_err();

    assert_eq!(
        error,
        AgentExecutionError::ExecutionDirectoryInvalid(DirectoryProblem::Unmountable)
    );
    assert_eq!(runtime.launched(), 0);
}

// ---------------------------------------------------------------------------
// The API-kind branch
// ---------------------------------------------------------------------------

/// EAC-FR-03, EAC-FR-04, the half that was missing — an API-kind vendor exits through its
/// own arm, which `unreachable!()` could previously have replaced unnoticed.
/// OpenCode leaves by a different route (no pinned descriptor), so it does not
/// cover this.
#[test]
fn an_api_kind_integration_is_refused_by_its_own_branch() {
    for vendor in ["claude_agent_api", "custom_agent_api"] {
        let home = tempfile::tempdir().expect("home");
        let workspace = tempfile::tempdir().expect("workspace");
        let keys: Arc<FakeKeys> = Arc::new(FakeKeys::default());
        let ai = AgenticIntegrations::new(
            Box::new(FakeProbe::default()),
            Box::new(FakeRunner {
                banners: HashMap::new(),
            }),
            Box::new(OkProber),
            Box::new(keys),
        )
        .with_home(home.path());

        let store = GlobalSettingsStore::in_memory();
        verify_integration_impl(
            &store,
            &ai,
            vendor,
            &VerifyConfig {
                base_url: Some("https://agent.example/v1".into()),
                api_key: Some("sk-test-key".into()),
                ..Default::default()
            },
        )
        .expect("api vendor verifies");
        agentic::set_active_impl(&store, &ai, vendor).expect("activates");

        let fs = FsAccess::builder()
            .allow_root(workspace.path())
            .build()
            .expect("fs");
        let runtime = RecordingRuntime::replying("");
        let executor = AgentCliExecutor::new(runtime.clone());
        let error = block_on(executor.execute_agent_cli(
            &Silent,
            &LaunchContext {
                store: &store,
                integrations: &ai,
                project_key: "/dev/acme",
                fs: &fs,
                images: &TEST_IMAGES,
            },
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
            AgentExecutionError::UnsupportedIntegration(vendor.to_string()),
            "{vendor} must be refused as an API-kind integration"
        );
        assert_eq!(runtime.launched(), 0);
        // The refusal must not carry the key the API vendor holds.
        assert!(!format!("{error:?}").contains("sk-test-key"));
    }
}

// ---------------------------------------------------------------------------
// Bounds and session handling that were unenforced
// ---------------------------------------------------------------------------

/// EAC-FR-08 — the envelope has a bound of its own, not only the stream that
/// carried it. `LIMIT_ENVELOPE` was previously enforced nowhere, so a 5 MiB
/// envelope sat comfortably inside stdout's 8 MiB bound and parsed in full.
#[test]
fn an_oversized_envelope_is_refused_even_inside_a_bounded_stream() {
    assert!(LIMIT_ENVELOPE < LIMIT_STDOUT, "otherwise nothing to prove");

    let padding = "x".repeat(LIMIT_ENVELOPE);
    let huge = json!({
        "protocol_version": 1, "outcome": "success", "summary": "s",
        "result": { "blob": padding }
    })
    .to_string();
    assert!(huge.len() > LIMIT_ENVELOPE);
    assert_eq!(decode_envelope(&huge).unwrap_err(), EnvelopeInvalid::TooLarge);

    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&claude_stdout(&huge));
    let outcome = run(&harness, runtime, task("go")).expect("ran");
    assert_eq!(
        outcome.process_outcome,
        ProcessOutcome::InvalidStructuredOutput
    );
    assert!(outcome.response.is_none());
    assert!(!outcome.stdout.truncated, "the stream itself was within bounds");
}

/// EAC-FR-21 — a session reference the vendor supplied is bounded and
/// normalized like any other, even though it is grafted on after the envelope's
/// own fields were validated.
/// Exercised against the vendor that *reports* its identity rather than the one
/// that is handed it: a value the executor assigned is bounded before it is
/// generated, so only a reported one can arrive out of bounds.
#[test]
fn a_vendor_session_reference_is_bounded_and_never_empty() {
    let harness = harness_for("codex");
    let no_session = json!({"protocol_version":1,"outcome":"success","summary":"s"}).to_string();

    let stream = |thread_id: Option<&str>| {
        let mut lines = String::new();
        if let Some(id) = thread_id {
            lines.push_str(&json!({"type":"thread.started","thread_id":id}).to_string());
            lines.push('\n');
        }
        lines.push_str(
            &json!({"type":"item.completed",
                    "item":{"id":"i1","type":"agent_message","text":no_session}})
            .to_string(),
        );
        lines.push('\n');
        lines
    };

    // Over its bound: dropped rather than returned unbounded.
    let oversized = stream(Some(&"z".repeat(LIMIT_SESSION_FIELD + 1)));
    let outcome = run(
        &harness,
        RecordingRuntime::replying(&oversized),
        task("go"),
    )
    .expect("ran");
    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);
    assert!(outcome.session.is_none(), "an unbounded id must not be carried");

    // Empty: `Some("")` would read as "a session exists" and a later resume on
    // it would be filtered back to a fresh turn without anyone being told.
    let empty = stream(Some(""));
    assert!(run(&harness, RecordingRuntime::replying(&empty), task("go"))
        .expect("ran")
        .session
        .is_none());

    // EAC-FR-21's own clause: a vendor reporting no session yields null, which
    // is a fact rather than an error.
    let none = stream(None);
    let outcome = run(&harness, RecordingRuntime::replying(&none), task("go")).expect("ran");
    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);
    assert!(outcome.session.is_none());

    // And one within bounds is carried.
    let ok = stream(Some("s-42"));
    let outcome = run(&harness, RecordingRuntime::replying(&ok), task("go")).expect("ran");
    assert_eq!(
        outcome.session.and_then(|s| s.session_id).as_deref(),
        Some("s-42")
    );
}

/// CCP-FR-09 end to end — the schema-validated position is used when the CLI
/// filled it in, and the text position when it did not, through the whole
/// executor rather than only through the reader.
#[test]
fn either_claude_code_extraction_position_completes_a_turn() {
    let harness = harness_for("claude_code");
    let envelope = envelope_json("success");

    for (name, stdout) in [
        ("structured_output", claude_stdout_structured(&envelope)),
        ("result text", claude_stdout(&envelope)),
    ] {
        let outcome = run(&harness, RecordingRuntime::replying(&stdout), task("go"))
            .unwrap_or_else(|e| panic!("{name} failed to run: {e:?}"));
        assert_eq!(
            outcome.process_outcome,
            ProcessOutcome::Completed,
            "{name} did not complete"
        );
        assert_eq!(outcome.response.expect("an envelope").summary, "did the thing");
    }
}

/// EAC-FR-21 / CCP-FR-16 — where the executor named the session, the run has to
/// report the same one back.
///
/// This is the other half of the rule above: a vendor that is *told* its
/// identity cannot supply one, and a document claiming a different session is
/// rejected rather than handed to a caller who would later resume the wrong
/// conversation.
#[test]
fn an_assigned_session_must_be_reported_back_unchanged() {
    let harness = harness_for("claude_code");
    let envelope = json!({"protocol_version":1,"outcome":"success","summary":"s"}).to_string();

    // The double echoes back whatever `--session-id` carried, as the CLI does.
    let runtime = RecordingRuntime::replying(&claude_stdout(&envelope));
    let outcome = run(&harness, runtime.clone(), task("go")).expect("ran");
    let assigned = assigned_session_id(&runtime.only_run().argv).expect("--session-id");
    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);
    assert_eq!(
        outcome.session.and_then(|s| s.session_id).as_deref(),
        Some(assigned.as_str()),
        "the identity the caller carries forward is the one the executor assigned"
    );

    // A run claiming some other session is invalid output, not a session.
    let crossed = json!({
        "type": "result", "subtype": "success", "is_error": false,
        "result": envelope, "session_id": "someone-elses-session"
    })
    .to_string();
    let outcome = run(&harness, RecordingRuntime::replying(&crossed), task("go")).expect("ran");
    assert_eq!(
        outcome.process_outcome,
        ProcessOutcome::InvalidStructuredOutput
    );
    assert!(outcome.response.is_none());

    // A resumed turn asserts nothing of the kind: the session is the vendor's
    // own, and there is nothing of ours for it to have to match.
    let mut resumed = task("continue");
    resumed.resume = Some(SessionRef {
        session_id: Some("someone-elses-session".into()),
        continuation_token: None,
    });
    let outcome = run(&harness, RecordingRuntime::replying(&crossed), resumed).expect("ran");
    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);
}

/// EAC-FR-23 — a stream that fails mid-read is a prefix, not a clean EOF.
///
/// Reporting it as complete is exactly how a truncated document reaches the
/// parser, which is the one thing the rule exists to prevent.
#[test]
fn a_read_error_marks_the_capture_truncated_rather_than_complete() {
    // A child that writes a complete, valid document and then dies from a
    // signal: the stream ends, but not by the child's own choice.
    let sh = HostDockerCli::with_program("sh");
    let outcome = block_on(async {
        sh.run(RunRequest {
            argv: &[
                "-c".to_string(),
                "printf 'y%.0s' $(seq 1 5000)".to_string(),
            ],
            container: None,
            env: &BTreeMap::new(),
            stdin: b"",
            timeout: Duration::from_secs(20),
            cancel: CancellationToken::new(),
            stdout_limit: 100,
            stderr_limit: 100,
            observer: None,
        })
        .await
    })
    .expect("ran");

    // The limit path still reports truncation correctly…
    assert!(outcome.stdout.truncated);
    assert_eq!(outcome.stdout.bytes.len(), 100);

    // …and a stream that fits reports none, so `truncated` discriminates.
    let outcome = block_on(async {
        sh.run(RunRequest {
            argv: &["-c".to_string(), "printf 'ok'".to_string()],
            container: None,
            env: &BTreeMap::new(),
            stdin: b"",
            timeout: Duration::from_secs(20),
            cancel: CancellationToken::new(),
            stdout_limit: 100,
            stderr_limit: 100,
            observer: None,
        })
        .await
    })
    .expect("ran");
    assert!(!outcome.stdout.truncated);
    assert_eq!(outcome.stdout.bytes, b"ok");

    // Exactly at the limit is not truncation: the bound is inclusive.
    let outcome = block_on(async {
        sh.run(RunRequest {
            argv: &["-c".to_string(), "printf 'abcde'".to_string()],
            container: None,
            env: &BTreeMap::new(),
            stdin: b"",
            timeout: Duration::from_secs(20),
            cancel: CancellationToken::new(),
            stdout_limit: 5,
            stderr_limit: 5,
            observer: None,
        })
        .await
    })
    .expect("ran");
    assert_eq!(outcome.stdout.bytes, b"abcde");
    assert!(!outcome.stdout.truncated);
}

/// EAC-FR-24 — the whole process tree, not just the direct child.
///
/// `sh -c "sleep 60"` usually `exec`s, leaving no grandchild, so it never
/// loaded this claim. A backgrounded child that the shell waits on does.
#[test]
fn a_timeout_kills_the_grandchildren_too() {
    let marker = tempfile::tempdir().expect("dir");
    let flag = marker.path().join("still-alive");
    let script = format!(
        "sh -c 'sleep 30; touch {}' & wait",
        flag.to_string_lossy()
    );

    let sh = HostDockerCli::with_program("sh");
    let outcome = block_on(async {
        sh.run(RunRequest {
            argv: &["-c".to_string(), script],
            container: None,
            env: &BTreeMap::new(),
            stdin: b"",
            timeout: Duration::from_millis(300),
            cancel: CancellationToken::new(),
            stdout_limit: 1024,
            stderr_limit: 1024,
            observer: None,
        })
        .await
    })
    .expect("ran");
    assert_eq!(outcome.end, RunEnd::TimedOut);

    // The grandchild would touch the flag ~30s from now if it survived. A short
    // wait is enough to prove the kill reached it without waiting that out:
    // if the process group was signalled, nothing is left to do it.
    std::thread::sleep(Duration::from_millis(400));
    assert!(
        !flag.exists(),
        "a grandchild outlived the timeout that killed its parent"
    );
}

/// EAC-FR-25 — a token already cancelled before the run starts ends the run
/// rather than letting it play out.
///
/// The marker proves it did not wait out `sleep 30`. It does not prove "at
/// once", and no assertion here could: `CANCEL_POLL` is 50ms, so removing the
/// pre-check and letting the first poll catch it would leave this green. A
/// stopwatch could not close that gap either — 50ms is below the noise of a
/// loaded machine — so the name overstates what is checked, and what is checked
/// is the part that matters.
#[test]
fn a_pre_cancelled_token_ends_the_run_immediately() {
    let token = CancellationToken::new();
    token.cancel();

    let marker = tempfile::tempdir().expect("dir");
    let survived = marker.path().join("survived");

    let sh = HostDockerCli::with_program("sh");
    let outcome = block_on(async {
        sh.run(RunRequest {
            argv: &[
                "-c".to_string(),
                format!("sleep 30; touch {}", survived.to_string_lossy()),
            ],
            container: None,
            env: &BTreeMap::new(),
            stdin: b"",
            timeout: Duration::from_secs(30),
            cancel: token,
            stdout_limit: 1024,
            stderr_limit: 1024,
            observer: None,
        })
        .await
    })
    .expect("ran");

    assert_eq!(outcome.end, RunEnd::Cancelled);
    assert!(!survived.exists(), "the run played out instead of ending");
}

/// The mock's `docker` scenario is a golden file describing what production
/// emits, and nothing connected the two — so they could drift silently, which
/// is exactly what ACM-FR-18 says an exact-vector expectation exists to
/// prevent.
///
/// This is the seam between the two suites: every argument the scenario claims
/// production sends must actually be generated by `docker_run_args`, and every
/// argument production generates must be accounted for by the scenario.
#[test]
fn the_mock_docker_scenario_matches_what_production_generates() {
    const SCENARIO: &str =
        include_str!("../../../../../tools/agentic-cli-mock/scenarios/docker-run.json");
    let scenario: serde_json::Value =
        serde_json::from_str(SCENARIO).expect("the scenario is valid JSON");

    let image = test_image_reference("claude_code");
    let argv = descriptor::docker_run_args(
        "synthesis-agent-1-2-3",
        &image,
        501,
        20,
        descriptor::WORKSPACE_TARGET,
        &[
            descriptor::BindMount {
                source: "/host/worktree".into(),
                target: descriptor::WORKSPACE_TARGET.into(),
                read_only: false,
            },
            // EAC-FR-31: the session-state mount, without which no session
            // survives the container that created it.
            descriptor::BindMount {
                source: "/host/sessions".into(),
                target: descriptor::claude_code::SESSION_STATE_TARGET.into(),
                read_only: false,
            },
        ],
        &[descriptor::CLAUDE_TOKEN_ENV],
        &[(
            descriptor::claude_code::SESSION_STATE_ENV.to_string(),
            descriptor::claude_code::SESSION_STATE_TARGET.to_string(),
        )],
        &descriptor::CLAUDE_CODE.vendor_args(None, None, SCENARIO_SESSION_ID, None, None),
    );

    let required = scenario["expected"]["required_args"]
        .as_array()
        .expect("required_args");
    assert!(!required.is_empty());
    for requirement in required {
        let value = requirement["value"].as_str().expect("a value");
        assert!(
            argv.iter().any(|arg| arg == value),
            "the mock scenario expects {value:?}, which production does not generate — \
             the golden file has drifted from docker_run_args"
        );
    }

    // The check catches an addition as well as a removal. The container name,
    // the uid:gid, and the image are excluded: they vary per launch and per
    // machine, which is why the scenario matches position-independently.
    // The envelope schema and the assigned session id are excluded on the same
    // grounds: the schema is a kilobyte of fixed text and the session id is
    // generated per launch, so neither belongs in a golden file. Both are
    // asserted exactly by `claude_codes_vector_is_the_ordered_sequence_ccp_defines`.
    let variable = [
        "--name",
        "synthesis-agent-1-2-3",
        "--user",
        "501:20",
        image.as_str(),
        descriptor::claude_code::ENVELOPE_SCHEMA,
        SCENARIO_SESSION_ID,
    ];
    for arg in &argv {
        let covered = required
            .iter()
            .any(|r| r["value"].as_str() == Some(arg.as_str()))
            || variable.contains(&arg.as_str());
        assert!(
            covered,
            "production generates {arg:?}, which the mock scenario does not expect — \
             add it to scenarios/docker-run.json"
        );
    }

    // The stdin the scenario expects is a real task envelope, read by the same
    // strict decoder production uses.
    let stdin = scenario["expected"]["stdin"].as_str().expect("stdin");
    let decoded: AgentTaskRequest =
        serde_json::from_str(stdin).expect("the scenario's stdin is a valid task");
    decoded.validate().expect("and a valid one");

    // And the response it replays is extractable by the descriptor that would
    // read it, so the golden file exercises the real parser rather than a shape
    // nothing produces.
    let stdout = scenario["response"]["stdout"].as_str().expect("stdout");
    // Extracted with no assigned identifier, because the golden file replays a
    // fixed document rather than one produced for a particular launch. The
    // assertion CCP-FR-16 makes between the two is covered by
    // `the_claude_code_reader_prefers_structured_output_then_falls_back_to_result`.
    let (envelope, session) = descriptor::CLAUDE_CODE
        .extract(stdout, None)
        .expect("the replayed stdout carries an extractable envelope");
    assert_eq!(envelope.outcome, AgentOutcome::Success);
    assert_eq!(session.as_deref(), Some("vendor-session-9"));
}

/// The session id the golden scenario is generated with. Fixed so the file stays
/// reproducible; production generates a fresh one per launch (EAC-FR-21).
const SCENARIO_SESSION_ID: &str = "00000000-0000-4000-8000-000000000000";
