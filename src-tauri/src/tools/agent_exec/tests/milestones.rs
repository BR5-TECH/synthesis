//! EAC-FR-HRTK … EAC-FR-HTSN — the milestone markers an agent writes in its text messages.

use super::*;

#[derive(Default)]
struct KeptActivity(StdMutex<Vec<DurableActivity>>);

impl DurableOutputSink for KeptActivity {
    fn record(&self, activity: DurableActivity) -> Result<(), DurableOutputFailure> {
        self.0.lock().unwrap().push(activity);
        Ok(())
    }
}

impl KeptActivity {
    fn kept(&self) -> Vec<DurableActivity> {
        self.0.lock().unwrap().clone()
    }
}

const PLAN: &str =
    r#"SYNTHESIS_MILESTONE {"event":"plan","milestones":["Read the spec","Write the code"]}"#;
const START_1: &str = r#"SYNTHESIS_MILESTONE {"event":"start","index":1}"#;
const DONE_1: &str = r#"SYNTHESIS_MILESTONE {"event":"done","index":1}"#;

fn observer_for<'a>(
    descriptor: &'static descriptor::VendorExecutionDescriptor,
    sink: &'a Arc<dyn AgentActivitySink>,
    durable: &'a Arc<dyn DurableOutputSink>,
    kind: TurnKind,
) -> LaunchObserver<'a, Silent> {
    LaunchObserver {
        descriptor,
        carry_len: protocol::longest_secret(&[SAMPLE_TOKEN]).saturating_sub(1),
        secrets: vec![SAMPLE_TOKEN.to_string()],
        sink: Some(sink),
        durable: Some(durable),
        cancel: CancellationToken::new(),
        durable_failure: StdMutex::new(None),
        log: Silent,
        logs_activity: true,
        milestones: kind
            .announces_milestones()
            .then(|| StdMutex::new(super::super::milestone::MilestoneState::default())),
        vendor: "test".to_string(),
        container: "synthesis-agent-test".to_string(),
        carry: StdMutex::new((Vec::new(), Vec::new())),
    }
}

fn claude_text(text: &str) -> String {
    json!({"type": "assistant", "message": {"content": [{"type": "text", "text": text}]}})
        .to_string()
}

fn codex_message(text: &str) -> String {
    json!({"type": "item.completed", "item": {"type": "agent_message", "text": text}}).to_string()
}

struct Watch {
    sink: Arc<CollectedActivity>,
    held: Arc<dyn AgentActivitySink>,
    kept: Arc<KeptActivity>,
    durable: Arc<dyn DurableOutputSink>,
}

fn watch() -> Watch {
    let sink = Arc::new(CollectedActivity::default());
    let held: Arc<dyn AgentActivitySink> = sink.clone();
    let kept = Arc::new(KeptActivity::default());
    let durable: Arc<dyn DurableOutputSink> = kept.clone();
    Watch { sink, held, kept, durable }
}

fn feed(observer: &LaunchObserver<'_, Silent>, line: &str) {
    observer.line(StreamChannel::Stdout, line.as_bytes(), true);
}

