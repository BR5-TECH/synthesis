//! The plan turns of an apply run (`OPU-openspec-plan-updates.md`
//! OPU-FR-RFFH, OPU-FR-YBLE, OPU-FR-MBIY, OPU-FR-XSRD, OPU-FR-CLYW,
//! OPU-FR-WJKF, OPU-FR-EPXL, OPU-FR-FDCW, OPU-FR-DTYE, OPU-FR-FPRX,
//! OPU-FR-QKLX, OPU-FR-SKTV).
//!
//! A work turn of an apply run may answer a plan update in place of a check
//! record. A refused one gets one plan update correction turn. A review that
//! finds the plan wrong gets one plan correction turn, in place of another
//! pass. This module decides those turns; `work_turn.rs` dispatches them.

use std::path::Path;

use super::drive::Step;
use super::*;
use crate::graduation::logs::{self, GraduationLogLevel, GraduationLogProducer, StructuredEvent};
use crate::graduation::plan_updates::{
    self as plan, scratch, PlanCorrection, PlanCorrectionStep, PlanRecording, PlanRefusalCause,
};
use crate::graduation::{FindingKind, GraduationRest, ReviewFinding};
use crate::tools::agent_exec::protocol::AgentEscalation;

/// What the work turn does after a plan turn's answer.
pub(super) enum PlanNext {
    /// The answer is no plan update: the check gate reads it.
    Gate,
    /// The run rests or is recorded.
    Stop,
    /// OPU-FR-XSRD: the saved plan update correction turn is dispatched next.
    Correct,
}

/// OPU-FR-RFFH: whether a turn of `part` may give a plan update. Only the
/// turns of part `work` of an apply run may: the work turn, its plan update
/// correction turn, and its plan correction turn.
pub(super) fn admits_plan_update(run: &GraduationRun, part: &str) -> bool {
    run.is_change_apply() && part == phases::PART_WORK
}

/// OPU-FR-EPXL: a `plan` finding of a review of any run but an apply run is
/// read as `work`.
pub(super) fn normalized(run: &GraduationRun, mut verdict: ReviewVerdict) -> ReviewVerdict {
    if !run.is_change_apply() {
        for finding in verdict.findings.iter_mut().filter(|f| f.is_plan()) {
            finding.kind = Some(FindingKind::Work);
        }
    }
    verdict
}

/// OPU-FR-FDCW: whether a verdict holds a `plan` finding of an apply review.
pub(super) fn holds_plan_finding(run: &GraduationRun, verdict: &ReviewVerdict) -> bool {
    run.is_change_apply()
        && verdict.verdict == ReviewOutcome::Revise
        && verdict.findings.iter().any(|f| f.is_plan())
}

/// OPU-FR-FDCW: a `revise` verdict that holds a `plan` finding starts no pass.
/// Its findings join the review history, and one plan correction turn is
/// dispatched next, in the session of the run's last work turn.
pub(super) fn route_plan_findings<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    verdict: &ReviewVerdict,
) -> Step {
    let pass = run.pass();
    run.checkpoint.review_history.extend(
        verdict
            .findings
            .iter()
            .map(|finding| ReviewHistoryEntry { pass, finding: finding.clone() }),
    );
    run.checkpoint.plan.correction = Some(PlanCorrection {
        step: PlanCorrectionStep::Plan,
        cause: None,
        findings: verdict.findings.clone(),
        resume: run.checkpoint.plan.last_work_session.clone(),
    });
    let _ = crate::graduation::save_run(app, run);
    let plans = verdict.findings.iter().filter(|f| f.is_plan()).count();
    logging::log_info(
        app,
        &crate::logging::BUFFER,
        &[Domain::Ai, Domain::Backend],
        "graduation routed plan findings to a plan correction turn",
        log_fields! {
            "run_id" => run.id.clone(),
            "findings" => verdict.findings.len() as u64,
            "plan_findings" => plans as u64,
        },
    );
    let routed = StructuredEvent::new(GraduationLogProducer::ReviewTurn, "plan findings routed")
        .with("findings", verdict.findings.len() as u64)
        .with("plan_findings", plans as u64);
    if !logs::emit(app, run, routed) {
        return Step::Stop;
    }
    let _ = transitions::enter_stage(
        app,
        run,
        observability::GraduationVisualStage::Working,
        observability::StageReason::ReviewRevision,
    );
    Step::Continue
}

