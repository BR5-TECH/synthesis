//! The merge notice of a stream merge whose archive step the CLI refused for
//! the content of a change (`OFP-openspec-failure-policy.md` OFP-FR-ETEN
//! through OFP-FR-HMED, OFP-FR-IJYA), with the OpenSpec CLI faked and the
//! repair run started on the production path.

use super::archive_support::*;
use super::*;
use crate::comments::{DiscussionTarget, Participant};
use crate::openspec::{OpenSpecFailure, ProcessError, ProcessOutput, Refusal};
use crate::streams::WorkStream;

pub(super) const CHANGE: &str = "add-logout";

/// The answer of `archive` whose deltas do not apply.
pub(super) fn conflicting() -> Archive {
    Archive::Answer(Ok(ProcessOutput {
        exit_status: Some(1),
        stdout: br#"{ "archive": null, "status": [
          { "severity": "error", "code": "archive_spec_update_failed",
            "message": "MODIFIED \"Session Expiration\" not found in auth",
            "fix": "Use ADDED for a new requirement" } ] }"#
            .to_vec(),
        stderr: Vec::new(),
    }))
}

/// The configuration a repair start needs, committed so the base stays clean.
pub(super) fn allow(fx: &Fixture) {
    fx.allow_execution();
    super::change_runs::commit_all(&fx.root(), "Configure the agent image");
}

/// A stream with one change that a completed apply run built.
pub(super) fn built(fx: &Fixture) -> WorkStream {
    let stream = stream_with_changes(fx, "editor", &[CHANGE]);
    completed_apply(fx, &stream, CHANGE, "2026-11-10T10:00:00Z");
    stream
}

/// The merge, with the loop off, as the command answers it.
pub(super) fn merge_raw(fx: &Fixture, stream: &WorkStream) -> Refusal {
    merge_refusal(fx, stream)
}

pub(super) fn discussions(fx: &Fixture, stream: &WorkStream) -> Vec<crate::comments::Discussion> {
    let target =
        DiscussionTarget::Change { stream_id: stream.id.clone(), change: CHANGE.to_string() };
    crate::comments::list_discussions_in(&records_store(fx), &target)
}

pub(super) fn failure_of(refusal: &Refusal) -> &OpenSpecFailure {
    refusal.openspec().unwrap_or_else(|| panic!("not an OpenSpec failure: {refusal}"))
}

pub(super) fn rework_runs(fx: &Fixture) -> Vec<GraduationRun> {
    crate::graduation::queue::project_queue(&fx.app)
        .unwrap()
        .runs
        .into_iter()
        .filter(|run| run.is_change_rework())
        .collect()
}

