//! The repair start of a merge notice and what the notice sends out
//! (`OFP-openspec-failure-policy.md` OFP-FR-PCQY, OFP-FR-PSVR, OFP-FR-MQGG,
//! OFP-FR-ETEN, OFP-FR-WRDX, OFP-FR-HMED), with the OpenSpec CLI faked and the
//! repair run started on the production path.

use std::sync::{Arc, Mutex};

use tauri::Listener;

use super::archive_support::*;
use super::openspec_merge_notice::{
    allow, built, conflicting, discussions, failure_of, merge_raw, rework_runs, CHANGE,
};
use super::*;
use crate::openspec::{CliDiagnostic, OpenSpecFailure};
use crate::streams::WorkStream;

/// The failure of a merge whose archive the CLI refused with a repair cause.
fn repair_failure() -> OpenSpecFailure {
    let mut failure = OpenSpecFailure::new("archive_failed").for_change(CHANGE);
    failure.cause = Some("archive_spec_update_failed".into());
    failure.diagnostics = vec![CliDiagnostic::of("error", "Session Expiration not found")];
    failure
}

/// The notice of [`repair_failure`], given as `merge_work_stream` gives it.
fn give(fx: &Fixture, stream: &WorkStream) -> crate::openspec_archive::MergeNotice {
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let notice =
        crate::openspec_archive::give_merge_notice(&fx.app, &stream.id, CHANGE, &repair_failure());
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    notice
}

// OFP-FR-PCQY, OFP-FR-HVHV, OFP-FR-HMED: a repair start refused with
// `proposal_pending`, here by a change edit proposal of the change that waits
// for the author, starts nothing. The discussion stays, and the notice record
// and the notice hold the code.
#[test]
fn a_pending_proposal_refuses_the_repair_start() {
    let (fx, _, _) = archive_fixture(ALL_DONE);
    allow(&fx);
    let stream = built(&fx);
    write_record(
        &fx,
        &stream,
        CHANGE,
        "proposals/p1.toml",
        &edit_proposal(&stream, CHANGE, "pending", false),
    );
    let notice = give(&fx, &stream);
    assert_eq!(notice.repair_refusal.as_deref(), Some("proposal_pending"));
    assert_eq!(notice.repair_run_id, None);
    assert!(notice.thread_id.is_some());
    assert_eq!(discussions(&fx, &stream).len(), 1, "the discussion stays");
    assert!(rework_runs(&fx).is_empty());
    let record =
        crate::openspec_archive::read_notice_record(&records_store(&fx), &stream.id, CHANGE)
            .expect("a notice record");
    assert_eq!(record.repair_refusal.as_deref(), Some("proposal_pending"));
    let refused = one_record(&stream, "merge notice repair start refused");
    assert_eq!(refused.level, crate::logging::LogLevel::Warn);
    assert_eq!(refused.fields["code"], "proposal_pending");
}

// OFP-FR-PCQY: a repair start refused with `openspec_run_active`, here by an
// apply run of the change that waits in its queue, starts nothing. The notice
// and its record hold the code alone, without the id of the other run.
#[test]
fn an_active_run_of_the_change_refuses_the_repair_start() {
    let (fx, _, _) = archive_fixture(ALL_DONE);
    allow(&fx);
    let stream = built(&fx);
    let mut active = fx.enqueue(&stream, &crate::graduation::apply_prompt(CHANGE));
    active.openspec = Some(RunOpenSpec::of_phase(crate::graduation::PHASE_APPLY, CHANGE));
    crate::graduation::save_run(&fx.app, &mut active).unwrap();
    let notice = give(&fx, &stream);
    assert_eq!(notice.repair_refusal.as_deref(), Some("openspec_run_active"));
    assert_eq!(notice.repair_run_id, None);
    assert_eq!(discussions(&fx, &stream).len(), 1);
    assert!(rework_runs(&fx).is_empty());
    let record =
        crate::openspec_archive::read_notice_record(&records_store(&fx), &stream.id, CHANGE)
            .expect("a notice record");
    assert_eq!(record.repair_refusal.as_deref(), Some("openspec_run_active"));
}

