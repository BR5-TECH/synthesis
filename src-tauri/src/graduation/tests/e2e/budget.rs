//! Journey 11 of the coverage matrix, and the settings every journey runs under
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).
//!
//! The retry/pass budget is one project setting with a loop-specific meaning:
//! the graduation loop counts passes with it and the conversation loop counts
//! the physical attempts of one provider call. Both stop for the author when it
//! is spent, and both begin again with a whole budget once the author asks.

use super::scenario::{Scenario, Setting, TurnScript as Script};
use crate::graduation::{GraduationRunState, ReviewSeverity};

// ---------------------------------------------------------------------------
// 11 — the pass budget, spent and reset
// ---------------------------------------------------------------------------

// GTE-FR-KAZX, GTE-FR-WUAP / GRL-FR-ARPX, GRL-FR-XBUE, GRD-FR-CYIB: a run
// spends the project's pass budget, rests for the author with the findings that
// stand, and the author's Continue resets that budget alone and resumes without
// repeating a completed pass.
#[test]
fn a_spent_pass_budget_rests_the_run_and_continue_resets_only_that_budget() {
    Scenario::named("budget exhausted")
        .with_setting(Setting::PassBudget, 1)
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_revise(
            ReviewSeverity::Major,
            "The empty state is not there.",
        ))
        .run()
        // One pass is the whole budget, so the first `revise` verdict is
        // already the last one this window allows.
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_pass(1)
        .expect_pass_window(1, 1)
        .expect_turns(1, 1)
        .expect_dispatch_order(&["work", "review"])
        .expect_loop_instruction_contains("The empty state is not there.")
        .expect_no_escalation()
        // GRL-FR-XBUE: the run says why it rested and what it spent, so the
        // author reads a bound rather than a run that stopped for no reason.
        .expect_rest_reason("pass_budget_exhausted", 1)
        .expect_stream_released()
        .expect_commits(0)
        // GRD-FR-CYIB: Continue grants a whole budget again, from the pass
        // after the one already made.
        .continued(vec![
            Script::reports_success().writes("src/panel.ts", "2\n"),
            Script::answers_ready("The empty state is there now."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_pass(2)
        .expect_pass_window(2, 2)
        // The completed pass is not made again: the run spent two work turns
        // and two review turns across the two windows, not three of each.
        .expect_turns(2, 2)
        .expect_dispatch_order(&["work", "review"])
        .expect_committed_paths(&["src/panel.ts"])
        .expect_stream_released()
        .expect_dispatch(0, |d| {
            d.part("work")
                .pass(2)
                // The findings the run rested on reach the turn that answers
                // them, so nothing of the context is lost by the pause.
                .field_contains("loop_instruction", "The empty state is not there.");
        });
}

// GTE-FR-KAZX / GRL-FR-ARPX: a project that grants a wider budget takes more
// passes before it rests, and the window the run records is the one it ran
// under.
#[test]
fn a_wider_pass_budget_grants_more_passes_before_the_run_rests() {
    Scenario::named("wider budget")
        .with_setting(Setting::PassBudget, 3)
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_revise(ReviewSeverity::Major, "Not yet."))
        .work_turn(Script::reports_success().writes("src/panel.ts", "2\n"))
        .review_turn(Script::answers_revise(ReviewSeverity::Major, "Still not."))
        .work_turn(Script::reports_success().writes("src/panel.ts", "3\n"))
        .review_turn(Script::answers_revise(ReviewSeverity::Major, "Nor now."))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_pass(3)
        .expect_pass_window(1, 3)
        .expect_turns(3, 3)
        .expect_dispatch_count(6)
        .expect_stream_released();
}

// GTE-FR-VNOU / GRL-FR-KWNP: the settings are read before **every** dispatch,
// so a value changed between two turns of one run reaches the second of them.
#[test]
fn a_setting_changed_between_two_turns_reaches_the_second_of_them() {
    Scenario::named("settings read per dispatch")
        .with_setting(Setting::ExecutionTimeoutMs, 60_000)
        .work_turn(
            Script::reports_success()
                .writes("src/panel.ts", "1\n")
                .changing(Setting::ExecutionTimeoutMs, 120_000),
        )
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch(0, |d| d.execution_timeout_ms(60_000))
        // The review turn is dispatched after the change, so it carries the new
        // value rather than the one the run started under.
        .expect_dispatch(1, |d| d.execution_timeout_ms(120_000));
}

// GTE-FR-GBLI / GRL-FR-KWNP: a resumed run uses the values active at each
// resumed dispatch rather than the values its earlier turns ran under.
#[test]
fn a_resumed_run_uses_the_settings_active_at_the_dispatch_that_resumes_it() {
    let outcome = Scenario::named("resumed run reads the settings again")
        .with_setting(Setting::ExecutionTimeoutMs, 60_000)
        .with_setting(Setting::PassBudget, 1)
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_revise(ReviewSeverity::Major, "Not yet."))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_pass_window(1, 1)
        .expect_dispatch(0, |d| d.execution_timeout_ms(60_000));

    // The author widens both bounds while the run rests.
    super::scenario::set_setting(outcome.fixture(), Setting::ExecutionTimeoutMs, 90_000);
    super::scenario::set_setting(outcome.fixture(), Setting::PassBudget, 2);

    outcome
        .continued(vec![
            Script::reports_success().writes("src/panel.ts", "2\n"),
            Script::answers_revise(ReviewSeverity::Major, "Still not."),
            Script::reports_success().writes("src/panel.ts", "3\n"),
            Script::answers_ready("Now it is."),
        ])
        .expect_state(GraduationRunState::Completed)
        // The new budget of two is granted from the pass after the one already
        // made, so the resumed run takes passes 2 and 3.
        .expect_pass(3)
        .expect_pass_window(2, 3)
        .expect_dispatch_count(4)
        .expect_dispatch(0, |d| d.execution_timeout_ms(90_000))
        .expect_dispatch(3, |d| d.execution_timeout_ms(90_000));
}

