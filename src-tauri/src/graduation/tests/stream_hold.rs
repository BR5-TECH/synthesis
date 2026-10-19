//! Who holds a stream, what advances its queue, and what a stop is called
//! (`GRD-graduation.md` GRD-FR-BNTC, GRD-FR-XVUD, GRD-FR-MDQZ).

use super::*;

/// GRD-FR-BNTC: the state the application manages may dispatch.
///
/// The loop reads this before it claims a stream, so a state that answered `no`
/// would leave every queued run queued for good and nothing would say why.
#[test]
fn the_state_the_application_manages_may_dispatch_a_run() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    // Exactly what `lib.rs` manages, with only the store root moved.
    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    assert!(
        fx.try_drive(&run, dispatch.clone()),
        "the default state refuses to dispatch, so no run would ever start"
    );
    assert_eq!(dispatch.turns_taken(), 2);
}

/// GRD-FR-BNTC: a run resting `blocked` still holds its queue, so the run
/// behind it does not start on top of the work it left standing.
#[test]
fn a_blocked_run_keeps_its_queue() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    // A captured prompt of nothing is refused before any container exists
    // (GXD-FR-ODGX), which is the cheapest way to reach a blocked run.
    let blocked = fx.enqueue(&stream, "   ");
    let behind = fx.enqueue(&stream, "The run behind it.");

    let blocked = fx.drive(&blocked, ScriptedDispatch::new(Vec::new()));
    assert_eq!(blocked.state, GraduationRunState::Blocked);
    assert!(blocked.holds_queue());

    assert!(
        fx.app.state::<GraduationState>().holder_of(&stream.id).is_none(),
        "the loop that drove the run holds no claim once it has ended"
    );
    let record = crate::streams::stream_of(&fx.app, &stream.id).expect("the stream");
    assert_eq!(record.busy_run_id.as_deref(), Some(blocked.id.as_str()));

    let second = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    assert!(!fx.try_drive(&behind, second.clone()), "the run behind a blocked one waits");
    assert_eq!(second.turns_taken(), 0);
    assert_eq!(fx.reload(&behind.id).state, GraduationRunState::Queued);
}

/// GRD-FR-CYIB, GRD-FR-BNTC: clearing the condition takes no new claim. The run
/// goes back to `queued` still holding its queue and its slot, so the run behind
/// it stays behind it.
#[test]
fn continuing_a_blocked_run_keeps_its_queue() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let blocked = fx.enqueue(&stream, "   ");

    let blocked = fx.drive(&blocked, ScriptedDispatch::new(Vec::new()));
    assert_eq!(blocked.state, GraduationRunState::Blocked);

    // The dispatch the command starts is not this test's subject, so the loop
    // is turned off for the one call that would start it.
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let continued = crate::graduation::continue_graduation_run(fx.app.clone(), blocked.id.clone())
        .expect("the run is continued");
    fx.app.state::<GraduationState>().set_loop_enabled(true);

    assert_eq!(continued.state, GraduationRunState::Queued);
    assert!(continued.blocker.is_none());
    assert!(
        continued.holds_queue() && continued.holds_slot(),
        "a continued run still holds its queue and its slot"
    );
    let record = crate::streams::stream_of(&fx.app, &stream.id).expect("the stream");
    assert_eq!(record.busy_run_id.as_deref(), Some(blocked.id.as_str()), "the durable mark stays");
}

/// GRD-FR-BNTC: a run that is completed releases its stream, and the run behind
/// it starts from what it committed.
#[test]
fn a_completed_run_frees_its_stream_and_the_next_run_stacks_on_it() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let first = fx.enqueue(&stream, "Write the panel.");

    let first = fx.drive(
        &first,
        ScriptedDispatch::new(vec![Turn::work().writing("src/panel.ts", "1\n"), Turn::ready()]),
    );
    assert_eq!(first.state, GraduationRunState::Completed);
    assert!(fx.app.state::<GraduationState>().holder_of(&stream.id).is_none());
    let record = crate::streams::stream_of(&fx.app, &stream.id).expect("the stream");
    assert_eq!(record.busy_run_id, None);

    let second = fx.enqueue(&stream, "Write the header.");
    let dispatch =
        ScriptedDispatch::new(vec![Turn::work().writing("src/header.ts", "1\n"), Turn::ready()]);
    let second = fx.drive(&second, dispatch.clone());

    assert_eq!(second.state, GraduationRunState::Completed);
    assert_eq!(
        second.base_commit.as_deref(),
        Some(first.commits.last().unwrap().as_str()),
        "the second run starts from the commit the first one made"
    );
    // And the second review reads the first run's work as the ground it stands
    // on rather than as part of the change set it judges.
    let review = &dispatch.of_part("review")[0];
    assert_eq!(review.diff_paths, vec!["src/header.ts".to_string()]);
    assert!(review.tree.contains_key("src/panel.ts"));
}

