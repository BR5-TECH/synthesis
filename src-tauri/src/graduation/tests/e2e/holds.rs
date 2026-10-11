//! Journeys 38 to 40 of the coverage matrix: a run that stops before it
//! completes keeps its queue, a discard takes the queued runs behind it, and a
//! failed run holds its queue until it is discarded
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).
//!
//! GTE-FR-YAEB: every run a scenario enqueues beside its own has auto-start
//! off, so the production queue starts nothing and the scenario drives each
//! run through the scripted seam.

use super::merge_arrange::{as_one_commit, conflicted_merge};
use super::scenario::{Scenario, Setting, TurnScript as Script};
use crate::graduation::{GraduationInterruptionReason, GraduationRunState, ReviewSeverity};

/// A scenario with one run queued behind its own on the same stream.
fn with_a_run_behind(name: &'static str) -> Scenario {
    Scenario::named(name).with_run_on_stream("behind", "editor", "Write the header.")
}

fn revising(description: &str) -> Script {
    Script::answers_revise(ReviewSeverity::Major, description)
}

// ---------------------------------------------------------------------------
// 38 — a run that rests keeps its queue and its slot
// ---------------------------------------------------------------------------

// GTE-FR-ANSL, GTE-FR-PFEO, GTE-FR-YAEB / GRD-FR-BNTC, GRD-FR-KKKN, GRD-FR-WKMD,
// GRD-FR-MDQZ, GRD-FR-PUXO: a run at rest in `blocked`, in `awaiting_author` for
// each of its reasons, and in `interrupted` for the author's pause and for a
// shutdown holds the stream and one project slot. The run behind it takes no
// turn and stays queued.
#[test]
fn a_run_at_rest_keeps_its_queue_and_its_slot_in_every_resting_state() {
    with_a_run_behind("hold: escalation")
        .work_turn(Script::escalates(&["Which panel is meant?"]))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_stream_held()
        .expect_slot_held(true)
        .expect_cannot_start("behind");

    with_a_run_behind("hold: spent pass budget")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(revising("The empty state is not there."))
        .work_turn(Script::reports_success().writes("src/panel.ts", "2\n"))
        .review_turn(revising("The empty state is still not there."))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_rest_reason("pass_budget_exhausted", 2)
        .expect_stream_held()
        .expect_slot_held(true)
        .expect_cannot_start("behind");

    with_a_run_behind("hold: blocked")
        .with_prompt("   ")
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_stream_held()
        .expect_slot_held(true)
        .expect_cannot_start("behind");

    with_a_run_behind("hold: repeated blocker")
        .with_prompt("   ")
        .run()
        .continued(Vec::new())
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_stream_held()
        .expect_slot_held(true)
        .expect_cannot_start("behind");

    with_a_run_behind("hold: author pause")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n").pausing_the_run())
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("author_pause")
        .expect_stream_held()
        .expect_slot_held(true)
        .expect_cannot_start("behind");

    with_a_run_behind("hold: application shutdown")
        .work_turn(
            Script::reports_success()
                .writes("src/panel.ts", "1\n")
                .stopping_the_application(GraduationInterruptionReason::ApplicationShutdown),
        )
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("application_shutdown")
        .expect_stream_held()
        .expect_slot_held(true)
        .expect_cannot_start("behind");
}

// GTE-FR-ANSL, GTE-FR-DQLR / GRD-FR-BSWV, GRD-FR-BNTC: the hold is read from the
// stored run, so a relaunch frees neither the queue nor the slot.
#[test]
fn a_hold_survives_a_relaunch() {
    with_a_run_behind("hold: relaunch")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n").pausing_the_run())
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .relaunching()
        .expect_state(GraduationRunState::Interrupted)
        .expect_stream_held()
        .expect_slot_held(true)
        .expect_slots_in_use(1)
        .expect_cannot_start("behind");
}

