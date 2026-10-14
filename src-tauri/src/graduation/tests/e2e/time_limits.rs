//! Journey 41 of the coverage matrix: the total time limit of a run
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`
//! GTE-FR-ZTLC).
//!
//! A turn runs under the smaller of the stage time limit and the time the run
//! has left. A run with no time left rests for the author, keeps its stream,
//! its slot and its checkpoint, and Continue gives it its whole total again.

use super::merge_arrange::{as_one_commit, conflicted_merge};
use super::scenario::{Scenario, Setting, TurnScript as Script};
use crate::graduation::{GraduationRunState, ReviewSeverity};

const HALF_HOUR: u64 = 30 * 60 * 1000;
const HOUR: u64 = 2 * HALF_HOUR;

// GTE-FR-ZTLC / GRL-FR-EFTM, GRL-FR-WZTE, GRL-FR-JNMI, GRL-FR-SPRE,
// GRD-FR-UTTX, GRD-FR-SKLO, GRD-FR-WKMD, GXD-FR-MTVR, GXD-FR-UZHX: a turn that
// the remaining total time ends rests the run `awaiting_author` with
// `total_time_limit` rather than interrupting it. What it wrote is committed,
// and the stream is held. Continue resets the used time alone and the run
// completes in the same pass.
#[test]
fn a_spent_total_time_rests_the_run_and_continue_resets_only_the_used_time() {
    Scenario::named("total time spent")
        .with_setting(Setting::TotalTimeLimitMs, HALF_HOUR)
        .work_turn(Script::times_out().writes("src/panel.ts", "1\n"))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_total_time_rest(HALF_HOUR)
        .expect_time_used(HALF_HOUR, HALF_HOUR + 60_000)
        .expect_no_interruption()
        .expect_no_escalation()
        .expect_blocker_absent()
        .expect_pass(1)
        .expect_commits(1)
        .expect_commit_message_contains(0, "Abandoned graduation turn")
        .expect_stream_held()
        // GRL-FR-EFTM: the stage limit of three hours is larger than the total
        // of half an hour, so the turn ran under the total.
        .expect_dispatch(0, |d| d.execution_timeout_ms(HALF_HOUR))
        .continued(vec![
            Script::reports_success().writes("src/panel.ts", "2\n"),
            Script::answers_ready("The panel is there."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_no_rest()
        .expect_pass(1)
        .expect_dispatch_order(&["work", "review"])
        .expect_dispatch(0, |d| d.pass(1).execution_timeout_ms(HALF_HOUR))
        .expect_time_used(0, 60_000);
}

// GTE-FR-ZTLC / GRL-FR-JNMI: a review the total time ended resumes at the
// review, so the work is not made again.
#[test]
fn a_review_stopped_by_the_total_time_resumes_at_the_review() {
    Scenario::named("total time spent in the review")
        .with_setting(Setting::TotalTimeLimitMs, HALF_HOUR)
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::times_out())
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_total_time_rest(HALF_HOUR)
        .expect_resume_phase("review")
        .expect_stream_held()
        .continued(vec![Script::answers_ready("The panel is there.")])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["review"])
        .expect_committed_paths(&["src/panel.ts"]);
}

// GTE-FR-ZTLC / GRL-FR-WOBK, GRL-FR-KWNP, GRL-FR-WZTE: a turn that reaches the
// stage limit while time is left rests `interrupted` on `execution_timeout`.
// A total the author then lowers to what the run has used leaves no time, so
// Continue dispatches no turn and the run rests on the total time limit.
#[test]
fn a_run_with_no_total_time_left_dispatches_no_turn() {
    let outcome = Scenario::named("total lowered below the time used")
        .with_setting(Setting::StageTimeLimitMs, HALF_HOUR)
        .with_setting(Setting::TotalTimeLimitMs, 2 * HOUR)
        .work_turn(Script::times_out().writes("src/panel.ts", "1\n"))
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("execution_timeout")
        .expect_interruption_detail_contains("reached the stage time limit of 30 min")
        .expect_no_rest()
        .expect_time_used(HALF_HOUR, HALF_HOUR + 60_000);

    super::scenario::set_setting(outcome.fixture(), Setting::TotalTimeLimitMs, HALF_HOUR);

    outcome
        .continued(vec![])
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_dispatch_count(0)
        .expect_total_time_rest(HALF_HOUR)
        .expect_stream_held()
        .continued(vec![
            Script::reports_success().writes("src/panel.ts", "2\n"),
            Script::answers_ready("The panel is there."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["work", "review"]);
}

// GTE-FR-ZTLC / GRL-FR-SPRE, GRL-FR-XBUE: Continue after a spent pass budget
// resets the pass budget alone and keeps the time the run used.
#[test]
fn continue_after_a_spent_pass_budget_keeps_the_used_time() {
    Scenario::named("pass budget spent keeps the time used")
        .with_setting(Setting::PassBudget, 1)
        .with_setting(Setting::StageTimeLimitMs, HALF_HOUR)
        .work_turn(Script::times_out().writes("src/panel.ts", "1\n"))
        .run()
        .expect_interruption("execution_timeout")
        .continued(vec![
            Script::reports_success().writes("src/panel.ts", "2\n"),
            Script::answers_revise(ReviewSeverity::Major, "Not yet."),
        ])
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_rest_reason("pass_budget_exhausted", 1)
        .expect_no_rest()
        .expect_time_used(HALF_HOUR, HALF_HOUR + 60_000)
        .continued(vec![
            Script::reports_success().writes("src/panel.ts", "3\n"),
            Script::answers_ready("Now it is."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_pass(2)
        .expect_time_used(HALF_HOUR, HALF_HOUR + 60_000);
}

// GTE-FR-KAZX / GRL-FR-NUDC, GRL-FR-REPL, PSS-FR-TQMV, PSS-FR-WPKS: the shared
// execution timeout and retry budget a project stores reach no graduation
// turn. The run takes the graduation defaults of three hours and two passes.
#[test]
fn the_shared_timeout_and_retry_budget_reach_no_graduation_turn() {
    Scenario::named("shared settings ignored")
        .with_setting(Setting::SharedExecutionTimeoutMs, 60_000)
        .with_setting(Setting::SharedRetryBudget, 1)
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_revise(ReviewSeverity::Major, "Not yet."))
        .work_turn(Script::reports_success().writes("src/panel.ts", "2\n"))
        .review_turn(Script::answers_ready("Now it is."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_pass(2)
        .expect_pass_window(1, 2)
        .expect_dispatch(0, |d| d.execution_timeout_ms(3 * HOUR))
        .expect_dispatch(3, |d| d.execution_timeout_ms(3 * HOUR));
}

// GTE-FR-ZTLC / GRB-FR-VTLQ, GRL-FR-CZBT, GRD-FR-DLWB: a merge run is bounded
// by the total time too. It rests on `total_time_limit` with its stream, its
// slot and its merge worktree kept, and writes no branch.
#[test]
fn a_merge_run_rests_on_its_total_time_limit() {
    conflicted_merge("merge total time spent")
        .with_setting(Setting::TotalTimeLimitMs, HALF_HOUR)
        .with_merge(as_one_commit())
        .merge_work_turn(Script::times_out().writes("README.md", "both sides\n"))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_total_time_rest(HALF_HOUR)
        .expect_stream_held()
        .expect_slots_in_use(1)
        .expect_merge_workspace_standing()
        .expect_branches_unchanged()
        .expect_dispatch(0, |d| d.part("merge_work").execution_timeout_ms(HALF_HOUR))
        // GRB-FR-VTLQ: Continue resets the used time alone, keeps the merge
        // worktree and the pass window, and the run applies its merge.
        .continued(vec![
            Script::reports_success().writes("README.md", "both sides\n"),
            Script::answers_ready("Both intents stand."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_no_rest()
        .expect_pass(1)
        .expect_pass_window(1, 2)
        .expect_dispatch_order(&["merge_work", "merge_review"])
        .expect_time_used(0, 60_000);
}

// GTE-FR-ZTLC / GRL-FR-SPRE, GRD-FR-SKLO: a run at the last pass of its window
// that the total time stops is not a run whose pass budget is spent. Continue
// resets the used time and leaves the pass window as it stands.
#[test]
fn continue_from_the_total_time_limit_keeps_the_pass_budget() {
    Scenario::named("total time spent on the last pass")
        .with_setting(Setting::PassBudget, 1)
        .with_setting(Setting::TotalTimeLimitMs, HALF_HOUR)
        .work_turn(Script::times_out().writes("src/panel.ts", "1\n"))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_total_time_rest(HALF_HOUR)
        .expect_pass_window(1, 1)
        .continued(vec![
            Script::reports_success().writes("src/panel.ts", "2\n"),
            Script::answers_ready("The panel is there."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_pass(1)
        .expect_pass_window(1, 1)
        .expect_dispatch(0, |d| d.pass(1));
}

// GTE-FR-ZTLC / GXD-FR-UZHX, GRL-FR-WZTE: a pause that races a timeout the
// total time governs is the author's stop. The run rests paused and records
// no rest.
#[test]
fn a_pause_that_races_the_total_time_reads_as_the_pause() {
    Scenario::named("pause racing the total time")
        .with_setting(Setting::TotalTimeLimitMs, HALF_HOUR)
        .work_turn(
            Script::times_out().writes("src/panel.ts", "1\n").pausing_the_run().finishing_anyway(),
        )
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("author_pause")
        .expect_no_rest()
        .expect_stream_held();
}

// GTE-FR-ZTLC / GRL-FR-WOBK, GRL-FR-JNMI: a total the author lowers while the
// work turn runs leaves no time for the review. The review is not dispatched,
// and Continue resumes at the review.
#[test]
fn a_run_with_no_time_left_for_the_review_rests_before_it() {
    Scenario::named("no time left for the review")
        .with_setting(Setting::StageTimeLimitMs, HALF_HOUR)
        .with_setting(Setting::TotalTimeLimitMs, 2 * HOUR)
        .work_turn(Script::times_out().writes("src/panel.ts", "1\n"))
        .run()
        .expect_interruption("execution_timeout")
        .continued(vec![Script::reports_success()
            .writes("src/panel.ts", "2\n")
            .changing(Setting::TotalTimeLimitMs, HALF_HOUR)])
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_total_time_rest(HALF_HOUR)
        .expect_resume_phase("review")
        .expect_dispatch_order(&["work"])
        .continued(vec![Script::answers_ready("The panel is there.")])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["review"]);
}
