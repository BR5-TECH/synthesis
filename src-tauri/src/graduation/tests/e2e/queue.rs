//! Journey 24 of the coverage matrix: runs that wait on a stream, and streams
//! that work at the same time
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).
//!
//! GTE-FR-YAEB: every run a scenario enqueues beside its own has auto-start
//! off, so the production queue starts nothing and the scenario drives each
//! run through the scripted seam.

use super::scenario::{Scenario, Setting, TurnScript as Script};
use crate::graduation::GraduationRunState;

// GTE-FR-YAEB, GTE-FR-MDVQ / GRD-FR-BNTC, GRD-FR-YBUM, GRD-FR-VLFO: a run behind
// a completed run on the same stream starts from the commit that run made, and
// its review judges only its own change set.
#[test]
fn a_run_behind_a_completed_run_starts_from_its_commit() {
    Scenario::named("run behind a completed run")
        .with_run_on_stream("behind", "editor", "Write the header.")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_state_of("behind", GraduationRunState::Queued)
        .driving(
            "behind",
            vec![
                Script::reports_success().writes("src/header.ts", "1\n"),
                Script::answers_ready("The header is there."),
            ],
        )
        .expect_state(GraduationRunState::Completed)
        .expect_base_is_last_commit_of_previous()
        .expect_dispatch(1, |d| d.changed_paths(&["src/header.ts"]))
        .expect_commits(1)
        .expect_committed_paths(&["src/header.ts"]);
}

// GTE-FR-YAEB, GTE-FR-PFEO, GTE-FR-ANSL / GRD-FR-BNTC, GRL-FR-QZFB, GRD-FR-WKMD:
// a run behind a `blocked` run cannot start, because the blocked run holds the
// queue. The same condition again rests the first run for the author, and it
// keeps the queue: the run behind it still cannot start. The author's discard
// of the first run is what ends the hold, and it removes the run behind it.
#[test]
fn a_run_behind_a_blocked_run_waits_for_the_author_and_goes_with_a_discard() {
    Scenario::named("run behind a blocked run")
        .with_prompt("   ")
        .with_run_on_stream("behind", "editor", "Write the header.")
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_cannot_start("behind")
        .continued(Vec::new())
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_stream_held()
        .expect_cannot_start("behind")
        .discarded()
        .expect_state(GraduationRunState::Discarded)
        .expect_stream_released()
        .expect_run_removed("behind");
}

// GTE-FR-HWQM, GTE-FR-KAZX / GRD-FR-BNTC, WKS-FR-NDSV: under the default limit of
// one working stream, a run on a second stream cannot claim it while the first
// run works. It stays queued and works once the first run is done.
#[test]
fn the_project_stream_limit_refuses_a_second_stream_while_one_works() {
    Scenario::named("stream limit of one")
        .with_run_on_stream("header run", "header", "Write the header.")
        .work_turn(
            Script::reports_success()
                .writes("src/panel.ts", "1\n")
                .starting_another_run("header run", Vec::new()),
        )
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_other_claimed("header run", false)
        .expect_state_of("header run", GraduationRunState::Queued)
        .driving(
            "header run",
            vec![
                Script::reports_success().writes("src/header.ts", "1\n"),
                Script::answers_ready("The header is there."),
            ],
        )
        .expect_state(GraduationRunState::Completed)
        .expect_committed_paths(&["src/header.ts"]);
}

// GTE-FR-HWQM, GTE-FR-KAZX / GRD-FR-BNTC, PSS-FR-JRWC: a project that allows two
// working streams lets a run on a second stream work to completion while the
// first run is still in its turn, and each commits only its own work.
#[test]
fn a_wider_stream_limit_lets_a_second_stream_work_alongside() {
    Scenario::named("stream limit of two")
        .with_setting(Setting::GraduationConcurrencyLimit, 2)
        .with_run_on_stream("header run", "header", "Write the header.")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n").starting_another_run(
            "header run",
            vec![
                Script::reports_success().writes("src/header.ts", "1\n"),
                Script::answers_ready("The header is there."),
            ],
        ))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_other_claimed("header run", true)
        .expect_state_of("header run", GraduationRunState::Completed)
        .expect_commits(1)
        .expect_committed_paths(&["src/panel.ts"])
        .expect_stream_released();
}

// GTE-FR-HWQM, GTE-FR-TKTJ / GRD-FR-EWTN, GRD-FR-QCZF, WKS-FR-JQJA: a discard
// during a turn releases the stream in memory and on its durable record and
// removes the run queued behind it. The cancelled turn's work stays uncommitted
// in the working copy.
#[test]
fn a_run_behind_a_run_discarded_mid_turn_is_removed_and_the_turns_work_stays() {
    Scenario::named("run behind a mid-turn discard")
        .with_run_on_stream("behind", "editor", "Write the header.")
        .work_turn(
            Script::reports_success().writes("src/left.ts", "left behind\n").discarding_the_run(),
        )
        .run()
        .expect_state(GraduationRunState::Discarded)
        .expect_stream_released()
        .expect_run_removed("behind")
        .worktree_holds("src/left.ts", "left behind\n");
}
