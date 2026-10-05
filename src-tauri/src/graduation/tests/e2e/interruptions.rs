//! Journeys 12 to 15 of the coverage matrix: a run the author or the
//! application stopped, a turn that ended without an answer, and a log stream
//! that could not be written
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).

use super::scenario::{Scenario, Setting, TurnScript as Script};

use crate::graduation::{
    GraduationInterruptionReason, GraduationLogStream, GraduationRunState, ReviewSeverity,
};

// ---------------------------------------------------------------------------
// 12 — the author's pause
// ---------------------------------------------------------------------------

// GTE-FR-HWQM, GTE-FR-RZKC / GRD-FR-MDQZ, GRD-FR-SWOJ, GRL-FR-ZDKP, GRL-FR-ISIL,
// GLG-FR-RLQZ: the author pauses a work turn through the one command. The run rests
// `interrupted` with the pause as its reason after the turn returns, what the
// turn wrote is committed as an abandoned turn, and Continue resumes it.
#[test]
fn an_author_pause_mid_work_rests_the_run_and_continue_resumes_it() {
    Scenario::named("author pause in the work turn")
        .work_turn(
            Script::reports_success()
                .writes("src/panel.ts", "1\n")
                .pausing_the_run(),
        )
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("author_pause")
        .expect_interruption_detail_contains("You paused this run.")
        // GLG-FR-RLQZ: a pause is no failure, so its record is at `info`.
        .expect_interrupted_record("author_pause", "info", &[])
        .expect_dispatch_count(1)
        .expect_commits(1)
        .expect_commit_message_contains(0, "Abandoned graduation turn")
        .expect_committed_paths(&["src/panel.ts"])
        .expect_checkpoint_paths(&["src/panel.ts"])
        .expect_stream_released()
        // A run at rest does no agent work, so a second pause is refused.
        .refusing_pause("run_state_not_permitted")
        .continued(vec![
            Script::reports_success().writes("src/panel.ts", "2\n"),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["work", "review"])
        .expect_dispatch(0, |d| d.purpose("resume").pass(1))
        .expect_commits(2)
        // GRD-FR-WQTN: a resumed turn that wrote something new makes a commit of
        // its own, and the abandoned commit keeps its message.
        .expect_commit_message_contains(0, "Abandoned graduation turn")
        .expect_commit_message_contains(1, "Graduated from the draft")
        .expect_committed_paths(&["src/panel.ts"])
        .worktree_holds("src/panel.ts", "2\n")
        .expect_stream_released();
}

// GTE-FR-HWQM / GRD-FR-MDQZ, GXD-FR-TJRV, GRD-FR-ARLT, GRD-FR-WQTN, GRL-FR-YKRI:
// a pause during the review resumes at the review. The work did not change
// while the run rested, so Continue composes no work turn, and the `ready`
// verdict gives the abandoned commit the graduation message rather than
// making a second commit.
#[test]
fn an_author_pause_during_the_review_resumes_at_the_review() {
    Scenario::named("author pause in the review")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_ready("The work answers the prompt.").pausing_the_run())
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("author_pause")
        .expect_resume_phase("review")
        .expect_commits(1)
        .expect_commit_message_contains(0, "Abandoned graduation turn")
        .expect_committed_paths(&["src/panel.ts"])
        .expect_review_checkout_reclaimed()
        .expect_stream_released()
        .continued(vec![Script::answers_ready("The work answers the prompt.")])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["review"])
        .expect_turns(1, 2)
        .expect_pass(1)
        .expect_dispatch(0, |d| d.changed_paths(&["src/panel.ts"]))
        // GRD-FR-ARLT / GRD-FR-WQTN: the branch head already holds the reviewed
        // tree, so no second commit follows; the one commit carries the
        // graduation message, and the run records its rewritten revision.
        .expect_commits(1)
        .expect_branch_commits_since_base(1)
        .expect_branch_head_message_contains("Graduated from the draft")
        .expect_commit_message_contains(0, "Add the empty state to the panel.")
        .expect_last_commit_is_branch_head()
        .expect_committed_paths(&["src/panel.ts"]);
}