/// OPU-FR-XSRD, OPU-FR-DTYE: what a saved plan correction turn carries, and
/// the session it resumes.
pub(super) fn correction_input(
    run: &GraduationRun,
    input: &mut task_input::GraduationTaskInput,
) -> Option<SessionRef> {
    let correction = run.checkpoint.plan.correction.as_ref()?;
    let section = match correction.step {
        PlanCorrectionStep::UpdateInvalid => plan::REFUSAL_PLAN_UPDATE_INVALID,
        PlanCorrectionStep::Plan => plan::REFUSAL_PLAN_CORRECTION,
    };
    input.correction = prompts::prompt_section(&prompts::REFUSALS, section);
    if correction.step == PlanCorrectionStep::Plan {
        // OPU-FR-DTYE: the `plan` findings of the verdict, in their order.
        let findings: Vec<ReviewFinding> =
            correction.findings.iter().filter(|f| f.is_plan()).cloned().collect();
        input.loop_instruction = Some(turns::compose_instruction(&ReviewVerdict {
            verdict: ReviewOutcome::Revise,
            rationale: String::new(),
            findings,
        }));
    }
    let resume = correction.resume.as_ref()?;
    Some(SessionRef {
        session_id: Some(resume.session_id.clone()?),
        continuation_token: resume.continuation_token.clone(),
    })
}

/// OPU-FR-SKTV: what the turn that Continue dispatched carries: the plan
/// update context, and the reads Continue made.
pub(super) fn attach_next_turn(run: &GraduationRun, input: &mut task_input::GraduationTaskInput) {
    let Some(next) = run.checkpoint.plan.next_turn.as_ref() else {
        return;
    };
    input.plan_update.clone_from(&next.plan_update);
    if let Some(reads) = plan::reads_for(&run.id) {
        input.openspec_validation = Some(reads.validation);
        input.openspec_apply_progress = Some(reads.progress);
    }
}

/// OPU-FR-WINO, OPU-FR-WJKF: before a work turn of an apply run, the
/// repository hides the scratch folder, and the folder is removed, except
/// before a plan update correction turn. A step that fails is logged and the
/// turn runs all the same.
pub(super) fn prepare_scratch<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
    worktree: &Path,
) {
    if !run.is_change_apply() {
        return;
    }
    if let Err(reason) = scratch::ensure_excluded(worktree) {
        logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation could not hide the scratch folder",
            log_fields! { "run_id" => run.id.clone(), "code" => reason },
        );
    }
    let corrects = run
        .checkpoint
        .plan
        .correction
        .as_ref()
        .is_some_and(|c| c.step == PlanCorrectionStep::UpdateInvalid);
    if corrects {
        return;
    }
    if let Err(reason) = scratch::clear(worktree) {
        logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation could not remove the scratch folder",
            log_fields! { "run_id" => run.id.clone(), "code" => reason },
        );
    }
}

/// The plan update an answer gives, if it gives one.
fn plan_update_of(execution: &AgentExecution) -> Option<&AgentEscalation> {
    execution
        .response
        .as_ref()
        .and_then(|r| r.escalation.as_ref())
        .filter(|escalation| escalation.is_plan_update())
}

/// OPU-FR-MBIY, OPU-FR-XLVF: read the scratch folder in place of the check
/// gate. A folder that cannot be read is no refusal of the agent: the
/// recording reads it again and rests the run `blocked` where it still fails.
fn refusal_of(
    run: &GraduationRun,
    worktree: &Path,
    update: &AgentEscalation,
) -> Option<PlanRefusalCause> {
    let change = run.openspec.as_ref().map(|d| d.change.clone()).unwrap_or_default();
    match scratch::read(worktree, &change) {
        Ok(hunks) if hunks.is_empty() && update.questions.is_empty() => {
            Some(PlanRefusalCause::PlanUpdateEmpty)
        }
        Ok(_) | Err(scratch::ScratchError::Unreadable(_)) => None,
        Err(scratch::ScratchError::Refused(cause)) => Some(cause),
    }
}

