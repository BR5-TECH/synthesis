//! A run that waits on its questions moves on only by the answers or by a
//! discard (`GRD-graduation.md` GRD-FR-ZJHD, `GXD-graduation-execution.md`
//! GXD-FR-EKEU, `../ai/GRL-graduation-loop.md` GRL-FR-NVGQ).

use super::*;

/// A run of `fx` that rests in `awaiting_author` on one question.
fn escalated(fx: &Fixture) -> GraduationRun {
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");
    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::answering(Answer::Escalate(vec![
            "Which limit stands?".to_string()
        ]))]),
    );
    assert_eq!(run.state, GraduationRunState::AwaitingAuthor);
    assert!(run.escalation.is_some());
    run
}

fn one_answer() -> Vec<GraduationEscalationAnswer> {
    vec![GraduationEscalationAnswer {
        position: 1,
        answer: "The first one.".to_string(),
        summary: "The first one".to_string(),
        note: None,
    }]
}

/// The author's answers, without the loop starting behind them.
fn answer(fx: &Fixture, run: &GraduationRun) -> Result<GraduationRun, String> {
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let answered = crate::graduation::answer_graduation_escalation(
        fx.app.clone(),
        run.id.clone(),
        one_answer(),
    );
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    answered
}

// GRD-FR-ZJHD: Continue against a run that waits on its questions is refused,
// and the run, its questions and its queue stay as they were.
#[test]
fn continue_is_refused_while_the_run_waits_on_its_questions() {
    let fx = Fixture::new();
    let mut run = escalated(&fx);
    // Values a Continue would change, so a write before the refusal shows.
    run.auto_start = false;
    run.turn_time_used_ms = 1_234;
    run.checkpoint.verdict_refusals = 1;
    crate::graduation::save_run(&fx.app, &mut run).expect("saved");
    let before = serde_json::to_value(fx.reload(&run.id)).expect("the record");

    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let refusal = crate::graduation::continue_graduation_run(fx.app.clone(), run.id.clone())
        .expect_err("a run that waits on its questions does not continue");
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    assert!(refusal.starts_with(ERR_RUN_STATE_NOT_PERMITTED), "{refusal}");

    let stored = fx.reload(&run.id);
    assert_eq!(serde_json::to_value(&stored).expect("the record"), before, "the record stands");
    assert_eq!(stored.state, GraduationRunState::AwaitingAuthor);
    assert!(stored.escalation.is_some(), "the questions stand");

    // The answers still move it on.
    let answered = answer(&fx, &stored).expect("the answers");
    assert_eq!(answered.state, GraduationRunState::Queued);
    assert!(answered.escalation.is_none());
}

// GXD-FR-EKEU: answers against a run that is not in `awaiting_author` are
// refused and record nothing, also where the record still names questions.
#[test]
fn answers_are_refused_unless_the_run_waits_on_the_author() {
    for state in [
        GraduationRunState::Queued,
        GraduationRunState::Working,
        GraduationRunState::Reviewing,
        GraduationRunState::Interrupted,
        GraduationRunState::Blocked,
    ] {
        let fx = Fixture::new();
        let mut run = escalated(&fx);
        run.state = state;
        crate::graduation::save_run(&fx.app, &mut run).expect("saved");

        let refusal = answer(&fx, &run).expect_err("refused");
        assert!(refusal.starts_with(ERR_RUN_STATE_NOT_PERMITTED), "{state:?}: {refusal}");

        let stored = fx.reload(&run.id);
        assert_eq!(stored.state, state, "the state stands");
        assert!(stored.escalation.is_some(), "{state:?}: the questions stand");
        assert!(stored.checkpoint.decisions.is_empty(), "{state:?}: no decision is recorded");
        assert!(stored.checkpoint.pending_escalation_answers.is_empty());
    }
}

// GRL-FR-NVGQ: an accepted answer resets the used total time and clears any
// rest, and leaves the pass and its budget as they were.
#[test]
fn an_accepted_answer_gives_the_next_turn_the_whole_total_time() {
    let fx = Fixture::new();
    let mut run = escalated(&fx);
    run.turn_time_used_ms = 17_910_000;
    run.rest = Some(crate::graduation::GraduationRest::total_time_limit(
        crate::notes::now_rfc3339(),
        18_000_000,
        17_910_000,
    ));
    // A pass above the window's floor, so a reset budget would show.
    run.checkpoint.pass = 3;
    run.checkpoint.pass_floor = 1;
    crate::graduation::save_run(&fx.app, &mut run).expect("saved");
    let pass = run.pass();
    let floor = run.checkpoint.pass_floor;
    let limit = run.checkpoint.pass_limit;

    answer(&fx, &run).expect("the answers");

    let stored = fx.reload(&run.id);
    assert_eq!(stored.turn_time_used_ms, 0, "the used total time is reset");
    assert!(stored.rest.is_none(), "no rest outlives the answer");
    assert_eq!(stored.pass(), pass, "the pass does not advance");
    assert_eq!(stored.checkpoint.pass_floor, floor, "the pass budget is not reset");
    assert_eq!(stored.checkpoint.pass_limit, limit, "the pass window stands");
    assert_eq!(stored.checkpoint.decisions.len(), 1);
}

// GXD-FR-EKEU, GRL-FR-NVGQ: a refused answer resets nothing.
#[test]
fn a_refused_answer_keeps_the_used_total_time() {
    let fx = Fixture::new();
    let mut run = escalated(&fx);
    run.state = GraduationRunState::Working;
    run.turn_time_used_ms = 1_234;
    crate::graduation::save_run(&fx.app, &mut run).expect("saved");

    answer(&fx, &run).expect_err("refused");
    assert_eq!(fx.reload(&run.id).turn_time_used_ms, 1_234);
}

// GRD-FR-ZJHD: questions that a run outside `awaiting_author` still holds are
// dropped by Continue, and the run continues as for its state.
#[test]
fn continue_drops_questions_a_run_outside_awaiting_author_still_holds() {
    for state in [GraduationRunState::Interrupted, GraduationRunState::Blocked] {
        let fx = Fixture::new();
        let mut run = escalated(&fx);
        run.state = state;
        crate::graduation::save_run(&fx.app, &mut run).expect("saved");

        let continued = fx.continue_run(&run);
        assert_eq!(continued.state, GraduationRunState::Queued, "{state:?}");
        assert!(continued.escalation.is_none(), "{state:?}: the stale questions are dropped");
        let stored = fx.reload(&run.id);
        assert!(stored.escalation.is_none(), "{state:?}: the drop is saved");
        assert!(stored.checkpoint.decisions.is_empty(), "{state:?}: no decision is recorded");
    }
}