// GTE-FR-MDVQ / GRD-FR-WQTN, GRD-FR-SWOJ: where a run made two abandoned
// commits, only the one at the branch head takes the graduation message. The
// earlier one keeps its own.
#[test]
fn only_the_abandoned_commit_at_the_head_takes_the_graduation_message() {
    Scenario::named("two abandoned commits")
        .work_turn(
            Script::reports_success()
                .writes("src/panel.ts", "1\n")
                .pausing_the_run(),
        )
        .run()
        .expect_commits(1)
        .continued(vec![
            Script::reports_success().writes("src/panel.ts", "2\n"),
            Script::answers_ready("The work answers the prompt.").pausing_the_run(),
        ])
        .expect_state(GraduationRunState::Interrupted)
        .expect_commits(2)
        .continued(vec![Script::answers_ready("The work answers the prompt.")])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["review"])
        .expect_commits(2)
        .expect_commit_message_contains(0, "Abandoned graduation turn")
        .expect_commit_message_contains(1, "Graduated from the draft")
        .expect_last_commit_is_branch_head()
        .expect_branch_commits_since_base(2)
        .worktree_holds("src/panel.ts", "2\n");
}

// GTE-FR-TPLD / GRD-FR-WQTN, GRD-FR-ARLT, GXD-FR-TJRV: a message rewrite the held
// index refuses blocks the run in the review with `commit_failed` and leaves
// the abandoned commit as it was. Once the index is free, Continue asks the
// review again and the rewrite lands.
#[test]
fn a_rewrite_the_index_refuses_blocks_the_run_and_continue_retries_it() {
    Scenario::named("rewrite refused")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_ready("The work answers the prompt.").pausing_the_run())
        .run()
        .expect_commits(1)
        .holding_the_index_while_resting()
        .continued(vec![Script::answers_ready("The work answers the prompt.")])
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker("commit_failed")
        .expect_commits(1)
        .expect_last_commit_is_branch_head()
        .expect_branch_head_message_contains("Abandoned graduation turn")
        .expect_stream_held()
        .releasing_the_index()
        .continued(vec![Script::answers_ready("The work answers the prompt.")])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["review"])
        .expect_commits(1)
        .expect_branch_head_message_contains("Graduated from the draft")
        .expect_last_commit_is_branch_head();
}

