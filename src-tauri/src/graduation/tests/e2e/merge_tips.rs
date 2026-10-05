//! Journey 33 of the coverage matrix: a branch tip moves under a merge run
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).
//!
//! A merge run pins both tips. A tip that moved ends the run `failed` with
//! `merge_branch_moved` before a dispatch and before the apply, and Continue and
//! an answer are refused with the same code. The run writes no branch and no
//! working copy in any of those cases.

use super::merge_arrange::{as_one_commit, conflicted_merge};
use super::scenario::TurnScript as Script;
use crate::graduation::{GraduationRunState, ReviewSeverity};

fn revising(description: &str) -> Script {
    Script::answers_revise(ReviewSeverity::Major, description)
}

// GTE-FR-KBVN, GTE-FR-LMXV / GRD-FR-XHSE: a tip that
// moved between the handoff and the first dispatch ends the run `failed` with
// `merge_branch_moved`. No turn is dispatched, no branch is written, the run
// frees its stream and its slot, and what it owned is reclaimed.
#[test]
fn a_base_tip_that_moved_before_the_first_dispatch_fails_the_run() {
    conflicted_merge("base moved before dispatch")
        .with_merge(as_one_commit())
        .run()
        .expect_state(GraduationRunState::Queued)
        .moving_the_base_tip()
        .driving_the_merge(Vec::new())
        .expect_state(GraduationRunState::Failed)
        .expect_failure_code("merge_branch_moved")
        .expect_dispatch_count(0)
        .expect_commits(0)
        .expect_no_merge_result()
        .expect_base_head_message_contains("Edit moved-on-base.txt")
        .expect_stream_branch_unchanged()
        .expect_base_working_copy_clean()
        .expect_stream_released()
        .expect_slots_in_use(0)
        .expect_merge_workspace_reclaimed()
        .expect_stream_row_merge_run(GraduationRunState::Failed);
}

// GTE-FR-KBVN / GRD-FR-XHSE: the stream's tip is pinned too, so a commit on the
// stream branch ends the run the same way, and the base branch is not written.
#[test]
fn a_stream_tip_that_moved_before_the_first_dispatch_fails_the_run() {
    conflicted_merge("stream moved before dispatch")
        .with_merge(as_one_commit())
        .run()
        .moving_the_stream_tip()
        .driving_the_merge(Vec::new())
        .expect_state(GraduationRunState::Failed)
        .expect_failure_code("merge_branch_moved")
        .expect_dispatch_count(0)
        .expect_base_branch_unchanged()
        .expect_base_working_copy_clean()
        .expect_merge_workspace_reclaimed();
}

// GTE-FR-KBVN, GTE-FR-RZKC, GTE-FR-HWQM / GRD-FR-XHSE, GRD-FR-CYIB,
// GXD-FR-BJYT: Continue and an escalation answer on a resting run
// are refused with `merge_branch_moved` and change nothing, whichever tip moved.
// The run keeps its merge worktree, and the author's discard reclaims it.
#[test]
fn continue_and_an_answer_are_refused_when_a_tip_moved() {
    conflicted_merge("refusals after a moved tip")
        .with_merge(as_one_commit())
        .merge_work_turn(Script::escalates(&["Which limit stands?"]))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .moving_the_stream_tip()
        .refusing_answers(&["The base limit stands."], "merge_branch_moved")
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_escalation_questions(&["Which limit stands?"])
        .refusing_continue("merge_branch_moved")
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_merge_workspace_standing()
        .expect_base_branch_unchanged()
        .expect_base_working_copy_clean()
        .discarded()
        .expect_state(GraduationRunState::Discarded)
        .expect_merge_workspace_reclaimed()
        .expect_base_branch_unchanged();
}

// GTE-FR-KBVN, GTE-FR-RZKC / GRD-FR-XHSE, GRD-FR-CYIB: Continue on a run that
// rests on a spent budget is refused the same way when the base tip moved.
#[test]
fn continue_after_a_spent_budget_is_refused_when_the_base_tip_moved() {
    conflicted_merge("continue after a moved base")
        .with_merge(as_one_commit())
        .merge_work_turn(Script::reports_success().writes("README.md", "one\n"))
        .merge_review_turn(revising("First."))
        .merge_work_turn(Script::reports_success().writes("README.md", "two\n"))
        .merge_review_turn(revising("Second."))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .moving_the_base_tip()
        .refusing_continue("merge_branch_moved")
        .expect_pass(2)
        .expect_pass_window(1, 2)
        .expect_merge_workspace_standing();
}

// GTE-FR-KBVN, GTE-FR-HWQM / GRD-FR-XHSE, GRL-FR-MWPQ: a tip that moves while a
// turn runs fails the run at the next dispatch. The review that would have
// followed is never dispatched, and nothing is written.
#[test]
fn a_tip_that_moves_during_a_turn_fails_the_run_at_the_next_dispatch() {
    conflicted_merge("tip moves during a turn")
        .with_merge(as_one_commit())
        .merge_work_turn(
            Script::reports_success()
                .writes("README.md", "both sides\n")
                .moving_the_base_tip_meanwhile(),
        )
        .run()
        .expect_state(GraduationRunState::Failed)
        .expect_failure_code("merge_branch_moved")
        .expect_dispatch_order(&["merge_work"])
        .expect_commits(0)
        .expect_no_merge_result()
        .expect_stream_branch_unchanged()
        .expect_base_working_copy_clean()
        .expect_stream_released()
        .expect_merge_workspace_reclaimed();
}

// GTE-FR-KBVN, GTE-FR-HWQM / GRD-FR-XHSE, GRD-FR-AQNW: a tip that moves during the
// review ends the run `failed` at the apply step, though the review answered
// `ready`. Nothing reaches the base branch or the base working copy.
#[test]
fn a_tip_that_moves_before_the_apply_fails_the_run_and_writes_nothing() {
    conflicted_merge("tip moves before apply")
        .with_merge(as_one_commit())
        .merge_work_turn(Script::reports_success().writes("README.md", "both sides\n"))
        .merge_review_turn(Script::answers_ready("Both intents stand.").moving_the_stream_tip_meanwhile())
        .run()
        .expect_state(GraduationRunState::Failed)
        .expect_failure_code("merge_branch_moved")
        .expect_dispatch_order(&["merge_work", "merge_review"])
        .expect_commits(0)
        .expect_no_merge_result()
        .expect_base_branch_unchanged()
        .expect_base_working_copy_clean()
        .expect_stream_released()
        .expect_merge_workspace_reclaimed();
}
