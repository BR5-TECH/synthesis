//! EAC-FR-VSNM … EAC-FR-CPEP — a durable sink keeps a run, and keeps only what
//! is safe to keep.

use super::*;

/// A durable sink that keeps what it is given, and that can be told to fail at
/// its first answer.
#[derive(Default)]
struct Kept {
    kept: StdMutex<Vec<DurableActivity>>,
    calls: StdMutex<usize>,
    fail_first: bool,
}

impl Kept {
    fn failing() -> Arc<Self> {
        Arc::new(Self {
            fail_first: true,
            ..Self::default()
        })
    }

    fn kept(&self) -> Vec<DurableActivity> {
        self.kept.lock().unwrap().clone()
    }

    fn calls(&self) -> usize {
        *self.calls.lock().unwrap()
    }
}

impl DurableOutputSink for Kept {
    fn record(&self, activity: DurableActivity) -> Result<(), DurableOutputFailure> {
        *self.calls.lock().unwrap() += 1;
        if self.fail_first {
            return Err(DurableOutputFailure {
                code: "log_append_failed".to_string(),
                message: "The record could not be stored.".to_string(),
            });
        }
        self.kept.lock().unwrap().push(activity);
        Ok(())
    }
}

fn run_keeping(
    harness: &Harness,
    runtime: Arc<RecordingRuntime>,
    task: AgentTaskRequest,
    durable: Arc<Kept>,
    cancellation: CancellationToken,
) -> Result<AgentExecution, AgentExecutionError> {
    let executor = AgentCliExecutor::new(runtime).with_session_state_root(harness.sessions_root());
    block_on(executor.execute_agent_cli(
        &Silent,
        &harness.context(),
        AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
            execution_directory: harness.workspace(),
            task,
            cancellation,
            activity: None,
            durable_output: Some(durable),
            supplementary_mount: None,
        },
    ))
}

/// EAC-FR-VSNM, EAC-FR-DWGS, EAC-FR-IRKZ, EAC-FR-CXUE — a durable sink receives
/// the safe kinds alone, in order, and never the invocation or the task.
#[test]
fn a_durable_sink_receives_the_safe_kinds_and_never_the_invocation_or_the_task() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let durable = Arc::new(Kept::default());

    const INSTRUCTION: &str = "MARKER-DURABLE-4c1e";
    let outcome = run_keeping(
        &harness,
        runtime.clone(),
        task(INSTRUCTION),
        durable.clone(),
        CancellationToken::new(),
    )
    .expect("runs");
    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);

    let kept = durable.kept();
    let kinds: Vec<&str> = kept.iter().map(|a| a.kind).collect();
    assert_eq!(kinds, ["started", "message", "finished", "finished"]);
    assert!(kept.iter().all(|a| is_safe_kind(a.kind)));
    let last = kept.last().expect("a closing activity");
    assert_eq!(last.channel, "executor", "the executor's own line arrives last");

    let rendered = kept
        .iter()
        .map(|a| format!("{} {}", a.channel, a.summary))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!rendered.contains(INSTRUCTION), "the task never reaches the sink");
    assert!(!rendered.contains("--interactive"), "the invocation never reaches the sink");
    let mut ids: Vec<&str> = kept.iter().map(|a| a.record_id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), kept.len(), "every activity has its own record id");
}

