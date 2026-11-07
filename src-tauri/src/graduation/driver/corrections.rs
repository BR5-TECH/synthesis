//! A refused answer asked again, once, in the session of the turn that gave it
//! (`GRL-graduation-loop.md` GRL-FR-QKDC, GRL-FR-MZHT, GRL-FR-JVNB,
//! GRL-FR-PTCE, GRL-FR-HWLD, GRL-FR-NSEY).
//!
//! The agent did the work and only its answer failed, so the turn is told what
//! to send again rather than lost. The first outcome is never settled where a
//! correction turn runs, so its work is committed once, with the outcome of the
//! correction.

use super::dispatch::{execute, TurnRequest};
use super::drive::settings_for_dispatch;
use super::settings::TurnBound;
use super::turns::{cancellation_reason_of, discarded_meanwhile};
use super::*;
use crate::graduation::logs::{self, GraduationLogLevel, GraduationLogProducer, StructuredEvent};
use crate::graduation::{GraduationAnswerCorrection, GraduationResumeRef};
use crate::tools::agent_exec::protocol::EnvelopeInvalid;

/// GRL-FR-MZHT: each correctable refusal and its section of `refusals.md`. No
/// other refusal is correctable.
const SECTIONS: &[(EnvelopeInvalid, &str)] = &[
    (EnvelopeInvalid::NotFound, "answer_missing"),
    (EnvelopeInvalid::Malformed, "answer_malformed"),
    (EnvelopeInvalid::ProtocolVersion, "answer_malformed"),
    (EnvelopeInvalid::MultipleDocuments, "answer_several"),
    (EnvelopeInvalid::SummaryEmpty, "answer_summary_empty"),
    (EnvelopeInvalid::Exclusivity, "answer_outcome_mismatch"),
    (EnvelopeInvalid::EscalationShape, "answer_escalation_shape"),
    (EnvelopeInvalid::TooLarge, "answer_too_large"),
];

/// GRL-FR-MZHT: the `refusals.md` section a refusal is corrected with, or
/// `None` where the refusal is not correctable.
pub fn correction_section(reason: &str) -> Option<&'static str> {
    SECTIONS.iter().find(|(refusal, _)| refusal.as_str() == reason).map(|(_, section)| *section)
}

/// GRL-FR-MZHT: every section a correction can name, without repeats.
pub fn correction_sections() -> Vec<&'static str> {
    let mut sections: Vec<&'static str> = SECTIONS.iter().map(|(_, s)| *s).collect();
    sections.dedup();
    sections
}

fn refusal_of(reason: &str) -> Option<EnvelopeInvalid> {
    SECTIONS.iter().find(|(refusal, _)| refusal.as_str() == reason).map(|(refusal, _)| *refusal)
}

/// What a turn and its correction leave for the caller.
pub(super) enum Corrected {
    /// The outcome to settle, with the bound the turn that gave it ran under.
    Settle(Box<Result<AgentExecution, AgentExecutionError>>, TurnBound),
    /// The run already rests, and nothing is settled.
    Stopped,
}

/// GRL-FR-QKDC: dispatch a turn, and ask it once more where its answer was
/// refused for a reason the agent can repair.
pub(super) async fn execute_with_correction<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    mut turn: TurnRequest<'_>,
    cancellation: &CancellationToken,
    dispatch: &dyn seams::GraduationDispatch<R>,
) -> Corrected {
    let part = turn.part;
    let execution_directory = turn.execution_directory;
    let input = turn.input.clone();
    let bound = turn.bound;

    // GRL-FR-HWLD: a correction saved before a stop is this turn.
    if let Some(pending) = pending_for(run, part) {
        apply(&mut turn, &pending);
        let outcome = execute(app, run, turn, cancellation, dispatch).await;
        return Corrected::Settle(Box::new(end_correction(app, run, &pending, outcome)), bound);
    }

    let outcome = execute(app, run, turn, cancellation, dispatch).await;
    let Some(pending) = correctable(part, &outcome) else {
        return Corrected::Settle(Box::new(outcome), bound);
    };
    // GRD-FR-EWTN: nothing is saved over a discarded run.
    if discarded_meanwhile(app, run) {
        return Corrected::Settle(Box::new(outcome), bound);
    }
    run.checkpoint.answer_correction = Some(pending.clone());
    // GRD-FR-MDQZ / GRD-FR-TWMA: a stop the author or the application recorded
    // while the turn ran stands, so the loop's own record is not saved over it.
    // GRL-FR-HWLD: the correction goes into the stored record alone, so Continue
    // sends it.
    if cancellation.is_cancelled() || cancellation_reason_of(app, &run.id).is_some() {
        if let Ok(mut stored) = crate::graduation::load_run(app, &run.id) {
            stored.checkpoint.answer_correction = Some(pending.clone());
            let _ = crate::graduation::save_run(app, &mut stored);
        }
        return Corrected::Settle(Box::new(outcome), bound);
    }
    let _ = crate::graduation::save_run(app, run);
    logging::log_warn(
        app,
        &crate::logging::BUFFER,
        &[Domain::Ai, Domain::Backend],
        "graduation asks a turn again because its answer was refused",
        log_fields! {
            "run_id" => run.id.clone(),
            "part" => part.to_string(),
            "refusal" => pending.reason.clone(),
        },
    );
    // GLG-FR-ZJTP: the refusal is in the run's own log before the turn is
    // asked again.
    let refused =
        StructuredEvent::new(GraduationLogProducer::StageTransition, "turn answer refused")
            .at_level(GraduationLogLevel::Warn)
            .with("part", part)
            .with("refusal", pending.reason.clone());
    if !logs::emit(app, run, refused) {
        return Corrected::Stopped;
    }

    // GRD-FR-XHSE: a merge run checks its pinned tips before every dispatch.
    if merge_drive::moved(app, run) {
        return Corrected::Stopped;
    }
    let settings = settings_for_dispatch(app, run, part);
    if settings.remaining_total_ms(run.turn_time_used_ms) == 0 {
        return Corrected::Settle(Box::new(outcome), bound);
    }

    // GRL-FR-NSEY: the correction is one more turn of its part.
    let producer = if part == phases::PART_REVIEW {
        run.review_turns += 1;
        GraduationLogProducer::ReviewTurn
    } else {
        run.work_turns += 1;
        GraduationLogProducer::WorkTurn
    };
    let turns = if part == phases::PART_REVIEW { run.review_turns } else { run.work_turns };
    let message =
        if part == phases::PART_REVIEW { "review turn started" } else { "work turn started" };
    // GLG-FR-ZJTP: the record that starts the correction turn says so.
    let started = StructuredEvent::new(producer, message)
        .with("turn", turns)
        .with("corrected", true)
        .with("refusal", pending.reason.clone());
    if !logs::emit(app, run, started) {
        return Corrected::Stopped;
    }

    let second_bound = settings.turn_bound(run.turn_time_used_ms);
    let mut second =
        TurnRequest { execution_directory, part, input, bound: second_bound, resume: None };
    apply(&mut second, &pending);
    let second_outcome = execute(app, run, second, cancellation, dispatch).await;
    Corrected::Settle(Box::new(end_correction(app, run, &pending, second_outcome)), second_bound)
}