/// GRD-FR-XVUD: a relaunch marks a `working` run abandoned and keeps its hold,
/// and leaves a `blocked` run exactly as the author must find it.
#[test]
fn a_relaunch_sweeps_a_working_run_and_leaves_a_blocked_one() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let blocked = fx.enqueue(&stream, "   ");
    let blocked = fx.drive(&blocked, ScriptedDispatch::new(Vec::new()));
    assert_eq!(blocked.state, GraduationRunState::Blocked);

    // A second stream, holding a run the store says is working with no loop
    // behind it — what the application leaves when it stops mid-turn.
    let other = fx.stream("header");
    let mut working = fx.enqueue(&other, "Write the header.");
    working.state = GraduationRunState::Working;
    crate::graduation::save_run(&fx.app, &mut working).expect("saved");

    let queue = crate::graduation::project_queue(&fx.app).expect("the queue");
    crate::graduation::sweep_abandoned_runs(&fx.app, &queue);

    let swept = fx.reload(&working.id);
    assert_eq!(
        swept.state,
        GraduationRunState::Interrupted,
        "a working run with no loop behind it is abandoned"
    );
    assert_eq!(
        swept.interruption.as_ref().map(|i| i.reason),
        Some(GraduationInterruptionReason::ExecutionAbandoned),
        "and the author is told why"
    );
    assert!(
        swept.holds_queue() && swept.holds_slot(),
        "and it keeps the queue and the slot it held (GRD-FR-BSWV)"
    );
    assert_eq!(
        fx.reload(&blocked.id).state,
        GraduationRunState::Blocked,
        "a blocked run is resting on a condition, not on a loop that has gone"
    );
}

// GRD-FR-XVUD, GXD-FR-ELUF: a relaunch points a run it finds reviewing at the
// review, and a run it finds working at no phase, so Continue repeats no work
// turn that already ended.
#[test]
fn a_relaunch_points_a_swept_reviewing_run_at_the_review() {
    for (state, expected) in [
        (GraduationRunState::Working, None),
        (GraduationRunState::Reviewing, Some(super::super::driver::phases::PART_REVIEW)),
    ] {
        let fx = Fixture::new();
        let stream = fx.stream("editor");
        let mut run = fx.enqueue(&stream, "Add the empty state to the panel.");
        run.state = state;
        run.checkpoint.resume_part = None;
        crate::graduation::save_run(&fx.app, &mut run).expect("saved");

        let queue = crate::graduation::project_queue(&fx.app).expect("the queue");
        crate::graduation::sweep_abandoned_runs(&fx.app, &queue);

        let swept = fx.reload(&run.id);
        assert_eq!(swept.state, GraduationRunState::Interrupted, "{state:?}: the run rests");
        assert_eq!(
            swept.interruption.as_ref().map(|i| i.reason),
            Some(GraduationInterruptionReason::ExecutionAbandoned),
            "{state:?}: the author is told why"
        );
        assert_eq!(
            swept.checkpoint.resume_part.as_deref(),
            expected,
            "{state:?}: the phase the run resumes at"
        );
        assert!(swept.holds_queue() && swept.holds_slot(), "{state:?}: the run keeps its hold");
    }
}

/// GRD-FR-MDQZ: a stop is reported as what it was. The author's pause and every
/// other stop ask different things of them.
#[test]
fn a_cancelled_turn_is_not_called_an_author_pause() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");
    let app = fx.app.clone();
    let run_id = run.id.clone();

    // The application stops the run, as a shutdown does.
    let dispatch = ScriptedDispatch::new(vec![Turn::work()
        .writing("src/panel.ts", "half\n")
        .doing(move || {
            app.state::<GraduationState>()
                .cancel_run(&run_id, GraduationInterruptionReason::ApplicationShutdown);
        })]);
    let run = fx.drive(&run, dispatch);

    let interruption = run.interruption.expect("an interruption");
    assert_eq!(interruption.reason, GraduationInterruptionReason::ApplicationShutdown);
    assert!(fx.reload(&run.id).holds_queue(), "GRD-FR-BNTC: an interrupted run holds its queue");
    let record = crate::streams::stream_of(&fx.app, &stream.id).expect("the stream");
    assert_eq!(
        record.busy_run_id.as_deref(),
        Some(run.id.as_str()),
        "the stream stays marked busy by the run at rest"
    );
    assert!(
        fx.app.state::<GraduationState>().holder_of(&stream.id).is_none(),
        "the loop that drove the run holds no claim once it has ended"
    );
}