// GTE-FR-HWQM / GXD-FR-TJRV, GRL-FR-ISIL: the review resume point is spent once
// the review has run. A pause in the next pass's work turn resumes that work
// turn, and not the review.
#[test]
fn the_review_resume_point_is_spent_once_the_review_has_run() {
    Scenario::named("resume point spent")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_ready("The work answers the prompt.").pausing_the_run())
        .run()
        .expect_interruption("author_pause")
        .continued(vec![
            Script::answers_revise(ReviewSeverity::Major, "The empty state is not there."),
            Script::reports_success()
                .writes("src/panel.ts", "2\n")
                .pausing_the_run(),
        ])
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("author_pause")
        .expect_pass(2)
        .expect_dispatch_order(&["review", "work"])
        .continued(vec![
            Script::reports_success().writes("src/panel.ts", "3\n"),
            Script::answers_ready("The empty state is there now."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["work", "review"])
        .expect_dispatch(0, |d| {
            d.purpose("revise")
                .pass(2)
                .field_contains("loop_instruction", "The empty state is not there.")
        })
        .worktree_holds("src/panel.ts", "3\n");
}

// GTE-FR-HWQM / GRD-FR-MDQZ: an agent that finishes at the moment the author
// pauses does not win over the pause. The run rests paused, and the verdict
// the review returned commits nothing past the abandoned turn.
#[test]
fn a_turn_that_finishes_as_the_author_pauses_still_rests_paused() {
    Scenario::named("pause races a finishing review")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(
            Script::answers_ready("The work answers the prompt.")
                .pausing_the_run()
                .finishing_anyway(),
        )
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("author_pause")
        .expect_commits(1)
        .expect_commit_message_contains(0, "Abandoned graduation turn")
        .expect_stream_released();
}

// GTE-FR-RSQD / GRD-FR-MDQZ, GXD-FR-XEUX, GXD-FR-UZHX, GXD-FR-WJOW,
// GLG-FR-RLQZ: a pause the author made
// earlier and continued past is not the reason for a later stop nobody made. A
// timeout on the resumed run is a timeout.
#[test]
fn a_timeout_after_an_earlier_pause_is_not_called_a_pause() {
    Scenario::named("timeout after a pause")
        .work_turn(
            Script::reports_success()
                .writes("src/panel.ts", "1\n")
                .pausing_the_run(),
        )
        .run()
        .expect_interruption("author_pause")
        .continued(vec![Script::times_out().writes("src/panel.ts", "2\n")])
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("execution_timeout")
        .expect_interruption_detail_contains("reached the execution time limit")
        .expect_interrupted_records(&[("author_pause", "info"), ("execution_timeout", "error")])
        .expect_commits(2)
        .expect_stream_released();
}

// GTE-FR-RSQD / GXD-FR-UZHX, GLG-FR-RLQZ: a pause that races the time limit is
// the author's stop. The recorded pause wins over the timeout the turn
// returned, so the author does not read their own pause as a timeout.
#[test]
fn a_pause_that_races_the_time_limit_reads_as_the_pause() {
    Scenario::named("pause racing the time limit")
        .with_setting(Setting::ExecutionTimeoutMs, 1_000)
        .work_turn(
            Script::times_out()
                .writes("src/panel.ts", "1\n")
                .pausing_the_run()
                .finishing_anyway(),
        )
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("author_pause")
        .expect_interruption_detail_contains("You paused this run.")
        // GLG-FR-RLQZ: the record keeps what the turn reported, though the
        // pause is the reason.
        .expect_interrupted_record(
            "author_pause",
            "info",
            &[
                ("process_outcome", serde_json::json!("timeout")),
                ("limit_ms", serde_json::json!(1_000)),
                ("duration_ms", serde_json::json!(0)),
            ],
        )
        .expect_commits(1)
        .expect_stream_released();
}

// GTE-FR-RSQD / GXD-FR-UZHX, GLG-FR-RLQZ: a shutdown that races an agent's own
// exit is the application's stop, and the log keeps the exit code.
#[test]
fn a_shutdown_that_races_an_exit_reads_as_the_shutdown() {
    Scenario::named("shutdown racing an exit")
        .work_turn(
            Script::exits_nonzero()
                .writes("src/panel.ts", "1\n")
                .stopping_the_application(GraduationInterruptionReason::ApplicationShutdown)
                .finishing_anyway(),
        )
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("application_shutdown")
        .expect_interrupted_record(
            "application_shutdown",
            "info",
            &[
                ("process_outcome", serde_json::json!("non_zero_exit")),
                ("exit_code", serde_json::json!(1)),
            ],
        )
        .expect_stream_released();
}

// GTE-FR-HWQM / GLG-FR-RLQZ, GRD-FR-IKVE: a stop whose record cannot be stored
// rests the run on the log failure, which the author must see first. The
// turn's work is still committed.
#[test]
fn a_timeout_whose_stop_cannot_be_logged_rests_on_the_log_failure() {
    Scenario::named("timeout with the log lost")
        .work_turn(
            Script::times_out()
                .writes("src/panel.ts", "1\n")
                .losing_the_log(GraduationLogStream::Structured),
        )
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("log_persistence_failed")
        .expect_logs_failed(GraduationLogStream::Structured)
        .expect_no_interrupted_record()
        .expect_commits(1)
        .expect_stream_released();
}

// GTE-FR-RSQD / GXD-FR-WJOW, GLG-FR-RLQZ, GRL-FR-KWNP: the limit a timeout
// names is the one the turn was dispatched under, not one changed while it ran.
#[test]
fn a_timeout_names_the_limit_it_was_dispatched_under() {
    Scenario::named("limit changed while the turn ran")
        .with_setting(Setting::ExecutionTimeoutMs, 1_000)
        .work_turn(
            Script::times_out()
                .writes("src/panel.ts", "1\n")
                .changing(Setting::ExecutionTimeoutMs, 5_000),
        )
        .run()
        .expect_interruption("execution_timeout")
        .expect_interruption_detail_contains("execution time limit of 1 s")
        .expect_interrupted_record(
            "execution_timeout",
            "error",
            &[("limit_ms", serde_json::json!(1_000))],
        );
}

// GTE-FR-RSQD / GXD-FR-WJOW: a timed-out turn that wrote nothing says that
// nothing was committed.
#[test]
fn a_timeout_that_wrote_nothing_says_nothing_was_committed() {
    Scenario::named("timeout with nothing written")
        .work_turn(Script::times_out())
        .run()
        .expect_interruption("execution_timeout")
        .expect_interruption_detail_contains("The turn changed nothing, so nothing was committed.")
        .expect_commits(0);
}

// ---------------------------------------------------------------------------
// 13 — the application stops the run
// ---------------------------------------------------------------------------

// GTE-FR-HWQM / GRD-FR-TWMA, GRD-FR-SWOJ, GRD-FR-DXWL, GLG-FR-RLQZ: an
// application shutdown stops the running turn, and the run records the
// shutdown as its reason. It
// is not terminal, so its draft stays locked.
#[test]
fn an_application_shutdown_stops_the_turn_and_names_itself() {
    Scenario::named("application shutdown")
        .work_turn(
            Script::reports_success()
                .writes("src/panel.ts", "1\n")
                .stopping_the_application(GraduationInterruptionReason::ApplicationShutdown),
        )
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("application_shutdown")
        .expect_interrupted_record("application_shutdown", "info", &[])
        .expect_commits(1)
        .expect_commit_message_contains(0, "Abandoned graduation turn")
        .expect_committed_paths(&["src/panel.ts"])
        .expect_stream_released()
        .expect_draft_locked(true);
}

// GTE-FR-HWQM / GRD-FR-TWMA, GXD-FR-TJRV, GLG-FR-RLQZ: a project change during
// the review records the project change, and Continue resumes at the review.
#[test]
fn a_project_change_during_the_review_resumes_at_the_review() {
    Scenario::named("project change in the review")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(
            Script::answers_ready("The work answers the prompt.")
                .stopping_the_application(GraduationInterruptionReason::ProjectChanged),
        )
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("project_changed")
        .expect_interrupted_record("project_changed", "info", &[])
        .expect_resume_phase("review")
        .expect_stream_released()
        .continued(vec![Script::answers_ready("The work answers the prompt.")])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["review"])
        .expect_commits(1)
        .expect_commit_message_contains(0, "Graduated from the draft")
        .expect_committed_paths(&["src/panel.ts"]);
}