/// GRL-FR-HWLD / GRL-FR-KYYW: whether the next turn of `part` is a saved
/// correction that will be sent, by the rule of [`pending_for`].
pub(super) fn will_correct(run: &GraduationRun, part: &str) -> bool {
    run.checkpoint.answer_correction.as_ref().is_some_and(|pending| {
        pending.part == part && correction_section(&pending.reason).is_some()
    })
}

/// GRL-FR-HWLD: the correction saved for this part, if any. A work turn that
/// runs first drops a pending review correction.
fn pending_for(run: &mut GraduationRun, part: &str) -> Option<GraduationAnswerCorrection> {
    let pending = run.checkpoint.answer_correction.clone()?;
    if pending.part == part && correction_section(&pending.reason).is_some() {
        return Some(pending);
    }
    if part == phases::PART_WORK || correction_section(&pending.reason).is_none() {
        run.checkpoint.answer_correction = None;
    }
    None
}

/// GRL-FR-QKDC: the correction a refused outcome gets, where it gets one.
fn correctable(
    part: &str,
    outcome: &Result<AgentExecution, AgentExecutionError>,
) -> Option<GraduationAnswerCorrection> {
    let execution = outcome.as_ref().ok()?;
    if execution.process_outcome != ProcessOutcome::InvalidStructuredOutput {
        return None;
    }
    let reason = execution.refusal?.as_str();
    correction_section(reason)?;
    let session = execution.session.as_ref()?;
    let session_id = session.session_id.clone().filter(|id| !id.is_empty())?;
    Some(GraduationAnswerCorrection {
        part: part.to_string(),
        reason: reason.to_string(),
        resume: GraduationResumeRef {
            session_id: Some(session_id),
            continuation_token: session.continuation_token.clone(),
        },
    })
}

/// GRL-FR-XSFA: the turn carries the correction text and resumes the refused
/// turn's session.
fn apply(turn: &mut TurnRequest<'_>, pending: &GraduationAnswerCorrection) {
    turn.input.correction = correction_section(&pending.reason)
        .and_then(|section| prompts::prompt_section(&prompts::REFUSALS, section));
    turn.resume = Some(SessionRef {
        session_id: pending.resume.session_id.clone(),
        continuation_token: pending.resume.continuation_token.clone(),
    });
}

/// GRL-FR-HWLD / GRL-FR-PTCE: what a correction turn leaves.
///
/// A launch error and a cancellation keep the correction, so the next turn of
/// the part after Continue is the correction. A correction whose resume the
/// vendor did not honour discards its own answer, and the refused turn settles
/// as it ended.
fn end_correction<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    pending: &GraduationAnswerCorrection,
    outcome: Result<AgentExecution, AgentExecutionError>,
) -> Result<AgentExecution, AgentExecutionError> {
    let keep = match &outcome {
        Err(_) => true,
        Ok(execution) => execution.process_outcome == ProcessOutcome::Cancelled,
    };
    if !keep {
        run.checkpoint.answer_correction = None;
    }
    match outcome {
        Ok(execution) if execution.resume_unavailable => {
            logging::log_warn(
                app,
                &crate::logging::BUFFER,
                &[Domain::Ai, Domain::Backend],
                "graduation discarded a correction turn that did not resume its session",
                log_fields! { "run_id" => run.id.clone(), "refusal" => pending.reason.clone() },
            );
            Ok(as_refused(execution, pending))
        }
        other => other,
    }
}

/// GRL-FR-PTCE: the outcome of the refused turn, rebuilt from a correction
/// whose answer is discarded.
fn as_refused(execution: AgentExecution, pending: &GraduationAnswerCorrection) -> AgentExecution {
    AgentExecution {
        process_outcome: ProcessOutcome::InvalidStructuredOutput,
        exit_code: Some(0),
        response: None,
        refusal: refusal_of(&pending.reason),
        session: None,
        ..execution
    }
}
