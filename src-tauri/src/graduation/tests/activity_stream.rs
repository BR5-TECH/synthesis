//! The durable activity stream of a graduation run
//! (`GRS-graduation-run-log-storage.md` GRS-FR-SQGZ, GRS-FR-WJIA, GRS-FR-VZUZ,
//! GRS-FR-RDJP, GRS-FR-WRAS, GRS-FR-ITWJ).

use super::*;

use super::log_reads::driven;
use crate::graduation::logs::read::{STATUS_AVAILABLE, STATUS_EMPTY};
use crate::graduation::logs::{self, GraduationLogPage, GraduationLogProducer, GraduationLogStream};
use crate::tools::agent_exec::{DurableActivity, DurableOutputSink};

fn activity(kind: &'static str, summary: &str) -> DurableActivity {
    DurableActivity {
        record_id: crate::notes::new_note_id(),
        at: crate::notes::now_rfc3339(),
        channel: "stdout",
        kind,
        summary: summary.to_string(),
    }
}

/// Call `read_graduation_logs` as the frontend does.
fn command(
    fx: &Fixture,
    run_id: &str,
    phase_id: &str,
    scope: serde_json::Value,
    cursor: Option<logs::GraduationLogCursor>,
) -> GraduationLogPage {
    crate::graduation::read_graduation_logs(
        fx.app.clone(),
        run_id.to_string(),
        phase_id.to_string(),
        scope,
        "activity".to_string(),
        cursor,
        Some(1000),
        None,
    )
    .expect("a page")
}

fn summaries(page: &GraduationLogPage) -> Vec<String> {
    page.entries
        .iter()
        .filter_map(|entry| entry.record["summary"].as_str().map(str::to_string))
        .collect()
}

/// GRS-FR-WJIA, GRS-FR-VZUZ, GRS-FR-RDJP, GRS-FR-JWBW — a record is built for
/// a safe kind from an agent turn and for nothing else.
#[test]
fn only_a_safe_kind_of_an_agent_turn_becomes_a_record() {
    for kind in ["started", "message", "tool_call", "tool_result", "command", "file_change", "retry", "usage", "finished", "error", "diagnostic"] {
        assert!(
            logs::activity_record("g1", GraduationLogProducer::WorkTurn, Some(1), &activity(kind, "x"))
                .is_some(),
            "{kind} is a safe kind"
        );
    }
    for kind in ["invocation", "task", "reasoning", "unrecognized", "turn"] {
        assert!(
            logs::activity_record("g1", GraduationLogProducer::WorkTurn, Some(1), &activity(kind, "x"))
                .is_none(),
            "{kind} is excluded"
        );
    }
    for producer in [
        GraduationLogProducer::QueueWait,
        GraduationLogProducer::Commit,
        GraduationLogProducer::StageTransition,
    ] {
        assert!(
            logs::activity_record("g1", producer, None, &activity("message", "x")).is_none(),
            "{producer:?} is no agent turn"
        );
    }
}

/// GRS-FR-RDJP, GRS-FR-NUXT, GRS-FR-KJVN — the attribution follows the
/// producer table, and a run-level record names a non-agent origin.
#[test]
fn a_record_is_attributed_by_its_producer_and_a_run_level_record_names_the_executor() {
    let work = logs::activity_record("g1", GraduationLogProducer::WorkTurn, Some(2), &activity("message", "a"))
        .expect("a record");
    assert_eq!(work.attribution.phase_id, "working");
    assert_eq!(work.attribution.pass, Some(2));
    assert_eq!(work.attribution.producer, "work_turn");
    assert_eq!(serde_json::to_value(work.attribution.origin).unwrap(), "agent");

    let review = logs::activity_record("g1", GraduationLogProducer::ReviewTurn, Some(1), &activity("message", "a"))
        .expect("a record");
    assert_eq!(review.attribution.phase_id, "review");

    let merge = logs::activity_record("g1", GraduationLogProducer::SemanticMergeTurn, Some(3), &activity("message", "a"))
        .expect("a record");
    assert_eq!(merge.attribution.phase_id, "done");
    assert_eq!(merge.attribution.pass, None, "a semantic merge turn stands in no pass");
    assert_eq!(serde_json::to_value(merge.attribution.origin).unwrap(), "executor");
}

