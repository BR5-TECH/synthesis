//! Journey 35 of the coverage matrix: the application restarts around a merge
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).
//!
//! Before the handoff nothing is durable. After it the run is on disk: a run
//! found working with no loop rests `interrupted` on the first queue read and
//! Continue resumes it, and a run that ended has what it owned reclaimed.

use super::merge_acts::MachineSetup;
use super::merge_arrange::{as_one_commit, bare_conflict, conflicted_merge};
use super::scenario::TurnScript as Script;
use crate::graduation::GraduationRunState;

// GTE-FR-DQLR, GTE-FR-RZKC / GRD-FR-PFMD, WKS-FR-ENRU: a merge whose check never
// reached the handoff leaves nothing durable. After the relaunch there is no run,
// no merge worktree and no snapshot ref, and the author starts Merge again.
#[test]
fn a_relaunch_before_the_handoff_leaves_nothing_durable() {
    bare_conflict("relaunch before handoff")
        .without_a_draft_run()
        .run()
        .setting_the_machine_up_to(MachineSetup::Nothing)
        .merging(as_one_commit())
        .expect_merge_refused("vendor_execution_unsupported")
        .relaunching()
        .expect_no_merge_run()
        .expect_stream_row_without_a_merge_run()
        .expect_live_state_unchanged()
        .setting_the_machine_up_to(MachineSetup::Complete)
        .merging(as_one_commit())
        .expect_conflicted(&["README.md"])
        .expect_state(GraduationRunState::Queued)
        .expect_merge_snapshot(&["README.md"]);
}

// GTE-FR-DQLR, GTE-FR-HWQM / GRD-FR-PFMD, GRD-FR-XVUD, GXD-FR-TJRV, WKS-FR-ENRU: a
// run the store holds as working with no loop behind it rests `interrupted` with
// `execution_abandoned` on the first queue read, and frees its stream and its
// slot. A merge run commits no abandoned turn and keeps its merge worktree, so
// Continue resumes the work where it stopped and finds the first turn's edit.
#[test]
fn a_working_merge_run_found_after_a_relaunch_rests_interrupted_and_continue_resumes_it() {
    conflicted_merge("relaunch while working")
        .with_merge(as_one_commit())
        .merge_work_turn(
            Script::reports_success()
                .writes("README.md", "half done\n")
                .abandoned_by_a_relaunch(),
        )
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("execution_abandoned")
        .expect_stream_released()
        .expect_slots_in_use(0)
        .expect_commits(0)
        .expect_merge_workspace_standing()
        .expect_base_branch_at_the_pinned_tip()
        .continued(vec![
            Script::reports_success().writes("README.md", "both sides\n"),
            Script::answers_ready("Both intents stand."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["merge_work", "merge_review"])
        .expect_dispatch(0, |d| d.part("merge_work").pass(1).directory_held("README.md", "half done\n"))
        .expect_base_holds_the_merge_commit("Merge the feature stream")
        .expect_merge_workspace_reclaimed();
}

// GTE-FR-DQLR, GTE-FR-HWQM / GRD-FR-XVUD, GRD-FR-PFMD: a run found reviewing rests
// `interrupted` the same way. The sweep records no resume phase, as for every run
// it finds, so Continue asks for the pass again from its work turn, which resumes
// the pass the run stood in, and the review follows it.
#[test]
fn a_reviewing_merge_run_found_after_a_relaunch_rests_interrupted_and_continue_resumes_the_pass() {
    conflicted_merge("relaunch while reviewing")
        .with_merge(as_one_commit())
        .merge_work_turn(Script::reports_success().writes("README.md", "both sides\n"))
        .merge_review_turn(Script::answers_ready("Both intents stand.").abandoned_by_a_relaunch())
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("execution_abandoned")
        .expect_stream_released()
        .expect_slots_in_use(0)
        .expect_merge_workspace_standing()
        .continued(vec![
            Script::reports_success(),
            Script::answers_ready("Both intents stand."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["merge_work", "merge_review"])
        .expect_dispatch(0, |d| {
            d.part("merge_work")
                .pass(1)
                .purpose("resume")
                .directory_held("README.md", "both sides\n")
        })
        .expect_pass(1)
        .expect_turns(2, 2)
        .expect_base_holds_the_merge_commit("Merge the feature stream");
}

// GTE-FR-DQLR / GRD-FR-PFMD, GRD-FR-KZPT, WKS-FR-ENRU: a run that ended before the
// relaunch has its merge worktree, its registration, its scratch branch and its
// snapshot ref reclaimed by the first queue read. What a crash left behind is put
// back by hand, for a completed run and for a failed one, and a run that still
// rests keeps what it owns.
#[test]
fn the_first_queue_read_reclaims_what_an_ended_merge_run_left() {
    conflicted_merge("relaunch after the end")
        .with_merge(as_one_commit())
        .merge_work_turn(Script::reports_success().writes("README.md", "both sides\n"))
        .merge_review_turn(Script::answers_ready("Both intents stand."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_merge_workspace_reclaimed()
        .stranding_what_the_run_owned()
        .expect_merge_workspace_standing()
        .relaunching()
        .expect_state(GraduationRunState::Completed)
        .expect_merge_workspace_reclaimed()
        .expect_no_merge_leftovers();

    conflicted_merge("relaunch after a failure")
        .with_merge(as_one_commit())
        .run()
        .moving_the_base_tip()
        .driving_the_merge(Vec::new())
        .expect_state(GraduationRunState::Failed)
        .stranding_what_the_run_owned()
        .expect_merge_workspace_standing()
        .relaunching()
        .expect_state(GraduationRunState::Failed)
        .expect_merge_workspace_reclaimed()
        .expect_no_merge_leftovers();

    conflicted_merge("relaunch of a resting run")
        .with_merge(as_one_commit())
        .merge_work_turn(Script::escalates(&["Which limit stands?"]))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .relaunching()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_escalation_questions(&["Which limit stands?"])
        .expect_merge_workspace_standing();
}
