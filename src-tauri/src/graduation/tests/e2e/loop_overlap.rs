//! A stopped loop that has not yet returned, and the author's next command
//! (GRD-FR-QWLP, GRD-FR-TNVD, GRD-FR-HRXA, GRD-FR-KSMB, GRL-FR-WCZE).
//!
//! Each hook runs inside the stopped turn, on the loop's own thread, so "the
//! old loop is still stopping" is the inside of the hook. Nothing waits on time.

use std::sync::{Arc, Mutex};

use tauri::Manager;

use super::scenario::{Scenario, TurnScript as Script};
use crate::graduation::{
    GraduationInterruptionReason, GraduationLogStream, GraduationRunState, GraduationState,
    ReviewSeverity,
};
use crate::tools::agent_exec::protocol::EnvelopeInvalid;

/// What a hook saw, for the checks after the scenario ends.
type Seen = Arc<Mutex<Vec<String>>>;

fn note(seen: &Seen, line: impl Into<String>) {
    seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(line.into());
}

fn lines(seen: &Seen) -> Vec<String> {
    seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
}

/// The script a second loop is given. A loop that claims wrongly fails the
/// scenario at once rather than waiting on a turn nobody scripted.
fn second_loop() -> Vec<Script> {
    vec![Script::reports_success().then(|_| panic!("a second loop dispatched a turn"))]
}

/// The whole stored record, for a check that nothing at all was written.
fn record_of(app: &tauri::AppHandle<super::super::Runtime>, run_id: &str) -> String {
    let run = crate::graduation::load_run(app, run_id).expect("the record");
    serde_json::to_string(&run).expect("a record")
}

// GRD-FR-QWLP, GRD-FR-TNVD, GRD-FR-CYIB, GRD-FR-HRXA, GRL-FR-WCZE: the author
// pauses a work turn and resumes the run while the paused turn still stops.
// The run waits in the queue, no second loop claims it, and a second Continue
// does not let one in either. When the old loop returns it records its commit,
// writes no state, and offers the queue the run. The run then completes.
#[test]
fn a_resume_while_the_paused_turn_still_stops_starts_no_second_loop() {
    let seen: Seen = Arc::default();
    let hook_seen = seen.clone();
    Scenario::named("resume while the paused turn stops")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n").then(move |ctx| {
            ctx.pause();
            let (looping, driving) = ctx.loop_state();
            note(&hook_seen, format!("after pause: looping={looping} driving={driving}"));
            let state = ctx.app.state::<GraduationState>();
            note(&hook_seen, format!("stopped={}", state.loop_stopped(&ctx.run_id)));
            note(&hook_seen, format!("reason={:?}", state.cancellation_reason(&ctx.run_id)));
            ctx.continue_run();
            note(&hook_seen, format!("after continue: {:?}", ctx.stored_state()));
            note(&hook_seen, format!("second loop claimed: {}", ctx.drive_self(second_loop())));
            ctx.continue_run();
            note(&hook_seen, format!("third loop claimed: {}", ctx.drive_self(second_loop())));
        }))
        .run()
        .expect_state(GraduationRunState::Queued)
        .expect_dispatch_count(1)
        // GRD-FR-HRXA: the pause's own record stands, completed by no loop.
        .expect_interruption("author_pause")
        .expect_interruption_detail_is("You paused this run.")
        .expect_commits(1)
        .expect_commit_message_contains(0, "Abandoned graduation turn")
        .expect_checkpoint_paths(&["src/panel.ts"])
        .expect_queue_offered_the_run_with_no_loop_standing()
        .driven_again(vec![
            Script::reports_success().writes("src/panel.ts", "2\n"),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["work", "review"])
        .worktree_holds("src/panel.ts", "2\n");
    assert_eq!(
        lines(&seen),
        vec![
            "after pause: looping=true driving=false".to_string(),
            "stopped=true".to_string(),
            "reason=Some(AuthorPause)".to_string(),
            "after continue: Queued".to_string(),
            "second loop claimed: false".to_string(),
            "third loop claimed: false".to_string(),
        ]
    );
}

