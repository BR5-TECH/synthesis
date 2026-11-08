//! The work turn of a pass, and the check gate its answer passes before the
//! review (`../ai/GRL-graduation-loop.md` GRL-FR-DXLU, GRL-FR-CPXI,
//! GRL-FR-CHDT, GRL-FR-SMQK, GRL-FR-LAVF, GRL-FR-SUBP, GRL-FR-BOHV,
//! GRL-FR-FGWT, GRL-FR-BVTS, GRL-FR-LNIZ).
//!
//! Split out of `drive.rs`, which holds the two phases and their order. One
//! call drives one **gate episode**: the work turn, and the check correction
//! and the escalate correction that a refused answer gets.

use super::check_gate::{self, GateReading};
use super::corrections::{self, Corrected};
use super::dispatch::TurnRequest;
use super::drive::{pass_task, pending_correction, settings_for_dispatch, Step};
use super::stops::Stop;
use super::turns::*;
use super::*;
use crate::graduation::logs::{self, GraduationLogLevel, GraduationLogProducer, StructuredEvent};
use crate::graduation::{
    GraduationCheckCause, GraduationCheckGate, GraduationCheckGateStep, GraduationRest,
    GraduationRestCheck,
};
use std::path::PathBuf;

/// GRL-FR-SUBP: the `refusals.md` section an escalate correction carries.
pub const REFUSAL_CHECKS_ESCALATE: &str = "checks_escalate";

/// What kind of work turn is dispatched next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TurnKind {
    /// A turn the gate did not ask for, which starts a gate episode.
    Episode,
    /// GRD-FR-OERK: the turn Continue from `checks_not_passed` dispatches. It
    /// starts a gate episode and carries the check correction of the cause.
    Continued(GraduationCheckCause),
    /// GRL-FR-BVTS: the saved step of an open check gate.
    Gate(GraduationCheckGateStep, GraduationCheckCause),
}

fn next_kind(run: &GraduationRun) -> TurnKind {
    if let Some(gate) = run.checkpoint.check_gate.as_ref() {
        return TurnKind::Gate(gate.step, gate.cause);
    }
    match run.checkpoint.continue_check_correction {
        Some(cause) => TurnKind::Continued(cause),
        None => TurnKind::Episode,
    }
}

/// GRL-FR-DXLU: the turn that does the work, and the gate on its answer.
pub(super) async fn work_turn<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &PathBuf,
    cancellation: &CancellationToken,
    dispatch: &dyn seams::GraduationDispatch<R>,
) -> Step {
    let mut first = true;
    loop {
        let kind = next_kind(run);
        let Some(turn) = prepare(app, run, worktree, cancellation, kind, first) else {
            return Step::Stop;
        };
        first = false;
        let resumed = turn.resume.is_some();
        // GRL-FR-QKDC: a refused answer is asked again once, in the same session.
        let (outcome, bound) =
            match execute_step(app, run, turn, cancellation, dispatch, resumed).await {
                Corrected::Settle(outcome, bound) => (*outcome, bound),
                Corrected::Stopped => return Step::Stop,
            };
        let settled =
            settle_execution(app, run, worktree, outcome, GraduationEscalationOrigin::Work, bound);
        // GXD-FR-HLQI: the continued correction is spent once the answer of its
        // turn is settled: accepted, gated, or a recorded escalation. A stop
        // before that keeps it.
        let escalated = settled.is_none()
            && run.state == GraduationRunState::AwaitingAuthor
            && run.escalation.is_some();
        if matches!(kind, TurnKind::Continued(_)) && (settled.is_some() || escalated) {
            run.checkpoint.continue_check_correction = None;
            if escalated {
                let _ = crate::graduation::save_run(app, run);
            }
        }
        let Some(execution) = settled else {
            return Step::Stop;
        };
        record_turn_end(app, run, worktree);
        // GRL-FR-XWNQ: every completed answer passes the gate, on every vendor.
        let reading = check_gate::judge(&execution, run.checkpoint.decisions.len());
        let escalate_step = match kind {
            TurnKind::Gate(GraduationCheckGateStep::Escalate, saved) => Some(saved),
            _ => None,
        };
        // GRL-FR-FGWT: an escalate correction whose answer is not a recorded
        // escalation rests the run, a green success included.
        let cause = match (reading.cause, escalate_step) {
            (Some(cause), _) => cause,
            (None, Some(saved)) => saved,
            (None, None) => return accept(app, run, &execution),
        };
        let step = match kind {
            TurnKind::Gate(GraduationCheckGateStep::Correct, _) => {
                Some(GraduationCheckGateStep::Escalate)
            }
            TurnKind::Gate(GraduationCheckGateStep::Escalate, _) => None,
            // GRL-FR-LAVF: one check correction in each gate episode.
            TurnKind::Episode | TurnKind::Continued(_) => Some(GraduationCheckGateStep::Correct),
        };
        if !note_gated(app, run, cause, step, &reading) {
            return Step::Stop;
        }
        let Some(step) = step else {
            rest_on_checks(app, run, cause, reading.failing);
            return Step::Stop;
        };
        // GRL-FR-BVTS: saved before the step is dispatched.
        run.checkpoint.check_gate =
            Some(GraduationCheckGate { cause, step, resume: session_of(&execution) });
        let _ = crate::graduation::save_run(app, run);
    }
}

