//! Journeys 18 to 20 of the coverage matrix: discarding a run, restarting it,
//! reverting what it committed, and what a run that has ended refuses
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).

use super::scenario::{Scenario, TurnScript as Script};
use crate::graduation::{GraduationRunState, StandingWork};

// ---------------------------------------------------------------------------
// 18 — discard
// ---------------------------------------------------------------------------

// GTE-FR-RZKC / GRD-FR-EWTN, GXD-FR-BJYT, GRD-FR-MDQZ, GRD-FR-DXWL: a discarded
// run is terminal. Continue, the answers to the escalation it still records,
// a pause and a second discard are each refused and change nothing, and the
// draft is unlocked.
#[test]
fn a_discarded_run_refuses_continue_answers_pause_and_discard() {
    Scenario::named("refusals after a discard")
        .work_turn(Script::escalates(&["Which panel is meant?"]))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_draft_locked(true)
        .discarded()
        .expect_state(GraduationRunState::Discarded)
        .expect_draft_locked(false)
        .refusing_continue("run_state_not_permitted")
        .refusing_answers(&["The editor's."], "run_state_not_permitted")
        .refusing_pause("run_state_not_permitted")
        .refusing_discard("run_state_not_permitted")
        .expect_stream_released()
        .expect_commits(0);
}

// GTE-FR-MDVQ / GRD-FR-EWTN, GRD-FR-GMTX: a discard reverts no commit and
// removes no branch or working copy.
#[test]
fn discarding_a_run_reverts_nothing_and_keeps_its_branch_and_working_copy() {
    Scenario::named("discard keeps everything")
        .work_turn(Script::is_cancelled().writes("src/half-done.ts", "as far as it got\n"))
        .run()
        .expect_commits(1)
        .discarded()
        .expect_state(GraduationRunState::Discarded)
        .expect_commits(1)
        .expect_branch_commits_since_base(1)
        .expect_working_copy_present()
        .worktree_holds("src/half-done.ts", "as far as it got\n")
        .expect_draft_locked(false);
}

// GTE-FR-HWQM / GRD-FR-EWTN, GRL-FR-ZDKP: a turn in flight when the author
// discards the run commits nothing, in the work turn and in the review turn
// alike, and whatever the turn then answers takes nothing back out of
// `discarded`. The work stays uncommitted in the working copy.
#[test]
fn a_run_discarded_mid_turn_commits_nothing_and_leaves_the_work_uncommitted() {
    Scenario::named("discard during the work turn")
        .work_turn(
            Script::reports_success()
                .writes("src/panel.ts", "1\n")
                .discarding_the_run(),
        )
        .run()
        .expect_state(GraduationRunState::Discarded)
        .expect_no_interruption()
        .expect_commits(0)
        .expect_branch_commits_since_base(0)
        .worktree_holds("src/panel.ts", "1\n")
        .expect_draft_locked(false)
        .expect_stream_released();

    Scenario::named("discard during the review")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_ready("The work answers the prompt.").discarding_the_run())
        .run()
        .expect_state(GraduationRunState::Discarded)
        .expect_no_interruption()
        .expect_commits(0)
        .expect_branch_commits_since_base(0)
        .worktree_holds("src/panel.ts", "1\n")
        .expect_review_checkout_reclaimed()
        .expect_stream_released();

    Scenario::named("discard races a finishing work turn")
        .work_turn(
            Script::reports_success()
                .writes("src/panel.ts", "1\n")
                .discarding_the_run()
                .finishing_anyway(),
        )
        .run()
        .expect_state(GraduationRunState::Discarded)
        .expect_dispatch_count(1)
        .expect_commits(0)
        .expect_branch_commits_since_base(0)
        .expect_stream_released();

    Scenario::named("discard races an escalation")
        .work_turn(
            Script::escalates(&["Which panel is meant?"])
                .discarding_the_run()
                .finishing_anyway(),
        )
        .run()
        .expect_state(GraduationRunState::Discarded)
        .expect_no_escalation()
        .expect_commits(0);
}