/// EAC-FR-IRKZ — the excluded kinds never reach a durable sink, while the
/// activity sink of EAC-FR-32 still receives them.
#[test]
fn the_excluded_kinds_stay_out_of_the_durable_sink_and_in_the_live_one() {
    let harness = harness_for("claude_code");
    let reasoning = format!(
        "{}\n{}\n{}\n",
        json!({ "type": "assistant", "message": { "content": [
            { "type": "thinking", "thinking": "PRIVATE-REASONING-77" }
        ] } }),
        "this line is not json at all",
        claude_result_line(&envelope_json("success")),
    );
    let runtime = RecordingRuntime::replying(&reasoning);
    let live = Arc::new(CollectedActivity::default());
    let durable = Arc::new(Kept::default());
    let executor = AgentCliExecutor::new(runtime).with_session_state_root(harness.sessions_root());
    block_on(executor.execute_agent_cli(
        &Silent,
        &harness.context(),
        AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
            execution_directory: harness.workspace(),
            task: task("go"),
            cancellation: CancellationToken::new(),
            activity: Some(live.clone()),
            durable_output: Some(durable.clone()),
            supplementary_mount: None,
        },
    ))
    .expect("runs");

    let live_kinds = live.kinds();
    for excluded in ["invocation", "task", "unrecognized"] {
        assert!(live_kinds.iter().any(|k| k == excluded), "the live sink still gets {excluded}");
    }
    for activity in durable.kept() {
        assert!(
            !["invocation", "task", "reasoning", "unrecognized"].contains(&activity.kind),
            "an excluded kind reached the durable sink: {}",
            activity.kind
        );
        assert!(!activity.summary.contains("PRIVATE-REASONING-77"));
        assert!(!activity.summary.contains("not json at all"));
    }
}

/// EAC-FR-FKCN, EAC-FR-29 — everything a durable sink receives is masked first.
#[test]
fn a_durable_sink_is_handed_nothing_a_record_may_not_carry() {
    let harness = harness_for("claude_code");
    let leaky = format!(
        "{}\n{}\n",
        json!({
            "type": "system", "subtype": "init", "model": "m", "tools": [],
            "session_id": SESSION_PLACEHOLDER,
            "env": { "CLAUDE_CODE_OAUTH_TOKEN": SAMPLE_TOKEN }
        }),
        claude_result_line(&envelope_json("success")),
    );
    let runtime = RecordingRuntime::replying(&leaky);
    let durable = Arc::new(Kept::default());

    run_keeping(&harness, runtime.clone(), task("go"), durable.clone(), CancellationToken::new())
        .expect("runs");

    let rendered = durable
        .kept()
        .iter()
        .map(|a| format!("{} {}", a.channel, a.summary))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!rendered.contains(SAMPLE_TOKEN));
    assert!(!rendered.contains(&SAMPLE_TOKEN[..20]));
    let assigned = assigned_session_id(&runtime.only_run().argv).expect("--session-id");
    assert!(!rendered.contains(&assigned));
}

/// EAC-FR-DUTR, EAC-FR-ZVRP, EAC-FR-CPEP — a sink that answers a typed failure
/// ends delivery, the call returns `DurableOutputFailed` after the container
/// removal, and the caller's own token is left as the caller set it.
#[test]
fn a_durable_failure_ends_delivery_and_is_returned_after_the_container_is_removed() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let durable = Kept::failing();
    let caller = CancellationToken::new();

    let error = run_keeping(
        &harness,
        runtime.clone(),
        task("go"),
        durable.clone(),
        caller.clone(),
    )
    .expect_err("the sink failed");

    match error {
        AgentExecutionError::DurableOutputFailed(failure) => {
            assert_eq!(failure.code, "log_append_failed");
        }
        other => panic!("expected DurableOutputFailed, got {other:?}"),
    }
    assert_eq!(durable.calls(), 1, "delivery ended at the first failure");
    assert!(!caller.is_cancelled(), "the caller's own token is never set");
    assert_eq!(
        runtime.removed.lock().unwrap().len(),
        1,
        "the container was removed before the failure was returned"
    );
}

/// EAC-FR-CPEP — a token linked to the caller's follows it and never sets it.
#[test]
fn a_linked_token_follows_its_parent_and_never_sets_it() {
    let parent = CancellationToken::new();
    let linked = parent.linked();
    linked.cancel();
    assert!(linked.is_cancelled());
    assert!(!parent.is_cancelled());

    let other = CancellationToken::new();
    let follower = other.linked();
    assert!(!follower.is_cancelled());
    other.cancel();
    assert!(follower.is_cancelled());
}

/// EAC-FR-RLIW — a call with no durable sink is the call it would have been.
#[test]
fn a_call_with_no_durable_sink_is_unchanged() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let outcome = run(&harness, runtime, task("go")).expect("runs");
    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);
}