// OFP-FR-PSVR, OFP-FR-MQGG, OFP-FR-HMED: a record that holds a repair refusal
// gives no second notice for the same change files: no discussion, no
// comment, and no start. The refusal carries the thread and the repair
// refusal of the record, and one INFO record tells it.
#[test]
fn the_same_change_files_replay_the_repair_refusal_of_the_record() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = built(&fx);
    runner.set_archive(CHANGE, conflicting());
    let first = merge_raw(&fx, &stream);
    let code = failure_of(&first).repair_refusal.clone().expect("a repair refusal");
    let comments_before = discussions(&fx, &stream)[0].comments.len();

    let second = merge_raw(&fx, &stream);
    let failure = failure_of(&second);
    assert_eq!(failure.thread_id, failure_of(&first).thread_id);
    assert_eq!(failure.repair_refusal.as_deref(), Some(code.as_str()));
    assert_eq!(failure.repair_run_id, None);
    let wire = serde_json::to_value(&second).unwrap();
    assert_eq!(wire["repairRefusal"], code.as_str());
    let threads = discussions(&fx, &stream);
    assert_eq!(threads.len(), 1, "no second discussion");
    assert_eq!(threads[0].comments.len(), comments_before, "no second comment");
    assert!(rework_runs(&fx).is_empty());
    let replayed = one_record(&stream, "merge notice already given for these change files");
    assert_eq!(replayed.level, crate::logging::LogLevel::Info);
    assert_eq!(replayed.fields["thread_id"], failure.thread_id.clone().unwrap().as_str());
    let refusals: Vec<_> = stream_records(&stream)
        .into_iter()
        .filter(|r| r.message == "merge notice repair start refused")
        .collect();
    assert_eq!(refusals.len(), 1, "the second merge starts nothing");
}

// OFP-FR-ETEN: the notice comes after the merge released the repository
// update guard. When the notice opens its discussion, a new update of the
// repository can take the guard.
#[test]
fn the_notice_comes_after_the_guards_are_released() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    allow(&fx);
    let stream = built(&fx);
    runner.set_archive(CHANGE, conflicting());
    let taken = Arc::new(Mutex::new(Vec::<bool>::new()));
    let (seen, app, stream_id) = (taken.clone(), fx.app.clone(), stream.id.clone());
    fx.app.listen(crate::comments::DISCUSSION_CHANGED, move |_| {
        let state = app.state::<crate::streams::StreamState>();
        let free = state
            .begin_repository_update(&stream_id, crate::streams::Reconciliation::Update)
            .is_ok();
        seen.lock().unwrap().push(free);
    });
    let refusal = merge_raw(&fx, &stream);
    assert!(failure_of(&refusal).repair_run_id.is_some());
    assert_eq!(*taken.lock().unwrap(), vec![true], "the guard is free at the notice");
}

// OFP-FR-WRDX: an event payload holds a diagnostic only where it carries the
// discussion whose comment holds it. The other events of a refused merge with
// a notice and a repair run hold none.
#[test]
fn only_the_discussion_event_carries_the_diagnostics() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    allow(&fx);
    let stream = built(&fx);
    runner.set_archive(CHANGE, conflicting());
    let events = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let sink = events.clone();
    for name in [
        crate::comments::DISCUSSION_CHANGED,
        crate::graduation::GRADUATION_QUEUE_CHANGED,
        crate::graduation::GRADUATION_RUN_CHANGED,
        crate::streams::WORK_STREAMS_CHANGED,
        crate::change_action_requests::CHANGE_ACTION_REQUESTS_CHANGED,
        crate::change_proposals::CHANGE_PROPOSALS_CHANGED,
        crate::progress::OPERATION_PROGRESS,
    ] {
        let sink = sink.clone();
        fx.app.listen(name, move |event| {
            sink.lock().unwrap().push((name.to_string(), event.payload().to_string()));
        });
    }
    let refusal = merge_raw(&fx, &stream);
    assert!(failure_of(&refusal).repair_run_id.is_some());
    let events = events.lock().unwrap().clone();
    let carrying: Vec<&str> = events
        .iter()
        .filter(|(_, payload)| payload.contains("Session Expiration"))
        .map(|(name, _)| name.as_str())
        .collect();
    assert!(!carrying.is_empty(), "the discussion event carries its comment");
    assert!(
        carrying.iter().all(|name| *name == crate::comments::DISCUSSION_CHANGED),
        "{carrying:?}"
    );
    assert!(events.iter().any(|(name, _)| name == crate::graduation::GRADUATION_QUEUE_CHANGED));
}
