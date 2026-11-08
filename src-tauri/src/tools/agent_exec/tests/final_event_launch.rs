//! EAC-FR-HZIE, EAC-FR-QHAY, EAC-FR-PPXB, EAC-FR-THUT — what a launch makes of
//! a run that the runtime ended after its final event, and which rule each
//! vendor's launch hands the runtime.

use super::*;
use crate::logging::{Domain, LogLevel, LogRecord};

const MARKER: &str = "final-event-marker-7Q2X";
const STOPPED: &str = "The agent process still ran 30 s after its answer, so it was stopped.";

fn lingering(stdout: &str, remove_fails: bool) -> Arc<RecordingRuntime> {
    Arc::new(RecordingRuntime {
        script: Script::AnswerThenLinger { stdout: stdout.to_string() },
        available: true,
        image_available: true,
        launch_error: None,
        remove_fails,
        runs: StdMutex::new(Vec::new()),
        removed: StdMutex::new(Vec::new()),
        images_asked: StdMutex::new(Vec::new()),
        socket_group: StdMutex::new(None),
        socket_probes: StdMutex::new(Vec::new()),
        socket_cancels_seen: StdMutex::new(Vec::new()),
    })
}

/// The records of one launch. The log buffer is shared by the whole process,
/// so a record belongs to this launch only through its container name.
fn records_of(container: &str) -> Vec<LogRecord> {
    let page = crate::logging::LogBuffer::query(
        super::super::log_buffer(),
        &crate::logging::LogFilter::default(),
        None,
        20_000,
    )
    .expect("query");
    page.records
        .into_iter()
        .filter(|record| record.fields.get("container").and_then(|v| v.as_str()) == Some(container))
        .collect()
}

fn stopped_warnings(records: &[LogRecord]) -> Vec<&LogRecord> {
    records
        .iter()
        .filter(|record| {
            record.level == LogLevel::Warn
                && record.message == "agent process still ran after its final event and was stopped"
        })
        .collect()
}

fn finished_summary(sink: &CollectedActivity) -> String {
    let events = sink.events();
    let last = events.last().expect("an activity");
    assert_eq!(last.kind, "finished");
    assert_eq!(last.summary, last.payload);
    last.summary.clone()
}

// EAC-FR-HZIE, EAC-FR-26: a run stopped after its final event is read as a
// clean exit. Its answer is the answer the same stream gives on a normal exit,
// its exit code is null, and its container is removed.
#[test]
fn a_run_stopped_after_its_final_event_completes_with_its_answer() {
    let harness = harness_for("claude_code");
    let normal =
        run(&harness, RecordingRuntime::replying(&valid_claude_stdout()), task("go")).expect("ran");

    let runtime = lingering(&valid_claude_stdout(), false);
    let execution = run(&harness, runtime.clone(), task("go")).expect("ran");
    assert_eq!(execution.process_outcome, ProcessOutcome::Completed);
    assert_eq!(execution.exit_code, None);
    assert!(execution.refusal.is_none());
    assert_eq!(execution.response, normal.response);
    assert!(execution.session.as_ref().is_some_and(|s| s.session_id.is_some()));
    assert!(execution.container_removed);
    let container = container_name(&runtime.only_run().argv);
    assert_eq!(*runtime.removed.lock().unwrap(), vec![container]);
}

// EAC-FR-PPXB, EAC-FR-29: the stop emits one WARN record that names the vendor,
// the container, and the grace, and carries no task material and no agent
// output. The end record says `completed` with no exit code.
#[test]
fn the_stop_emits_one_warning_with_no_task_material_or_output() {
    let harness = harness_for("claude_code");
    let envelope =
        json!({ "protocol_version": 1, "outcome": "success", "summary": MARKER }).to_string();
    let runtime = lingering(&claude_stdout(&envelope), false);
    let execution = run(&harness, runtime.clone(), task(MARKER)).expect("ran");
    assert_eq!(execution.process_outcome, ProcessOutcome::Completed);

    let container = container_name(&runtime.only_run().argv);
    let records = records_of(&container);
    let warnings = stopped_warnings(&records);
    assert_eq!(warnings.len(), 1, "{records:?}");
    let every_warning = records.iter().filter(|record| record.level == LogLevel::Warn).count();
    assert_eq!(every_warning, 1, "the stop emitted more than one WARN record: {records:?}");
    let warning = warnings[0];
    assert!(warning.domains.contains(&Domain::Ai) && warning.domains.contains(&Domain::Backend));
    assert_eq!(warning.fields.get("vendor").and_then(|v| v.as_str()), Some("claude_code"));
    assert_eq!(warning.fields.get("grace_ms").and_then(|v| v.as_u64()), Some(30_000));
    // The DEBUG records of EAC-FR-32 and EAC-FR-34 carry the task and the
    // stream by design. The boundary records do not.
    for record in records.iter().filter(|record| record.level != LogLevel::Debug) {
        let rendered =
            format!("{} {}", record.message, serde_json::to_string(&record.fields).unwrap());
        assert!(!rendered.contains(MARKER), "a record carried task or output: {rendered}");
    }

    let finished = records
        .iter()
        .find(|record| record.message == "agent execution finished")
        .expect("the end record");
    assert_eq!(finished.fields.get("outcome").and_then(|v| v.as_str()), Some("completed"));
    assert!(finished.fields.get("exit_code").is_none_or(serde_json::Value::is_null));
}