/// GRS-FR-SQGZ — a summary is one line whatever reached the sink.
#[test]
fn a_summary_is_one_bounded_line() {
    let long = format!("first\nsecond\t{}", "x".repeat(900));
    let record = logs::activity_record("g1", GraduationLogProducer::WorkTurn, Some(1), &activity("message", &long))
        .expect("a record");
    assert!(!record.summary.contains('\n') && !record.summary.contains('\t'));
    assert!(record.summary.chars().count() <= 301);
}

/// GRS-FR-WRAS, GRS-FR-ITWJ, GRS-FR-UCZL — a record is readable as soon as the
/// sink has acknowledged it, while the run is still queued and no turn has
/// ended.
#[test]
fn a_record_is_readable_as_soon_as_the_sink_acknowledges_it() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let mut run = fx.enqueue(&stream, "Write the panel.");
    logs::initialize(&fx.app, &mut run);

    let sink = logs::GraduationActivitySink::new(&fx.app, &run, GraduationLogProducer::WorkTurn);
    sink.record(activity("message", "first line")).expect("acknowledged");
    sink.record(activity("tool_call", "second line")).expect("acknowledged");

    let page = command(&fx, &run.id, "working", serde_json::json!({ "kind": "pass", "pass": run.pass() }), None);
    assert_eq!(page.status, STATUS_AVAILABLE);
    assert_eq!(summaries(&page), vec!["first line", "second line"]);
    assert_eq!(page.latest_sequence, 2);

    // An `after` cursor reads only what arrived later.
    let third = activity("message", "third line");
    sink.record(third).expect("acknowledged");
    let later = command(
        &fx,
        &run.id,
        "working",
        serde_json::json!({ "kind": "pass", "pass": run.pass() }),
        Some(logs::GraduationLogCursor {
            run_id: run.id.clone(),
            stream: GraduationLogStream::Activity,
            direction: "after".to_string(),
            sequence: 2,
        }),
    );
    assert_eq!(summaries(&later), vec!["third line"]);
}

/// GRS-FR-VZUZ, GRS-FR-JWBW — the sink refuses an excluded kind before it
/// reaches the file, and answers `ok` because nothing was lost.
#[test]
fn the_sink_writes_no_excluded_kind() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let mut run = fx.enqueue(&stream, "Write the panel.");
    logs::initialize(&fx.app, &mut run);
    let sink = logs::GraduationActivitySink::new(&fx.app, &run, GraduationLogProducer::WorkTurn);

    for kind in ["invocation", "task", "reasoning", "unrecognized"] {
        sink.record(activity(kind, "SHOULD-NOT-BE-KEPT")).expect("refused quietly");
    }
    let text = std::fs::read_to_string(
        logs::paths_for(&fx.app, &run.id).unwrap().stream(GraduationLogStream::Activity),
    )
    .unwrap();
    assert!(text.is_empty(), "nothing reached the file: {text}");
}

/// GRS-FR-WRAS, GRS-FR-RGPN, GRS-FR-FORV — what the sink acknowledged is read
/// back from the file after the process has forgotten the run.
#[test]
fn acknowledged_records_are_read_back_after_a_relaunch() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let mut run = fx.enqueue(&stream, "Write the panel.");
    logs::initialize(&fx.app, &mut run);
    let sink = logs::GraduationActivitySink::new(&fx.app, &run, GraduationLogProducer::WorkTurn);
    sink.record(activity("message", "kept across a relaunch")).expect("acknowledged");

    // A relaunch holds no live index for the run.
    logs::release(&fx.app, &run.id);
    let mut loaded = crate::graduation::load_run(&fx.app, &run.id).expect("the run");
    logs::reconcile_saved(&fx.app, &mut loaded);
    assert_eq!(loaded.logs.activity.record_count, 1);

    let page = command(&fx, &run.id, "working", serde_json::json!({ "kind": "phase" }), None);
    assert_eq!(summaries(&page), vec!["kept across a relaunch"]);
}

