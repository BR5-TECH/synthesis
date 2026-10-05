//! Journey 32 of the coverage matrix: the pass budget of a merge run
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).
//!
//! A merge run takes two passes whatever the project configures. A spent budget
//! rests the run for the author, Continue adds two passes and keeps the merge
//! worktree, and discard reclaims what the run owns without writing a branch.

use super::merge_arrange::{as_one_commit, conflicted_merge};
use super::scenario::{Setting, TurnScript as Script};
use crate::graduation::{GraduationRunState, ReviewSeverity};

fn revising(description: &str) -> Script {
    Script::answers_revise(ReviewSeverity::Major, description)
}

// GTE-FR-HJGA, GTE-FR-KAZX / GRL-FR-CZBT, GRL-FR-XBUE, GXD-FR-BQLN, GXD-FR-PWYD,
// GRL-FR-REPL, GRL-FR-MRVK: a merge run takes two passes whatever the project's
// pass budget holds, and a budget the author changes while it works does not move
// its window. A review that still answers `revise` after the second pass, with
// minor findings alone, rests the run `awaiting_author` with the reason
// `pass_budget_exhausted`. The stream and the slot are free, the findings stand,
// and the merge worktree and the snapshot are kept.
#[test]
fn a_merge_run_takes_two_passes_whatever_the_project_budget_and_then_rests() {
    conflicted_merge("merge budget spent")
        .with_setting(Setting::PassBudget, 5)
        .with_merge(as_one_commit())
        .merge_work_turn(
            Script::reports_success()
                .writes("README.md", "both sides\n")
                .changing(Setting::PassBudget, 1),
        )
        .merge_review_turn(revising("Say what the merge kept."))
        .merge_work_turn(Script::reports_success().writes("README.md", "both sides, said\n"))
        .merge_review_turn(Script::answers_revise(ReviewSeverity::Minor, "Say it once more."))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_dispatch_order(&["merge_work", "merge_review", "merge_work", "merge_review"])
        .expect_pass(2)
        .expect_pass_window(1, 2)
        .expect_turns(2, 2)
        .expect_rest_reason("pass_budget_exhausted", 2)
        .expect_blocker_absent()
        .expect_no_escalation()
        .expect_loop_instruction_contains("Say it once more.")
        .expect_stream_released()
        .expect_slots_in_use(0)
        .expect_merge_workspace_standing()
        .expect_merge_worktree_holds("README.md", "both sides, said\n")
        .expect_no_merge_result()
        .expect_branches_unchanged()
        .expect_base_branch_at_the_pinned_tip();
}

// GTE-FR-HJGA, GTE-FR-TPLD, GTE-FR-RDPE / GRL-FR-CZBT, GXD-FR-BQLN, GRD-FR-CYIB,
// GRL-FR-XBUE: Continue after the budget is spent adds two passes. The floor moves
// to the next pass, the passes made are not made again, the findings that stood
// reach the new work turn, and the work turn finds its earlier edits in the same
// merge worktree. A `ready` review of the new pass applies the merge.
#[test]
fn continue_adds_two_passes_and_resumes_the_same_merge_worktree() {
    conflicted_merge("merge continue")
        .with_merge(as_one_commit())
        .merge_work_turn(Script::reports_success().writes("README.md", "both sides\n"))
        .merge_review_turn(revising("Say what the merge kept."))
        .merge_work_turn(Script::reports_success().writes("README.md", "both sides, said\n"))
        .merge_review_turn(revising("Say it once more."))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .continued(vec![
            Script::reports_success().writes("src/glue.ts", "glue\n"),
            Script::answers_ready("Both intents stand."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["merge_work", "merge_review"])
        .expect_dispatch(0, |d| {
            d.part("merge_work")
                .pass(3)
                .purpose("revise")
                .field_contains("loop_instruction", "Say it once more.")
                .directory_held("README.md", "both sides, said\n")
        })
        .expect_dispatch(1, |d| d.part("merge_review").pass(3))
        .expect_pass(3)
        .expect_pass_window(3, 4)
        .expect_turns(3, 3)
        .expect_base_holds("README.md", "both sides, said\n")
        .expect_base_holds("src/glue.ts", "glue\n")
        .expect_merge_workspace_reclaimed();
}

// GTE-FR-HJGA, GTE-FR-TPLD / GRL-FR-CZBT, GXD-FR-BQLN: a second exhaustion rests
// the run again, and a second Continue grants two more passes numbered after
// the four already made.
#[test]
fn continue_again_after_a_second_exhaustion_grants_two_more_passes() {
    conflicted_merge("merge continue twice")
        .with_merge(as_one_commit())
        .merge_work_turn(Script::reports_success().writes("README.md", "one\n"))
        .merge_review_turn(revising("First."))
        .merge_work_turn(Script::reports_success().writes("README.md", "two\n"))
        .merge_review_turn(revising("Second."))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .continued(vec![
            Script::reports_success().writes("README.md", "three\n"),
            revising("Third."),
            Script::reports_success().writes("README.md", "four\n"),
            revising("Fourth."),
        ])
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_pass(4)
        .expect_pass_window(3, 4)
        .expect_rest_reason_count("pass_budget_exhausted", 2)
        .expect_stream_released()
        .continued(vec![
            Script::reports_success().writes("README.md", "five\n"),
            Script::answers_ready("Both intents stand."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch(0, |d| d.part("merge_work").pass(5).field_contains("loop_instruction", "Fourth."))
        .expect_pass(5)
        .expect_pass_window(5, 6)
        .expect_base_holds("README.md", "five\n");
}

// GTE-FR-HJGA, GTE-FR-LMXV / GRD-FR-EWTN, GRD-FR-YTCR, GRD-FR-KZPT, GRD-FR-GMTX,
// WKS-FR-KHJS: discard of a run that rests on a spent budget moves it to
// `discarded`. It leaves both branches and both working copies as they were,
// and reclaims the merge worktree, its registration, the scratch branch and the
// snapshot ref. The stream's row no longer names the run.
#[test]
fn discarding_a_resting_merge_run_reclaims_what_it_owns_and_writes_no_branch() {
    conflicted_merge("merge discard")
        .with_merge(as_one_commit())
        .merge_work_turn(Script::reports_success().writes("README.md", "one\n"))
        .merge_review_turn(revising("First."))
        .merge_work_turn(Script::reports_success().writes("README.md", "two\n"))
        .merge_review_turn(revising("Second."))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_merge_workspace_standing()
        .expect_stream_row_merge_run(GraduationRunState::AwaitingAuthor)
        .discarded()
        .expect_state(GraduationRunState::Discarded)
        .expect_merge_workspace_reclaimed()
        .expect_no_merge_leftovers()
        .expect_live_state_unchanged()
        .expect_base_branch_at_the_pinned_tip()
        .expect_no_merge_result()
        .expect_commits(0)
        .expect_stream_released()
        .expect_stream_row_without_a_merge_run();
}
