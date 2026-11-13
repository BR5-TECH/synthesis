//! What Continue does with the rest of a run and its check gate
//! (`../core/GRD-graduation.md` GRD-FR-OERK, `../ai/GRL-graduation-loop.md`
//! GRL-FR-SPRE, GRL-FR-LNIZ).

use super::*;
use crate::graduation::{
    GraduationCheckCause as Cause, GraduationCheckGate, GraduationCheckGateStep, GraduationRest,
};

fn resting(rest: GraduationRest, gate: Option<Cause>) -> GraduationRun {
    let mut run = GraduationRun::new_for_test("g1", "d1", "2026-11-08T09:00:00Z");
    run.state = GraduationRunState::AwaitingAuthor;
    run.turn_time_used_ms = 5_000;
    run.rest = Some(rest);
    run.checkpoint.check_gate = gate.map(|cause| GraduationCheckGate {
        cause,
        step: GraduationCheckGateStep::Escalate,
        resume: None,
    });
    run
}

fn checks_rest(cause: Option<Cause>) -> GraduationRest {
    let mut rest = GraduationRest::checks_not_passed("t0".into(), Cause::ChecksRed, Vec::new());
    rest.cause = cause.map(Into::into);
    rest
}

// GRD-FR-OERK, GRL-FR-LNIZ: Continue from `checks_not_passed` clears the rest
// and the gate and asks for the check correction of the rest's cause. A rest
// that names no cause falls back to the gate's cause, and with no gate to
// `record_missing`. The used total time is kept.
#[test]
fn continue_from_checks_not_passed_takes_the_cause_of_the_rest_then_of_the_gate() {
    let fx = Fixture::new();
    let cases = [
        (
            "the rest's cause",
            Some(Cause::ChecksUnfinished),
            Some(Cause::WorkFailed),
            Cause::ChecksUnfinished,
        ),
        ("the gate's cause", None, Some(Cause::WorkFailed), Cause::WorkFailed),
        ("no cause at all", None, None, Cause::RecordMissing),
    ];
    for (case, rest_cause, gate_cause, expected) in cases {
        let mut run = resting(checks_rest(rest_cause), gate_cause);
        crate::graduation::continue_rest::clear_rest(&fx.app, &mut run);
        assert_eq!(run.rest, None, "{case}: the rest is cleared");
        assert_eq!(run.checkpoint.check_gate, None, "{case}: the gate is cleared");
        assert_eq!(run.checkpoint.continue_check_correction, Some(expected), "{case}");
        assert_eq!(run.turn_time_used_ms, 5_000, "{case}: the used time is kept");
    }
}

// GRL-FR-SPRE, GRL-FR-BVTS: Continue from `total_time_limit` resets the used
// total time alone, so an open gate and a continued correction stay for the
// turn after it.
#[test]
fn continue_from_a_total_time_rest_keeps_the_gate_and_the_continued_correction() {
    let fx = Fixture::new();
    let mut run = resting(GraduationRest::total_time_limit("t0".into(), 5_000, 5_000), None);
    run.checkpoint.check_gate = Some(GraduationCheckGate {
        cause: Cause::ChecksRed,
        step: GraduationCheckGateStep::Correct,
        resume: None,
    });
    run.checkpoint.continue_check_correction = Some(Cause::ChecksRed);
    let gate = run.checkpoint.check_gate.clone();
    crate::graduation::continue_rest::clear_rest(&fx.app, &mut run);
    assert_eq!(run.rest, None);
    assert_eq!(run.turn_time_used_ms, 0);
    assert_eq!(run.checkpoint.check_gate, gate);
    assert_eq!(run.checkpoint.continue_check_correction, Some(Cause::ChecksRed));
}

// GRD-FR-OERK: a rest on a run that no longer waits for the author is dropped,
// and nothing else of the checkpoint changes.
#[test]
fn a_rest_on_a_run_that_does_not_wait_for_the_author_changes_nothing_else() {
    let fx = Fixture::new();
    let mut run = resting(checks_rest(Some(Cause::ChecksRed)), Some(Cause::ChecksRed));
    run.state = GraduationRunState::Interrupted;
    let gate = run.checkpoint.check_gate.clone();
    crate::graduation::continue_rest::clear_rest(&fx.app, &mut run);
    assert_eq!(run.rest, None);
    assert_eq!(run.checkpoint.check_gate, gate);
    assert_eq!(run.checkpoint.continue_check_correction, None);
    assert_eq!(run.turn_time_used_ms, 5_000);
}
