//! Journey 36 of the coverage matrix, continued: a discard, a pause or a lost
//! log that lands at the end of a merge run
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).
//!
//! A discard reclaims what the run owns only once no loop stands in it. A discard
//! or a pause that lands between a `ready` verdict and the apply stops the apply.
//! A log record that cannot be written after the apply does not undo the apply.

use super::merge_arrange::{as_one_commit, conflicted_merge};
use super::scenario::TurnScript as Script;
use crate::graduation::GraduationRunState;

fn settling() -> Script {
    Script::reports_success().writes("README.md", "both sides\n")
}

// GTE-FR-WXEJ, GTE-FR-HWQM, GTE-FR-DQLR / GRD-FR-EWTN, GRD-FR-KZPT, GRD-FR-PFMD,
// GRD-FR-BNTC: the author discards a working merge run while its turn still runs.
// The discard takes the stream and the slot at once, but the merge worktree, its
// registration, its scratch branch and the snapshot ref stand until the turn has
// returned. When the loop has returned all four are gone, and no branch or
// working copy was written.
#[test]
fn a_discard_of_a_working_merge_run_reclaims_nothing_until_the_turn_has_returned() {
    conflicted_merge("discard while working")
        .with_merge(as_one_commit())
        .merge_work_turn(
            Script::reports_success()
                .writes("README.md", "half done\n")
                .discarding_the_run_and_checking_what_it_owns_stands(),
        )
        .run()
        .expect_state(GraduationRunState::Discarded)
        .expect_dispatch_order(&["merge_work"])
        .expect_no_loop_inside_the_run()
        .expect_merge_workspace_reclaimed()
        .expect_no_merge_leftovers()
        .expect_stream_released()
        .expect_slots_in_use(0)
        .expect_commits(0)
        .expect_no_merge_result()
        .expect_live_state_unchanged();
}

// GTE-FR-WXEJ, GTE-FR-HWQM / GRD-FR-EWTN, GRD-FR-KZPT: the same discard during a
// review turn waits for that turn too.
#[test]
fn a_discard_of_a_reviewing_merge_run_reclaims_nothing_until_the_turn_has_returned() {
    conflicted_merge("discard while reviewing")
        .with_merge(as_one_commit())
        .merge_work_turn(settling())
        .merge_review_turn(
            Script::answers_ready("Both intents stand.")
                .discarding_the_run_and_checking_what_it_owns_stands(),
        )
        .run()
        .expect_state(GraduationRunState::Discarded)
        .expect_dispatch_order(&["merge_work", "merge_review"])
        .expect_no_loop_inside_the_run()
        .expect_merge_workspace_reclaimed()
        .expect_no_merge_result()
        .expect_live_state_unchanged();
}

// GTE-FR-WXEJ, GTE-FR-TPLD / GRD-FR-EWTN, GRD-FR-KZPT: a merge run that rests has
// no loop behind it, so the author's discard reclaims what it owns at once.
#[test]
fn a_discard_of_a_resting_merge_run_reclaims_at_once() {
    conflicted_merge("discard at rest")
        .with_merge(as_one_commit())
        .merge_work_turn(Script::escalates(&["Which limit stands?"]))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_no_loop_inside_the_run()
        .expect_merge_workspace_standing()
        .discarded()
        .expect_state(GraduationRunState::Discarded)
        .expect_merge_workspace_reclaimed()
        .expect_no_merge_leftovers()
        .expect_live_state_unchanged();
}

