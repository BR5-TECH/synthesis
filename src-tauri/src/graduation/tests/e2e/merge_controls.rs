//! Journey 36 of the coverage matrix: the controls of a merge run
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).
//!
//! A merge run is a run like any other to the controls: an escalation rests it
//! and its answers reach the phase that asked, the author pauses and discards it,
//! and files it away. Restart and revert have nothing to act on and are refused.

use super::merge_arrange::{as_one_commit, conflicted_merge};
use super::scenario::TurnScript as Script;
use crate::graduation::{GraduationEscalationOrigin, GraduationRunState};

// GTE-FR-WXEJ, GTE-FR-XKGB / GRL-FR-VBCL, GRL-FR-MWPQ, GXD-FR-HSQV, GXD-FR-BJYT,
// GXD-FR-XPUR: an escalation of a `merge_work` turn rests the run
// `awaiting_author` with its ordered questions and frees the stream and the slot.
// The answers reach the next `merge_work` turn of the same pass, which resumes the
// session that asked, and the pass does not advance.
#[test]
fn an_escalation_of_a_merge_work_turn_is_answered_into_the_work_turn() {
    conflicted_merge("merge work escalates")
        .with_merge(as_one_commit())
        .merge_work_turn(
            Script::escalates(&["Which limit stands?", "Keep the stream's file?"]).leaves_a_session(),
        )
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_escalation_questions(&["Which limit stands?", "Keep the stream's file?"])
        .expect_escalation_origin(GraduationEscalationOrigin::Work)
        .expect_pass(1)
        .expect_stream_released()
        .expect_slots_in_use(0)
        .expect_merge_workspace_standing()
        .answered(
            &["The base limit stands.", "Yes."],
            vec![
                Script::reports_success().writes("README.md", "both sides\n"),
                Script::answers_ready("Both intents stand."),
            ],
        )
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["merge_work", "merge_review"])
        .expect_dispatch(0, |d| {
            d.part("merge_work")
                .pass(1)
                .purpose("escalation_answer")
                .escalation_answers(&["The base limit stands.", "Yes."])
                .resumes_session("s1")
        })
        .expect_dispatch(1, |d| d.part("merge_review").pass(1).resumes_no_session())
        .expect_pass(1)
        .expect_no_escalation()
        .expect_base_holds_the_merge_commit("Merge the feature stream");
}

// GTE-FR-WXEJ, GTE-FR-XKGB / GRL-FR-VBCL, GXD-FR-HSQV, GXD-FR-XPUR: an escalation
// of a `merge_review` turn rests the run on the same terms, and its answers go
// into a fresh review turn with no work turn composed.
#[test]
fn an_escalation_of_a_merge_review_turn_is_answered_into_a_fresh_review() {
    conflicted_merge("merge review escalates")
        .with_merge(as_one_commit())
        .merge_work_turn(Script::reports_success().writes("README.md", "both sides\n"))
        .merge_review_turn(Script::escalates(&["Is the stream's file wanted?"]))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_escalation_questions(&["Is the stream's file wanted?"])
        .expect_escalation_origin(GraduationEscalationOrigin::Review)
        .expect_stream_released()
        .expect_slots_in_use(0)
        .answered(
            &["Yes, keep it."],
            vec![Script::answers_ready("Both intents stand.")],
        )
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["merge_review"])
        .expect_dispatch(0, |d| {
            d.part("merge_review")
                .pass(1)
                .escalation_answers(&["Yes, keep it."])
                .resumes_no_session()
                .merge_review_instruction()
        })
        .expect_turns(1, 2)
        .expect_base_holds_the_merge_commit("Merge the feature stream");
}

// GTE-FR-WXEJ, GTE-FR-HWQM / GRD-FR-MDQZ, GRD-FR-SWOJ, GXD-FR-TJRV: the author's
// pause during a `merge_work` turn rests the run `interrupted`. A merge run commits
// no abandoned turn: what the turn wrote stays in the merge worktree, no branch or
// working copy is written, and Continue resumes at the work. A pause during the
// review resumes at the review.
#[test]
fn a_pause_rests_a_merge_run_and_continue_resumes_the_phase_that_stopped() {
    conflicted_merge("merge pause")
        .with_merge(as_one_commit())
        .merge_work_turn(
            Script::reports_success()
                .writes("README.md", "half done\n")
                .checking_the_live_trees_stand()
                .pausing_the_run(),
        )
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("author_pause")
        .expect_commits(0)
        .expect_stream_released()
        .expect_merge_worktree_holds("README.md", "half done\n")
        .expect_base_branch_at_the_pinned_tip()
        .expect_base_working_copy_clean()
        .continued(vec![
            Script::reports_success().writes("README.md", "both sides\n"),
            Script::answers_ready("Both intents stand.").pausing_the_run().finishing_anyway(),
        ])
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("author_pause")
        .expect_resume_phase("review")
        .expect_stream_released()
        .continued(vec![Script::answers_ready("Both intents stand.")])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["merge_review"])
        .expect_base_holds_the_merge_commit("Merge the feature stream");
}

