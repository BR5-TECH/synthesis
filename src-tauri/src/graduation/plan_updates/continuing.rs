//! Continue from a plan rest (OPU-FR-EUZU, OPU-FR-VKBO, OPU-FR-THOZ through
//! OPU-FR-TZBD).
//!
//! Continue checks that the author has done their part, reads the change
//! once, enters the answers and the hunk decisions in the decision ledger,
//! moves the run's base to the plan base where the plan proposal committed,
//! and leaves on the checkpoint what the next work turn carries. The caller,
//! `continue_graduation_run`, writes all of it in its one durable write.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use super::*;
use crate::openspec::{
    ApplyInstructions, ChangeValidation, OpenSpecFailure, OpenSpecReadError, Refusal,
};

/// OPU-FR-PXDY / OPU-FR-SKTV: the reads Continue made, for the turn it
/// dispatches. Held in memory, so the run record holds no text of a project
/// file. A relaunch loses them, and the turn then goes without them.
#[derive(Clone, Debug)]
pub struct PlanReads {
    pub validation: ChangeValidation,
    pub progress: ApplyInstructions,
}

static READS: LazyLock<Mutex<HashMap<String, PlanReads>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn reads() -> std::sync::MutexGuard<'static, HashMap<String, PlanReads>> {
    READS.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// OPU-FR-SKTV: the reads of the turn that Continue dispatched for `run_id`.
pub fn reads_for(run_id: &str) -> Option<PlanReads> {
    reads().get(run_id).cloned()
}

/// The reads of `run_id` are spent once its turn has ended.
pub fn forget_reads(run_id: &str) {
    reads().remove(run_id);
}

/// What one Continue from a plan rest leaves for the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanContinue {
    /// The run does not rest on a plan rest.
    NotPlan,
    /// The rest is cleared, and the run is ready to queue.
    Continued,
}

