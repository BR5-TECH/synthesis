//! What a condition the author clears does to a run, and what a condition that
//! repeats does instead (`../ai/GRL-graduation-loop.md` GRL-FR-QZFB,
//! `GRD-graduation.md` GRD-FR-WKMD, `GXD-graduation-execution.md` GXD-FR-TJRV).
//!
//! The defect these stand against: a blocker with no counter offered the author
//! a Continue that repeated a deterministic fault, and `blocked` holds the
//! stream, so one run wedged its whole queue and no surface said how often it
//! had already tried.

use super::*;

/// The unreadable verdict is the one blocker a scripted turn can raise on
/// demand, so every scenario below is built from it.
fn unreadable() -> Turn {
    Turn::answering(Answer::RawResult(serde_json::json!({ "shape": "nobody can read" })))
}

/// GRL-FR-QZFB: the first block rests the run `blocked`, which holds the stream
/// so the author clears the condition against the work standing in the copy.
#[test]
fn the_first_block_rests_the_run_blocked_and_keeps_the_stream() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work(), unreadable(), unreadable()]),
    );

    assert_eq!(run.state, GraduationRunState::Blocked);
    let blocker = run.blocker.as_ref().expect("a blocker");
    assert_eq!(blocker.code, "review_verdict_invalid");
    assert_eq!(blocker.attempt, 1, "the first block is attempt one");
    assert_eq!(run.checkpoint.block_attempts, 1);
    assert_eq!(
        run.checkpoint.blocked_code.as_deref(),
        Some("review_verdict_invalid")
    );
    assert!(
        run.state.holds_stream(),
        "GRD-FR-BNTC: a blocked run holds its stream"
    );
}

/// GRL-FR-QZFB / GRD-FR-WKMD: the same condition twice rests the run for the
/// author instead, which releases the stream so the queue behind it moves.
#[test]
fn the_same_condition_twice_rests_the_run_for_the_author() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work(), unreadable(), unreadable()]),
    );
    assert_eq!(run.state, GraduationRunState::Blocked);

    let run = fx.continue_run(&run);
    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![unreadable(), unreadable()]),
    );

    assert_eq!(
        run.state,
        GraduationRunState::AwaitingAuthor,
        "a fault that repeats rests the run for the author"
    );
    let blocker = run.blocker.as_ref().expect("GRD-FR-WKMD: the blocker is kept");
    assert_eq!(blocker.code, "review_verdict_invalid");
    assert_eq!(blocker.attempt, 2);
    assert!(
        run.escalation.is_none(),
        "GRD-FR-WKMD: this shape holds no escalation"
    );
    assert!(
        !run.state.holds_stream(),
        "GRD-FR-WKMD: resting for the author releases the stream"
    );
}

/// GRD-FR-WKMD: the release is what unwedges the queue. The stream a run at
/// rest was holding stops being held by it, so the run behind is no longer
/// waiting on a fault that nobody can clear.
///
/// A captured prompt of nothing is refused before any container exists
/// (GXD-FR-ODGX), which is the cheapest way to reach the same blocker twice.
#[test]
fn the_queue_behind_a_repeated_fault_moves_again() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let first = fx.enqueue(&stream, "   ");
    let behind = fx.enqueue(&stream, "The run behind it.");

    // The first block holds the stream, so the run behind it waits.
    let first = fx.drive(&first, ScriptedDispatch::new(Vec::new()));
    assert_eq!(first.state, GraduationRunState::Blocked);
    assert_eq!(
        fx.app.state::<GraduationState>().holder_of(&stream.id).as_deref(),
        Some(first.id.as_str())
    );
    let waiting = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    assert!(
        !fx.try_drive(&behind, waiting.clone()),
        "the run behind a blocked one waits"
    );
    assert_eq!(waiting.turns_taken(), 0);

    // The same condition again rests the run for the author.
    let first = fx.continue_run(&first);
    let first = fx.drive(&first, ScriptedDispatch::new(Vec::new()));
    assert_eq!(first.state, GraduationRunState::AwaitingAuthor);

    // The stream is no longer this run's, so the queue is not wedged on it.
    assert_ne!(
        fx.app.state::<GraduationState>().holder_of(&stream.id).as_deref(),
        Some(first.id.as_str()),
        "GRD-FR-WKMD: resting for the author gives the stream back"
    );
    assert!(!first.holds_stream());

    // And the stream passed to the run behind it, which is what the release is
    // for. The scheduler takes it the moment it comes free, so the handoff is
    // what proves the queue moved rather than waiting on a fault nobody clears.
    assert_eq!(
        fx.app.state::<GraduationState>().holder_of(&stream.id).as_deref(),
        Some(behind.id.as_str()),
        "GRD-FR-WKMD: the run behind never got the stream"
    );
    let record = crate::streams::stream_of(&fx.app, &stream.id).expect("the stream");
    assert_eq!(record.busy_run_id.as_deref(), Some(behind.id.as_str()));
}