/// Compose and announce the next turn, or rest the run where it may not run.
fn prepare<'a, R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &'a PathBuf,
    cancellation: &CancellationToken,
    kind: TurnKind,
    first: bool,
) -> Option<TurnRequest<'a>> {
    if !first {
        if let Some(reason) = cancellation_reason(app, &run.id, cancellation) {
            abandon(app, run, worktree, Stop::of_reason(reason));
            return None;
        }
    }
    // GRD-FR-XHSE: a merge run checks its pinned tips before every dispatch.
    if merge_drive::moved(app, run) {
        return None;
    }
    let settings = settings_for_dispatch(app, run, phases::PART_WORK);
    // GRL-FR-ARPX / GRL-FR-XBUE: the window was just recomputed from the
    // settings as they stand, so a budget the author lowered rests the run here
    // rather than reaching the task validation below as a fault.
    if first && run.pass_beyond_window() {
        rest_for_author(app, run);
        return None;
    }
    // GRL-FR-WOBK / GRL-FR-BVTS: a run with no total time left dispatches no
    // turn, and an open check gate stays saved for Continue.
    if time_budget::rest_if_spent(app, run, &settings, phases::PART_WORK) {
        return None;
    }
    let purpose = purpose_for(run);
    let mut input = task_input::compose_input(run, phases::PART_WORK, purpose);
    let refusal = |section: &str| prompts::prompt_section(&prompts::REFUSALS, section);
    let mut resume = None;
    match kind {
        TurnKind::Episode => {}
        TurnKind::Continued(cause) => input.check_correction = refusal(cause.correction_section()),
        TurnKind::Gate(step, cause) => {
            match step {
                GraduationCheckGateStep::Correct => {
                    input.check_correction = refusal(cause.correction_section());
                }
                GraduationCheckGateStep::Escalate => {
                    input.correction = refusal(REFUSAL_CHECKS_ESCALATE);
                }
            }
            // GRL-FR-SMQK: the session the refused turn reported.
            resume =
                run.checkpoint.check_gate.as_ref().and_then(|gate| gate.resume.as_ref()).and_then(
                    |resume| {
                        resume.session_id.clone().map(|session_id| SessionRef {
                            session_id: Some(session_id),
                            continuation_token: resume.continuation_token.clone(),
                        })
                    },
                );
        }
    }
    if let Err(reason) = input.validate(run.checkpoint.pass_limit) {
        note_blocked(app, run, "task_invalid");
        let _ = transitions::block(
            app,
            run,
            "task_invalid",
            reason,
            "Continue the run.",
            phases::PART_WORK,
        );
        return None;
    }
    if first {
        // GRL-FR-ISIL: a resumed turn continues the pass it was in, so a pass
        // already open is kept rather than opened again.
        run.observability.open_pass(run.pass(), pass_task(run), crate::notes::now_rfc3339());
        let _ = transitions::enter_stage(
            app,
            run,
            observability::GraduationVisualStage::Working,
            observability::StageReason::WorkStarted,
        );
        if transitions::transition(app, run, GraduationRunState::Working).is_err() {
            return None;
        }
    }
    // GRL-FR-BOHV: each correction of the gate is one more work turn.
    run.work_turns += 1;
    let escalate = matches!(kind, TurnKind::Gate(GraduationCheckGateStep::Escalate, _));
    // GLG-FR-ZJTP / GLG-FR-WLFB: the record that starts the turn says what it is.
    let started = StructuredEvent::new(GraduationLogProducer::WorkTurn, "work turn started")
        .with("purpose", purpose)
        .with("turn", run.work_turns)
        .with("corrected", escalate || pending_correction(run, phases::PART_WORK))
        .with("check_correction", input.check_correction.is_some());
    if !logs::emit(app, run, started) {
        return None;
    }
    // GRL-FR-KYYW: a turn that corrects keeps the record of the turn before it.
    let keeps_record = kind != TurnKind::Episode;
    prepare_check_folder(app, run, worktree, keeps_record);
    // GRL-FR-EFTM / GRL-FR-SMQK: the bound is read before each dispatch.
    let bound = settings.turn_bound(run.turn_time_used_ms);
    Some(TurnRequest {
        execution_directory: worktree,
        part: phases::PART_WORK,
        input,
        bound,
        resume,
    })
}