/// OPU-FR-THOZ through OPU-FR-TZBD, OPU-FR-VKBO: Continue from a plan rest.
///
/// A refusal changes nothing of the run in hand and is returned as the
/// typed error. The caller holds the store hold.
pub fn continue_plan_rest<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
) -> Result<PlanContinue, Refusal> {
    let Some(rest) = run.rest.clone().filter(GraduationRest::is_plan_rest) else {
        return Ok(PlanContinue::NotPlan);
    };
    if run.state != GraduationRunState::AwaitingAuthor {
        return Ok(PlanContinue::NotPlan);
    }
    let change = run.openspec.as_ref().map(|d| d.change.clone()).unwrap_or_default();
    // OPU-FR-VKBO: a fresh work turn of the same pass, and the used total
    // time stays.
    if rest.reason == REST_PLAN_UPDATE_INVALID {
        run.rest = None;
        run.checkpoint.plan.next_turn =
            Some(PlanNextTurn { purpose: driver::PURPOSE_RESUME.to_string(), plan_update: None });
        log_continued(app, run, &change, &rest, 0, false);
        return Ok(PlanContinue::Continued);
    }
    // OPU-FR-THOZ: the questions are answered first.
    if rest.reason == REST_PLAN_UPDATE && rest.set_id.is_some() && rest.answers.is_none() {
        return Err(refuse(app, run, &change, ERR_PLAN_QUESTIONS_PENDING).into());
    }
    // OPU-FR-THOZ: and the edits are decided, and their commit has landed.
    let proposal = match rest.proposal_id.as_deref() {
        None => None,
        Some(id) => {
            let store =
                change_runs::machine_store(app).map_err(|_| ERR_WRITE_FAILED.to_string())?;
            let proposal = crate::change_proposals::read_change_proposal(&store, id)
                .ok_or_else(|| refuse(app, run, &change, ERR_WRITE_FAILED))?;
            if proposal.state == crate::change_proposals::ChangeProposalState::Pending
                || proposal.commit_owed
            {
                return Err(refuse(app, run, &change, ERR_PROPOSAL_PENDING).into());
            }
            Some(proposal)
        }
    };
    // OPU-FR-PXDY, OFP-FR-ITUD: Continue clears a kept `failed` CLI check,
    // then reads the change once for its validation and once for its
    // progress. A read that fails refuses with its code as the cause and its
    // diagnostics (OFP-FR-QLQJ).
    if let Some(seam) = app.try_state::<crate::openspec::OpenSpec>() {
        seam.clear_failed_checks(app, &BUFFER);
    }
    let plan_reads = read_change(app, run, &change).map_err(|error| {
        let failure = OpenSpecFailure::wrapping(ERR_OPENSPEC_VALIDATION_UNAVAILABLE, &error)
            .for_change(&change);
        refuse(app, run, &change, &failure.to_string());
        Refusal::from(failure)
    })?;
    // OPU-FR-NQBK / OPU-FR-YNXV: the plan base, where the proposal committed.
    let plan_base = match proposal.as_ref().and_then(|p| p.commit.clone()) {
        None => None,
        Some(close) => {
            let base = run.base_commit.clone().unwrap_or_default();
            let made = direct::target_worktree(app, run)
                .ok_or_else(|| ERR_WRITE_FAILED.to_string())
                .and_then(|worktree| plan_base::make(&worktree, &base, &close, &run.id));
            Some(made.map_err(|reason| {
                logging::log_warn(
                    app,
                    &BUFFER,
                    &[Domain::Backend],
                    "graduation could not make a plan base",
                    log_fields! { "run_id" => run.id.clone(), "code" => reason },
                );
                refuse(app, run, &change, ERR_WRITE_FAILED)
            })?)
        }
    };

    // Every check has passed: from here the run in hand changes.
    let findings = rest.findings.clone().unwrap_or_default();
    let hunks = proposal.as_ref().map(hunk_outcomes).unwrap_or_default();
    let mut entries = Vec::new();
    let next = if rest.reason == REST_PLAN_FINDINGS && proposal.is_none() {
        // OPU-FR-TZBD: every finding, and no plan update.
        run.checkpoint.loop_instruction = Some(instruction_of(&findings));
        PlanNextTurn { purpose: driver::PURPOSE_RESUME.to_string(), plan_update: None }
    } else {
        let pass = run.pass();
        if rest.reason == REST_PLAN_UPDATE {
            entries.extend(answer_entries(&rest, pass));
        }
        entries.extend(hunk_entries(proposal.as_ref(), &hunks, pass));
        let origin = rest.origin.clone().unwrap_or_else(|| ORIGIN_WORK.to_string());
        // OPU-FR-SKTV: the `work` findings of the verdict, for the origin
        // `review`.
        if origin == ORIGIN_REVIEW {
            let work: Vec<ReviewFinding> =
                findings.iter().filter(|f| !f.is_plan()).cloned().collect();
            run.checkpoint.loop_instruction = (!work.is_empty()).then(|| instruction_of(&work));
        }
        // OPU-FR-WPSR: the session of the turn that gave the plan update.
        run.checkpoint.resume = run.checkpoint.plan.rest_session.clone();
        let reason = rest.plan_reason.clone().filter(|_| rest.reason == REST_PLAN_UPDATE);
        PlanNextTurn {
            purpose: driver::PURPOSE_ESCALATION_ANSWER.to_string(),
            plan_update: Some(PlanUpdateTaskContext { origin, reason, hunks }),
        }
    };
    // OPU-FR-CAGI, OPU-FR-ZNRR: the ledger, in the same write.
    let appended = entries.len();
    run.checkpoint.decisions.extend(entries);
    if let Some(revision) = plan_base.clone() {
        run.base_commit = Some(revision);
    }
    // GRL-FR-LAVF: the turn that takes the decisions starts a gate episode.
    run.checkpoint.check_gate = None;
    run.checkpoint.continue_check_correction = None;
    run.checkpoint.plan.rest_session = None;
    run.checkpoint.plan.next_turn = Some(next);
    // OPU-FR-YTKN, OPU-FR-TZBD: the used total time is reset.
    run.turn_time_used_ms = 0;
    run.rest = None;
    reads().insert(run.id.clone(), plan_reads);
    log_continued(app, run, &change, &rest, appended, plan_base.is_some());
    Ok(PlanContinue::Continued)
}

/// OPU-FR-VGFJ: a refused Continue is a `WARN` record with the code.
fn refuse<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
    change: &str,
    code: &str,
) -> String {
    logging::log_warn(
        app,
        &BUFFER,
        &[Domain::Backend],
        "graduation refused to continue a plan rest",
        log_fields! {
            "run_id" => run.id.clone(),
            "change" => change.to_string(),
            "reason" => run.rest.as_ref().map(|r| r.reason.clone()).unwrap_or_default(),
            "code" => code.to_string(),
        },
    );
    code.to_string()
}

/// OPU-FR-VGFJ: an accepted Continue, with the counts and the ids.
fn log_continued<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
    change: &str,
    rest: &GraduationRest,
    entries: usize,
    plan_base: bool,
) {
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "graduation continued a plan rest",
        log_fields! {
            "run_id" => run.id.clone(),
            "change" => change.to_string(),
            "reason" => rest.reason.clone(),
            "origin" => rest.origin.clone().unwrap_or_default(),
            "proposal_id" => rest.proposal_id.clone().unwrap_or_default(),
            "set_id" => rest.set_id.clone().unwrap_or_default(),
            "entries" => entries as u64,
            "plan_base" => plan_base,
        },
    );
}