/// OPU-FR-RMZN, OPU-FR-XSRD, OPU-FR-CLYW, OPU-FR-FPRX, OPU-FR-QKLX: what an
/// answer of an apply run's work turn means, before the check gate.
pub(super) fn after_answer<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &Path,
    execution: &AgentExecution,
) -> PlanNext {
    if !run.is_change_apply() {
        return PlanNext::Gate;
    }
    // The saved correction is spent: its turn has answered.
    let correction = run.checkpoint.plan.correction.take();
    let update = plan_update_of(execution);
    let refusal = update.and_then(|update| refusal_of(run, worktree, update));
    let step = correction.as_ref().map(|c| c.step);
    match (step, update, refusal) {
        // A valid plan update is recorded, with the origin of its turn.
        (_, Some(update), None) => {
            let review = step == Some(PlanCorrectionStep::Plan);
            let findings = correction.map(|c| c.findings).unwrap_or_default();
            let origin = if review { plan::ORIGIN_REVIEW } else { plan::ORIGIN_WORK };
            note_plan_update(app, run, origin, update, None);
            let recording = PlanRecording {
                rest: plan::REST_PLAN_UPDATE.to_string(),
                origin: origin.to_string(),
                reason: update.reason.clone(),
                questions: questions_of(update),
                findings: if review { findings } else { Vec::new() },
                session: work_turn::session_of(execution),
                ..PlanRecording::default()
            };
            let _ = super::plan_record::begin(app, run, worktree, recording);
            PlanNext::Stop
        }
        // OPU-FR-QKLX: every other answer of a plan correction turn.
        (Some(PlanCorrectionStep::Plan), _, _) => {
            if let (Some(update), Some(cause)) = (update, refusal) {
                note_plan_update(app, run, plan::ORIGIN_REVIEW, update, Some(cause));
            }
            let findings = correction.map(|c| c.findings).unwrap_or_default();
            let recording = PlanRecording {
                rest: plan::REST_PLAN_FINDINGS.to_string(),
                origin: plan::ORIGIN_REVIEW.to_string(),
                findings,
                session: work_turn::session_of(execution),
                ..PlanRecording::default()
            };
            let _ = super::plan_record::begin(app, run, worktree, recording);
            PlanNext::Stop
        }
        // OPU-FR-CLYW: a plan update correction turn that gave no valid plan
        // update.
        (Some(PlanCorrectionStep::UpdateInvalid), _, _) => {
            if let (Some(update), Some(cause)) = (update, refusal) {
                note_plan_update(app, run, plan::ORIGIN_WORK, update, Some(cause));
            }
            let cause = refusal
                .or_else(|| correction.and_then(|c| c.cause))
                .unwrap_or(PlanRefusalCause::PlanUpdateEmpty);
            rest_invalid(app, run, cause);
            PlanNext::Stop
        }
        // OPU-FR-XSRD: a refused plan update gets one correction turn.
        (None, Some(update), Some(cause)) => {
            note_plan_update(app, run, plan::ORIGIN_WORK, update, Some(cause));
            run.checkpoint.plan.correction = Some(PlanCorrection {
                step: PlanCorrectionStep::UpdateInvalid,
                cause: Some(cause),
                findings: Vec::new(),
                resume: work_turn::session_of(execution),
            });
            let _ = crate::graduation::save_run(app, run);
            PlanNext::Correct
        }
        (None, None, _) => PlanNext::Gate,
    }
}

/// OPU-FR-VGFJ: a plan update read, accepted or refused, with counts and the
/// cause alone.
fn note_plan_update<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    origin: &str,
    update: &AgentEscalation,
    refused: Option<PlanRefusalCause>,
) {
    let mut fields = log_fields! {
        "run_id" => run.id.clone(),
        "change" => run.openspec.as_ref().map(|d| d.change.clone()).unwrap_or_default(),
        "origin" => origin.to_string(),
        "questions" => update.questions.len() as u64,
    };
    let mut event = StructuredEvent::new(GraduationLogProducer::WorkTurn, "plan update given")
        .with("origin", origin.to_string())
        .with("questions", update.questions.len() as u64);
    match refused {
        Some(cause) => {
            fields.insert("cause".to_string(), cause.as_str().into());
            event = event.at_level(GraduationLogLevel::Warn).with("cause", cause.as_str());
            logging::log_warn(
                app,
                &crate::logging::BUFFER,
                &[Domain::Ai, Domain::Backend],
                "graduation refused a plan update",
                fields,
            );
        }
        None => logging::log_info(
            app,
            &crate::logging::BUFFER,
            &[Domain::Ai, Domain::Backend],
            "graduation accepted a plan update",
            fields,
        ),
    }
    let _ = logs::emit(app, run, event);
}

/// OPU-FR-RMZN: the questions as the agent gave them, numbered.
fn questions_of(update: &AgentEscalation) -> Vec<GraduationEscalationQuestion> {
    update
        .questions
        .iter()
        .enumerate()
        .map(|(index, question)| GraduationEscalationQuestion {
            position: index as u32 + 1,
            question: question.question.clone(),
            options: question
                .options
                .iter()
                .map(|option| GraduationProposedResponse {
                    answer: option.answer.clone(),
                    summary: option.summary.clone(),
                    description: option.description.clone(),
                })
                .collect(),
        })
        .collect()
}

/// OPU-FR-CLYW: rest the run on `plan_update_invalid` with the cause. Nothing
/// is recorded in a discussion, and nothing is committed.
fn rest_invalid<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    cause: PlanRefusalCause,
) {
    let event =
        StructuredEvent::new(GraduationLogProducer::StageTransition, "run rested for the author")
            .leaving(run.observability.current_stage)
            .with("reason", plan::REST_PLAN_UPDATE_INVALID)
            .with("cause", cause.as_str());
    if !logs::emit(app, run, event) {
        return;
    }
    logging::log_warn(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "graduation rested a run whose plan update was refused",
        log_fields! {
            "run_id" => run.id.clone(),
            "change" => run.openspec.as_ref().map(|d| d.change.clone()).unwrap_or_default(),
            "cause" => cause.as_str(),
        },
    );
    run.checkpoint.check_gate = None;
    run.checkpoint.continue_check_correction = None;
    run.rest = Some(GraduationRest::plan_update_invalid(crate::notes::now_rfc3339(), cause));
    let _ = transitions::transition(app, run, GraduationRunState::AwaitingAuthor);
}
