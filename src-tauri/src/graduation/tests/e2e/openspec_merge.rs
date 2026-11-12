//! The OpenSpec archive at a stream merge, and the merge notice of an archive
//! the CLI refuses, with the apply runs before them driven through the
//! production loop and the fake CLI of `openspec_cli.rs` behind the process
//! seam (GTE-FR-QPMA, GTE-FR-LOGE, GTE-FR-SBVT).

use super::merge_arrange::as_one_commit;
use super::openspec_arrange::change_file;
use super::scenario::{Scenario, TurnScript as Script};
use crate::graduation::GraduationRunState;

/// A work turn of an apply run of `change` that implements every task and
/// marks each one done.
fn finishing(change: &str) -> Script {
    Script::reports_success()
        .writes(&format!("src/{change}.ts"), "export const done = true;\n")
        .writes(&change_file(change, "tasks.md"), "- [x] Add the button\n- [x] Wire the call\n")
}

fn ready() -> Script {
    Script::answers_ready("Every task is done.")
}

// GTE-FR-LOGE, GTE-FR-QPMA, OAR-FR-SURB, OAR-FR-IAKL, OAR-FR-LZLS,
// OAR-FR-SOFQ: a stream with two changes that completed apply runs landed
// merges clean with one archive commit for each change on the stream branch,
// in archive order, before the merge, and the `merged` result names each
// archived change in that order. The change that completed first is the one
// whose name sorts last, so an order by name fails.
#[test]
fn a_merge_archives_each_landed_change_before_it_merges() {
    Scenario::named("openspec archive")
        .in_openspec_mode()
        .with_change("add-login")
        .with_change("add-logout")
        .applying("add-logout")
        .work_turn(finishing("add-logout"))
        .review_turn(ready())
        .run()
        .expect_state(GraduationRunState::Completed)
        .applying_another("add-login", vec![finishing("add-login"), ready()])
        .expect_state(GraduationRunState::Completed)
        .merging(as_one_commit())
        .expect_changes_archived(&["add-logout", "add-login"])
        .expect_archive_commits_before_the_merge(&["add-logout", "add-login"])
        .expect_cli_processes("archive", 2)
        .expect_queue_offered_no_run();
}

// GTE-FR-SBVT, OFP-FR-ETEN, OFP-FR-XQDY, OFP-FR-PSVR, OFP-FR-MQGG,
// OFP-FR-KYXI: an archive the CLI refuses with a repair cause refuses the
// merge with `archive_failed` and writes no branch and no working copy. The
// merge opens one change discussion and enqueues one repair rework run, and
// the refusal carries both. A second merge with the same change files is
// refused the same way, gives no second notice, and carries the ids of the
// record. After the change files change, a third merge gives a second notice.
#[test]
fn an_archive_refused_for_a_repair_cause_gives_one_notice_and_one_repair_run() {
    const CHANGE: &str = "add-logout";
    const CAUSE: &str = "archive_spec_update_failed";
    Scenario::named("openspec archive conflict")
        .in_openspec_mode()
        .with_change(CHANGE)
        .applying(CHANGE)
        .work_turn(finishing(CHANGE))
        .review_turn(ready())
        .run()
        .expect_state(GraduationRunState::Completed)
        .refusing_the_next_archive(CAUSE)
        .merging(as_one_commit())
        .expect_merge_refused("archive_failed")
        .expect_live_state_unchanged()
        .expect_merge_notice(CHANGE, 1, 1)
        .expect_refusal_carries_the_notice_record(CHANGE)
        .expect_queue_offered_the_repair_run_alone()
        .discarding_the_repair_run()
        .refusing_the_next_archive(CAUSE)
        .merging(as_one_commit())
        .expect_merge_refused("archive_failed")
        .expect_live_state_unchanged()
        .expect_merge_notice(CHANGE, 1, 1)
        .expect_refusal_carries_the_notice_record(CHANGE)
        .expect_queue_offered_no_run()
        .expect_cli_processes("archive", 2)
        .committing_on_the_stream(
            &change_file(CHANGE, "tasks.md"),
            "- [x] Add the button\n- [x] Wire the call\n- [x] Log the logout\n",
        )
        .refusing_the_next_archive(CAUSE)
        .merging(as_one_commit())
        .expect_merge_refused("archive_failed")
        .expect_live_state_unchanged()
        .expect_merge_notice(CHANGE, 2, 2)
        .expect_refusal_carries_the_notice_record(CHANGE)
        .expect_queue_offered_the_repair_run_alone()
        .expect_cli_processes("archive", 3);
}

// GTE-FR-SBVT, OFP-FR-ETEN, OFP-FR-XQDY, OFP-FR-MQGG, OFP-FR-HQQN: an archive
// the CLI refuses for a content cause that is not a repair cause refuses the
// merge, writes no branch and no working copy, and gives a notice with no
// repair run. The refusal carries the thread alone.
#[test]
fn an_archive_refused_for_another_content_cause_gives_a_notice_and_no_repair_run() {
    const CHANGE: &str = "add-logout";
    Scenario::named("openspec archive target exists")
        .in_openspec_mode()
        .with_change(CHANGE)
        .applying(CHANGE)
        .work_turn(finishing(CHANGE))
        .review_turn(ready())
        .run()
        .expect_state(GraduationRunState::Completed)
        .refusing_the_next_archive("archive_target_exists")
        .merging(as_one_commit())
        .expect_merge_refused("archive_failed")
        .expect_live_state_unchanged()
        .expect_merge_notice(CHANGE, 1, 0)
        .expect_refusal_carries_the_notice_record(CHANGE)
        .expect_queue_offered_no_run();
}

// GTE-FR-SBVT, OFP-FR-ETEN, OFP-FR-YKGQ, OFP-FR-PQNS: an archive whose two
// processes time out is a `transient` failure. It refuses the merge with
// `archive_failed` and the cause `cli_timed_out`, writes no branch and no
// working copy, and gives no notice: no discussion, no record, and no run.
#[test]
fn an_archive_that_times_out_twice_refuses_the_merge_with_no_notice() {
    const CHANGE: &str = "add-logout";
    Scenario::named("openspec archive timed out")
        .in_openspec_mode()
        .with_change(CHANGE)
        .applying(CHANGE)
        .work_turn(finishing(CHANGE))
        .review_turn(ready())
        .run()
        .expect_state(GraduationRunState::Completed)
        .timing_out_the_next_archives(2)
        .merging(as_one_commit())
        .expect_merge_refused("archive_failed")
        .expect_refusal_without_a_notice("archive_failed", "cli_timed_out")
        .expect_live_state_unchanged()
        .expect_no_merge_notice(CHANGE)
        .expect_cli_processes("archive", 2)
        .expect_waits(&[2_000])
        .expect_queue_offered_no_run();
}