// GTE-FR-ANSL, GTE-FR-KAZX / GRD-FR-KKKN, GRD-FR-CYIB, GRD-FR-IJKV: a run at rest
// holds the only slot of a limit of one, so a run of another stream cannot start.
// The Continue of the resting run takes no new claim on the limit and dispatches
// at once.
#[test]
fn continue_takes_no_new_claim_on_a_full_limit() {
    Scenario::named("continue under a full limit")
        .with_setting(Setting::GraduationConcurrencyLimit, 1)
        .with_run_on_stream("other", "header", "Write the footer.")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n").pausing_the_run())
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_slots_in_use(1)
        .expect_cannot_start("other")
        .continued(vec![
            Script::reports_success().writes("src/panel.ts", "2\n"),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_stream_released()
        .expect_slot_held(false)
        .expect_slots_in_use(0);
}

// ---------------------------------------------------------------------------
// 39 — a discard takes the queued runs behind it
// ---------------------------------------------------------------------------

// GTE-FR-TKTJ, GTE-FR-YAEB / GRD-FR-EWTN, GRD-FR-QCZF, GRD-FR-EPWW, DRS-FR-CFJJ: the
// discard of a run at rest removes every run queued behind it, auto-start off
// included. The run ahead of it stays, and the removed runs' drafts are unlocked.
#[test]
fn a_discard_removes_the_queued_runs_behind_it_and_keeps_the_run_ahead() {
    Scenario::named("cascade by discard")
        .with_run_on_stream("a", "editor", "Write the header.")
        .with_run_on_stream("b", "editor", "Write the footer.")
        .with_run_on_stream("c", "editor", "Write the navigation.")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .driving("a", vec![Script::escalates(&["Which header is meant?"])])
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_stream_held()
        .discarded()
        .expect_state(GraduationRunState::Discarded)
        .expect_stream_released()
        .expect_slot_held(false)
        .expect_run_removed("b")
        .expect_run_removed("c")
        .expect_previous_state(GraduationRunState::Completed);
}

// GTE-FR-TKTJ / GRD-FR-QCZF, GRD-FR-BLCR: the revert of an interrupted run ends it
// as `discarded` and removes the runs queued behind it. Its commits are reverted
// and its history is kept.
#[test]
fn a_revert_of_an_interrupted_run_removes_the_queued_runs_behind_it() {
    with_a_run_behind("cascade by revert")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n").pausing_the_run())
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_commits(1)
        .reverted()
        .expect_state(GraduationRunState::Discarded)
        .expect_history_kept_and_grown_by(1)
        .expect_stream_released()
        .expect_run_removed("behind");
}

// GTE-FR-TKTJ / GRD-FR-QCZF: the discard of a run that never held its queue
// removes no other run. The run behind it stays queued and a held run stays held.
#[test]
fn the_discard_of_a_run_that_never_held_its_queue_removes_nothing() {
    Scenario::named("discard of a waiting run")
        .with_run_on_stream("b", "editor", "Write the footer.")
        .with_run_on_stream("c", "editor", "Write the navigation.")
        .work_turn(Script::escalates(&["Which panel is meant?"]))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .discarding("b")
        .expect_state_of("b", GraduationRunState::Discarded)
        .expect_state_of("c", GraduationRunState::Queued)
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_stream_held()
        .expect_slot_held(true);
}

// ---------------------------------------------------------------------------
// 40 — a failed run holds its queue until it is discarded
// ---------------------------------------------------------------------------

// GTE-FR-MRFT, GTE-FR-RZKC, GTE-FR-PFEO / GRD-FR-RKWQ, GRD-FR-KKKN, GRD-FR-ZAMI,
// GRD-FR-QCZF: a merge run that failed on a moved tip holds the stream and no
// slot. The run behind it takes no turn, the restart is refused, and the
// author's discard releases the stream and removes the run behind it.
#[test]
fn a_failed_run_holds_its_queue_until_it_is_discarded() {
    conflicted_merge("failed run")
        .with_merge(as_one_commit())
        .run()
        .moving_the_base_tip()
        .driving_the_merge(Vec::new())
        .expect_state(GraduationRunState::Failed)
        .expect_failure_code("merge_branch_moved")
        .expect_stream_held()
        .expect_slot_held(false)
        .expect_slots_in_use(0)
        .enqueuing_on_the_stream("behind", "Write the header.")
        .expect_cannot_start("behind")
        .refusing_restart("run_state_not_permitted")
        .discarded()
        .expect_state(GraduationRunState::Discarded)
        .expect_stream_released()
        .expect_run_removed("behind");
}
