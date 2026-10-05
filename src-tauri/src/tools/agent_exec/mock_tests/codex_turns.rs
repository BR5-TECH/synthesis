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
    assert_eq!(
        outcome.session.and_then(|s| s.session_id).as_deref(),
        Some(CODEX_THREAD_ID)
    );
}

/// EAC-FR-12, EAC-FR-30 / CDX-FR-09, CDX-FR-10 — a resumed Codex turn is its own grammar over a real
/// process boundary, not the fresh vector with `resume` inserted.
#[test]
fn a_resumed_codex_turn_uses_its_own_grammar() {
    let harness = harness_for("codex");
    let mut request = task("continue");
    request.resume = Some(SessionRef {
        session_id: Some("sess-earlier".into()),
        continuation_token: None,
    });

    // The resumed vector drops `--sandbox` — which `exec resume` does not accept
    // — and carries the policy as a config override instead, so the scenario
    // demands the override and the fresh flags are replaced rather than added to.
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
                    { "value": "--skip-git-repo-check" },
                    { "value": "-c" },
                    { "value": "sandbox_mode=\"danger-full-access\"" },
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
        (
            "no agent message",
            format!("{}\n", json!({"type":"thread.started","thread_id":"t"})),
        ),
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
