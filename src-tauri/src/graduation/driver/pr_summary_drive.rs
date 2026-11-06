//! The one turn of a PR summary run (`../ai/GRL-graduation-loop.md`
//! GRL-FR-UNKD, GRL-FR-ZZGO; `GRD-graduation.md` GRD-FR-TYVT, GRD-FR-SWHW,
//! GRD-FR-CCHK).
//!
//! A PR summary run has one turn of part `pr_summary` and turn kind
//! `pr_maintenance`. It has no review, no pass budget and no escalation answer.
//! A valid answer completes the run. Every other end of the turn fails it with
//! its typed cause, except a stop the author or the application made, which
//! rests it `interrupted` so Continue dispatches the turn again.

use super::corrections::{self, Corrected};
use super::dispatch::TurnRequest;
use super::drive::settings_for_dispatch;
use super::stops::{self, Stop};
use super::turns::{cancellation_reason, cancellation_reason_of, discarded_meanwhile};
use super::*;
use crate::graduation::logs::{self, GraduationLogLevel, GraduationLogProducer, StructuredEvent};
use crate::graduation::{GraduationFailure, RunSummary};
use crate::tools::agent_exec::protocol::AgentOutcome;

/// GRD-FR-SWHW: the typed causes a PR summary run fails with, beside the
/// interruption reasons a turn that stopped on its own names.
pub(crate) const FAIL_ESCALATED: &str = "pr_summary_escalated";
pub(crate) const FAIL_AGENT_REPORTED: &str = "agent_reported_failure";
pub(crate) const FAIL_CHECKOUT: &str = "pr_summary_checkout_failed";
pub(crate) const FAIL_NO_MERGE_BASE: &str = "no_merge_base";
pub(crate) const FAIL_TOTAL_TIME: &str = "total_time_limit";
pub(crate) const FAIL_TASK_INVALID: &str = "task_invalid";

/// GRL-FR-UNKD: drive the one turn of a PR summary run to its end.
pub(super) async fn drive<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    cancellation: &CancellationToken,
    dispatch: &dyn seams::GraduationDispatch<R>,
) {
    // GRD-FR-CCHK: Continue dispatches the turn again from the start, so no
    // correction or session of an earlier dispatch is carried into it.
    if run.checkpoint.answer_correction.is_some() || run.checkpoint.resume.is_some() {
        run.checkpoint.answer_correction = None;
        run.checkpoint.resume = None;
        let _ = crate::graduation::save_run(app, run);
    }
    // GRD-FR-TYVT: the head tip and the merge base are pinned once.
    if !pin(app, run) {
        return;
    }
    // GRS-FR-EIXS: a required stream that cannot be written stops the run
    // before anything else happens.
    let dispatched = StructuredEvent::new(GraduationLogProducer::QueueWait, "run dispatched")
        .with("mode", mode_of(run))
        .with("pass", 1u32);
    if !logs::emit(app, run, dispatched) {
        return;
    }
    if let Some(reason) = cancellation_reason(app, &run.id, cancellation) {
        rest(app, run, Stop::of_reason(reason));
        return;
    }
    // GRL-FR-UNKD / GRL-FR-KWNP: the two time limits, read before the dispatch.
    let settings = settings_for_dispatch(app, run, phases::PART_PR_SUMMARY);
    if settings.remaining_total_ms(run.turn_time_used_ms) == 0 {
        fail(
            app,
            run,
            FAIL_TOTAL_TIME,
            "The run has used its total time limit, so no turn was dispatched.",
        );
        return;
    }
    let Some(input) = task_input::compose_pr_summary_input(run) else {
        return;
    };
    if let Err(reason) = input.validate(0) {
        fail(app, run, FAIL_TASK_INVALID, &reason);
        return;
    }
    let _ = transitions::enter_stage(
        app,
        run,
        observability::GraduationVisualStage::Working,
        observability::StageReason::WorkStarted,
    );
    if transitions::transition(app, run, GraduationRunState::Working).is_err() {
        return;
    }
    run.work_turns += 1;
    let started = StructuredEvent::new(GraduationLogProducer::WorkTurn, "work turn started")
        .with("part", phases::PART_PR_SUMMARY)
        .with("turn", run.work_turns)
        .with("corrected", false);
    if !logs::emit(app, run, started) {
        return;
    }
    let checkout = match pr_summary_checkout::create(app, run) {
        Ok(path) => path,
        Err(reason) => {
            logging::log_error(
                app,
                &crate::logging::BUFFER,
                &[Domain::Backend],
                "graduation could not make the checkout of a PR summary run",
                log_fields! { "run_id" => run.id.clone(), "reason" => reason },
            );
            // Whatever the attempt left is removed with it.
            let _ = pr_summary_checkout::release(app, run);
            fail(
                app,
                run,
                FAIL_CHECKOUT,
                "The application could not make a checkout of the head branch.",
            );
            return;
        }
    };
    let bound = settings.turn_bound(run.turn_time_used_ms);
    let turns_before = run.work_turns;
    let turn = TurnRequest {
        execution_directory: &checkout,
        part: phases::PART_PR_SUMMARY,
        input: input.clone(),
        bound,
        resume: None,
    };
    // GRL-FR-QKDC / GRL-FR-ZZGO: an unreadable answer is corrected once, in the
    // checkout of the turn it corrects.
    let mut corrected =
        corrections::execute_with_correction(app, run, turn, cancellation, dispatch).await;
    // GRL-FR-ZZGO: an answer the executor took but that is not a summary this
    // application can read is corrected once too, where no correction ran yet.
    if run.work_turns == turns_before {
        if let Corrected::Settle(outcome, _) = &corrected {
            if let Some(session) = unreadable_in_session(app, run, outcome, cancellation) {
                corrected =
                    correct_answer(app, run, &checkout, input, session, cancellation, dispatch)
                        .await;
            }
        }
    }
    // GRD-FR-TYVT: the checkout is gone once the turn has ended, whatever it
    // answered. A release that does not finish is reported, and the next
    // dispatch or the terminal reclaim finishes it.
    if let Err(reason) = pr_summary_checkout::release(app, run) {
        logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation could not remove the checkout of a PR summary run",
            log_fields! { "run_id" => run.id.clone(), "reason" => reason },
        );
    }
    match corrected {
        Corrected::Settle(outcome, bound) => settle(app, run, *outcome, bound),
        Corrected::Stopped => {}
    }
}

