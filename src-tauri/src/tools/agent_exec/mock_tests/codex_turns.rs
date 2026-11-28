//! Codex driven end to end through the same boundary, with its own grammar.

use super::*;

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

/// The same normalized result shape, from a vendor with a different headless
/// contract, a different output protocol, and a different credential form.
#[test]
fn codex_runs_end_to_end_against_the_mock() {
    let harness = harness_for("codex");
    let request = task("run the suite");
    let scenario = scenario_for(
        "codex",
        &stdin_for(&request),
        &codex_stdout(&envelope_json("success")),
        "",
        0,
        0,
        &[],
    );

    let outcome = drive(&harness, &scenario, request, CancellationToken::new()).expect("ran");

    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);
    let response = outcome.response.expect("an envelope");
    assert_eq!(response.outcome, AgentOutcome::Success);
    // EAC-FR-21: the thread the CLI itself reported on `thread.started` is what
    // the caller carries forward, not the `sess-1` the agent wrote into its
    // envelope — resuming on the latter would continue nothing.
    assert_eq!(outcome.session.and_then(|s| s.session_id).as_deref(), Some(CODEX_THREAD_ID));
}

/// EAC-FR-12, EAC-FR-30 / CDX-FR-09, CDX-FR-10 — a resumed Codex turn is its own grammar over a real
/// process boundary, not the fresh vector with `resume` inserted.
#[test]
fn a_resumed_codex_turn_uses_its_own_grammar() {
    let harness = harness_for("codex");
    let mut request = task("continue");
    request.resume =
        Some(SessionRef { session_id: Some("sess-earlier".into()), continuation_token: None });

    // The resumed vector carries the same bypass flag as a fresh one, and no
    // `--sandbox`, which `exec resume` does not accept (CDX-FR-04, CDX-FR-11).
    let scenario = Scenario::new(
        stack_tool("codex"),
        json!({
            "version": 1,
            "expected": {
                "required_args": [
                    { "value": "run" },
                    { "value": "--rm" },
                    { "value": "--interactive" },
                    { "value": "exec" },
                    { "value": "resume" },
                    { "value": "--json" },
                    { "value": "--dangerously-bypass-approvals-and-sandbox" },
                    { "value": "--skip-git-repo-check" },
                    { "value": "sess-earlier" },
                    { "value": "-" },
                ],
                "stdin": stdin_for(&request),
            },
            "response": {
                "stdout": codex_stdout(&envelope_json("success")),
                "stderr": "",
                "exit_code": 0,
                "delay_ms": 0,
            }
        }),
    );

    let outcome = drive(&harness, &scenario, request, CancellationToken::new()).expect("ran");
    // Reaching `Completed` means every token the scenario demanded was present
    // in the vector production emitted; a vector it did not expect exits 65 and
    // suppresses the configured response entirely.
    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);
}

/// A malformed JSONL stream — the shape only Codex can produce — rejected over
/// a real pipe.
#[test]
fn a_codex_stream_that_is_not_events_is_rejected() {
    let harness = harness_for("codex");

    for (name, stdout) in [
        // An agent message whose text is prose rather than an envelope. The CLI
        // does not validate the envelope's shape (CDX-FR-14), so this is the
        // failure mode this vendor actually produces.
        (
            "an agent message that is not an envelope",
            format!(
                "{}\n{}\n",
                json!({"type":"thread.started","thread_id":"t"}),
                json!({"type":"item.completed","item":{"id":"i1","type":"agent_message",
                       "text":"I finished the task."}}),
            ),
        ),
        ("no agent message", format!("{}\n", json!({"type":"thread.started","thread_id":"t"}))),
        // A turn the CLI itself reports as failed yields no envelope even though
        // one is present earlier in the stream (CDX-FR-16).
        (
            "a failed turn",
            format!(
                "{}\n{}\n{}\n",
                json!({"type":"thread.started","thread_id":"t"}),
                json!({"type":"item.completed","item":{"id":"i1","type":"agent_message",
                       "text": envelope_json("success")}}),
                json!({"type":"turn.failed","error":{"message":"model unavailable"}}),
            ),
        ),
        ("empty stream", String::new()),
    ] {
        let request = task("go");
        let scenario = scenario_for("codex", &stdin_for(&request), &stdout, "", 0, 0, &[]);
        let outcome = drive(&harness, &scenario, request, CancellationToken::new()).expect("ran");
        assert_eq!(
            outcome.process_outcome,
            ProcessOutcome::InvalidStructuredOutput,
            "{name} was accepted"
        );
    }
}

