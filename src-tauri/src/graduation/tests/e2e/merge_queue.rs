//! Journey 31 of the coverage matrix: a merge run takes its turn from the queue
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).
//!
//! A merge run claims the stream and one project slot like every run, holds both
//! while it works, reviews or is blocked, and holds neither once it rests.

use super::merge_arrange::{as_one_commit, conflicted_merge};
use super::scenario::{Setting, TurnScript as Script};
use crate::graduation::{GraduationRunState, ReviewSeverity};

fn settling() -> Script {
    Script::reports_success().writes("README.md", "both sides\n")
}

// GTE-FR-YCQW, GTE-FR-PFEO / GRD-FR-DLWB, GRD-FR-BNTC, GRD-FR-KKKN:
// a merge run holds the stream and one project slot while it works and while it
// reviews, and holds neither once it rests in `awaiting_author`, `interrupted` or
// a terminal state.
#[test]
fn a_merge_run_holds_the_stream_and_a_slot_while_it_works_and_neither_at_rest() {
    use GraduationRunState::{Reviewing, Working};
    conflicted_merge("merge claims")
        .with_merge(as_one_commit())
        .merge_work_turn(settling().holding_the_stream_and_a_slot(Working).pausing_the_run())
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("author_pause")
        .expect_stream_released()
        .expect_slots_in_use(0)
        .continued(vec![
            settling().holding_the_stream_and_a_slot(Working),
            Script::answers_revise(ReviewSeverity::Major, "Say what the merge kept.")
                .holding_the_stream_and_a_slot(Reviewing),
            Script::reports_success().writes("README.md", "both sides, said\n"),
            Script::answers_revise(ReviewSeverity::Major, "Say it once more.")
                .holding_the_stream_and_a_slot(Reviewing),
        ])
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_stream_released()
        .expect_slots_in_use(0)
        .continued(vec![
            Script::reports_success().writes("README.md", "both sides, said twice\n"),
            Script::answers_ready("Both intents stand.").holding_the_stream_and_a_slot(Reviewing),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_stream_released()
        .expect_slots_in_use(0);
}

// GTE-FR-YCQW, GTE-FR-KAZX / GRD-FR-DLWB, GRD-FR-KKKN, GRD-FR-GRHC, GRD-FR-NYSH:
// a merge run handed off while the project's limit is full is offered no
// dispatch. It waits for a slot alone, takes no turn, and takes its turn once a
// slot is free.
#[test]
fn a_merge_run_waits_for_a_free_project_slot_when_the_limit_is_full() {
    conflicted_merge("merge waits for a slot")
        .with_slot_held_by("other")
        .with_merge(as_one_commit())
        .run()
        .expect_conflicted(&["README.md"])
        .expect_state(GraduationRunState::Queued)
        .expect_queue_offered_nothing()
        .expect_waiting_for_a_slot()
        .expect_the_merge_cannot_start()
        .expect_slots_in_use(1)
        .releasing_the_held_slots()
        .driving_the_merge(vec![
            settling().holding_the_stream_and_a_slot(GraduationRunState::Working),
            Script::answers_ready("Both intents stand."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_slots_in_use(0);
}

// GTE-FR-YCQW, GTE-FR-KAZX / GRD-FR-KKKN, PSS-FR-JRWC: a project whose limit
// allows two working streams offers the merge run its dispatch while another
// stream holds a slot.
#[test]
fn a_wider_limit_offers_the_merge_run_a_dispatch_beside_another_stream() {
    conflicted_merge("merge beside another stream")
        .with_setting(Setting::GraduationConcurrencyLimit, 2)
        .with_slot_held_by("other")
        .with_merge(as_one_commit())
        .run()
        .expect_conflicted(&["README.md"])
        .expect_queue_offered_the_merge_run();
}

// GTE-FR-YCQW, GTE-FR-HWQM, GTE-FR-YAEB / GRD-FR-DLWB, GRD-FR-BNTC, WKS-FR-RQVM:
// a run enqueued on the stream after the merge run waits for it. It cannot claim
// the stream while the merge run works, and the queue offers it a dispatch once
// the merge run rests and the stream is free.
#[test]
fn a_run_behind_a_merge_run_waits_until_the_merge_run_rests() {
    conflicted_merge("run behind a merge run")
        .with_merge(as_one_commit())
        .run()
        .expect_state(GraduationRunState::Queued)
        .enqueuing_on_the_stream("behind", "Write the header.")
        .driving_the_merge(vec![
            settling().starting_another_run("behind", Vec::new()),
            Script::answers_ready("Both intents stand."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_other_claimed("behind", false)
        .expect_state_of("behind", GraduationRunState::Queued)
        .expect_stream_released()
        .expect_queue_offered_dispatch("behind");
}

// GTE-FR-YCQW, GTE-FR-RZKC / GRD-FR-DLWB, WKS-FR-GKPX: a stream whose merge run has
// not ended refuses a second merge with `stream_busy`, whether the run waits,
// works or rests, and the refusal writes nothing.
#[test]
fn a_second_merge_is_refused_while_the_stream_holds_a_merge_run() {
    conflicted_merge("second merge")
        .with_merge(as_one_commit())
        .run()
        .expect_state(GraduationRunState::Queued)
        .merging(as_one_commit())
        .expect_merge_refused("stream_busy")
        .expect_runs_in_the_queue(1)
        .driving_the_merge(vec![
            settling(),
            Script::answers_revise(ReviewSeverity::Major, "Say what the merge kept."),
            Script::reports_success().writes("README.md", "both sides, said\n"),
            Script::answers_revise(ReviewSeverity::Major, "Say it once more."),
        ])
        .expect_state(GraduationRunState::AwaitingAuthor)
        .merging(as_one_commit())
        .expect_merge_refused("stream_busy")
        .expect_runs_in_the_queue(1)
        .expect_base_working_copy_clean();
}
