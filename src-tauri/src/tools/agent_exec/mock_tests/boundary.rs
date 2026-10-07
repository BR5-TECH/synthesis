//! Secrecy, concurrency, and the mock's own location across the process boundary.

use super::*;

// ---------------------------------------------------------------------------
// Secrecy across the boundary
// ---------------------------------------------------------------------------

/// EAC-FR-09 / EAC-FR-15 / EAC-FR-16, checked where it actually matters: on the
/// argument vector the operating system delivered to another process.
///
/// The mock records nothing, so the evidence is the scenario's own verdict plus
/// what the executor hands back — but the vector is asserted here directly by
/// demanding a scenario that would fail if the token appeared in it.
#[test]
fn no_credential_or_task_data_crosses_as_an_argument() {
    let harness = harness_for("claude_code");
    let mut request = task("MARKER-INSTRUCTION-mock");
    let mut input = serde_json::Map::new();
    input.insert("ctx".into(), json!("MARKER-INPUT-mock"));
    request.input = Some(input);

    let scenario = scenario_for(
        "claude_code",
        &stdin_for(&request),
        &mock_claude_stdout(&envelope_json("success")),
        "",
        0,
        0,
        &[],
    );
    let outcome = drive(&harness, &scenario, request, CancellationToken::new()).expect("ran");
    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);

    // The scenario itself is the record of what was demanded of the vector. It
    // names the environment *variable* and never a token value.
    let written = &scenario.document;
    assert!(written.contains(descriptor::CLAUDE_TOKEN_ENV));
    assert!(!written.contains(SAMPLE_TOKEN));
    assert!(!written.contains("sk-ant-oat01-"));

    // Neither the returned result nor either captured stream carries the token
    // or the task's own text.
    let rendered = format!("{outcome:?}");
    for secret in [SAMPLE_TOKEN, "MARKER-INSTRUCTION-mock", "MARKER-INPUT-mock"] {
        assert!(!rendered.contains(secret), "{secret} crossed back");
    }
}

/// Two launches through one executor and one runtime, over real processes.
#[test]
fn concurrent_launches_over_real_processes_stay_separate() {
    let harness = harness_for("claude_code");

    let first = task("FIRST-TASK");
    let second = task("SECOND-TASK");
    let a = scenario_for(
        "claude_code",
        &stdin_for(&first),
        &mock_claude_stdout(&envelope_json("success")),
        "",
        0,
        200,
        &[],
    );
    let b = scenario_for(
        "claude_code",
        &stdin_for(&second),
        &mock_claude_stdout(&envelope_json("failure")),
        "",
        0,
        200,
        &[],
    );

    let (left, right) = block_on(async {
        let one = AgentCliExecutor::new(MockDockerRuntime::new(&a))
            .with_session_state_root(harness.sessions_root());
        let two = AgentCliExecutor::new(MockDockerRuntime::new(&b))
            .with_session_state_root(harness.sessions_root());
        let context = harness.context();
        tokio::join!(
            one.execute_agent_cli(
                &Silent,
                &context,
                AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
                    execution_directory: harness.workspace(),
                    task: first,
                    cancellation: CancellationToken::new(),
                    activity: None,
                    durable_output: None,
                    supplementary_mount: None,
                },
            ),
            two.execute_agent_cli(
                &Silent,
                &context,
                AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
                    execution_directory: harness.workspace(),
                    task: second,
                    cancellation: CancellationToken::new(),
                    activity: None,
                    durable_output: None,
                    supplementary_mount: None,
                },
            )
        )
    });

    // Each scenario demanded its *own* stdin, so a crossed payload would have
    // exited 65 rather than completing.
    let left = left.expect("ran");
    let right = right.expect("ran");
    assert_eq!(left.process_outcome, ProcessOutcome::Completed);
    assert_eq!(right.process_outcome, ProcessOutcome::Completed);
    assert_eq!(left.response.expect("envelope").outcome, AgentOutcome::Success);
    assert_eq!(right.response.expect("envelope").outcome, AgentOutcome::Failure);
}

/// The mock is where ACM-FR-02 says it is, and the suite builds it rather than
/// skipping when it is not.
#[test]
fn the_mock_is_located_at_the_documented_path() {
    let binary = mock_binary();
    assert!(binary.is_file());
    assert!(binary.ends_with(format!(
        "agentic-cli-mock{}",
        std::env::consts::EXE_SUFFIX
    )));
    assert!(binary.to_string_lossy().contains("tools/agentic-cli-mock/target/debug"));
}
