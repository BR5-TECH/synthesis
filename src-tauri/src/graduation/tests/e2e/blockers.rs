//! Journeys 16 and 17 of the coverage matrix: a condition the author clears,
//! and a condition that repeats
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).

use super::scenario::{Scenario, TurnScript as Script};
use crate::graduation::{GraduationRunState, StandingWork};

// ---------------------------------------------------------------------------
// 16 — a commit the repository refuses
// ---------------------------------------------------------------------------

// GTE-FR-HWQM, GTE-FR-TPLD / GRD-FR-ARLT, GRD-FR-BNTC, GXD-FR-TJRV, GRL-FR-QZFB:
// a `ready` verdict whose commit the held index refuses blocks the run in the
// review. The run keeps its stream and commits nothing; once the index is
// free, Continue asks the review again and commits.
#[test]
fn a_commit_the_index_refuses_blocks_the_run_and_continue_resumes_at_the_review() {
    Scenario::named("commit refused")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_ready("The work answers the prompt.").holding_the_index())
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker("commit_failed")
        .expect_blocker_attempt(1)
        .expect_stream_held()
        .expect_commits(0)
        .expect_branch_commits_since_base(0)
        .releasing_the_index()
        .continued(vec![Script::answers_ready("The work answers the prompt.")])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["review"])
        .expect_commits(1)
        .expect_committed_paths(&["src/panel.ts"])
        .expect_stream_released();
}

// GTE-FR-TPLD / GRD-FR-HQPD, GRD-FR-YBUM, GRD-FR-KDWA: a standing commit the
// held index refuses blocks the run before any turn and moves no branch. Once
// the index is free, Continue commits the standing work as the base.
#[test]
fn a_standing_commit_the_index_refuses_blocks_before_any_turn() {
    Scenario::named("standing commit refused")
        .with_standing_work(StandingWork::Commit)
        .with_standing_file("notes/authored.md", "mine\n")
        .with_index_held()
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker("base_commit_failed")
        .expect_dispatch_count(0)
        .expect_branch_commits_since_base(0)
        .expect_stream_held()
        .releasing_the_index()
        .continued(vec![
            Script::reports_success().writes("src/panel.ts", "1\n"),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_standing_work(true, None, None)
        .expect_base_is_standing_commit()
        .expect_paths_since_run_base(&["src/panel.ts"])
        .expect_commits(1);
}

// ---------------------------------------------------------------------------
// 17 — a condition that repeats
// ---------------------------------------------------------------------------

// GTE-FR-TCUW, GTE-FR-PFEO / GRL-FR-QZFB, GRD-FR-WKMD, GXD-FR-ODGX: a blank
// prompt is refused before any container exists. The first refusal blocks
// the run and holds the stream; the same refusal after Continue rests the run
// for the author, keeps the blocker, and releases the stream. Continue from
// there starts the count over.
#[test]
fn a_blank_prompt_blocks_twice_and_rests_for_the_author_keeping_the_blocker() {
    Scenario::named("blank prompt twice")
        .with_prompt("   ")
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker("task_invalid")
        .expect_blocker_attempt(1)
        .expect_dispatch_count(0)
        .expect_stream_held()
        .continued(Vec::new())
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_blocker("task_invalid")
        .expect_blocker_attempt(2)
        .expect_no_escalation()
        .expect_dispatch_count(0)
        .expect_stream_released()
        .expect_commits(0)
        .continued(Vec::new())
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker_attempt(1);
}

// GTE-FR-TCUW / GRL-FR-QZFB, GRL-FR-TVXI, GXD-FR-TJRV: two unreadable verdicts
// after a Continue from `blocked` are the same condition twice, so the run
// rests for the author. The next Continue asks the review again.
#[test]
fn an_unreadable_verdict_repeated_after_continue_rests_the_run_for_the_author() {
    Scenario::named("unreadable verdicts repeated")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_unreadably())
        .review_turn(Script::answers_unreadably())
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker_attempt(1)
        .continued(vec![Script::answers_unreadably(), Script::answers_unreadably()])
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_blocker("review_verdict_invalid")
        .expect_blocker_attempt(2)
        .expect_dispatch_order(&["review", "review"])
        .expect_stream_released()
        .expect_commits(0)
        .continued(vec![Script::answers_ready("The work answers the prompt.")])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["review"])
        .expect_commits(1)
        .expect_committed_paths(&["src/panel.ts"]);
}

// GTE-FR-TPLD / GRD-FR-QJHM, GXD-FR-TJRV, GXD-FR-GMDI: a stream whose working
// copy went while its run rested blocks on `stream_missing` with no dispatch.
// The change set the run held is kept, and so is the phase it resumes at.
#[test]
fn a_stream_whose_working_copy_went_while_resting_blocks_on_stream_missing() {
    Scenario::named("working copy gone")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_unreadably())
        .review_turn(Script::answers_unreadably())
        .run()
        .expect_state(GraduationRunState::Blocked)
        .removing_the_working_copy()
        .continued(Vec::new())
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker("stream_missing")
        .expect_blocker_attempt(1)
        .expect_dispatch_count(0)
        .expect_checkpoint_paths(&["src/panel.ts"])
        .expect_resume_phase("review")
        .expect_commits(0)
        .expect_branch_commits_since_base(0);
}

// GTE-FR-TPLD / GRL-FR-YKRI, GXD-FR-TJRV, GRD-FR-BNTC: a review checkout the
// repository cannot make blocks the run in the review with no review
// dispatched. Once the obstruction goes, Continue asks the review again.
#[test]
fn a_review_checkout_that_cannot_be_made_blocks_and_continue_retries_the_review() {
    Scenario::named("review checkout refused")
        // A branch where the review's scratch branch needs a directory.
        .with_branch("synthesis/review")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker("review_checkout_failed")
        .expect_dispatch_count(1)
        .expect_stream_held()
        .expect_commits(0)
        .deleting_branch("synthesis/review")
        .continued(vec![Script::answers_ready("The work answers the prompt.")])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["review"])
        .expect_commits(1)
        .expect_committed_paths(&["src/panel.ts"])
        .expect_review_checkout_reclaimed();
}