// GTE-FR-WXEJ, GTE-FR-HWQM / GRD-FR-EWTN, GRD-FR-YTCR, GRD-FR-GMTX, GRD-FR-KZPT: the
// author discards the run while its turn runs. The run is `discarded`, no
// branch and no working copy is written, no commit is made, and what the run owned
// is reclaimed once the turn has returned.
#[test]
fn a_discard_during_a_merge_turn_writes_nothing_and_reclaims_what_the_run_owned() {
    conflicted_merge("merge discard mid-turn")
        .with_merge(as_one_commit())
        .merge_work_turn(
            Script::reports_success()
                .writes("README.md", "half done\n")
                .discarding_the_run(),
        )
        .run()
        .expect_state(GraduationRunState::Discarded)
        .expect_commits(0)
        .expect_no_merge_result()
        .expect_live_state_unchanged()
        .expect_stream_released()
        .expect_slots_in_use(0)
        .expect_merge_workspace_reclaimed()
        .expect_stream_row_without_a_merge_run();
}

// GTE-FR-WXEJ, GTE-FR-RZKC / GRD-FR-YTCR, GRD-FR-ZAMI, GRD-FR-BLCR, GRD-FR-CYIB:
// `restart_graduation_run` is refused with `run_state_not_permitted` and
// `revert_graduation_run` with `run_not_revertable`, from a resting run, from a
// discarded run, which is the one state a restart is otherwise accepted from, and
// from a completed run that made a merge commit. Each refusal leaves the run and
// both branches as they were.
#[test]
fn restart_and_revert_are_refused_for_a_merge_run() {
    conflicted_merge("merge restart and revert")
        .with_merge(as_one_commit())
        .merge_work_turn(Script::escalates(&["Which limit stands?"]))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .refusing_restart("run_state_not_permitted")
        .refusing_revert("run_not_revertable")
        .discarded()
        .refusing_restart("run_state_not_permitted")
        .refusing_revert("run_not_revertable")
        .expect_state(GraduationRunState::Discarded)
        .expect_runs_in_the_queue(1)
        .expect_live_state_unchanged();

    conflicted_merge("merge revert of a merge commit")
        .with_merge(as_one_commit())
        .merge_work_turn(Script::reports_success().writes("README.md", "both sides\n"))
        .merge_review_turn(Script::answers_ready("Both intents stand."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_commits(1)
        .refusing_revert("run_not_revertable")
        .refusing_restart("run_state_not_permitted")
        .expect_base_holds_the_merge_commit("Merge the feature stream")
        .expect_runs_in_the_queue(1);
}

// GTE-FR-WXEJ / GRD-FR-DLWB, GRD-FR-TKUR, GRD-FR-JOFE, GRD-FR-RHNP, WKS-FR-KHJS:
// auto-start, reorder and archive apply to a merge run as to every run. A run with
// auto-start off is not offered a dispatch, a run enqueued behind it can be
// moved ahead of it, and a finished run filed away leaves the stream's row.
#[test]
fn auto_start_reorder_and_archive_apply_to_a_merge_run() {
    conflicted_merge("merge arrangement")
        .with_merge(as_one_commit())
        .run()
        .expect_state(GraduationRunState::Queued)
        .turning_auto_start_off()
        .offering_the_queue_a_dispatch()
        .expect_queue_offered_nothing()
        .enqueuing_on_the_stream("behind", "Write the header.")
        .expect_queue_position(0)
        .reordering_the_merge_run(0, 1)
        .expect_queue_position(1)
        .reordering_the_merge_run(1, 0)
        .expect_queue_position(0)
        .driving_the_merge(vec![
            Script::reports_success().writes("README.md", "both sides\n"),
            Script::answers_ready("Both intents stand."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_stream_row_merge_run(GraduationRunState::Completed)
        .archiving()
        .expect_archived()
        .expect_stream_row_without_a_merge_run()
        .expect_queue_offered_dispatch("behind");
}