/// EAC-FR-HRTK, EAC-FR-ZNBD, EAC-FR-RGWY, EAC-FR-HTSN, EAC-FR-VSNM, EAC-FR-DWGS — a valid
/// marker becomes a `milestone` activity for both sinks, in order, and its line leaves the
/// `message` activity while the other lines stay.
#[test]
fn a_marker_becomes_a_milestone_activity_for_both_sinks() {
    for (descriptor, line) in [
        (
            &descriptor::CLAUDE_CODE,
            claude_text(&format!("Starting now.\n{PLAN}\n{START_1}\nDoing the first part.")),
        ),
        (
            &descriptor::CODEX,
            codex_message(&format!("Starting now.\n{PLAN}\n{START_1}\nDoing the first part.")),
        ),
    ] {
        let w = watch();
        let observer = observer_for(descriptor, &w.held, &w.durable, TurnKind::Work);
        feed(&observer, &line);

        let events = w.sink.events();
        let kinds: Vec<&str> = events.iter().map(|e| e.kind).collect();
        assert_eq!(kinds, vec!["milestone", "milestone", "message"], "{}", descriptor.vendor);
        assert_eq!(
            events[0].summary,
            r#"{"event":"plan","milestones":["Read the spec","Write the code"]}"#
        );
        assert_eq!(events[0].payload, events[0].summary);
        assert_eq!(events[1].summary, r#"{"event":"start","index":1}"#);
        assert_eq!(events[0].channel, "stdout");
        assert!(events[2].summary.contains("Starting now."));
        assert!(events[2].summary.contains("Doing the first part."));
        assert!(!events[2].summary.contains("SYNTHESIS_MILESTONE"));

        let kept = w.kept.kept();
        let kept_kinds: Vec<&str> = kept.iter().map(|a| a.kind).collect();
        assert_eq!(kept_kinds, vec!["milestone", "milestone", "message"]);
        assert_eq!(kept[0].summary, events[0].summary);
        assert_ne!(kept[0].record_id, kept[1].record_id);
    }
}

/// EAC-FR-HTSN — a message that holds only markers leaves no `message` activity, and a text
/// block beside a tool call keeps its tool call.
#[test]
fn a_message_of_markers_alone_leaves_no_message_activity() {
    let w = watch();
    let observer = observer_for(&descriptor::CLAUDE_CODE, &w.held, &w.durable, TurnKind::Review);
    feed(&observer, &claude_text(PLAN));
    feed(&observer, &claude_text(DONE_1));
    assert_eq!(w.sink.kinds(), vec!["milestone", "milestone"]);

    let with_tool = json!({"type": "assistant", "message": {"content": [
        {"type": "text", "text": START_1},
        {"type": "tool_use", "name": "Read", "input": {"file_path": "a.md"}},
    ]}})
    .to_string();
    let w = watch();
    let observer = observer_for(&descriptor::CLAUDE_CODE, &w.held, &w.durable, TurnKind::MergeWork);
    feed(&observer, &claude_text(PLAN));
    feed(&observer, &with_tool);
    let events = w.sink.events();
    assert_eq!(events.last().unwrap().kind, "tool_call");
    assert!(events.last().unwrap().summary.contains("Read(a.md)"));
    assert_eq!(events.iter().filter(|e| e.kind == "milestone").count(), 2);
}

/// EAC-FR-PQLW, EAC-FR-XDGT, EAC-FR-VCBJ, EAC-FR-QLMD, EAC-FR-BQNV, EAC-FR-WMKB — a malformed
/// marker, a second plan, an index outside the plan, and a repeated done marker make no
/// milestone activity and stay in the message.
#[test]
fn a_bad_marker_is_ignored_and_stays_in_the_message() {
    let w = watch();
    let observer =
        observer_for(&descriptor::CLAUDE_CODE, &w.held, &w.durable, TurnKind::MergeReview);
    let early = r#"SYNTHESIS_MILESTONE {"event":"start","index":1}"#;
    let malformed = r#"SYNTHESIS_MILESTONE {"event":"plan","milestones":[]}"#;
    feed(&observer, &claude_text(&format!("{early}\n{malformed}")));
    assert_eq!(w.sink.kinds(), vec!["message"], "before a plan, and an empty plan");

    feed(&observer, &claude_text(PLAN));
    let second = r#"SYNTHESIS_MILESTONE {"event":"plan","milestones":["Other"]}"#;
    let outside = r#"SYNTHESIS_MILESTONE {"event":"done","index":9}"#;
    feed(&observer, &claude_text(&format!("{second}\n{outside}")));
    feed(&observer, &claude_text(DONE_1));
    feed(&observer, &claude_text(&format!("Again.\n{DONE_1}")));

    assert_eq!(w.sink.kinds(), vec!["message", "milestone", "message", "milestone", "message"]);
    let events = w.sink.events();
    assert!(events[2].summary.contains("Other") && events[2].summary.contains("\"index\":9"));
    assert!(events[4].summary.contains("Again."));
}

/// EAC-FR-HRTK — a `semantic_rebase` turn and a `pr_maintenance` turn are not scanned.
#[test]
fn a_turn_that_announces_nothing_is_not_scanned() {
    for kind in [TurnKind::SemanticRebase, TurnKind::PrMaintenance] {
        let w = watch();
        let observer = observer_for(&descriptor::CODEX, &w.held, &w.durable, kind);
        feed(&observer, &codex_message(PLAN));
        assert_eq!(w.sink.kinds(), vec!["message"], "{kind:?}");
        assert!(w.sink.events()[0].summary.contains("SYNTHESIS_MILESTONE"));
    }
    for kind in [TurnKind::Work, TurnKind::Review, TurnKind::MergeWork, TurnKind::MergeReview] {
        assert!(kind.announces_milestones(), "{kind:?}");
    }
}

/// EAC-FR-ZNBD, EAC-FR-FKCN, EAC-FR-29 — a credential in a marker line, and in a line beside it,
/// reaches neither sink.
#[test]
fn a_credential_in_a_marker_line_is_masked_for_both_sinks() {
    let w = watch();
    let observer = observer_for(&descriptor::CLAUDE_CODE, &w.held, &w.durable, TurnKind::Work);
    let plan =
        format!(r#"SYNTHESIS_MILESTONE {{"event":"plan","milestones":["{SAMPLE_TOKEN}","Next"]}}"#);
    feed(&observer, &claude_text(&format!("The key is {SAMPLE_TOKEN}\n{plan}")));

    let rendered = w.sink.rendered();
    assert!(!rendered.contains(SAMPLE_TOKEN), "a credential reached the activity sink");
    let kept: String = w.kept.kept().iter().map(|a| a.summary.clone()).collect();
    assert!(!kept.contains(SAMPLE_TOKEN), "a credential reached the durable sink");
    assert!(rendered.contains(&masked_form(SAMPLE_TOKEN)));
}

/// EAC-FR-DWGS — `milestone` is a safe kind.
#[test]
fn milestone_is_a_safe_kind() {
    assert!(is_safe_kind("milestone"));
    assert!(!is_safe_kind("reasoning"));
}

/// EAC-FR-HRTK, EAC-FR-IRRD, EAC-FR-ZNBD — through a real launch, the turn kind decides whether a
/// marker in the agent's text becomes a `milestone` activity.
#[test]
fn a_launch_scans_the_four_kinds_of_turn_and_no_other() {
    let stdout = format!(
        "{}\n{}\n{}\n",
        json!({"type": "system", "subtype": "init", "session_id": SESSION_PLACEHOLDER,
               "model": "claude-sonnet-5", "tools": []}),
        json!({"type": "assistant", "session_id": SESSION_PLACEHOLDER,
               "message": {"role": "assistant", "content": [{"type": "text", "text": PLAN}]}}),
        claude_result_line(&envelope_json("success")),
    );
    for (kind, scanned) in [
        (TurnKind::Work, true),
        (TurnKind::Review, true),
        (TurnKind::MergeWork, true),
        (TurnKind::MergeReview, true),
        (TurnKind::SemanticRebase, false),
        (TurnKind::PrMaintenance, false),
    ] {
        let harness = harness_for("claude_code");
        let runtime = RecordingRuntime::replying(&stdout);
        let sink = Arc::new(CollectedActivity::default());
        let held: Arc<dyn AgentActivitySink> = sink.clone();
        let mount = (kind == TurnKind::SemanticRebase).then(|| {
            // The rebase kind carries its artifact mount, as every real launch of it does.
            SupplementaryMount::SemanticRebaseArtifact {
                host_path: rebase_bundle(&harness.sessions_root(), "milestone-bundle"),
            }
        });
        let outcome = run_as(&harness, runtime, mount, Some(held), kind);
        outcome.expect("the launch runs");
        let milestones = sink.kinds().iter().filter(|k| *k == "milestone").count();
        assert_eq!(milestones, usize::from(scanned), "{kind:?}");
    }
}

/// One launch of `kind` with `instruction`, and the argument vector it ran.
fn argv_of(vendor: &str, kind: TurnKind, instruction: &str) -> Vec<String> {
    let harness = harness_for(vendor);
    let stdout = match vendor {
        "codex" => codex_stdout(&envelope_json("success")),
        _ => valid_claude_stdout(),
    };
    let runtime = RecordingRuntime::replying(&stdout);
    let mount =
        (kind == TurnKind::SemanticRebase).then(|| SupplementaryMount::SemanticRebaseArtifact {
            host_path: rebase_bundle(&harness.sessions_root(), "reminder-bundle"),
        });
    let executor =
        AgentCliExecutor::new(runtime.clone()).with_session_state_root(harness.sessions_root());
    block_on(executor.execute_agent_cli(
        &Silent,
        &harness.context(),
        AgentExecutionRequest {
            sandbox_scope: None,
            turn_kind: kind,
            execution_directory: harness.workspace(),
            task: task(instruction),
            cancellation: CancellationToken::new(),
            activity: None,
            durable_output: None,
            supplementary_mount: mount,
        },
    ))
    .expect("the launch runs");
    runtime.only_run().argv
}

const WITH_BLOCK: &str =
    "Do the work.\n\n## Milestones\n\nSYNTHESIS_MILESTONE {\"event\":\"start\",\"index\":1}\n";

/// EAC-FR-WBIF, CCP-FR-WZJS — a Claude Code launch of a scanned kind whose instruction holds the
/// block ends its vector with `--settings` and the reminder hook, and no other launch carries it.
#[test]
fn only_a_scanned_turn_with_the_block_carries_the_reminder() {
    let reminder = descriptor::claude_code::milestone_reminder_args();
    for kind in [TurnKind::Work, TurnKind::Review, TurnKind::MergeWork, TurnKind::MergeReview] {
        let argv = argv_of("claude_code", kind, WITH_BLOCK);
        assert_eq!(argv[argv.len() - 2..], reminder[..], "{kind:?}");
        assert_eq!(argv.iter().filter(|a| *a == "--settings").count(), 1, "{kind:?}");
    }
    // GRL-FR-LCSB: a correction turn's instruction has no block, which
    // `graduation::tests::milestone_prompts::a_correction_turn_carries_no_milestone_block`
    // shows for the real instruction.
    let argv = argv_of("claude_code", TurnKind::Review, "Correct the verdict.");
    assert!(!argv.iter().any(|a| a == "--settings"), "{argv:?}");
    for kind in [TurnKind::SemanticRebase, TurnKind::PrMaintenance] {
        let argv = argv_of("claude_code", kind, WITH_BLOCK);
        assert!(!argv.iter().any(|a| a == "--settings"), "{kind:?}");
    }
    // A vendor with no reminder mechanism gets none.
    let argv = argv_of("codex", TurnKind::Work, WITH_BLOCK);
    assert!(!argv.iter().any(|a| a == "--settings" || a.contains("PostToolUse")), "{argv:?}");
}

/// CCP-FR-WZJS — the settings object holds one `PostToolUse` hook for every tool, and its command
/// prints the one fixed `additionalContext` object, which holds the reminder and no task input.
#[test]
fn the_reminder_hook_prints_the_fixed_context() {
    let args = descriptor::claude_code::milestone_reminder_args();
    assert_eq!(args[0], "--settings");
    let settings: serde_json::Value = serde_json::from_str(&args[1]).expect("settings are JSON");
    let hooks = settings["hooks"]["PostToolUse"].as_array().expect("one PostToolUse list");
    assert_eq!(hooks.len(), 1);
    assert_eq!(hooks[0]["matcher"], "*");
    let inner = hooks[0]["hooks"].as_array().expect("hook list");
    assert_eq!(inner.len(), 1);
    assert_eq!(inner[0]["type"], "command");
    let command = inner[0]["command"].as_str().expect("a command");

    // The command is `printf '%s\n' '<object>'`: one single-quoted word, so the
    // shell prints the object as it stands.
    let printed = command
        .strip_prefix("printf '%s\\n' '")
        .and_then(|rest| rest.strip_suffix('\''))
        .expect("printf of one quoted word");
    assert!(!printed.contains('\''), "the object holds no single quote");
    let output: serde_json::Value = serde_json::from_str(printed).expect("the output is JSON");
    assert_eq!(output["hookSpecificOutput"]["hookEventName"], "PostToolUse");
    assert_eq!(
        output["hookSpecificOutput"]["additionalContext"],
        descriptor::claude_code::MILESTONE_REMINDER
    );
    // The text of the contract surface of `CCP-claude-code-cli-protocol.md`.
    assert_eq!(
        output,
        json!({"hookSpecificOutput": {"hookEventName": "PostToolUse", "additionalContext":
            "Milestone reminder: if you are now in a different milestone, write its start marker \
             in your next text message. A start marker marks every earlier milestone done."}})
    );
    assert_eq!(
        settings,
        json!({"hooks": {"PostToolUse": [{"matcher": "*", "hooks": [
            {"type": "command", "command": command}
        ]}]}})
    );
}

/// CCP-FR-WZJS — a shell runs the hook command, and it prints the one object as one line and
/// does nothing else.
#[cfg(unix)]
#[test]
fn the_reminder_hook_command_prints_one_line_in_a_shell() {
    let args = descriptor::claude_code::milestone_reminder_args();
    let settings: serde_json::Value = serde_json::from_str(&args[1]).expect("settings are JSON");
    let command = settings["hooks"]["PostToolUse"][0]["hooks"][0]["command"]
        .as_str()
        .expect("a command")
        .to_string();
    let ran = std::process::Command::new("sh").arg("-c").arg(&command).output().expect("sh runs");
    assert!(ran.status.success(), "{:?}", ran.status);
    assert!(ran.stderr.is_empty(), "{}", String::from_utf8_lossy(&ran.stderr));
    let stdout = String::from_utf8(ran.stdout).expect("UTF-8");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "{stdout:?}");
    let output: serde_json::Value = serde_json::from_str(lines[0]).expect("one JSON object");
    assert_eq!(
        output["hookSpecificOutput"]["additionalContext"],
        descriptor::claude_code::MILESTONE_REMINDER
    );
}

/// CCP-FR-WZJS, EAC-FR-WBIF — a resumed turn carries the reminder at the end of its vector too,
/// once, and the reminder holds nothing of the task.
#[test]
fn a_resumed_turn_carries_the_reminder_and_no_task_input() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    run(&harness, runtime.clone(), task(WITH_BLOCK)).expect("the fresh turn runs");
    let assigned = assigned_session_id(&runtime.only_run().argv).expect("--session-id");

    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let instruction = format!("{WITH_BLOCK}SYNTHESIS_MILESTONE instruction-canary\n");
    let mut resumed = task(&instruction);
    resumed.input = Some(
        [("draft".to_string(), json!("SYNTHESIS_MILESTONE input-canary"))].into_iter().collect(),
    );
    resumed.resume = Some(SessionRef { session_id: Some(assigned), continuation_token: None });
    run(&harness, runtime.clone(), resumed).expect("the resumed turn runs");
    let argv = runtime.only_run().argv;

    let reminder = descriptor::claude_code::milestone_reminder_args();
    assert!(argv.iter().any(|a| a == "--resume"), "{argv:?}");
    assert_eq!(argv[argv.len() - 2..], reminder[..]);
    assert_eq!(argv.iter().filter(|a| *a == "--settings").count(), 1);
    for canary in ["instruction-canary", "input-canary"] {
        assert!(!argv.iter().any(|a| a.contains(canary)), "{canary} reached the argv");
    }
}