// GTE-FR-KAZX / PSS-FR-ZLCF: a stored value outside its bounds is repaired to
// unset on read, which leaves the loop at its own default rather than at a
// bound nobody chose.
#[test]
fn a_stored_bound_outside_its_range_leaves_the_loop_at_its_own_default() {
    Scenario::named("malformed settings are repaired")
        .with_raw_setting(Setting::ExecutionTimeoutMs, 0)
        .with_raw_setting(Setting::PassBudget, -4)
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_revise(ReviewSeverity::Major, "Not yet."))
        .work_turn(Script::reports_success().writes("src/panel.ts", "2\n"))
        .review_turn(Script::answers_revise(ReviewSeverity::Major, "Still not."))
        .run()
        // The loop's own two passes, rather than none and rather than four.
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_pass_window(1, 2)
        .expect_dispatch(0, |d| {
            d.execution_timeout_ms(
                crate::graduation::driver::phases::DEFAULT_EXECUTION_TIMEOUT_MS,
            )
        });
}

// ---------------------------------------------------------------------------
// The conversation loop's half of the shared contract
// ---------------------------------------------------------------------------
//
// The retry budget is one project setting with a loop-specific meaning: the
// journeys here count passes with it, and `CVL-conversation-loop.md`'s loop
// counts the physical attempts of one provider call. That half is proved where
// that loop lives, by `crate::agent_conversations::tests::project_bounds`,
// which drives real turns under a configured project — this suite drives the
// graduation loop and nothing else (GTE-FR-QVHM).

// GTE-FR-KAZX / GRL-FR-KWNP, GRL-FR-XBUE, GXD-FR-PWYD: a budget the author
// **lowers** while a work turn runs rests the run at the next dispatch, rather
// than starting a pass they did not grant or failing the task validation.
#[test]
fn a_budget_lowered_while_a_turn_runs_rests_the_run_at_the_next_dispatch() {
    Scenario::named("budget lowered mid-run")
        .with_setting(Setting::PassBudget, 3)
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_revise(ReviewSeverity::Major, "Not yet."))
        // The author lowers the budget to one while the second work turn runs,
        // so the pass this run stands at is already past the window.
        .work_turn(
            Script::reports_success()
                .writes("src/panel.ts", "2\n")
                .changing(Setting::PassBudget, 1),
        )
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_pass(2)
        .expect_pass_window(1, 1)
        // Three dispatches: the review of the second pass is never composed,
        // because the run rested before it.
        .expect_dispatch_count(3)
        .expect_dispatch_order(&["work", "review", "work"])
        .expect_turns(2, 1)
        // GRL-FR-XBUE: it rests rather than blocking, so the author reads a
        // budget they lowered rather than an internal task fault.
        .expect_blocker_absent()
        .expect_stream_released();
}