/// GRL-FR-SMQK: dispatch one turn. A gate step whose resume the vendor did
/// not honour ran nothing, so it runs once more as a fresh turn in the same
/// working copy.
async fn execute_step<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    turn: TurnRequest<'_>,
    cancellation: &CancellationToken,
    dispatch: &dyn seams::GraduationDispatch<R>,
    resumed: bool,
) -> Corrected {
    let retry = resumed.then(|| TurnRequest {
        execution_directory: turn.execution_directory,
        part: turn.part,
        input: turn.input.clone(),
        bound: turn.bound,
        resume: None,
    });
    let pending = corrections::will_correct(run, phases::PART_WORK);
    let corrected =
        corrections::execute_with_correction(app, run, turn, cancellation, dispatch).await;
    let Some(mut retry) = retry.filter(|_| !pending) else {
        return corrected;
    };
    let unresumed = matches!(&corrected, Corrected::Settle(outcome, _) if outcome.as_ref().as_ref().is_ok_and(|execution| {
        execution.resume_unavailable
            && execution.process_outcome != ProcessOutcome::InvalidStructuredOutput
    }));
    if !unresumed {
        return corrected;
    }
    logging::log_warn(
        app,
        &crate::logging::BUFFER,
        &[Domain::Ai, Domain::Backend],
        "graduation runs a check gate step as a fresh turn: the vendor did not resume",
        log_fields! { "run_id" => run.id.clone() },
    );
    if let Some(gate) = run.checkpoint.check_gate.as_mut() {
        gate.resume = None;
    }
    let settings = settings_for_dispatch(app, run, phases::PART_WORK);
    retry.bound = settings.turn_bound(run.turn_time_used_ms);
    corrections::execute_with_correction(app, run, retry, cancellation, dispatch).await
}

/// GRL-FR-XWNQ / GRL-FR-LNIZ / GRL-FR-DNKA: an accepted answer closes the
/// gate, and its account goes to the review.
fn accept<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    execution: &AgentExecution,
) -> Step {
    if let Some(gate) = run.checkpoint.check_gate.take() {
        logging::log_info(
            app,
            &crate::logging::BUFFER,
            &[Domain::Ai, Domain::Backend],
            "graduation check gate accepted a corrected answer",
            log_fields! { "run_id" => run.id.clone(), "cause" => gate.cause.as_str() },
        );
    }
    run.checkpoint.agent_account = account_of(execution);
    let _ = crate::graduation::save_run(app, run);
    if !logs::emit(
        app,
        run,
        StructuredEvent::new(GraduationLogProducer::WorkTurn, "work turn ended")
            .with("changed_paths", run.checkpoint.changed_paths.len() as u64)
            .with("hidden_paths", run.checkpoint.hidden_paths.len() as u64),
    ) {
        return Step::Stop;
    }
    Step::Continue
}