/// CDX-FR-19 — a non-zero exit is a failure whatever the stream holds: an exit 2 for an unknown
/// argument and an exit 1 after a valid envelope both read as a non-zero exit, never as an answer.
#[test]
fn a_codex_process_that_exits_non_zero_is_a_failure() {
    let harness = harness_for("codex");
    for (code, stdout) in [(2, String::new()), (1, codex_stdout(&envelope_json("success")))] {
        let request = task("go");
        let scenario = scenario_for(
            "codex",
            &stdin_for(&request),
            &stdout,
            "error: unexpected argument",
            code,
            0,
            &[],
        );
        let outcome = drive(&harness, &scenario, request, CancellationToken::new()).expect("ran");
        assert_eq!(outcome.process_outcome, ProcessOutcome::NonZeroExit, "exit {code}");
        assert!(outcome.response.is_none(), "exit {code} gave an answer");
    }
}

/// EAC-FR-QHIU, EAC-FR-15, CDX-FR-EUBZ — a Codex turn in the `access_token` mode, over a real
/// process boundary: the token by name, `CODEX_HOME` at the dedicated directory, and the
/// recorded session naming the mode.
#[test]
fn a_codex_access_token_turn_runs_against_the_mock() {
    let harness = harness_verified_with(
        "codex",
        Some(VerifyConfig {
            path: Some("/usr/bin/codex".into()),
            auth_mode: Some("access_token".into()),
            access_token: Some("codex-access-FAKE-TEST-TOKEN-0002".into()),
            ..Default::default()
        }),
    );
    let request = task("run the suite");
    let scenario = scenario_for(
        "codex",
        &stdin_for(&request),
        &codex_stdout(&envelope_json("success")),
        "",
        0,
        0,
        &["--env", "CODEX_ACCESS_TOKEN", "--env", "CODEX_HOME=/home/agent/.codex"],
    );

    let outcome = drive(&harness, &scenario, request, CancellationToken::new()).expect("ran");

    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);
    let session = outcome.session.expect("a session");
    assert_eq!(session.session_id.as_deref(), Some(CODEX_THREAD_ID));
    assert_eq!(session.continuation_token.as_deref(), Some("access_token"));
}

/// CDX-FR-14, EAC-FR-BOXF, CDX-FR-TPFM — a contract turn over a real process boundary carries
/// the schema file, and an answer that leaves optional fields out with `null` is accepted.
#[test]
fn a_codex_contract_turn_carries_its_schema_file_against_the_mock() {
    let harness = harness_for("codex");
    let mut request = task("summarize the change");
    request.result_contract = Some(ResultContract::PullRequestSummary);
    let answer = json!({
        "protocol_version": 1, "outcome": "success", "summary": "summarized",
        "result": { "title": "Add a thing", "description": "It adds a thing." },
        "failure": null, "escalation": null, "session": null, "metadata": null
    })
    .to_string();
    let scenario = scenario_for(
        "codex",
        &stdin_for(&request),
        &codex_stdout(&answer),
        "",
        0,
        0,
        &["--output-schema", "/opt/synthesis/vendor/output-schema-pull_request_summary.json"],
    );

    let outcome = drive(&harness, &scenario, request, CancellationToken::new()).expect("ran");

    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);
    let response = outcome.response.expect("an envelope");
    assert_eq!(response.result.expect("a result")["title"], "Add a thing");
}