// OFP-FR-ETEN, OFP-FR-EJPE, OFP-FR-XQDY, OFP-FR-FAHB, OFP-FR-HVHV,
// OFP-FR-MQGG, OFP-FR-QLQJ, OFP-FR-ZDEE, OFP-FR-NJSO, OAR-FR-EUXZ,
// OCR-FR-TWDK, WKS-FR-YTJY: an archive refused with a repair cause refuses
// the merge with `archive_failed` as an object, opens one discussion as the
// Synthesis participant whose first comment is the notice text and the
// diagnostics, starts one rework run in that thread with that text as its
// instruction, and writes the notice record. The refusal carries the thread
// and the run.
#[test]
fn a_repair_cause_opens_a_notice_and_starts_a_repair_run() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    allow(&fx);
    let stream = built(&fx);
    runner.set_archive(CHANGE, conflicting());
    let base_before = tip(&fx, &stream.base_branch);

    let refusal = merge_raw(&fx, &stream);
    let failure = failure_of(&refusal);
    assert_eq!(failure.code, "archive_failed");
    assert_eq!(failure.cause.as_deref(), Some("archive_spec_update_failed"));
    assert_eq!(failure.change.as_deref(), Some(CHANGE));
    assert_eq!(failure.diagnostics[0].fix.as_deref(), Some("Use ADDED for a new requirement"));
    assert_eq!(tip(&fx, &stream.base_branch), base_before, "nothing was merged");
    // OFP-FR-UPDO: a content refusal runs no archive process again.
    assert_eq!(runner.archive_calls().len(), 1);
    assert!(super::change_runs::waits(&fx).is_empty());

    let threads = discussions(&fx, &stream);
    assert_eq!(threads.len(), 1);
    let thread = &threads[0];
    assert_eq!(failure.thread_id.as_deref(), Some(thread.id.as_str()));
    let first = &thread.comments[0];
    assert_eq!(first.author, Participant::synthesis());
    let expected = format!(
        "{}\n- error archive_spec_update_failed: MODIFIED \"Session Expiration\" not found in auth Fix: Use ADDED for a new requirement",
        crate::openspec_archive::notice::notice_text(CHANGE, "archive_spec_update_failed")
    );
    assert_eq!(first.body, expected);
    assert!(first.body.starts_with(
        "The stream merge could not archive the OpenSpec change \"add-logout\" (archive_spec_update_failed). Change the deltas of \"add-logout\""
    ));

    let runs = rework_runs(&fx);
    assert_eq!(runs.len(), 1);
    let repair = &runs[0];
    assert_eq!(failure.repair_run_id.as_deref(), Some(repair.id.as_str()));
    assert_eq!(failure.repair_refusal, None);
    let data = repair.openspec.as_ref().unwrap();
    assert_eq!(data.thread_id.as_deref(), Some(thread.id.as_str()));
    assert_eq!(data.instruction.as_deref(), Some(expected.as_str()));
    assert_eq!(data.requester.as_ref(), Some(&Participant::synthesis()));

    let record =
        crate::openspec_archive::read_notice_record(&records_store(&fx), &stream.id, CHANGE)
            .expect("a notice record");
    assert_eq!(record.cause, "archive_spec_update_failed");
    assert_eq!(record.thread_id, thread.id);
    assert_eq!(record.repair_run_id.as_deref(), Some(repair.id.as_str()));
    assert_eq!(record.repair_refusal, None);
    assert_eq!(record.digest.len(), 64);

    let wire = serde_json::to_value(&refusal).unwrap();
    assert_eq!(wire["threadId"], thread.id.as_str());
    assert_eq!(wire["repairRunId"], repair.id.as_str());
    assert!(wire.get("repairRefusal").is_none());

    // OFP-FR-HMED: the records name the ids and hold no comment text and no
    // diagnostic.
    let records = stream_records(&stream);
    let given = records.iter().find(|r| r.message == "merge notice given").expect("a record");
    assert_eq!(given.level, crate::logging::LogLevel::Info);
    assert_eq!(given.fields["thread_id"], thread.id.as_str());
    assert_eq!(given.fields["cause"], "archive_spec_update_failed");
    let started = records.iter().find(|r| r.message == "merge notice started a repair run");
    assert_eq!(started.expect("a record").fields["run_id"], repair.id.as_str());
    let written = records.iter().find(|r| r.message == "merge notice record written");
    let written = written.expect("a record of the notice record");
    assert_eq!(written.level, crate::logging::LogLevel::Info);
    assert_eq!(written.fields["change"], CHANGE);
    assert_eq!(written.fields["cause"], "archive_spec_update_failed");
    assert_eq!(written.fields["thread_id"], thread.id.as_str());
    assert_eq!(written.fields["run_id"], repair.id.as_str());
    // OFP-FR-WRDX: no record of the whole buffer, with a stream id or not,
    // holds a diagnostic, the fix, the notice text, or the instruction.
    assert_no_notice_text_logged();
}

/// OFP-FR-WRDX, OFP-FR-HMED: no record of the application's diagnostic buffer
/// holds a diagnostic of [`conflicting`], its fix, or the text of the notice,
/// which is also the instruction of the repair run.
fn assert_no_notice_text_logged() {
    let filter = crate::logging::LogFilter {
        min_level: crate::logging::LogLevel::Debug,
        ..Default::default()
    };
    let records = crate::logging::BUFFER.query(&filter, None, 5000).unwrap().records;
    assert!(!records.is_empty());
    let text = serde_json::to_string(&records).unwrap();
    for needle in [
        "Session Expiration",
        "Use ADDED",
        "The stream merge could not archive",
        "Change the deltas",
        "The OpenSpec CLI reported",
    ] {
        assert!(!text.contains(needle), "a record holds {needle:?}");
    }
}