/// GRL-FR-ZZGO: the session of a completed turn whose answer is not a summary
/// this application can read, where that answer is one a correction can repair.
///
/// `None` for a readable answer, an escalation, a reported failure, a turn with
/// no session, and a run that was stopped or discarded while the turn ran.
fn unreadable_in_session<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
    outcome: &Result<AgentExecution, AgentExecutionError>,
    cancellation: &CancellationToken,
) -> Option<SessionRef> {
    let execution = outcome.as_ref().ok()?;
    if execution.process_outcome != ProcessOutcome::Completed {
        return None;
    }
    let response = execution.response.as_ref()?;
    if response.escalation.is_some()
        || response.outcome != AgentOutcome::Success
        || read_pull_request_summary(execution).is_some()
    {
        return None;
    }
    if cancellation.is_cancelled() || cancellation_reason_of(app, &run.id).is_some() {
        return None;
    }
    let session = execution.session.as_ref().or(response.session.as_ref())?;
    session.session_id.as_ref().filter(|id| !id.is_empty())?;
    Some(session.clone())
}

/// GRL-FR-ZZGO: ask the turn once more, in its own session and checkout, for an
/// answer this application can read.
async fn correct_answer<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    checkout: &std::path::Path,
    mut input: task_input::GraduationTaskInput,
    session: SessionRef,
    cancellation: &CancellationToken,
    dispatch: &dyn seams::GraduationDispatch<R>,
) -> Corrected {
    logging::log_warn(
        app,
        &crate::logging::BUFFER,
        &[Domain::Ai, Domain::Backend],
        "graduation asks a PR summary turn again because its answer could not be read",
        log_fields! { "run_id" => run.id.clone() },
    );
    let settings = settings_for_dispatch(app, run, phases::PART_PR_SUMMARY);
    if settings.remaining_total_ms(run.turn_time_used_ms) == 0 {
        fail(
            app,
            run,
            FAIL_TOTAL_TIME,
            "The run has used its total time limit, so the answer was not asked for again.",
        );
        return Corrected::Stopped;
    }
    input.correction = prompts::prompt_section(&prompts::REFUSALS, REFUSAL_PR_SUMMARY_RESULT);
    run.work_turns += 1;
    let started = StructuredEvent::new(GraduationLogProducer::WorkTurn, "work turn started")
        .with("part", phases::PART_PR_SUMMARY)
        .with("turn", run.work_turns)
        .with("corrected", true)
        .with("refusal", REFUSAL_PR_SUMMARY_RESULT);
    if !logs::emit(app, run, started) {
        return Corrected::Stopped;
    }
    let bound = settings.turn_bound(run.turn_time_used_ms);
    let turn = TurnRequest {
        execution_directory: checkout,
        part: phases::PART_PR_SUMMARY,
        input,
        bound,
        resume: Some(session),
    };
    let outcome = super::dispatch::execute(app, run, turn, cancellation, dispatch).await;
    Corrected::Settle(Box::new(outcome), bound)
}