/// GLG-FR-FSZH: the refusal is in the run's own log. It carries counts and
/// never a check name or a command.
fn note_gated<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    cause: GraduationCheckCause,
    step: Option<GraduationCheckGateStep>,
    reading: &GateReading,
) -> bool {
    let next = step.map(GraduationCheckGateStep::as_str).unwrap_or("rest");
    logging::log_warn(
        app,
        &crate::logging::BUFFER,
        &[Domain::Ai, Domain::Backend],
        "graduation check gate refused a work answer",
        log_fields! {
            "run_id" => run.id.clone(),
            "cause" => cause.as_str(),
            "next" => next,
            "checks" => reading.checks as i64,
            "unfinished" => reading.unfinished as i64,
            "red" => reading.red as i64,
            "waived" => reading.waived as i64,
        },
    );
    let part = if run.is_merge() { phases::PART_MERGE_WORK } else { phases::PART_WORK };
    let gated = StructuredEvent::new(GraduationLogProducer::StageTransition, "work answer gated")
        .at_level(GraduationLogLevel::Warn)
        .with("part", part)
        .with("cause", cause.as_str())
        .with("next", next)
        .with("checks", reading.checks as u64)
        .with("unfinished", reading.unfinished as u64)
        .with("red", reading.red as u64)
        .with("waived", reading.waived as u64);
    logs::emit(app, run, gated)
}

/// GRL-FR-FGWT / GRD-FR-PYJN: the gate is spent. The run rests for the author
/// with nothing reviewed and nothing committed, and keeps its queue, its
/// project slot and its checkpoint.
fn rest_on_checks<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    cause: GraduationCheckCause,
    failing: Vec<GraduationRestCheck>,
) {
    // GLG-FR-YCVA: the count of checks, never a name or a command.
    let event =
        StructuredEvent::new(GraduationLogProducer::StageTransition, "run rested for the author")
            .leaving(run.observability.current_stage)
            .with("reason", crate::graduation::REST_CHECKS_NOT_PASSED)
            .with("cause", cause.as_str())
            .with("checks", failing.len() as u64);
    // GRD-FR-IKVE: the rest is set only after its record is stored.
    if !logs::emit(app, run, event) {
        return;
    }
    logging::log_info(
        app,
        &crate::logging::BUFFER,
        &[Domain::Ai, Domain::Backend],
        "graduation rested a run whose checks did not pass",
        log_fields! {
            "run_id" => run.id.clone(),
            "cause" => cause.as_str(),
            "checks" => failing.len() as i64,
        },
    );
    run.rest = Some(GraduationRest::checks_not_passed(crate::notes::now_rfc3339(), cause, failing));
    let _ = transitions::transition(app, run, GraduationRunState::AwaitingAuthor);
}

/// GRL-FR-SMQK: the session a refused turn reported.
fn session_of(execution: &AgentExecution) -> Option<GraduationResumeRef> {
    let session = execution
        .session
        .as_ref()
        .or_else(|| execution.response.as_ref().and_then(|r| r.session.as_ref()))?;
    let session_id = session.session_id.clone().filter(|id| !id.is_empty())?;
    Some(GraduationResumeRef {
        session_id: Some(session_id),
        continuation_token: session.continuation_token.clone(),
    })
}

/// GRL-FR-DNKA: the account of an accepted turn, which is its summary. A
/// blank account is none at all, so an agent that reported nothing adds no
/// empty line to the review's input.
fn account_of(execution: &AgentExecution) -> Option<String> {
    execution
        .response
        .as_ref()
        .map(|r| r.summary.clone())
        .filter(|account| !account.trim().is_empty())
}

/// GRL-FR-LXWU / GRL-FR-KYYW: before a `work` or `merge_work` turn, the
/// repository hides the check folder and the folder holds no earlier result.
///
/// A turn that carries `correction` or `check_correction` continues the record
/// of the turn before it, so the folder is kept for it. A step that fails is
/// reported and the turn runs all the same: the review then finds the report
/// wrong or missing, which is a finding (GRL-FR-NTFD) rather than a stop.
fn prepare_check_folder<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
    worktree: &std::path::Path,
    keeps_record: bool,
) {
    match checks_folder::ensure_excluded(worktree) {
        Ok(true) => logging::log_debug(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation hid the check folder in the repository exclude file",
            log_fields! { "run_id" => run.id.clone() },
        ),
        Ok(false) => {}
        Err(reason) => logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation could not hide the check folder",
            log_fields! { "run_id" => run.id.clone(), "reason" => reason },
        ),
    }
    if keeps_record || corrections::will_correct(run, phases::PART_WORK) {
        return;
    }
    match checks_folder::clear(worktree) {
        Ok(removed) => logging::log_debug(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation emptied the check folder",
            log_fields! { "run_id" => run.id.clone(), "removed" => removed },
        ),
        Err(reason) => logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation could not empty the check folder",
            log_fields! { "run_id" => run.id.clone(), "reason" => reason },
        ),
    }
}
