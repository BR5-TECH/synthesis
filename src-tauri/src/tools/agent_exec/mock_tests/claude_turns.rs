//! Claude Code driven end to end through a real process boundary.

use super::*;

// ---------------------------------------------------------------------------
// Claude Code
// ---------------------------------------------------------------------------

/// Valid structured success, end to end through a real process.
///
/// The mock exits `65` before emitting anything if a token the scenario demanded
/// is missing or the stdin bytes differ, so reaching `Completed` at all is the
/// assertion that production emitted them. That check is presence-only, though —
/// it cannot see a token production *added* or a reordering — so the recorded
/// vector is compared exactly here as well.
#[test]
fn claude_code_runs_end_to_end_against_the_mock() {
    let harness = harness_for("claude_code");
    let request = task("revise the draft");
    let scenario = scenario_for(
        "claude_code",
        &stdin_for(&request),
        &mock_claude_stdout(&envelope_json("success")),
        "",
        0,
        0,
        &[],
    );

    let runtime = MockDockerRuntime::new(&scenario);
    let outcome = drive_with(&harness, runtime.clone(), request, CancellationToken::new())
        .expect("ran");

    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);
    assert_eq!(outcome.exit_code, Some(0));
    let response = outcome.response.expect("an envelope");
    assert_eq!(response.outcome, AgentOutcome::Success);
    assert_eq!(response.summary, "did the thing");
    assert!(outcome.stderr.bytes.is_empty());
    assert!(outcome.container_removed);

    // The identity the caller carries forward is the one the executor assigned
    // and the process reported back — asserted across a real pipe, not only in
    // the in-process double (CCP-FR-16, EAC-FR-21).
    let image = test_image_reference("claude_code");
    let tail = runtime.only_vendor_tail(&image);
    let assigned = assigned_session_id(&tail).expect("--session-id");
    assert_eq!(
        outcome.session.and_then(|s| s.session_id).as_deref(),
        Some(assigned.as_str())
    );

    // And the exact vendor vector, which the scenario's presence-only check
    // cannot pin.
    assert_eq!(
        tail,
        [
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--json-schema",
            descriptor::claude_code::ENVELOPE_SCHEMA,
            "--permission-mode",
            "bypassPermissions",
            "--session-id",
            &assigned,
        ]
    );
}

/// The negative that proves the positive: when the vector is *not* what the
/// scenario expects, the mock refuses and no configured response is emitted.
///
/// Without this, every test above would pass just as happily against a mock
/// that validated nothing.
#[test]
fn a_vector_the_scenario_does_not_expect_is_refused_by_the_mock() {
    let harness = harness_for("claude_code");
    let request = task("go");

    // A scenario demanding a vendor argument production does not generate.
    let scenario = scenario_for(
        "claude_code",
        &stdin_for(&request),
        &mock_claude_stdout(&envelope_json("success")),
        "",
        0,
        0,
        &["--dangerously-skip-permissions"],
    );

    let outcome = drive(&harness, &scenario, request, CancellationToken::new()).expect("ran");

    // Exit 65 is the mock's expectation-mismatch code, and it suppressed the
    // configured stdout entirely (ACM-FR-26).
    assert_eq!(outcome.process_outcome, ProcessOutcome::NonZeroExit);
    assert_eq!(outcome.exit_code, Some(65));
    assert!(outcome.response.is_none());
    assert!(outcome.stdout.bytes.is_empty());

    // The mock's failure record names the location and category and discloses
    // no value (ACM-FR-24) — so a diagnostic from a failed assertion cannot
    // leak the task or a credential.
    let record: serde_json::Value =
        serde_json::from_slice(&outcome.stderr.bytes).expect("one JSON record");
    assert_eq!(record["kind"], "expectation_mismatch");
    assert_eq!(record["check"], "required_args");
}

