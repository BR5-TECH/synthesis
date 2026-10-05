//! Journey 23 of the coverage matrix: what a run does with work the author left
//! standing in its stream
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).

use super::scenario::{Scenario, TurnScript as Script};
use crate::graduation::{GraduationRunState, StandingWork};

// GTE-FR-MDVQ / GRD-FR-HQPD, GRD-FR-YBUM, GRD-FR-KDWA, GRD-FR-ARLT: `keep`
// commits nothing before the first turn. The run is measured from the revision
// the branch stands at, so the standing file is part of the run's own change
// set and its commit.
#[test]
fn keep_leaves_the_standing_work_to_the_runs_own_commit() {
    Scenario::named("standing work kept")
        .with_standing_work(StandingWork::Keep)
        .with_standing_file("notes/authored.md", "mine\n")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_standing_work(false, None, None)
        .expect_base_is_head_before_run()
        .expect_dispatch(1, |d| d.changed_paths(&["notes/authored.md", "src/panel.ts"]))
        .expect_commits(1)
        .expect_committed_paths(&["notes/authored.md", "src/panel.ts"]);
}

// GTE-FR-MDVQ / GRD-FR-HQPD, GRD-FR-RJFC, GRD-FR-KDWA: `commit` commits the
// standing work as the author's own commit, which becomes the run's base, so
// the run's commit holds only what the turn wrote.
#[test]
fn commit_makes_the_standing_work_the_base_and_keeps_it_out_of_the_runs_commit() {
    Scenario::named("standing work committed")
        .with_standing_work(StandingWork::Commit)
        .with_standing_message("Notes from the meeting")
        .with_standing_file("notes/authored.md", "mine\n")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_standing_work(true, None, None)
        .expect_base_is_standing_commit()
        .expect_dispatch(1, |d| d.changed_paths(&["src/panel.ts"]))
        .expect_commits(1)
        .expect_paths_since_run_base(&["src/panel.ts"])
        .expect_branch_commits_since_base(2);
}

// GTE-FR-MDVQ / GRD-FR-PXVJ, GRD-FR-KDWA: `commit_and_push` against a project
// with no remote commits the standing work, records the push the remote did
// not take with its typed reason, and does not stop the run.
#[test]
fn commit_and_push_without_a_remote_records_the_refused_push_and_completes() {
    Scenario::named("standing work pushed nowhere")
        .with_standing_work(StandingWork::CommitAndPush)
        .with_standing_file("notes/authored.md", "mine\n")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_blocker_absent()
        .expect_standing_work(true, Some(false), Some("no remote configured"))
        .expect_base_is_standing_commit()
        .expect_commits(1)
        .expect_paths_since_run_base(&["src/panel.ts"]);
}