/// GRD-FR-MDQZ: the author's own pause is reported as one.
#[test]
fn the_authors_own_pause_is_reported_as_one() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");
    let app = fx.app.clone();
    let run_id = run.id.clone();

    let dispatch = ScriptedDispatch::new(vec![Turn::work().doing(move || {
        app.state::<GraduationState>()
            .cancel_run(&run_id, GraduationInterruptionReason::AuthorPause);
    })]);
    let run = fx.drive(&run, dispatch);

    assert_eq!(run.interruption.map(|i| i.reason), Some(GraduationInterruptionReason::AuthorPause));
}

/// GRL-FR-ISIL, GXD-FR-GMDI, GXD-FR-PWYD: a run that stopped and was continued goes on with
/// the pass it was in. A resume is the loop's ordinary path, not a pass spent.
#[test]
fn a_resumed_run_continues_its_pass() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    // The first turn cannot be launched, so the run rests.
    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::answering(Answer::Unlaunchable(
            crate::tools::agent_exec::AgentExecutionError::RuntimeUnavailable,
        ))]),
    );
    assert_eq!(run.state, GraduationRunState::Interrupted);
    assert_eq!(run.pass(), 1);

    // The author continues it, and this time the work is done and reviewed.
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    crate::graduation::continue_graduation_run(fx.app.clone(), run.id.clone()).expect("continued");
    fx.app.state::<GraduationState>().set_loop_enabled(true);

    let run = fx.reload(&run.id);
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n"),
        Turn::revise(ReviewSeverity::Major, "Not yet."),
        Turn::work().writing("src/panel.ts", "2\n"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(
        run.state,
        GraduationRunState::Completed,
        "the interruption did not spend the run's fix pass"
    );
    assert_eq!(run.observability.passes.len(), 2, "two passes were made");
    let work = dispatch.of_part("work");
    assert_eq!(work[0].purpose, "resume", "a handed-back run resumes");
    assert_eq!(work[0].pass, 1);
    assert_eq!(work[1].purpose, "revise");
    assert_eq!(work[1].pass, 2);
}

/// GRL-FR-ISIL, GXD-FR-XPUR, GXD-FR-PWYD: answering an escalation continues the pass the
/// turn that asked was in.
#[test]
fn an_answered_escalation_continues_its_pass() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::answering(Answer::Escalate(vec![
            "Which limit stands?".to_string()
        ]))]),
    );
    assert_eq!(run.state, GraduationRunState::AwaitingAuthor);
    assert_eq!(run.pass(), 1);

    fx.app.state::<GraduationState>().set_loop_enabled(false);
    crate::graduation::answer_graduation_escalation(
        fx.app.clone(),
        run.id.clone(),
        vec![GraduationEscalationAnswer {
            position: 1,
            answer: "The first one.".to_string(),
            summary: "The first one".to_string(),
        }],
    )
    .expect("the answers");
    fx.app.state::<GraduationState>().set_loop_enabled(true);

    let run = fx.reload(&run.id);
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n"),
        Turn::revise(ReviewSeverity::Major, "Not yet."),
        Turn::work().writing("src/panel.ts", "2\n"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(
        run.state,
        GraduationRunState::Completed,
        "the escalation did not spend the run's fix pass"
    );
    let work = dispatch.of_part("work");
    assert_eq!(work[0].purpose, "escalation_answer");
    assert_eq!(work[0].pass, 1);
    // GXD-FR-XPUR: the answers were delivered into the turn that asked. A later
    // turn of the run neither carries them again nor resumes that turn's
    // session, and it reports the purpose it actually has.
    assert_eq!(work[1].purpose, "revise");
    assert_eq!(work[1].pass, 2);
    assert_eq!(work[1].list_len("escalation_answers"), 0, "the answers are not sent a second time");
    assert!(run.checkpoint.pending_escalation_answers.is_empty());
    assert!(run.checkpoint.resume.is_none());
}