/// GRL-FR-ZZGO: the section of `refusals.md` an unreadable PR summary is asked
/// again with.
pub const REFUSAL_PR_SUMMARY_RESULT: &str = "pr_summary_result";

/// The mode of a PR summary run, for its records.
fn mode_of(run: &GraduationRun) -> &'static str {
    run.pr_summary.as_ref().map(|data| data.mode.as_str()).unwrap_or_default()
}

/// GRD-FR-TYVT: pin the head tip and the merge base at the first dispatch.
///
/// Answers `false` where the run failed instead, having recorded why.
fn pin<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &mut GraduationRun) -> bool {
    let Some(data) = run.pr_summary.as_ref() else {
        return false;
    };
    if data.head_tip.is_some() && data.merge_base.is_some() {
        return true;
    }
    let pinned = (|| -> Result<(git2::Oid, git2::Oid), (&'static str, &'static str)> {
        const GONE: (&str, &str) =
            ("unknown branch", "The head branch or the base branch is not a branch any more.");
        let repo = crate::graduation::pr_summary::project_repo(app).map_err(|_| GONE)?;
        let head = crate::changes::resolve_branch_commit(&repo, &data.head).map_err(|_| GONE)?;
        let base = crate::changes::resolve_branch_commit(&repo, &data.base).map_err(|_| GONE)?;
        let merge_base = repo.merge_base(head, base).map_err(|_| {
            (FAIL_NO_MERGE_BASE, "The head branch and the base branch share no commit.")
        })?;
        Ok((head, merge_base))
    })();
    match pinned {
        Ok((head, merge_base)) => {
            if let Some(data) = run.pr_summary.as_mut() {
                data.head_tip = Some(head.to_string());
                data.merge_base = Some(merge_base.to_string());
            }
            let _ = crate::graduation::save_run(app, run);
            logging::log_info(
                app,
                &crate::logging::BUFFER,
                &[Domain::Backend],
                "graduation pinned the head of a PR summary run",
                log_fields! {
                    "run_id" => run.id.clone(),
                    "head_tip" => head.to_string(),
                    "merge_base" => merge_base.to_string(),
                },
            );
            true
        }
        Err((code, message)) => {
            fail(app, run, code, message);
            false
        }
    }
}

/// GRD-FR-SWHW / GRD-FR-CCHK: what the end of the turn means for the run.
fn settle<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    outcome: Result<AgentExecution, AgentExecutionError>,
    bound: settings::TurnBound,
) {
    // GRD-FR-EWTN: a run discarded while its turn ran takes nothing.
    if discarded_meanwhile(app, run) {
        return;
    }
    // GRD-FR-CCHK: a stop the author or the application made while the turn ran
    // rests the run `interrupted`, whatever the turn answered.
    if let Some(reason) = cancellation_reason_of(app, &run.id) {
        let stop = match &outcome {
            Ok(execution) => {
                Stop::of_reason_with(reason, stops::facts_of(execution, bound.limit_ms))
            }
            Err(_) => Stop::of_reason(reason),
        };
        rest(app, run, stop);
        return;
    }
    match outcome {
        Ok(execution) => match execution.process_outcome {
            ProcessOutcome::Completed => settle_answer(app, run, &execution),
            // GRD-FR-CCHK: a cancelled turn whose cause was not recorded rests
            // the run too.
            ProcessOutcome::Cancelled => rest(
                app,
                run,
                Stop::of_reason_with(
                    GraduationInterruptionReason::RetryableFailure,
                    stops::facts_of(&execution, bound.limit_ms),
                ),
            ),
            _ => {
                let stop =
                    stops::stop_of_execution(&execution, bound.limit_ms).unwrap_or_else(|| {
                        Stop::of_reason(GraduationInterruptionReason::RetryableFailure)
                    });
                fail(app, run, stop.reason.as_str(), &stop.detail);
            }
        },
        Err(AgentExecutionError::DurableOutputFailed(failure)) => {
            logs::note_durable_failure(app, run, &failure);
            let reason = GraduationInterruptionReason::LogPersistenceFailed;
            fail(app, run, reason.as_str(), stops::detail_of(reason));
        }
        Err(error) => {
            let mut fields = log_fields! {
                "run_id" => run.id.clone(),
                "kind" => stops::launch_error_kind(&error),
            };
            if let Some(text) = stops::launch_error_text(&error) {
                fields.insert("error".to_string(), text.into());
            }
            logging::log_error(
                app,
                &crate::logging::BUFFER,
                &[Domain::Ai, Domain::Backend],
                "graduation could not launch the turn of a PR summary run",
                fields,
            );
            let stop = stops::stop_of_launch_error(&error, bound.limit_ms);
            fail(app, run, stop.reason.as_str(), &stop.detail);
        }
    }
}