// GTE-FR-KAZX / GRL-FR-KWNP, GRL-FR-XBUE: the same lowering, landing between a
// work turn and its review. The review
// path has a window check of its own, and it must rest the run on the same
// terms rather than refuse the task.
#[test]
fn a_budget_lowered_before_the_review_rests_the_run_rather_than_blocking_it() {
    Scenario::named("budget lowered before the review")
        .with_setting(Setting::PassBudget, 3)
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_revise(ReviewSeverity::Major, "Not yet."))
        .work_turn(Script::reports_success().writes("src/panel.ts", "2\n"))
        .review_turn(
            Script::answers_revise(ReviewSeverity::Major, "Still not.")
                .changing(Setting::PassBudget, 1),
        )
        .work_turn(Script::reports_success().writes("src/panel.ts", "3\n"))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_pass_window(1, 1)
        .expect_blocker_absent()
        .expect_stream_released();
}

// GTE-FR-GBLI / GRD-FR-CYIB, GRL-FR-XBUE: a run resumed under a budget smaller than the one
// its earlier passes ran under rests again rather than starting a pass the
// author did not grant.
#[test]
fn a_run_resumed_under_a_smaller_budget_rests_again() {
    let outcome = Scenario::named("resumed under a smaller budget")
        .with_setting(Setting::PassBudget, 2)
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_revise(ReviewSeverity::Major, "Not yet."))
        .work_turn(Script::is_cancelled().writes("src/panel.ts", "2\n"))
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_pass(2);

    super::scenario::set_setting(outcome.fixture(), Setting::PassBudget, 1);

    outcome
        .continued(Vec::new())
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_pass(2)
        .expect_pass_window(1, 1)
        // No turn is dispatched at all: the run rests before it composes one.
        .expect_dispatch_count(0)
        .expect_stream_released();
}

// GTE-FR-TCUW / GRD-FR-CYIB, GRL-FR-XBUE: Continue resets the budget for a spent
// one alone.
// A run resting on an escalation, and one resting on a repeated blocker, each
// keep the window they stand in.
#[test]
fn continue_leaves_the_window_alone_for_a_run_resting_on_something_else() {
    // An escalation: the pass and the floor are what they were.
    Scenario::named("continue against an escalation")
        .work_turn(Script::escalates(&["Which panel is meant?"]))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_pass(1)
        .expect_pass_window(1, 2)
        .answered(
            &["The editor's."],
            vec![
                Script::reports_success().writes("src/panel.ts", "1\n"),
                Script::answers_ready("The work answers the prompt."),
            ],
        )
        .expect_state(GraduationRunState::Completed)
        // The floor never moved, so the answered turn continued the pass it
        // was in rather than being granted a window of its own.
        .expect_pass(1)
        .expect_pass_window(1, 2);

    // A blocker the author continues: the window is likewise untouched.
    Scenario::named("continue against a blocker")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_unreadably())
        .review_turn(Script::answers_unreadably())
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_pass_window(1, 2)
        .continued(vec![Script::answers_ready("The work answers the prompt.")])
        .expect_state(GraduationRunState::Completed)
        .expect_pass(1)
        .expect_pass_window(1, 2);
}

// GTE-FR-TCUW / GRD-FR-CYIB, GXD-FR-GMDI: a checkpoint written before the budget
// window
// existed holds no window at all. The author's **first** Continue must still
// reset the budget, rather than spending a pass repeating work the run already
// did.
#[test]
fn a_run_recorded_before_the_window_existed_resets_on_the_first_continue() {
    let outcome = Scenario::named("a record from before the window")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_revise(ReviewSeverity::Major, "Not yet."))
        .work_turn(Script::reports_success().writes("src/panel.ts", "2\n"))
        .review_turn(Script::answers_revise(ReviewSeverity::Major, "Still not."))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_pass(2);

    // What an older build persisted: the pass and nothing about a window.
    outcome
        .rewriting_checkpoint(|checkpoint| {
            checkpoint.pass_floor = 0;
            checkpoint.pass_limit = 0;
        })
        .continued(vec![
            Script::reports_success().writes("src/panel.ts", "3\n"),
            Script::answers_ready("Now it is."),
        ])
        .expect_state(GraduationRunState::Completed)
        // Pass 3 under a window of its own, rather than pass 2 all over again.
        .expect_pass(3)
        .expect_pass_window(3, 4)
        .expect_dispatch_order(&["work", "review"])
        .expect_turns(3, 3);
}