/// Stdin is compared byte for byte on the far side of the pipe: a task the
/// caller did not send is a mismatch, not a pass.
#[test]
fn the_task_envelope_crosses_the_pipe_byte_for_byte() {
    let harness = harness_for("claude_code");
    let request = task("the exact instruction");

    // A scenario expecting a *different* task envelope.
    let other = task("some other instruction");
    let scenario = scenario_for(
        "claude_code",
        &stdin_for(&other),
        &mock_claude_stdout(&envelope_json("success")),
        "",
        0,
        0,
        &[],
    );

    let outcome = drive(&harness, &scenario, request.clone(), CancellationToken::new())
        .expect("ran");
    assert_eq!(outcome.exit_code, Some(65));
    let record: serde_json::Value =
        serde_json::from_slice(&outcome.stderr.bytes).expect("one record");
    assert_eq!(record["check"], "stdin");
    // ACM-FR-24: the record carries lengths and a category, never the payload.
    let rendered = String::from_utf8_lossy(&outcome.stderr.bytes);
    assert!(!rendered.contains("the exact instruction"));
    assert!(!rendered.contains("some other instruction"));

    // And the matching envelope passes, so the comparison discriminates.
    let scenario = scenario_for(
        "claude_code",
        &stdin_for(&request),
        &mock_claude_stdout(&envelope_json("success")),
        "",
        0,
        0,
        &[],
    );
    assert_eq!(
        drive(&harness, &scenario, request, CancellationToken::new())
            .expect("ran")
            .process_outcome,
        ProcessOutcome::Completed
    );
}

/// An agent-reported failure and an escalation are valid completed executions
/// across a real process boundary, not executor failures.
#[test]
fn agent_reported_outcomes_survive_the_process_boundary() {
    let harness = harness_for("claude_code");

    for (outcome_name, check) in [
        ("failure", "failure"),
        ("escalation_required", "escalation"),
    ] {
        let request = task("go");
        let scenario = scenario_for(
            "claude_code",
            &stdin_for(&request),
            &mock_claude_stdout(&envelope_json(outcome_name)),
            "",
            0,
            0,
            &[],
        );
        let outcome = drive(&harness, &scenario, request, CancellationToken::new()).expect("ran");

        assert_eq!(
            outcome.process_outcome,
            ProcessOutcome::Completed,
            "{outcome_name} is the agent's report, not a process failure"
        );
        let response = outcome.response.expect("an envelope");
        match check {
            "failure" => assert!(response.failure.is_some()),
            _ => assert!(response.escalation.is_some()),
        }
    }
}

/// A non-zero exit with a real diagnostic on stderr, over real pipes.
#[test]
fn a_non_zero_exit_and_its_stderr_arrive_separately() {
    let harness = harness_for("claude_code");
    let request = task("go");
    let scenario = scenario_for(
        "claude_code",
        &stdin_for(&request),
        "",
        "Invalid API key · Please run /login\n",
        1,
        0,
        &[],
    );

    let outcome = drive(&harness, &scenario, request, CancellationToken::new()).expect("ran");

    assert_eq!(outcome.process_outcome, ProcessOutcome::NonZeroExit);
    assert_eq!(outcome.exit_code, Some(1));
    assert!(outcome.response.is_none());
    assert!(outcome.stdout.bytes.is_empty(), "the streams never merge");
    assert_eq!(
        String::from_utf8_lossy(&outcome.stderr.bytes),
        "Invalid API key · Please run /login\n"
    );
}

/// Malformed, partial, and repeated structured output, each replayed by a real
/// process and each rejected without becoming an agent-reported outcome.
#[test]
fn malformed_partial_and_repeated_output_are_rejected_over_a_real_pipe() {
    let harness = harness_for("claude_code");
    let valid = envelope_json("success");

    let cases = [
        ("prose", "I have finished the task.\n".to_string()),
        ("partial", mock_claude_stdout(&valid[..valid.len() / 2])),
        ("repeated", mock_claude_stdout(&format!("{valid}{valid}"))),
        (
            "wrong version",
            mock_claude_stdout(
                &json!({"protocol_version": 2, "outcome": "success", "summary": "s"}).to_string(),
            ),
        ),
        (
            "exclusivity",
            mock_claude_stdout(
                &json!({"protocol_version": 1, "outcome": "failure", "summary": "s"}).to_string(),
            ),
        ),
    ];

    for (name, stdout) in cases {
        let request = task("go");
        let scenario = scenario_for("claude_code", &stdin_for(&request), &stdout, "", 0, 0, &[]);
        let outcome = drive(&harness, &scenario, request, CancellationToken::new()).expect("ran");

        assert_eq!(
            outcome.process_outcome,
            ProcessOutcome::InvalidStructuredOutput,
            "{name} was accepted"
        );
        assert!(outcome.response.is_none(), "{name}");
        // The bytes are preserved for the caller to read.
        assert!(!outcome.stdout.bytes.is_empty(), "{name}");
    }
}