// GRD-FR-QWLP, GRD-FR-HRXA, GRL-FR-WCZE: a shutdown stops every turn through
// the other cancellation path. The loop still knows who stopped it after its
// claim is gone, and a Continue made before it returns starts no second loop.
#[test]
fn a_continue_after_a_shutdown_waits_for_the_stopped_loop() {
    let seen: Seen = Arc::default();
    let hook_seen = seen.clone();
    Scenario::named("continue after a shutdown, while the turn stops")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n").then(move |ctx| {
            ctx.stop_application(GraduationInterruptionReason::ApplicationShutdown);
            let state = ctx.app.state::<GraduationState>();
            note(&hook_seen, format!("reason={:?}", state.cancellation_reason(&ctx.run_id)));
            ctx.continue_run();
            note(&hook_seen, format!("second loop claimed: {}", ctx.drive_self(second_loop())));
        }))
        .run()
        .expect_state(GraduationRunState::Queued)
        .expect_dispatch_count(1)
        .expect_interruption("application_shutdown")
        .expect_commits(1)
        .expect_queue_offered_the_run_with_no_loop_standing();
    assert_eq!(
        lines(&seen),
        vec![
            "reason=Some(ApplicationShutdown)".to_string(),
            "second loop claimed: false".to_string(),
        ]
    );
}

// GRD-FR-HRXA, GRD-FR-SWOJ: a turn that answers success after the pause, and
// after the author continued, takes nothing of its answer. No review runs, the
// run stays queued, and the commit of the stopped turn is still recorded.
#[test]
fn a_turn_that_finishes_after_pause_and_continue_takes_nothing_of_its_answer() {
    Scenario::named("finishing turn after pause and continue")
        .work_turn(
            Script::reports_success()
                .writes("src/panel.ts", "1\n")
                .then(|ctx| {
                    ctx.pause();
                    ctx.continue_run();
                })
                .finishing_anyway(),
        )
        .run()
        .expect_state(GraduationRunState::Queued)
        .expect_dispatch_count(1)
        .expect_interruption("author_pause")
        .expect_commits(1)
        .expect_commit_message_contains(0, "Abandoned graduation turn")
        .expect_checkpoint_paths(&["src/panel.ts"])
        .expect_queue_offered_the_run_with_no_loop_standing()
        .expect_interruption_detail_is("You paused this run.");
}

// GRD-FR-HRXA: a stopped turn that wrote nothing leaves the record the
// Continue wrote as it stands: no commit and no state.
#[test]
fn a_stopped_turn_that_wrote_nothing_changes_nothing_of_the_continued_run() {
    let seen: Seen = Arc::default();
    let hook_seen = seen.clone();
    let outcome = Scenario::named("stopped turn with no work, after pause and continue")
        .work_turn(Script::reports_success().then(move |ctx| {
            ctx.pause();
            ctx.continue_run();
            note(&hook_seen, record_of(&ctx.app, &ctx.run_id));
        }))
        .run()
        .expect_state(GraduationRunState::Queued)
        .expect_commits(0)
        .expect_queue_offered_the_run_with_no_loop_standing();
    let stored = record_of(&outcome.fx.app, outcome.run_id());
    assert_eq!(lines(&seen), vec![stored], "the loop wrote nothing");
}

// GRL-FR-WCZE, GRD-FR-KSMB, GRD-FR-HRXA: a pause and a Continue between the
// verdict and the next pass. The stopped loop dispatches no further turn and
// does not move the queued run back into agent work.
#[test]
fn a_stopped_loop_does_not_start_the_next_pass_of_a_continued_run() {
    Scenario::named("pause and continue after a revise verdict")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(
            Script::answers_revise(ReviewSeverity::Major, "The empty state is missing.")
                .then_once_the_verdict_is_recorded(|ctx| {
                    ctx.pause();
                    ctx.continue_run();
                }),
        )
        .run()
        .expect_state(GraduationRunState::Queued)
        .expect_dispatch_order(&["work", "review"])
        .expect_interruption("author_pause")
        .expect_queue_offered_the_run_with_no_loop_standing();
}