/// OPU-FR-PXDY: `validate_change` and `apply_instructions` on the stream's
/// working copy, or the error of the read that failed.
fn read_change<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
    change: &str,
) -> Result<PlanReads, OpenSpecReadError> {
    let seam = app
        .try_state::<crate::openspec::OpenSpec>()
        .ok_or(OpenSpecReadError::OpenSpecModeInactive)?;
    let active = app.try_state::<ProjectState>().and_then(|state| state.require_root().ok());
    let location = crate::openspec::ChangeLocation::Stream { stream_id: run.stream_id.clone() };
    let at = crate::openspec::change::working_copy(app, &location)?;
    let validation = seam.validate_change(app, &BUFFER, active.as_ref(), &at, change)?;
    let progress = seam.apply_instructions(app, &BUFFER, active.as_ref(), &at, change)?;
    Ok(PlanReads { validation, progress })
}

/// GRL-FR-GQAB: findings in the order of the verdict, as an instruction.
fn instruction_of(findings: &[ReviewFinding]) -> String {
    let verdict = ReviewVerdict {
        verdict: ReviewOutcome::Revise,
        rationale: String::new(),
        findings: findings.to_vec(),
    };
    driver::turns::compose_instruction(&verdict)
}

/// OPU-FR-ZNRR, OPU-FR-SKTV: each hunk that is not withdrawn, in proposal
/// order, with its position among the hunks of its path and its outcome.
fn hunk_outcomes(proposal: &crate::change_proposals::ChangeEditProposal) -> Vec<PlanHunkOutcome> {
    use crate::prompt_proposals::HunkState;
    let mut seen: HashMap<&str, u32> = HashMap::new();
    let mut out = Vec::new();
    for row in proposal.ledger.iter().filter(|row| row.state != HunkState::Withdrawn) {
        let position = seen.entry(row.path.as_str()).or_insert(0);
        *position += 1;
        let outcome = match row.state {
            HunkState::Accepted if row.edited => "edited",
            HunkState::Accepted => "accepted",
            _ => "rejected",
        };
        out.push(PlanHunkOutcome {
            path: row.path.clone(),
            position: *position,
            outcome: outcome.to_string(),
        });
    }
    out
}

/// OPU-FR-ZNRR: one ledger entry for each hunk, `<path> edit <n>`, with the
/// feedback of its decision as the note.
fn hunk_entries(
    proposal: Option<&crate::change_proposals::ChangeEditProposal>,
    hunks: &[PlanHunkOutcome],
    pass: u32,
) -> Vec<GraduationDecision> {
    let Some(proposal) = proposal else {
        return Vec::new();
    };
    let rows = proposal
        .ledger
        .iter()
        .filter(|row| row.state != crate::prompt_proposals::HunkState::Withdrawn);
    rows.zip(hunks)
        .map(|(row, hunk)| GraduationDecision {
            question: format!("{} edit {}", hunk.path, hunk.position),
            answer: hunk.outcome.clone(),
            summary: hunk.outcome.clone(),
            note: row.feedback.clone(),
            turn: ORIGIN_WORK.to_string(),
            pass,
        })
        .collect()
}

/// OPU-FR-CAGI: one ledger entry for each question, in position order. A
/// chosen option gives its `answer` and its `summary`; the author's own words
/// are both.
fn answer_entries(rest: &GraduationRest, pass: u32) -> Vec<GraduationDecision> {
    let questions = rest.questions.clone().unwrap_or_default();
    let answers = rest.answers.clone().unwrap_or_default();
    let mut ordered: Vec<&GraduationEscalationQuestion> = questions.iter().collect();
    ordered.sort_by_key(|q| q.position);
    ordered
        .into_iter()
        .filter_map(|question| {
            let given =
                answers.iter().find(|a| a.question_position == question.position as usize)?;
            let chosen =
                given.option_position.and_then(|at| question.options.get(at.checked_sub(1)?));
            let (answer, summary) = match (chosen, given.own_answer.as_ref()) {
                (Some(option), _) => (option.answer.clone(), option.summary.clone()),
                (None, Some(own)) => (own.clone(), own.clone()),
                (None, None) => return None,
            };
            Some(GraduationDecision {
                question: question.question.clone(),
                answer,
                summary,
                note: given.note.clone().filter(|n| !n.trim().is_empty()),
                turn: ORIGIN_WORK.to_string(),
                pass,
            })
        })
        .collect()
}