// EAC-FR-THUT, EAC-FR-HZIE: the finished activity of a stopped run says that
// the run finished, that it was stopped after its answer, and names the grace.
#[test]
fn the_finished_line_of_a_stopped_run_names_the_stop_and_the_grace() {
    let harness = harness_for("claude_code");
    let sink = Arc::new(CollectedActivity::default());
    run_watching(&harness, lingering(&valid_claude_stdout(), false), task("go"), sink.clone())
        .expect("ran");
    let summary = finished_summary(&sink);
    assert!(summary.starts_with("Turn finished after "), "{summary}");
    assert!(summary.ends_with(STOPPED), "{summary}");
}

// EAC-FR-QHAY, EAC-FR-PPXB, EAC-FR-THUT: a run that exits on its own gets no
// warning, and its finished line says nothing of a stop.
#[test]
fn a_normal_exit_gets_no_warning_and_no_stop_in_its_line() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let sink = Arc::new(CollectedActivity::default());
    let execution = run_watching(&harness, runtime.clone(), task("go"), sink.clone()).expect("ran");
    assert_eq!(execution.exit_code, Some(0));

    let container = container_name(&runtime.only_run().argv);
    let records = records_of(&container);
    // The records of this launch are there, so an empty result below is not
    // an empty buffer.
    assert!(records.iter().any(|record| record.message == "agent execution finished"));
    assert!(stopped_warnings(&records).is_empty());
    let summary = finished_summary(&sink);
    assert!(summary.starts_with("Turn finished after "), "{summary}");
    assert!(summary.ends_with('.') && !summary.contains("still ran"), "{summary}");
    assert_eq!(summary.matches('.').count(), 1, "{summary}");
}

// EAC-FR-HZIE, EAC-FR-QHAY, CCP-FR-RGVA: a Claude Code launch hands the runtime
// the result line as its final event and the grace of 30 seconds. A Codex
// launch hands it no rule.
#[test]
fn each_vendor_launch_hands_the_runtime_its_own_rule() {
    let claude = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    run(&claude, runtime.clone(), task("go")).expect("ran");
    let rule = runtime.only_run().final_event.expect("Claude Code has a rule");
    assert_eq!(rule.grace, Duration::from_secs(30));
    let stream = valid_claude_stdout();
    let finals: Vec<&str> =
        stream.lines().filter(|line| (rule.is_final)(line.as_bytes())).collect();
    assert_eq!(finals.len(), 1, "{stream}");
    assert!(finals[0].contains(r#""type":"result""#));
    assert!(stream.lines().count() > 1);

    let codex = harness_for("codex");
    let runtime = RecordingRuntime::replying(&codex_stdout(&envelope_json("success")));
    run(&codex, runtime.clone(), task("go")).expect("ran");
    assert!(runtime.only_run().final_event.is_none());
}

// EAC-FR-HZIE, EAC-FR-THUT, EAC-FR-PPXB: an error result stopped after its
// grace is refused as the vendor's failed run. Its finished line keeps the
// refusal and adds the stop, and the warning is still emitted.
#[test]
fn an_error_result_stopped_after_its_grace_is_refused_and_says_it_was_stopped() {
    let harness = harness_for("claude_code");
    let error = format!(
        "{}\n",
        json!({
            "type": "result", "subtype": "error_during_execution", "is_error": true,
            "session_id": SESSION_PLACEHOLDER
        })
    );
    let runtime = lingering(&error, false);
    let sink = Arc::new(CollectedActivity::default());
    let execution = run_watching(&harness, runtime.clone(), task("go"), sink.clone()).expect("ran");
    assert_eq!(execution.process_outcome, ProcessOutcome::InvalidStructuredOutput);
    assert_eq!(execution.refusal, Some(EnvelopeInvalid::VendorRunFailed));
    assert_eq!(execution.exit_code, None);
    assert!(execution.container_removed);

    let summary = finished_summary(&sink);
    assert!(summary.contains("could not be read"), "{summary}");
    assert!(summary.ends_with(STOPPED), "{summary}");
    let container = container_name(&runtime.only_run().argv);
    assert_eq!(stopped_warnings(&records_of(&container)).len(), 1);
}

// EAC-FR-26, EAC-FR-HZIE: a container that cannot be removed after the stop is
// reported, as on every other path out.
#[test]
fn a_failed_removal_after_the_stop_is_reported() {
    let harness = harness_for("claude_code");
    let runtime = lingering(&valid_claude_stdout(), true);
    let execution = run(&harness, runtime.clone(), task("go")).expect("ran");
    assert!(!execution.container_removed);
    let container = container_name(&runtime.only_run().argv);
    assert!(records_of(&container).iter().any(|record| {
        record.level == LogLevel::Error && record.message == "agent container could not be removed"
    }));
}