// GRD-FR-PDNY, GRD-FR-QWLP: a paused turn whose container the executor could
// not confirm removed rests the run `blocked`, also after the author resumed it,
// so no new turn starts beside a container that can still write. Nothing of the
// turn is committed, the queue offers the run no dispatch, and Continue, made
// once the author stopped the container, resumes the work.
#[test]
fn a_container_left_running_after_pause_and_resume_blocks_the_run() {
    Scenario::named("container left running after pause and resume")
        .work_turn(
            Script::reports_success()
                .writes("src/panel.ts", "1\n")
                .then(|ctx| {
                    ctx.pause();
                    ctx.continue_run();
                })
                .leaving_its_container_running(),
        )
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker("agent_container_running")
        .expect_commits(0)
        .expect_dispatch_count(1)
        .expect_stream_held()
        .continued(vec![
            Script::reports_success().writes("src/panel.ts", "2\n"),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["work", "review"]);
}

// GRD-FR-PDNY: the same rest where nothing moved the run on after the pause.
#[test]
fn a_container_left_running_after_a_pause_blocks_the_run() {
    Scenario::named("container left running after a pause")
        .work_turn(
            Script::reports_success()
                .writes("src/panel.ts", "1\n")
                .pausing_the_run()
                .leaving_its_container_running(),
        )
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker("agent_container_running")
        .expect_commits(0);
}

// GRD-FR-PDNY: a discarded run stays discarded, whatever its container did.
#[test]
fn a_container_left_running_after_a_discard_keeps_the_run_discarded() {
    Scenario::named("container left running after a discard")
        .work_turn(
            Script::reports_success()
                .writes("src/panel.ts", "1\n")
                .discarding_the_run()
                .leaving_its_container_running(),
        )
        .run()
        .expect_state(GraduationRunState::Discarded)
        .expect_commits(0);
}

// GRD-FR-HRXA, GXD-FR-TJRV, GRL-FR-WCZE: a pause and a Resume while the review
// turn stops. The run waits in the queue on the pause, no second loop claims
// it, and once the old loop returned it resumes at the review, with no work
// turn.
#[test]
fn a_resume_while_the_paused_review_still_stops_resumes_at_the_review() {
    let seen: Seen = Arc::default();
    let hook_seen = seen.clone();
    Scenario::named("resume while the paused review stops")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_ready("The work answers the prompt.").then(move |ctx| {
            ctx.pause();
            ctx.continue_run();
            note(&hook_seen, format!("second loop claimed: {}", ctx.drive_self(second_loop())));
        }))
        .run()
        .expect_state(GraduationRunState::Queued)
        .expect_dispatch_order(&["work", "review"])
        .expect_interruption("author_pause")
        .expect_interruption_detail_is("You paused this run.")
        .expect_queue_offered_the_run_with_no_loop_standing()
        .driven_again(vec![Script::answers_ready("The work answers the prompt.")])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["review"]);
    assert_eq!(lines(&seen), vec!["second loop claimed: false".to_string()]);
}

// GRD-FR-QWLP, GRD-FR-EWTN: a discard frees the queue while the discarded
// run's turn still stops. The next run of the same stream is not claimed
// until that loop returned, and the loop's end offers it the queue.
#[test]
fn the_next_run_of_a_queue_waits_for_the_discarded_runs_loop() {
    let seen: Seen = Arc::default();
    let hook_seen = seen.clone();
    Scenario::named("next run while the discarded run's turn stops")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n").then(move |ctx| {
            ctx.discard();
            ctx.enqueue_beside("next", "Add the header.");
            note(&hook_seen, format!("next claimed: {}", ctx.drive_other("next", second_loop())));
        }))
        .run()
        .expect_state(GraduationRunState::Discarded)
        .expect_queue_offered_dispatch("next")
        .expect_no_loop_stood_at_the_offers()
        .driving(
            "next",
            vec![
                Script::reports_success().writes("src/header.ts", "1\n"),
                Script::answers_ready("The work answers the prompt."),
            ],
        )
        .expect_state(GraduationRunState::Completed);
    assert_eq!(lines(&seen), vec!["next claimed: false".to_string()]);
}

// GRD-FR-PDNY, GRL-FR-XSFA: a refused answer whose container the executor
// could not confirm removed is not corrected. No second turn starts beside a
// container that may still run, and the run rests on that container.
#[test]
fn a_refused_answer_with_its_container_left_running_is_not_corrected() {
    Scenario::named("refused answer, container left running")
        .work_turn(
            Script::answers_refused(EnvelopeInvalid::EscalationShape)
                .writes("src/panel.ts", "1\n")
                .leaving_its_container_running(),
        )
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker("agent_container_running")
        .expect_dispatch_count(1)
        .expect_commits(0);
}

// GRD-FR-PDNY, EAC-FR-ZVRP, GRD-FR-IKVE: a turn that ends on a log stream
// failure and whose container the executor could not confirm removed rests the
// run on the container, and the log failure stays on the record for Continue.
#[test]
fn a_log_failure_with_its_container_left_running_blocks_the_run() {
    Scenario::named("log failure, container left running")
        .work_turn(
            Script::reports_success()
                .writes("src/panel.ts", "1\n")
                .losing_the_log(GraduationLogStream::Activity)
                .leaving_its_container_running(),
        )
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker("agent_container_running")
        .expect_logs_failed(GraduationLogStream::Activity)
        .expect_commits(0);
}