/// GRL-FR-QZFB: a block carrying a different code starts the count again, so
/// two unrelated faults do not add up to a rest the author did not earn.
#[test]
fn a_block_with_another_code_starts_the_count_again() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work(), unreadable(), unreadable()]),
    );
    assert_eq!(run.checkpoint.block_attempts, 1);

    // A different condition, reached without the loop getting past the first.
    let mut run = run;
    run.checkpoint.blocked_code = Some("stream_missing".to_string());
    run.checkpoint.block_attempts = 1;
    crate::graduation::save_run(&fx.app, &mut run).expect("saved");

    let run = fx.continue_run(&run);
    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![unreadable(), unreadable()]),
    );

    assert_eq!(run.state, GraduationRunState::Blocked, "not a second of the same");
    assert_eq!(run.blocker.as_ref().expect("a blocker").attempt, 1);
}

/// GRL-FR-QZFB: Continue from `blocked` keeps the count, so a second block on
/// the same code reaches the bound. This is the direct guard on the endless
/// loop: a reset here makes every block for ever the first one.
#[test]
fn continue_from_blocked_keeps_the_count() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "   ");

    let run = fx.drive(&run, ScriptedDispatch::new(Vec::new()));
    assert_eq!(run.checkpoint.block_attempts, 1);

    let continued = fx.continue_run(&run);
    assert_eq!(
        continued.checkpoint.block_attempts, 1,
        "the count is what Continue is one more attempt against"
    );
    assert_eq!(
        continued.checkpoint.blocked_code.as_deref(),
        Some("task_invalid")
    );
}

/// GRL-FR-QZFB: Continue from `awaiting_author` starts the bound over. The
/// author saw what stopped the run twice and asked for it again.
#[test]
fn continue_from_awaiting_author_starts_the_bound_over() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "   ");

    let run = fx.drive(&run, ScriptedDispatch::new(Vec::new()));
    let run = fx.continue_run(&run);
    let run = fx.drive(&run, ScriptedDispatch::new(Vec::new()));
    assert_eq!(run.state, GraduationRunState::AwaitingAuthor);
    assert_eq!(run.checkpoint.block_attempts, 2);

    let continued = fx.continue_run(&run);

    assert_eq!(continued.checkpoint.block_attempts, 0);
    assert!(continued.checkpoint.blocked_code.is_none());
    assert!(continued.blocker.is_none());
}

/// GRL-FR-QZFB: a turn that ran is the phase getting past what blocked it, so a
/// much later block on the same code is a first attempt rather than a second.
#[test]
fn a_turn_that_ran_clears_the_count() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work(), unreadable(), unreadable()]),
    );
    assert_eq!(run.checkpoint.block_attempts, 1);

    // Continue, and this time the review answers. The count is behind the run.
    let run = fx.continue_run(&run);
    let run = fx.drive(&run, ScriptedDispatch::new(vec![Turn::ready()]));

    assert_eq!(run.state, GraduationRunState::Completed);
    assert_eq!(run.checkpoint.block_attempts, 0);
    assert!(run.checkpoint.blocked_code.is_none());
}

/// GOB-FR-BTXN: the return to the queue is a backward move, so the author reads
/// that the run was sent round again rather than started afresh.
#[test]
fn continuing_a_blocked_run_records_a_backward_move() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work(), unreadable(), unreadable()]),
    );
    let continued = fx.continue_run(&run);

    let back = continued
        .observability
        .stage_history
        .iter()
        .find(|entry| entry.reason == crate::graduation::observability::StageReason::BlockedRetry)
        .expect("GOB-FR-BTXN: the retry is in the stage history");
    assert_eq!(back.to, crate::graduation::observability::GraduationVisualStage::Queued);
    assert_eq!(
        back.from,
        crate::graduation::observability::GraduationVisualStage::Review,
        "it goes back from the stage the run was blocked at"
    );
}

/// GXD-FR-TJRV: a run that blocked in the review resumes at the review, and
/// composes no work turn.
///
/// The work did not change while the run was blocked, so a work turn would
/// spend a whole container to decide again what nobody reported a problem with
/// — the reasoning the verdict-refusal loop already stands on, applied to the
/// path that leaves the process.
#[test]
fn a_run_blocked_in_the_review_resumes_at_the_review() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    let first = ScriptedDispatch::new(vec![Turn::work(), unreadable(), unreadable()]);
    let run = fx.drive(&run, first.clone());
    assert_eq!(run.state, GraduationRunState::Blocked);
    assert_eq!(run.work_turns, 1);
    assert_eq!(
        run.checkpoint.resume_part.as_deref(),
        Some("review"),
        "the checkpoint records the phase the run stopped in"
    );

    let run = fx.continue_run(&run);
    let again = ScriptedDispatch::new(vec![Turn::ready()]);
    let run = fx.drive(&run, again.clone());

    assert_eq!(run.state, GraduationRunState::Completed);
    assert!(
        again.of_part("work").is_empty(),
        "a work container was spent on work nobody reported a problem with"
    );
    assert_eq!(run.work_turns, 1, "the work turn count did not grow");
    assert_eq!(again.of_part("review").len(), 1);
}