/// GRL-FR-ZZGO: a completed turn that answered with a `PullRequestSummary`
/// completes the run. An escalation, a reported failure and an unreadable
/// answer each fail it.
fn settle_answer<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    execution: &AgentExecution,
) {
    let response = execution.response.as_ref();
    if response.is_some_and(|r| r.escalation.is_some()) {
        fail(
            app,
            run,
            FAIL_ESCALATED,
            "The agent asked a question, and a PR summary run has no answer to one.",
        );
        return;
    }
    if response.is_some_and(|r| r.outcome == AgentOutcome::Failure) {
        fail(
            app,
            run,
            FAIL_AGENT_REPORTED,
            "The agent reported that it could not write the summary.",
        );
        return;
    }
    match read_pull_request_summary(execution) {
        Some(summary) => complete(app, run, summary),
        None => {
            let reason = GraduationInterruptionReason::UnreadableAnswer;
            fail(
                app,
                run,
                reason.as_str(),
                "The agent's answer is not a title and a description this application can read.",
            );
        }
    }
}

/// GRL-FR-ZZGO: the `PullRequestSummary` a turn answered with.
pub(crate) fn read_pull_request_summary(execution: &AgentExecution) -> Option<RunSummary> {
    let result = execution.response.as_ref()?.result.as_ref()?;
    RunSummary::read(&serde_json::Value::Object(result.clone()))
}

/// GRD-FR-SWHW: record the answer and complete the run.
fn complete<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    summary: RunSummary,
) {
    // The lengths alone: the title and the description are user content.
    let recorded = StructuredEvent::new(GraduationLogProducer::WorkTurn, "pr summary recorded")
        .with("title_chars", summary.title.chars().count() as u64)
        .with("description_chars", summary.description.chars().count() as u64);
    if !logs::emit(app, run, recorded) {
        return;
    }
    if let Some(data) = run.pr_summary.as_mut() {
        data.result = Some(summary);
    }
    let _ = transitions::enter_stage(
        app,
        run,
        observability::GraduationVisualStage::Done,
        observability::StageReason::Finished,
    );
    let _ = transitions::transition(app, run, GraduationRunState::Completed);
    logging::log_info(
        app,
        &crate::logging::BUFFER,
        &[Domain::Ai, Domain::Backend],
        "graduation completed a PR summary run",
        log_fields! { "run_id" => run.id.clone(), "mode" => mode_of(run) },
    );
}

/// GRD-FR-SWHW: end the run `failed` with its typed cause. No result is
/// recorded, and the run holds neither its queue nor a slot (GRD-FR-CCHK).
fn fail<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    code: &str,
    message: &str,
) {
    logging::log_warn(
        app,
        &crate::logging::BUFFER,
        &[Domain::Ai, Domain::Backend],
        "graduation failed a PR summary run",
        log_fields! { "run_id" => run.id.clone(), "code" => code.to_string() },
    );
    if let Some(data) = run.pr_summary.as_mut() {
        data.result = None;
    }
    run.failure = Some(GraduationFailure {
        code: code.to_string(),
        message: message.to_string(),
        retryable: false,
    });
    let failed = StructuredEvent::new(GraduationLogProducer::StageTransition, "run failed")
        .at_level(GraduationLogLevel::Error)
        .leaving(run.observability.current_stage)
        .with("code", code.to_string());
    let _ = logs::emit(app, run, failed);
    let _ = transitions::enter_stage(
        app,
        run,
        observability::GraduationVisualStage::Done,
        observability::StageReason::Ended,
    );
    let _ = transitions::transition(app, run, GraduationRunState::Failed);
}

/// GRD-FR-CCHK: rest the run `interrupted`, so Continue dispatches the turn
/// again from the start.
fn rest<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &mut GraduationRun, stop: Stop) {
    if !stops::note_interrupted(app, run, &stop) {
        return;
    }
    let _ = transitions::interrupt(app, run, stop.reason, stop.detail);
}