// OFP-FR-KYXI, OFP-FR-PSVR, OFP-FR-MQGG: a second refusal while the change
// folder digest is unchanged gives no notice: no discussion, no comment, and
// no run. The refusal carries the thread and the run of the record. A change
// of the files gives a new notice, which replaces the record.
#[test]
fn the_same_change_files_give_no_second_notice() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    allow(&fx);
    let stream = built(&fx);
    runner.set_archive(CHANGE, conflicting());
    let first = merge_raw(&fx, &stream);
    let repair = failure_of(&first).repair_run_id.clone().unwrap();
    crate::graduation::discard_graduation_run(fx.app.clone(), repair.clone()).expect("a discard");

    let second = merge_raw(&fx, &stream);
    assert_eq!(failure_of(&second).thread_id, failure_of(&first).thread_id);
    assert_eq!(failure_of(&second).repair_run_id.as_deref(), Some(repair.as_str()));
    assert_eq!(discussions(&fx, &stream).len(), 1, "no second discussion");
    assert_eq!(rework_runs(&fx).len(), 1, "no second run");

    let tasks = stream.worktree().join("openspec/changes").join(CHANGE).join("tasks.md");
    std::fs::write(tasks, "- [x] Do it again\n").unwrap();
    super::change_runs::commit_all(&stream.worktree(), "Change the change");
    let third = merge_raw(&fx, &stream);
    assert_eq!(discussions(&fx, &stream).len(), 2, "new files give a new notice");
    assert_ne!(failure_of(&third).thread_id, failure_of(&first).thread_id);
    assert_eq!(rework_runs(&fx).len(), 2);
    let record =
        crate::openspec_archive::read_notice_record(&records_store(&fx), &stream.id, CHANGE)
            .unwrap();
    assert_eq!(Some(record.thread_id), failure_of(&third).thread_id);
}

// OFP-FR-PCQY, OFP-FR-MQGG, OFP-FR-HMED: a repair start that is refused, here
// by the image preflight, starts nothing. The discussion stays, the record
// holds the refusal code, and the merge refusal carries it.
#[test]
fn a_refused_repair_start_starts_nothing_and_says_why() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = built(&fx);
    runner.set_archive(CHANGE, conflicting());
    let refusal = merge_raw(&fx, &stream);
    let failure = failure_of(&refusal);
    let code = failure.repair_refusal.clone().expect("a repair refusal");
    // No agent can execute on this machine, so the image preflight refuses.
    assert_eq!(code, crate::graduation::ERR_VENDOR_EXECUTION_UNSUPPORTED);
    let wire = serde_json::to_value(&refusal).unwrap();
    assert_eq!(wire["repairRefusal"], code.as_str());
    assert!(wire.get("repairRunId").is_none());
    assert_eq!(failure.repair_run_id, None);
    assert!(failure.thread_id.is_some());
    assert_eq!(discussions(&fx, &stream).len(), 1);
    assert!(rework_runs(&fx).is_empty());
    let record =
        crate::openspec_archive::read_notice_record(&records_store(&fx), &stream.id, CHANGE)
            .unwrap();
    assert_eq!(record.repair_refusal.as_deref(), Some(code.as_str()));
    let refused = stream_records(&stream)
        .into_iter()
        .find(|r| r.message == "merge notice repair start refused")
        .expect("a record");
    assert_eq!(refused.level, crate::logging::LogLevel::Warn);
    assert_eq!(refused.fields["code"], code.as_str());
}

// OFP-FR-ETEN, OFP-FR-EJPE, OFP-FR-MQGG, OFP-FR-HQQN: a content refusal that
// is no repair cause posts the other notice text and starts no run, and its
// refusal carries the thread alone. A change whose tasks are not done gives
// the same notice through `change_not_archivable`.
#[test]
fn another_content_refusal_posts_a_notice_and_starts_no_run() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    allow(&fx);
    let stream = built(&fx);
    let exists = br#"{ "archive": null, "status": [ { "code": "archive_target_exists" } ] }"#;
    runner.set_archive(
        CHANGE,
        Archive::Answer(Ok(ProcessOutput {
            exit_status: Some(1),
            stdout: exists.to_vec(),
            stderr: Vec::new(),
        })),
    );
    let refusal = merge_raw(&fx, &stream);
    let failure = failure_of(&refusal);
    assert_eq!(failure.cause.as_deref(), Some("archive_target_exists"));
    assert!(failure.thread_id.is_some());
    assert_eq!(failure.repair_run_id, None);
    assert_eq!(failure.repair_refusal, None);
    let body = &discussions(&fx, &stream)[0].comments[0].body;
    assert!(
        body.contains("(archive_target_exists). Nothing was merged. The OpenSpec CLI reported:")
    );
    assert!(rework_runs(&fx).is_empty());

    let (fx, runner, _) = archive_fixture(READY);
    let stream = built(&fx);
    let _ = runner;
    let refusal = merge_raw(&fx, &stream);
    let failure = failure_of(&refusal);
    assert_eq!(failure.code, "change_not_archivable");
    assert_eq!(failure.cause.as_deref(), Some("tasks_incomplete"));
    assert!(failure.thread_id.is_some());
    assert!(rework_runs(&fx).is_empty());
}