/// GXD-FR-TJRV: the pointer is spent when it is read, so a later stop of the
/// same run does not skip a work turn it never blocked in.
#[test]
fn the_resume_pointer_is_spent_when_it_is_read() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work(), unreadable(), unreadable()]),
    );
    let run = fx.continue_run(&run);
    let run = fx.drive(&run, ScriptedDispatch::new(vec![Turn::ready()]));

    assert!(
        run.checkpoint.resume_part.is_none(),
        "the pointer outlived the resumption it was written for"
    );
}

/// GXD-FR-TJRV: a run that blocked before any turn resumes at the work turn,
/// which is where a record written before the pointer existed also resumes.
#[test]
fn a_run_blocked_in_the_work_phase_resumes_at_the_work_turn() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "   ");

    let run = fx.drive(&run, ScriptedDispatch::new(Vec::new()));
    assert_eq!(run.state, GraduationRunState::Blocked);
    assert_eq!(run.checkpoint.resume_part.as_deref(), Some("work"));

    let mut run = fx.continue_run(&run);
    // The captured prompt is what made it invalid, so it is repaired the way an
    // author repairing the condition would.
    run.input.prompt = "Write the panel.".to_string();
    crate::graduation::save_run(&fx.app, &mut run).expect("saved");

    let again = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    let run = fx.drive(&run, again.clone());

    assert_eq!(run.state, GraduationRunState::Completed);
    assert_eq!(again.of_part("work").len(), 1, "the work turn ran");
}

/// GXD-FR-TJRV: the skip is spent after one turn, so a resumed run works again
/// on its next pass.
///
/// The pointer is read once and cleared for the rest of the loop. Without that
/// reset every later iteration would skip its work turn too, and a run answered
/// `revise` would spend its whole pass bound reviewing work nobody changed.
#[test]
fn a_resumed_run_works_again_on_the_pass_after_the_one_it_resumed() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work(), unreadable(), unreadable()]),
    );
    assert_eq!(run.state, GraduationRunState::Blocked);
    let run = fx.continue_run(&run);

    // The resumed review asks for a revision, so the pass advances and the
    // second iteration must run a work turn of its own.
    let again = ScriptedDispatch::new(vec![
        Turn::revise(ReviewSeverity::Major, "The empty state is missing."),
        Turn::work().writing("src/empty.ts", "export const empty = 1;\n"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, again.clone());

    assert_eq!(run.state, GraduationRunState::Completed);
    assert_eq!(
        again.of_part("work").len(),
        1,
        "the resumed run skipped every work turn, not just the one it resumed past"
    );
    assert_eq!(run.work_turns, 2);
    assert!(
        run.checkpoint.changed_paths.iter().any(|p| p == "src/empty.ts"),
        "the pass-2 work turn never ran: {:?}",
        run.checkpoint.changed_paths
    );
}

/// GXD-FR-TJRV: a run resting for the author after a second block resumes at
/// the phase it stopped in, exactly as a blocked one does.
#[test]
fn a_run_resting_for_the_author_also_resumes_at_the_phase_it_stopped_in() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work(), unreadable(), unreadable()]),
    );
    let run = fx.continue_run(&run);
    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![unreadable(), unreadable()]),
    );
    assert_eq!(run.state, GraduationRunState::AwaitingAuthor);
    assert_eq!(
        run.checkpoint.resume_part.as_deref(),
        Some("review"),
        "the phase is recorded whichever state the run rests in"
    );

    let run = fx.continue_run(&run);
    let again = ScriptedDispatch::new(vec![Turn::ready()]);
    let run = fx.drive(&run, again.clone());

    assert_eq!(run.state, GraduationRunState::Completed);
    assert!(
        again.of_part("work").is_empty(),
        "a run continued from rest spent a work container it did not need"
    );
}

/// GOB-FR-BTXN: only a run that was blocked is sent round again. An
/// interruption is not a backward move, so continuing one appends nothing.
///
/// Without the gate a run the author paused would be reported as having looped,
/// and the surface would tell them they had retried something they had not.
#[test]
fn continuing_an_interrupted_run_records_no_backward_move() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::answering(Answer::Process(
            ProcessOutcome::Timeout,
        ))]),
    );
    assert_eq!(run.state, GraduationRunState::Interrupted);
    assert!(run.blocker.is_none());

    let continued = fx.continue_run(&run);

    assert!(
        !continued
            .observability
            .stage_history
            .iter()
            .any(|e| e.reason == crate::graduation::observability::StageReason::BlockedRetry),
        "an interruption was reported as a retry"
    );
}

/// GOB-FR-BTXN: a run blocked before it left the queue appends nothing either,
/// because the stage it returns to is the stage it already stands at.
#[test]
fn continuing_a_run_blocked_at_the_queue_records_no_backward_move() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "   ");

    let run = fx.drive(&run, ScriptedDispatch::new(Vec::new()));
    assert_eq!(run.state, GraduationRunState::Blocked);

    let continued = fx.continue_run(&run);

    assert!(
        !continued
            .observability
            .stage_history
            .iter()
            .any(|e| e.reason == crate::graduation::observability::StageReason::BlockedRetry),
        "a run that never left the queue was reported as having gone back to it"
    );
}