// GTE-FR-HWQM / GRD-FR-TWMA, GXD-FR-TJRV, GRD-FR-ARLT: an application shutdown
// during the review records the shutdown, and Continue resumes at the review
// with no empty commit after the abandoned one.
#[test]
fn an_application_shutdown_during_the_review_resumes_at_the_review() {
    Scenario::named("application shutdown in the review")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(
            Script::answers_ready("The work answers the prompt.")
                .stopping_the_application(GraduationInterruptionReason::ApplicationShutdown),
        )
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("application_shutdown")
        .expect_resume_phase("review")
        .expect_commits(1)
        .expect_stream_released()
        .continued(vec![Script::answers_ready("The work answers the prompt.")])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["review"])
        .expect_commits(1)
        .expect_branch_commits_since_base(1)
        .expect_branch_head_message_contains("Graduated from the draft");
}

// GTE-FR-HWQM, GTE-FR-YAEB / GRD-FR-TWMA, GRD-FR-XVUD, GRD-FR-BNTC: a shutdown
// stops the turns that are running and nothing else. A run resting `blocked`
// on another stream runs no turn, so it stays blocked.
#[test]
fn a_shutdown_leaves_a_blocked_run_on_another_stream_blocked() {
    Scenario::named("shutdown beside a blocked run")
        .with_setting(super::scenario::Setting::GraduationConcurrencyLimit, 2)
        .with_run_on_stream("header run", "header", "Write the header.")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_unreadably())
        .review_turn(Script::answers_unreadably())
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_stream_held()
        .driving(
            "header run",
            vec![Script::reports_success()
                .writes("src/header.ts", "1\n")
                .stopping_the_application(GraduationInterruptionReason::ApplicationShutdown)],
        )
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("application_shutdown")
        .expect_commits(1)
        .expect_committed_paths(&["src/header.ts"])
        .expect_previous_state(GraduationRunState::Blocked)
        .expect_previous_holds_its_stream();
}