/// GLW-FR-JMXD, GRS-FR-NKZP, GRS-FR-TQAO, GRS-FR-RZXA — the passes of the
/// review phase stay separate and readable after the run has returned to
/// working.
#[test]
fn earlier_review_passes_stay_separate_after_the_run_returns_to_working() {
    let fx = Fixture::new();
    let run = driven(&fx);

    let review = |pass: u32| {
        command(&fx, &run.id, "review", serde_json::json!({ "kind": "pass", "pass": pass }), None)
    };
    let first = review(1);
    let second = review(2);
    assert_eq!(first.status, STATUS_AVAILABLE);
    assert_eq!(second.status, STATUS_AVAILABLE);
    for (page, pass) in [(&first, 1), (&second, 2)] {
        for entry in &page.entries {
            assert_eq!(entry.record["phase_id"], "review");
            assert_eq!(entry.record["pass"], pass);
        }
    }
    let ids = |page: &GraduationLogPage| -> Vec<String> {
        page.entries.iter().map(|e| e.record["record_id"].as_str().unwrap().to_string()).collect()
    };
    assert!(ids(&first).iter().all(|id| !ids(&second).contains(id)));

    // The work phase holds no record of the review phase.
    let working = command(&fx, &run.id, "working", serde_json::json!({ "kind": "phase" }), None);
    assert!(working.entries.iter().all(|e| e.record["phase_id"] == "working"));

    // A pass that never entered a phase holds nothing there.
    let none = command(&fx, &run.id, "review", serde_json::json!({ "kind": "pass", "pass": 9 }), None);
    assert_eq!(none.status, STATUS_EMPTY);
}

/// GRS-FR-PQVK, GRS-FR-RZXA — the read names the activity stream, and the
/// raw `source` stream no longer exists.
#[test]
fn the_read_refuses_the_source_stream() {
    let fx = Fixture::new();
    let run = driven(&fx);
    let refused = crate::graduation::read_graduation_logs(
        fx.app.clone(),
        run.id.clone(),
        "working".to_string(),
        serde_json::json!({ "kind": "phase" }),
        "source".to_string(),
        None,
        None,
        None,
    )
    .expect_err("no such stream");
    assert_eq!(refused, crate::graduation::ERR_UNKNOWN_STREAM);
}

/// GRS-FR-ITWJ, GRS-FR-THZA, GRS-FR-EIXS — a failed append is answered with a
/// typed failure, and the page says persistence failed rather than empty.
#[test]
fn a_failed_append_is_a_typed_failure_and_never_an_empty_page() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let mut run = fx.enqueue(&stream, "Write the panel.");
    logs::initialize(&fx.app, &mut run);
    let paths = logs::paths_for(&fx.app, &run.id).unwrap();
    std::fs::remove_file(paths.stream(GraduationLogStream::Activity)).unwrap();
    std::fs::create_dir(paths.stream(GraduationLogStream::Activity)).unwrap();

    let sink = logs::GraduationActivitySink::new(&fx.app, &run, GraduationLogProducer::WorkTurn);
    let failure = sink.record(activity("message", "lost?")).expect_err("not stored");
    assert_eq!(failure.code, logs::index::CODE_APPEND_FAILED);

    let mut seen = crate::graduation::load_run(&fx.app, &run.id).expect("the run");
    logs::overlay_live(&fx.app, &mut seen);
    assert!(!seen.logs.persistence.is_healthy());
    let page = command(&fx, &run.id, "working", serde_json::json!({ "kind": "phase" }), None);
    assert_eq!(page.status, "persistence_failed");
    assert_eq!(page.failure.map(|f| f.stream), Some(GraduationLogStream::Activity));
}