// OFP-FR-ETEN, OFP-FR-IJYA, OFP-FR-PQNS: an archive whose outcome is
// transient runs once more after its restore; a second transient outcome
// refuses the merge with that code, and a transient refusal gives no notice.
#[test]
fn a_transient_archive_runs_once_more_and_gives_no_notice() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = built(&fx);
    runner.set_archive(CHANGE, Archive::Answer(Err(ProcessError::timed_out())));
    let refusal = merge_raw(&fx, &stream);
    assert_eq!(refusal, "archive_failed: add-logout: cli_timed_out");
    assert_eq!(runner.archive_calls().len(), 2, "one archive, run once more");
    assert!(failure_of(&refusal).thread_id.is_none());
    assert!(discussions(&fx, &stream).is_empty(), "no notice for a transient refusal");
    assert!(dirty(&stream.worktree()).is_empty(), "each attempt was restored");
    let filter = crate::logging::LogFilter {
        min_level: crate::logging::LogLevel::Debug,
        ..Default::default()
    };
    let records = crate::logging::BUFFER.query(&filter, None, 5000).unwrap().records;
    let retry = records.iter().find(|r| r.message == "openspec cli runs again").expect("a record");
    assert_eq!(retry.fields["subcommand"], "archive");
    assert_eq!(retry.fields["code"], "cli_timed_out");
}

// OFP-FR-IJYA: a transient archive whose second process succeeds archives
// the change, and the merge goes on.
#[test]
fn a_transient_archive_that_succeeds_on_the_second_try_merges() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = built(&fx);
    runner.set_archive(
        CHANGE,
        Archive::Then(
            Box::new(Archive::Answer(Err(ProcessError::timed_out()))),
            Box::new(Archive::Succeed),
        ),
    );
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let merged =
        crate::streams::merge_work_stream_blocking(&fx.app, &stream.id, commit_publication());
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    let merged = merged.expect("the merge goes on");
    assert!(matches!(merged, crate::streams::StreamMergeResult::Merged { .. }), "{merged:?}");
    assert_eq!(runner.archive_calls().len(), 2);
}

// OFP-FR-PWAV: a notice whose discussion cannot be opened writes no record and
// starts no run, and the refusal carries no thread, no run, and no refusal.
#[test]
fn a_notice_that_cannot_open_its_discussion_carries_nothing() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    allow(&fx);
    let stream = built(&fx);
    runner.set_archive(CHANGE, conflicting());
    // A file where the discussions of the change are kept, so none opens.
    let folder = records_store(&fx).path().join("changes").join(&stream.id).join(CHANGE);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("comments"), "not a folder").unwrap();
    let refusal = merge_raw(&fx, &stream);
    let failure = failure_of(&refusal);
    assert_eq!(failure.code, "archive_failed");
    assert_eq!(failure.thread_id, None);
    assert_eq!(failure.repair_run_id, None);
    assert_eq!(failure.repair_refusal, None);
    assert!(rework_runs(&fx).is_empty());
    assert!(crate::openspec_archive::read_notice_record(&records_store(&fx), &stream.id, CHANGE)
        .is_none());
}

// OAR-FR-MSOM, OFP-FR-HVHV: the notice record moves with the change's store
// folder when a later merge archives the change.
#[test]
fn the_notice_record_moves_with_the_change_at_its_archive() {
    let (fx, runner, _) = archive_fixture(READY);
    let stream = built(&fx);
    let refusal = merge_raw(&fx, &stream);
    assert_eq!(failure_of(&refusal).cause.as_deref(), Some("tasks_incomplete"));
    assert!(crate::openspec_archive::read_notice_record(&records_store(&fx), &stream.id, CHANGE)
        .is_some());

    runner.inner.set("instructions", Ok(crate::openspec::tests::ok(ALL_DONE)));
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let merged =
        crate::streams::merge_work_stream_blocking(&fx.app, &stream.id, commit_publication());
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    merged.expect("the merge goes on");
    assert!(crate::openspec_archive::read_notice_record(&records_store(&fx), &stream.id, CHANGE)
        .is_none());
    let moved = records_store(&fx)
        .path()
        .join("changes")
        .join(&stream.id)
        .join("archive")
        .join(format!("{DATE}-{CHANGE}"))
        .join(crate::openspec_archive::NOTICE_RECORD_FILENAME);
    assert!(moved.is_file(), "the record moved with the folder");
}