// GTE-FR-WXEJ, GTE-FR-HWQM / GRD-FR-EWTN, GRD-FR-AQNW, GRD-FR-KZPT: a discard that
// lands after the review wrote its `ready` verdict and before the apply stops the
// apply. Neither branch and neither working copy is written, the run records no
// result, and what it owned is reclaimed once the loop has returned.
#[test]
fn a_discard_between_the_verdict_and_the_apply_applies_nothing() {
    conflicted_merge("discard after the verdict")
        .with_merge(as_one_commit())
        .merge_work_turn(settling())
        .merge_review_turn(Script::answers_ready("Both intents stand.").discarding_the_run_after_the_verdict())
        .run()
        .expect_state(GraduationRunState::Discarded)
        .expect_dispatch_order(&["merge_work", "merge_review"])
        .expect_commits(0)
        .expect_no_merge_result()
        .expect_live_state_unchanged()
        .expect_base_branch_at_the_pinned_tip()
        .expect_base_working_copy_clean()
        .expect_stream_released()
        .expect_slots_in_use(0)
        .expect_no_loop_inside_the_run()
        .expect_merge_workspace_reclaimed();
}

// GTE-FR-WXEJ, GTE-FR-HWQM / GRD-FR-MDQZ, GRD-FR-AQNW: a pause that lands between
// the verdict and the apply stops the apply the same way. The run rests
// `interrupted`, keeps what it owns, and nothing reached either branch.
#[test]
fn a_pause_between_the_verdict_and_the_apply_applies_nothing() {
    conflicted_merge("pause after the verdict")
        .with_merge(as_one_commit())
        .merge_work_turn(settling())
        .merge_review_turn(Script::answers_ready("Both intents stand.").pausing_the_run_after_the_verdict())
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("author_pause")
        .expect_dispatch_order(&["merge_work", "merge_review"])
        .expect_commits(0)
        .expect_no_merge_result()
        .expect_live_state_unchanged()
        .expect_base_branch_at_the_pinned_tip()
        .expect_base_working_copy_clean()
        .expect_stream_released()
        .expect_slots_in_use(0)
        .expect_merge_workspace_standing();
}

// GTE-FR-WXEJ, GTE-FR-HWQM / GRD-FR-EWTN, GRD-FR-MDQZ, GRD-FR-AQNW: a turn that
// finishes with a `ready` verdict at the very moment the author discards or pauses
// the run does not apply the merge either. The author's act stands.
#[test]
fn a_ready_verdict_that_arrives_with_a_discard_or_a_pause_applies_nothing() {
    conflicted_merge("discard with the verdict")
        .with_merge(as_one_commit())
        .merge_work_turn(settling())
        .merge_review_turn(
            Script::answers_ready("Both intents stand.")
                .discarding_the_run()
                .finishing_anyway(),
        )
        .run()
        .expect_state(GraduationRunState::Discarded)
        .expect_commits(0)
        .expect_no_merge_result()
        .expect_live_state_unchanged()
        .expect_merge_workspace_reclaimed();

    conflicted_merge("pause with the verdict")
        .with_merge(as_one_commit())
        .merge_work_turn(settling())
        .merge_review_turn(
            Script::answers_ready("Both intents stand.")
                .pausing_the_run()
                .finishing_anyway(),
        )
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_commits(0)
        .expect_no_merge_result()
        .expect_live_state_unchanged()
        .expect_merge_workspace_standing();
}

// GTE-FR-VRHN, GTE-FR-CZPM / GRS-FR-WNRC, GRD-FR-AQNW, GRD-FR-IKVE: the log stream
// stops being writable after the verdict, so the record of the apply is lost. The
// merge has reached the base branch by then, so the run is `completed`, records
// its result and its commit, releases its stream and its slot, and reclaims what
// it owned. It is not left `interrupted` over a record that follows the apply.
#[test]
fn a_lost_apply_record_does_not_undo_a_completed_apply() {
    conflicted_merge("apply record lost")
        .with_merge(as_one_commit())
        .merge_work_turn(settling())
        .merge_review_turn(Script::answers_ready("Both intents stand.").losing_the_log_after_the_verdict())
        .run()
        .expect_structured_log_obstructed()
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["merge_work", "merge_review"])
        .expect_commits(1)
        .expect_base_holds_the_merge_commit("Merge the feature stream")
        .expect_merge_result("commit", true, &["README.md", "stream-only.txt"])
        .expect_base_working_copy_clean()
        .expect_stream_released()
        .expect_slots_in_use(0)
        .expect_merge_workspace_reclaimed();
}