/// How long the mock stays alive and silent when a test needs a process that
/// hangs. The mock's own ceiling (`MAX_DELAY_MS`, ACM-FR-14), so the hang is as
/// long as a scenario is allowed to make it — anything larger is rejected as
/// `BadDelay` and the process exits at once, which is not a hang at all.
const HANG_MS: u64 = 10_000;

// How "killed, not waited out" is proved here, and why no clock is involved.
//
// It used to be a wall-clock bound: 8s against a 9s delay, one second of
// margin. It failed in CI, and not because the kill was slow — the timed region
// contained a `cargo build`. `mock_binary()` compiles the mock crate on first
// use and is called from inside `drive`, and the `backend` lane caches only
// `src-tauri`, so every run there starts with the mock absent and the first of
// these tests to reach it paid the compile inside its own measurement. A bound
// cannot be trusted as evidence when what it measures is not only the code
// under test, and no amount of widening changes that.
//
// The mock writes its configured response only on the far side of `delay_ms`.
// So an EMPTY stdout says the delay did not run to completion — on any machine,
// at any speed, with any amount of unrelated work in the way. Read together
// with the `process_outcome` assertion beside it (which rules out the mock
// exiting early for some other reason, such as a rejected scenario), that is
// the whole claim, and it is a statement about state rather than about time.
/// A hanging process, ended by the task's own deadline. The mock's `delay_ms`
/// keeps it alive and silent, so this is a real child being killed rather than
/// a fabricated outcome (ACM-FR-21).
#[test]
fn a_hanging_agent_is_ended_by_the_task_deadline() {
    let harness = harness_for("claude_code");
    let mut request = task("go");
    request.execution.timeout_ms = MIN_TIMEOUT_MS;

    let scenario = scenario_for(
        "claude_code",
        &stdin_for(&request),
        &mock_claude_stdout(&envelope_json("success")),
        "",
        0,
        // Far beyond the deadline: the response never gets a chance to appear.
        HANG_MS,
        &[],
    );

    let outcome = drive(&harness, &scenario, request, CancellationToken::new()).expect("ran");

    assert_eq!(outcome.process_outcome, ProcessOutcome::Timeout);
    assert!(outcome.response.is_none());
    // The claim "killed, not waited out", proved without a clock: the mock
    // writes its response only after the delay, so an empty stdout means the
    // delay did not run to completion.
    assert!(
        outcome.stdout.bytes.is_empty(),
        "the configured response is written only after the delay"
    );
    assert!(outcome.container_removed);
}

/// The same hanging process, ended by the caller instead.
#[test]
fn a_hanging_agent_is_ended_by_caller_cancellation() {
    let harness = harness_for("claude_code");
    let request = task("go");
    let scenario = scenario_for(
        "claude_code",
        &stdin_for(&request),
        &mock_claude_stdout(&envelope_json("success")),
        "",
        0,
        HANG_MS,
        &[],
    );

    let token = CancellationToken::new();
    let trigger = token.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        trigger.cancel();
    });

    let outcome = drive(&harness, &scenario, request, token).expect("ran");

    assert_eq!(outcome.process_outcome, ProcessOutcome::Cancelled);
    assert!(outcome.response.is_none());
    // As in the deadline case, the clock-free half of the claim: the mock's
    // response exists only on the far side of the delay.
    assert!(
        outcome.stdout.bytes.is_empty(),
        "the configured response is written only after the delay"
    );
}