// ---------------------------------------------------------------------------
// 19 — restart
// ---------------------------------------------------------------------------

// GTE-FR-TPLD / GRD-FR-ZAMI, GRD-FR-FTBQ: a restart creates a new run over the
// discarded run's captured prompt, which answers it from its first pass to
// completion. The discarded run stays discarded.
#[test]
fn a_restarted_run_answers_the_discarded_prompt_to_completion() {
    Scenario::named("restart")
        .with_execution_allowed()
        .with_prompt("Add the empty state to the panel.")
        .work_turn(Script::escalates(&["Which panel is meant?"]))
        .run()
        .discarded()
        .restarted(
            StandingWork::Keep,
            vec![
                Script::reports_success().writes("src/panel.ts", "1\n"),
                Script::answers_ready("The work answers the prompt."),
            ],
        )
        .expect_state(GraduationRunState::Completed)
        .expect_restarted_from_previous()
        .expect_previous_state(GraduationRunState::Discarded)
        .expect_dispatch(0, |d| {
            d.purpose("generate")
                .pass(1)
                .field("prompt", "Add the empty state to the panel.")
        })
        .expect_commits(1)
        .expect_committed_paths(&["src/panel.ts"])
        .expect_stream_released();
}

// GTE-FR-HWQM / GRD-FR-EWTN, GRD-FR-HQPD, GSU-FR-IRAC: what a turn left
// uncommitted when its run was discarded is standing work to the restart,
// and the restart's own choice settles it.
#[test]
fn a_restart_over_a_discarded_turns_work_takes_it_as_standing_work() {
    Scenario::named("restart over discarded work")
        .with_execution_allowed()
        .work_turn(
            Script::reports_success()
                .writes("src/left.ts", "left behind\n")
                .discarding_the_run(),
        )
        .run()
        .expect_state(GraduationRunState::Discarded)
        .worktree_holds("src/left.ts", "left behind\n")
        .restarted(
            StandingWork::Commit,
            vec![
                Script::reports_success().writes("src/panel.ts", "1\n"),
                Script::answers_ready("The work answers the prompt."),
            ],
        )
        .expect_state(GraduationRunState::Completed)
        .expect_standing_work(true, None, None)
        .expect_base_is_standing_commit()
        .expect_paths_since_run_base(&["src/panel.ts"])
        .expect_committed_paths(&["src/left.ts", "src/panel.ts"]);
}

// GTE-FR-RZKC / GRD-FR-ZAMI: a restart is accepted from `discarded` alone.
#[test]
fn a_restart_is_refused_from_any_state_but_discarded() {
    Scenario::named("restart of a resting run")
        .with_execution_allowed()
        .work_turn(Script::escalates(&["Which panel is meant?"]))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .refusing_restart("run_state_not_permitted")
        .expect_commits(0);
}

// ---------------------------------------------------------------------------
// 20 — revert, and what a run that ended refuses
// ---------------------------------------------------------------------------

// GTE-FR-MDVQ / GRD-FR-BLCR: a revert adds commits on the stream branch and
// rewrites nothing. The run's own record is unchanged.
#[test]
fn reverting_a_completed_run_adds_commits_and_rewrites_nothing() {
    Scenario::named("revert")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_commits(1)
        .reverted()
        .expect_state(GraduationRunState::Completed)
        .expect_commits(1)
        .expect_history_kept_and_grown_by(1)
        .expect_branch_head_message_contains("Revert")
        .expect_worktree_lacks("src/panel.ts");
}