// GTE-FR-HWQM / GRD-FR-XVUD, GXD-FR-QGYA: the process behind a working run is
// gone, and the first read of the queue after the relaunch rests the run with
// `execution_abandoned` and frees its stream. Continue then takes it on.
#[test]
fn a_run_abandoned_by_a_relaunch_rests_as_execution_abandoned() {
    Scenario::named("relaunch sweep")
        .work_turn(
            Script::reports_success()
                .writes("src/panel.ts", "1\n")
                .abandoned_by_a_relaunch(),
        )
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("execution_abandoned")
        .expect_stream_released()
        // What the turn left is committed as an abandoned turn, as for any
        // other stop, so the stream is usable by what runs next.
        .expect_commits(1)
        .expect_commit_message_contains(0, "Abandoned graduation turn")
        .expect_working_copy_present()
        .continued(vec![
            Script::reports_success().writes("src/panel.ts", "2\n"),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_committed_paths(&["src/panel.ts"])
        .worktree_holds("src/panel.ts", "2\n");
}

// ---------------------------------------------------------------------------
// 14 — a turn that ended without an answer
// ---------------------------------------------------------------------------

// GTE-FR-RSQD / GXD-FR-XEUX, GXD-FR-NRWD, GRD-FR-SWOJ, GXD-FR-UZHX,
// GXD-FR-WJOW, GLG-FR-RLQZ: a non-zero exit, a malformed answer and a
// terminated process each rest the run on the reason that names how the turn
// ended, with what the turn wrote committed as an abandoned turn.
#[test]
fn a_turn_that_exits_nonzero_answers_malformed_or_is_terminated_rests_with_its_work_committed() {
    let endings: [(&'static str, fn() -> Script, &str, &str, &str, Option<i32>); 3] = [
        (
            "non-zero exit",
            Script::exits_nonzero,
            "agent_exited",
            "exit code 1",
            "non_zero_exit",
            Some(1),
        ),
        (
            "malformed answer",
            Script::answers_malformed,
            "unreadable_answer",
            "not in a shape this application can read",
            "invalid_structured_output",
            Some(0),
        ),
        (
            "terminated process",
            Script::is_terminated,
            "agent_terminated",
            "stopped from outside the application",
            "terminated",
            None,
        ),
    ];
    for (name, ending, reason, detail, outcome, exit_code) in endings {
        let mut fields = vec![("process_outcome", serde_json::json!(outcome))];
        if let Some(code) = exit_code {
            fields.push(("exit_code", serde_json::json!(code)));
        }
        Scenario::named(name)
            .work_turn(ending().writes("src/half-done.ts", "as far as it got\n"))
            .run()
            .expect_state(GraduationRunState::Interrupted)
            .expect_interruption(reason)
            .expect_interruption_detail_contains(detail)
            .expect_interrupted_record(reason, "error", &fields)
            .expect_dispatch_count(1)
            .expect_commits(1)
            .expect_commit_message_contains(0, "Abandoned graduation turn")
            .expect_committed_paths(&["src/half-done.ts"])
            .expect_stream_released();
    }
}

// GTE-FR-RSQD / GRD-FR-SWOJ, GXD-FR-TJRV, GRD-FR-ARLT, GXD-FR-UZHX,
// GLG-FR-RLQZ: a review turn that runs out of time commits the work it had not yet judged as an abandoned turn. A
// timeout is not a stop the review resumes at, so Continue works again first,
// and a resumed turn that writes nothing new makes no second commit.
#[test]
fn a_review_turn_that_times_out_commits_the_unreviewed_work_and_resumes_with_a_work_turn() {
    Scenario::named("review timeout")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::times_out())
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("execution_timeout")
        .expect_interrupted_record("execution_timeout", "error", &[("process_outcome", serde_json::json!("timeout"))])
        .expect_commits(1)
        .expect_committed_paths(&["src/panel.ts"])
        .expect_review_checkout_reclaimed()
        .expect_stream_released()
        .continued(vec![
            Script::reports_success(),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["work", "review"])
        .expect_dispatch(0, |d| d.purpose("resume"))
        .expect_commits(1)
        .expect_branch_commits_since_base(1)
        .expect_branch_head_message_contains("Graduated from the draft");
}

// GTE-FR-MDVQ / GRD-FR-SWOJ, GXD-FR-WJOW: a second stop of a turn that wrote
// nothing new after the first abandoned commit makes no empty commit, and says
// so.
#[test]
fn a_second_stop_that_wrote_nothing_new_makes_no_empty_commit() {
    Scenario::named("second stop with nothing new")
        .work_turn(Script::is_cancelled().writes("src/panel.ts", "1\n"))
        .run()
        .expect_commits(1)
        .continued(vec![Script::is_cancelled()])
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption_detail_contains("The turn changed nothing, so nothing was committed.")
        .expect_commits(1)
        .expect_branch_commits_since_base(1);
}

// GTE-FR-MDVQ / GRD-FR-ARLT, GRL-FR-XNQU: a resumed turn that puts the working
// copy back to the base commits the base tree back, so the branch does not
// keep an abandoned change the reviewed work no longer holds.
#[test]
fn a_resumed_turn_that_restores_the_base_commits_the_base_back() {
    Scenario::named("resumed turn restores the base")
        .work_turn(Script::is_cancelled().writes("README.md", "changed\n"))
        .run()
        .expect_commits(1)
        .expect_committed_paths(&["README.md"])
        .continued(vec![
            Script::reports_success().writes("README.md", "seed\n"),
            Script::answers_ready("Nothing is left to change."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_commits(2)
        .expect_branch_commits_since_base(2)
        .expect_committed_paths(&[])
        .worktree_holds("README.md", "seed\n");
}

// ---------------------------------------------------------------------------
// 15 — a log stream that cannot be written
// ---------------------------------------------------------------------------

// GTE-FR-HWQM, GTE-FR-TPLD, GTE-FR-RZKC / GRD-FR-IKVE: a log stream lost while
// a turn runs rests the run `log_persistence_failed` before anything else
// happens. Continue is refused while the stream still cannot be written, and
// after the repair the run goes on to completion.
#[test]
fn a_log_stream_lost_mid_run_rests_the_run_and_continue_waits_for_the_repair() {
    Scenario::named("log stream lost")
        .work_turn(
            Script::reports_success()
                .writes("src/panel.ts", "1\n")
                .losing_the_log(GraduationLogStream::Structured),
        )
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("log_persistence_failed")
        .expect_logs_failed(GraduationLogStream::Structured)
        .expect_dispatch_count(1)
        .expect_commits(0)
        .worktree_holds("src/panel.ts", "1\n")
        .expect_stream_released()
        .refusing_continue("run_state_not_permitted")
        .repairing_the_log(GraduationLogStream::Structured)
        .continued(vec![
            Script::reports_success().writes("src/panel.ts", "2\n"),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_logs_healthy()
        .expect_commits(1)
        .expect_committed_paths(&["src/panel.ts"]);
}
