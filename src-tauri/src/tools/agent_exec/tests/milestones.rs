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