// GTE-FR-MDVQ, GTE-FR-RZKC / GRD-FR-BLCR, GRD-FR-EWTN: an interrupted run and a
// discarded run that made commits each have them reverted as new commits, with
// no history rewritten. The interrupted run ends `discarded` with the revert,
// so it cannot be continued on a branch that no longer holds its work.
#[test]
fn a_resting_or_discarded_run_that_made_commits_can_be_reverted() {
    Scenario::named("revert of an interrupted run")
        .work_turn(Script::is_cancelled().writes("src/half-done.ts", "as far as it got\n"))
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_commits(1)
        .reverted()
        .expect_state(GraduationRunState::Discarded)
        .expect_commits(1)
        .expect_history_kept_and_grown_by(1)
        .expect_branch_head_message_contains("Revert")
        .expect_worktree_lacks("src/half-done.ts")
        .expect_stream_released()
        .expect_draft_locked(false)
        .refusing_continue("run_state_not_permitted");

    Scenario::named("revert of a discarded run")
        .work_turn(Script::is_cancelled().writes("src/half-done.ts", "as far as it got\n"))
        .run()
        .discarded()
        .reverted()
        .expect_state(GraduationRunState::Discarded)
        .expect_commits(1)
        .expect_history_kept_and_grown_by(1)
        .expect_branch_head_message_contains("Revert")
        .expect_worktree_lacks("src/half-done.ts");
}

// GTE-FR-RZKC / GRD-FR-BLCR, GXD-FR-BJYT: a run waiting on the author's answers
// that made commits is reverted and ends `discarded`, so its answers are
// refused afterwards.
#[test]
fn a_revert_of_a_run_that_waits_on_the_author_ends_it() {
    Scenario::named("revert of a run waiting on the author")
        .work_turn(
            Script::reports_success()
                .writes("src/half-done.ts", "as far as it got\n")
                .pausing_the_run(),
        )
        .run()
        .expect_commits(1)
        .continued(vec![Script::escalates(&["Which panel is meant?"])])
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_commits(1)
        .reverted()
        .expect_state(GraduationRunState::Discarded)
        .expect_history_kept_and_grown_by(1)
        .expect_worktree_lacks("src/half-done.ts")
        .refusing_answers(&["The editor's."], "run_state_not_permitted");
}

// GTE-FR-RZKC / GRD-FR-BLCR: a queued run that made commits refuses a revert,
// because the queue may start it on the reverted branch at any moment.
#[test]
fn a_queued_run_refuses_revert() {
    Scenario::named("revert of a queued run")
        .work_turn(Script::is_cancelled().writes("src/half-done.ts", "as far as it got\n"))
        .run()
        .expect_commits(1)
        .continuing_without_a_dispatch()
        .expect_state(GraduationRunState::Queued)
        .refusing_revert("run_state_not_permitted")
        .expect_commits(1)
        .expect_branch_commits_since_base(1);
}

// GTE-FR-RZKC / GRD-FR-BLCR: a revert is refused with `run_not_revertable`
// where the run committed nothing, and while the run holds its stream.
#[test]
fn revert_is_refused_where_nothing_was_committed_or_the_run_holds_its_stream() {
    Scenario::named("revert with nothing committed")
        .work_turn(Script::escalates(&["Which panel is meant?"]))
        .run()
        .refusing_revert("run_not_revertable");

    Scenario::named("revert of a blocked run")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_unreadably())
        .review_turn(Script::answers_unreadably())
        .run()
        .expect_state(GraduationRunState::Blocked)
        .refusing_revert("run_state_not_permitted")
        .expect_stream_held();
}

// GTE-FR-RZKC / GRD-FR-CYIB, GRD-FR-MDQZ, GRD-FR-EWTN, GXD-FR-BJYT: a completed
// run is terminal, so Continue, a pause, answers and a discard are each
// refused and change nothing.
#[test]
fn a_completed_run_refuses_continue_pause_answers_and_discard() {
    Scenario::named("refusals after completion")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .refusing_continue("run_state_not_permitted")
        .refusing_pause("run_state_not_permitted")
        .refusing_answers(&["Anything."], "run_state_not_permitted")
        .refusing_discard("run_state_not_permitted")
        .expect_commits(1)
        .expect_draft_locked(true);
}
