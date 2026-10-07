//! What a completed turn means for the run, and what it leaves behind
//! (`GRD-graduation.md` GRD-FR-ARLT, GRD-FR-SWOJ, `GXD-graduation-execution.md`
//! GXD-FR-LBYG, GXD-FR-XEUX).
//!
//! Split out of the loop itself so `drive.rs` holds the two phases and their
//! order, and this file holds what one turn's outcome is read as. The loop is
//! unchanged by the move: the module root re-exports nothing, and every name
//! here is reached from `drive` alone.

use super::drive::Step;
use super::*;
use crate::graduation::commit;
use super::stops::{self, Stop};
use crate::graduation::logs::{self, GraduationLogLevel, GraduationLogProducer, StructuredEvent};
use std::path::PathBuf;

/// What a completed turn means for the run.
pub(super) fn settle_turn<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &PathBuf,
    outcome: Result<AgentExecution, AgentExecutionError>,
    origin: GraduationEscalationOrigin,
    limit_ms: u64,
) -> Step {
    match settle_execution(app, run, worktree, outcome, origin, limit_ms) {
        Some(_) => {
            // GXD-FR-LBYG: what changed is read from the working copy, never
            // from what the agent said it did.
            run.checkpoint.changed_paths = changed_paths(worktree, run);
            let (hidden, omitted) = hidden_paths(worktree, run);
            run.checkpoint.hidden_paths = hidden;
            run.checkpoint.hidden_paths_omitted = omitted;
            // GXD-FR-XPUR: the answers were delivered into the turn that asked,
            // and that turn has now ended. Left standing they would be sent
            // again by every later turn of the run, which would also keep
            // resuming the session of the turn that asked — and would make the
            // revise purpose unreachable for the rest of the run.
            run.checkpoint.pending_escalation_answers.clear();
            run.checkpoint.resume = None;
            // GRL-FR-QZFB: the turn ran, so whatever blocked the run before it
            // is behind the run rather than in front of it.
            transitions::clear_block_count(run);
            // The turn's own output went to the source stream from the thread
            // that read it, so the record is brought up to what both threads
            // wrote before it is saved.
            logs::refresh(app, run);
            let _ = crate::graduation::save_run(app, run);
            Step::Continue
        }
        None => Step::Stop,
    }
}

/// The outcomes every turn shares, and what each rests the run on.
///
/// `limit_ms` is the execution timeout the turn was dispatched under, so a
/// timeout names the limit the turn actually reached (GXD-FR-WJOW).
pub(super) fn settle_execution<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &PathBuf,
    outcome: Result<AgentExecution, AgentExecutionError>,
    origin: GraduationEscalationOrigin,
    limit_ms: u64,
) -> Option<AgentExecution> {
    // GRD-FR-EWTN: a run discarded while its turn ran takes nothing of what the
    // turn answered, and nothing is saved over the discarded record.
    if discarded_meanwhile(app, run) {
        return None;
    }
    // GRD-FR-MDQZ / GRD-FR-TWMA: a stop the author or the application made
    // while the turn ran stands, whatever the turn answered.
    if let Some(reason) = stored_stop_reason(app, &run.id) {
        let stop = match &outcome {
            Ok(execution) => Stop::of_reason_with(reason, stops::facts_of(execution, limit_ms)),
            Err(_) => Stop::of_reason(reason),
        };
        abandon(app, run, worktree, stop);
        return None;
    }
    match outcome {
        Ok(execution) => match &execution.process_outcome {
            ProcessOutcome::Completed => {
                let response = execution.response.as_ref();
                // GRL-FR-VBCL: an escalation ends the turn without a review.
                if let Some(escalation) = response.and_then(|r| r.escalation.as_ref()) {
                    let questions = escalation
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
                        .collect::<Vec<_>>();
                    if crate::graduation::validate_escalation_request(&escalation.reason, &questions)
                        .is_ok()
                    {
                        // GXD-FR-XPUR: a review's answers go into a fresh review
                        // turn, so its session is not kept and the run resumes
                        // at the review rather than at a work turn.
                        let resume = if origin == GraduationEscalationOrigin::Review {
                            run.checkpoint.resume_part = Some(phases::PART_REVIEW.to_string());
                            None
                        } else {
                            execution.response.as_ref().and_then(|r| {
                                r.session.as_ref().map(|s| GraduationResumeRef {
                                    session_id: s.session_id.clone(),
                                    continuation_token: s.continuation_token.clone(),
                                })
                            })
                        };
                        let _ = crate::graduation::record_escalation(
                            app,
                            run,
                            escalation.reason.clone(),
                            questions,
                            origin,
                            resume,
                        );
                        let _ =
                            transitions::transition(app, run, GraduationRunState::AwaitingAuthor);
                        return None;
                    }
                }
                // GRL-FR-DNKA: the work turn's own account of what it did and
                // did not do, carried to the review that judges what it wrote —
                // on **every** turn that completed, whichever outcome it
                // reported.
                //
                // Not gated on the outcome, because the account the review most
                // needs comes from a turn that reported **success**: one that
                // wrote real work and knows it left some of what was asked
                // undone reports success, that being the only outcome that does
                // not misdescribe what it wrote, and says the rest in its
                // summary. Gated on failure, that account is dropped and the
                // review judges the change set with no knowledge that its author
                // said it was incomplete.
                //
                // A failed turn's account is its failure message where it gave
                // one, and its summary otherwise; a successful turn's is its
                // summary. A blank account is none at all, so an agent that
                // reported nothing adds no empty line to the review's input.
                if origin == GraduationEscalationOrigin::Work {
                    run.checkpoint.agent_account = response
                        .map(|r| {
                            // The failure's message where it says something, and
                            // the summary otherwise. A `Failure` carrying a blank
                            // message still has a summary worth reading, so the
                            // fallback turns on the message being **empty**
                            // rather than on the failure being absent.
                            r.failure
                                .as_ref()
                                .map(|failure| failure.message.trim())
                                .filter(|message| !message.is_empty())
                                .map(str::to_string)
                                .unwrap_or_else(|| r.summary.clone())
                        })
                        .filter(|account| !account.trim().is_empty());
                }
                Some(execution)
            }
            // GRD-FR-SWOJ: a turn that stopped without a review has whatever it
            // wrote committed, so the stream is left usable by the run behind
            // it rather than carrying work nothing accounts for.
            //
            // GXD-FR-UZHX: a stop the author or the application recorded wins,
            // so a pause that races the time limit reads as the pause. Otherwise
            // the run rests on the reason that names how the turn ended.
            _ => {
                let stop = match cancellation_reason_of(app, &run.id) {
                    Some(reason) => {
                        Stop::of_reason_with(reason, stops::facts_of(&execution, limit_ms))
                    }
                    None => stops::stop_of_execution(&execution, limit_ms)
                        .unwrap_or_else(|| Stop::of_reason(GraduationInterruptionReason::RetryableFailure)),
                };
                logging::log_warn(
                    app,
                    &crate::logging::BUFFER,
                    &[Domain::Ai, Domain::Backend],
                    "graduation turn stopped without an answer",
                    log_fields! {
                        "run_id" => run.id.clone(),
                        "reason" => stop.reason.as_str(),
                        "process_outcome" => execution.process_outcome.as_str(),
                        "duration_ms" => execution.duration_ms as i64,
                        "limit_ms" => limit_ms as i64,
                        "exit_code" => execution.exit_code,
                    },
                );
                abandon(app, run, worktree, stop);
                None
            }
        },
        // GLG-FR-UCRL / GXD-FR-MMFM: the executor stopped the turn because the
        // run's activity could not be kept. The run rests on
        // `log_persistence_failed` and names the failure, whatever outcome the
        // stopped container reported.
        Err(AgentExecutionError::DurableOutputFailed(failure)) => {
            logging::log_error(
                app,
                &crate::logging::BUFFER,
                &[Domain::Ai, Domain::Backend],
                "graduation stopped a turn because its activity could not be kept",
                log_fields! {
                    "run_id" => run.id.clone(),
                    "code" => failure.code.clone(),
                },
            );
            logs::note_durable_failure(app, run, &failure);
            abandon(
                app,
                run,
                worktree,
                Stop::of_reason(GraduationInterruptionReason::LogPersistenceFailed),
            );
            None
        }
        Err(error) => {
            // GXD-FR-XEUX: the seam ends no run. Every pre-launch error rests
            // it instead, so a machine that could not start a container leaves
            // something to continue.
            //
            // GXD-FR-WJOW: the author reads one sentence for the kind of error;
            // its full text goes to the application log alone.
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
                "graduation could not launch a turn",
                fields,
            );
            abandon(app, run, worktree, stops::stop_of_launch_error(&error, limit_ms));
            None
        }
    }
}

/// GRD-FR-ARLT: a review that let the work through commits it onto the stream.
pub(super) fn finish<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &PathBuf,
) {
    // GRD-FR-VAUE: a direct run commits only on the branch it pinned, in the
    // worktree it pinned. Anything else commits nothing and rests the run.
    if !crate::graduation::commit_target_ok(run, worktree) {
        note_blocked(app, run, "direct_target_changed");
        let _ = transitions::block(
            app,
            run,
            "direct_target_changed",
            "The worktree is no longer on the branch this run was started on, so nothing was committed.",
            "Check the pinned branch out again, then continue the run.",
            phases::PART_REVIEW,
        );
        return;
    }
    let base = run.base_commit.clone().unwrap_or_default();
    let message = graduation_message(run);
    match commit::commit_run_work(worktree, &base, &message) {
        Ok(Some(committed)) => {
            run.commits.push(committed.revision);
            run.checkpoint.changed_paths = committed.paths;
        }
        Ok(None) => {
            // GRD-FR-WQTN: the branch head already holds the reviewed tree. Where
            // that head is this run's own abandoned turn, the run completes by
            // giving that commit its graduation message.
            let retitled = match run.commits.last() {
                Some(last) => commit::retitle_head_commit(worktree, last, ABANDONED_TITLE, &message),
                None => Ok(None),
            };
            match retitled {
                Ok(Some(revision)) => {
                    if let Some(slot) = run.commits.last_mut() {
                        *slot = revision;
                    }
                    logging::log_info(
                        app,
                        &crate::logging::BUFFER,
                        &[Domain::Backend],
                        "graduation gave an abandoned turn's commit the run's message",
                        log_fields! { "run_id" => run.id.clone() },
                    );
                }
                Ok(None) => {}
                Err(reason) => {
                    note_blocked(app, run, "commit_failed");
                    let _ = transitions::block(
                        app,
                        run,
                        "commit_failed",
                        reason,
                        "Resolve the stream's state, then continue the run.",
                        phases::PART_REVIEW,
                    );
                    return;
                }
            }
        }
        Err(reason) => {
            note_blocked(app, run, "commit_failed");
            let _ = transitions::block(
                app,
                run,
                "commit_failed",
                reason,
                "Resolve the stream's state, then continue the run.",
                phases::PART_REVIEW,
            );
            return;
        }
    }
    let committed = StructuredEvent::new(GraduationLogProducer::Commit, "work committed")
        .with("commits", run.commits.len() as u64)
        .with("paths", run.checkpoint.changed_paths.len() as u64);
    if !logs::emit(app, run, committed) {
        return;
    }
    let _ = transitions::enter_stage(
        app,
        run,
        observability::GraduationVisualStage::Done,
        observability::StageReason::Finished,
    );
    let _ = transitions::transition(app, run, GraduationRunState::Completed);
}

/// GRD-FR-SWOJ: the title every abandoned turn's commit opens with.
pub(super) const ABANDONED_TITLE: &str = "Abandoned graduation turn";

/// GRD-FR-SWOJ: a turn that stopped without a review has whatever it wrote
/// committed as an abandoned turn, so the stream is left usable.
pub(super) fn abandon<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &PathBuf,
    stop: Stop,
) {
    let reason = stop.reason;
    // GRD-FR-EWTN: the turn of a discarded run commits nothing, and its work
    // stays uncommitted in the working copy.
    if discarded_meanwhile(app, run) {
        return;
    }
    // GXD-FR-TJRV: a run the author or the application stopped in the review
    // resumes at the review. The work did not change while it rested.
    if run.state == GraduationRunState::Reviewing && stops_in_place(reason) {
        run.checkpoint.resume_part = Some(phases::PART_REVIEW.to_string());
    }
    // GRD-FR-VAUE: the commit an abandoned turn makes is held to the same pin.
    let pinned = crate::graduation::commit_target_ok(run, worktree);
    if !pinned {
        logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation did not commit a stopped turn: the pinned branch is not checked out",
            log_fields! { "run_id" => run.id.clone() },
        );
    }
    // GXD-FR-WJOW: what became of the turn's work, for the detail the author
    // reads. `None` is work that is not committed, for whatever cause.
    let mut committed = None;
    // A merge run commits nothing at all: what its turn wrote stays in the merge
    // worktree the run owns, and Continue resumes from it (GRD-FR-KZPT).
    if let Some(base) = run
        .base_commit
        .clone()
        .filter(|_| pinned && !run.is_merge())
    {
        let message = format!(
            "{ABANDONED_TITLE}\n\nThis turn of \"{}\" stopped before it was reviewed.",
            run.input.draft_name
        );
        match commit::commit_run_work(worktree, &base, &message) {
            Ok(Some(made)) => {
                run.commits.push(made.revision);
                run.checkpoint.changed_paths = made.paths;
                committed = Some(true);
            }
            Ok(None) => committed = Some(false),
            Err(reason) => logging::log_warn(
                app,
                &crate::logging::BUFFER,
                &[Domain::Backend],
                "graduation could not commit what a stopped turn wrote",
                log_fields! { "run_id" => run.id.clone(), "reason" => reason },
            ),
        }
    }
    let stop = Stop {
        detail: stops::settled_detail(&stop, committed),
        ..stop
    };
    // GLG-FR-RLQZ: the stop is in the run's own log before the run rests. A
    // record that could not be stored has already rested the run on
    // `log_persistence_failed`, which is the stop the author must see first.
    if !stops::note_interrupted(app, run, &stop) {
        return;
    }
    let _ = transitions::interrupt(app, run, reason, stop.detail);
}

/// Why a cancelled turn was cancelled, or `None` where it was not.
///
/// The token says only that the turn must stop. Who stopped it is what the
/// author is shown, so it is read from the run's own cancellation record rather
/// than assumed to be the author (GRD-FR-MDQZ, GRD-FR-TWMA).
pub(super) fn cancellation_reason<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    cancellation: &CancellationToken,
) -> Option<GraduationInterruptionReason> {
    if !cancellation.is_cancelled() {
        return None;
    }
    Some(cancellation_reason_of(app, run_id).unwrap_or(GraduationInterruptionReason::RetryableFailure))
}

/// What the run's own record says stopped it, whatever the token says.
///
/// GRD-FR-MDQZ / GRD-FR-TWMA: a pause, a shutdown and a project change each
/// interrupt the run before its turn returns, and that interruption releases
/// the stream and the in-memory reason with it. The stored interruption is then
/// the only record of who stopped the turn.
pub(super) fn cancellation_reason_of<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
) -> Option<GraduationInterruptionReason> {
    app.try_state::<GraduationState>()
        .and_then(|state| state.cancellation_reason(run_id))
        .or_else(|| stored_stop_reason(app, run_id))
}

/// The reason a command already recorded on a run it interrupted while this
/// loop drove it.
///
/// Only the stops a command or a relaunch makes are read. A stored reason of
/// any other kind is one the loop wrote itself on an earlier rest.
fn stored_stop_reason<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
) -> Option<GraduationInterruptionReason> {
    let stored = crate::graduation::load_run(app, run_id).ok()?;
    if stored.state != GraduationRunState::Interrupted {
        return None;
    }
    stored
        .interruption
        .map(|interruption| interruption.reason)
        .filter(|reason| {
            stops_in_place(*reason) || *reason == GraduationInterruptionReason::ExecutionAbandoned
        })
}

/// GXD-FR-TJRV: the stops after which a run resumes at the phase it stood in.
fn stops_in_place(reason: GraduationInterruptionReason) -> bool {
    matches!(
        reason,
        GraduationInterruptionReason::AuthorPause
            | GraduationInterruptionReason::ApplicationShutdown
            | GraduationInterruptionReason::ProjectChanged
    )
}

/// GRD-FR-EWTN: whether the author discarded the run while this loop drove it.
///
/// The store answers, not the copy in hand: the discard is written from
/// another thread while the turn runs.
fn discarded_meanwhile<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &GraduationRun) -> bool {
    let ended = crate::graduation::load_run(app, &run.id)
        .is_ok_and(|stored| stored.state.is_terminal());
    if ended {
        logging::log_info(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation left the work of an ended run uncommitted",
            log_fields! { "run_id" => run.id.clone() },
        );
    }
    ended
}

/// GRS-FR-EPPM: the run stopped on a condition, recorded against the phase it
/// was standing in.
///
/// A log failure here writes nothing further: the run is already resting on one,
/// and a second attempt would report the same thing twice.
pub(super) fn note_blocked<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &mut GraduationRun, code: &str) {
    let stage = run.observability.current_stage;
    let event = StructuredEvent::new(GraduationLogProducer::StageTransition, "run blocked")
        .at_level(GraduationLogLevel::Error)
        .leaving(stage)
        .with("code", code.to_string());
    let _ = logs::emit(app, run, event);
}

/// GRL-FR-ARPX: a second review that still asks for revision stops the run for
/// the author rather than starting a third pass.
pub(super) fn rest_for_author<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &mut GraduationRun) {
    if transitions::transition(app, run, GraduationRunState::AwaitingAuthor).is_err() {
        // A rest the record did not take is not one to write a line about.
        return;
    }
    // GRL-FR-XBUE: the run says what it spent, so the author reads a bound
    // rather than a review that stopped for no stated reason. The checkpoint is
    // kept whole, which is what lets Continue resume without repeating a pass.
    //
    // `passes_made` is counted from the window's floor, so a run whose budget
    // the author lowered mid-run reports the passes it actually made rather
    // than the size of the window it ended up under.
    let _ = crate::graduation::logs::emit(
        app,
        run,
        crate::graduation::logs::StructuredEvent::new(
            crate::graduation::logs::GraduationLogProducer::ReviewTurn,
            "run rested for the author",
        )
        .with("reason", REST_PASS_BUDGET_EXHAUSTED)
        .with("pass", run.pass())
        .with("pass_budget", run.pass_budget())
        .with(
            "passes_made",
            run.pass().saturating_sub(run.pass_floor()).saturating_add(1),
        ),
    );
}

/// GRL-FR-XBUE: why a run whose budget window is spent rests for the author.
pub const REST_PASS_BUDGET_EXHAUSTED: &str = "pass_budget_exhausted";

/// GRD-FR-YBUM / GRD-FR-HQPD: the revision this run is measured from.
///
/// The run's standing-work choice is applied here and nowhere else, so the
/// base commit and what the choice did to the stream are one act.
pub(super) fn establish_base<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &PathBuf,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    let outcome = crate::graduation::standing::apply(app, run, worktree, cancellation)?;
    // A choice that committed nothing, and one that found nothing standing,
    // both measure the run from the revision the branch already holds.
    run.base_commit = Some(match outcome.commit.clone() {
        Some(revision) => revision,
        None => {
            let repo = git2::Repository::open(worktree).map_err(|e| e.to_string())?;
            let head = repo
                .head()
                .and_then(|h| h.peel_to_commit())
                .map_err(|e| e.to_string())?;
            head.id().to_string()
        }
    });
    // GRD-FR-KDWA: written before the first turn starts, so a push the remote
    // refused is readable from the moment the run works.
    run.standing_work_outcome = Some(outcome.clone());
    crate::graduation::save_run(app, run)?;
    report_standing_work(app, run, &outcome);
    Ok(())
}

/// GRD-FR-KDWA: what the standing-work step did, in the run's own log.
///
/// A step that wrote nothing reports nothing: a run whose stream was clean did
/// not act on it, and a record saying so would stand in every run's log. The
/// write is not guarded here — the dispatch record that follows it is, so a log
/// stream that cannot be written stops the run one record later rather than in
/// two places (GRD-FR-IKVE).
fn report_standing_work<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    outcome: &crate::graduation::StandingWorkOutcome,
) {
    if outcome.commit.is_none() && outcome.pushed.is_none() {
        return;
    }
    // The queue wait is the phase this happens in: the run is dispatched after
    // it, and no turn has started.
    let mut event = StructuredEvent::new(GraduationLogProducer::QueueWait, "standing work settled")
        .with("choice", run.standing_work.as_str())
        .with("committed", outcome.commit.is_some());
    if let Some(pushed) = outcome.pushed {
        event = event.with("pushed", pushed);
    }
    if let Some(failure) = outcome.push_failure.as_ref() {
        event = event.with("push_failure", failure.code.clone());
    }
    let _ = logs::emit(app, run, event);
}

/// What the repository's own ignore rules kept out of the change set.
///
/// The change set is what the application computed by walking the tree under
/// those rules, so a file the turn wrote that one of them hides is absent from
/// it: nobody reviews it and no commit writes it. Naming what was kept out is
/// what lets a reader decide which of it is authored work rather than build
/// output — a decision no rule can make.
///
/// An ignored directory is reported as one entry rather than walked, so a
/// dependency tree costs one line instead of forty thousand.
pub(super) fn hidden_paths(worktree: &PathBuf, run: &GraduationRun) -> (Vec<String>, u32) {
    /// How many the input carries. The rest are counted rather than named.
    const BOUND: usize = 200;

    let Some(base) = run.base_commit.as_deref() else {
        return (Vec::new(), 0);
    };
    let Ok(repo) = git2::Repository::open(worktree) else {
        return (Vec::new(), 0);
    };
    let Ok(oid) = git2::Oid::from_str(base) else {
        return (Vec::new(), 0);
    };
    let Ok(tree) = repo.find_commit(oid).and_then(|c| c.tree()) else {
        return (Vec::new(), 0);
    };
    let mut opts = git2::DiffOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(true)
        .recurse_ignored_dirs(false);
    let Ok(diff) = repo.diff_tree_to_workdir_with_index(Some(&tree), Some(&mut opts)) else {
        return (Vec::new(), 0);
    };
    let mut paths: Vec<String> = Vec::new();
    for delta in diff.deltas() {
        if delta.status() != git2::Delta::Ignored {
            continue;
        }
        if let Some(path) = delta.new_file().path() {
            paths.push(path.to_string_lossy().into_owned());
        }
    }
    paths.sort();
    paths.dedup();
    let omitted = paths.len().saturating_sub(BOUND) as u32;
    paths.truncate(BOUND);
    (paths, omitted)
}

/// GXD-FR-LBYG: what the working copy holds against the base commit.
pub(super) fn changed_paths(worktree: &PathBuf, run: &GraduationRun) -> Vec<String> {
    let Some(base) = run.base_commit.as_deref() else {
        return Vec::new();
    };
    let Ok(repo) = git2::Repository::open(worktree) else {
        return Vec::new();
    };
    let Ok(oid) = git2::Oid::from_str(base) else {
        return Vec::new();
    };
    let Ok(tree) = repo.find_commit(oid).and_then(|c| c.tree()) else {
        return Vec::new();
    };
    let mut opts = git2::DiffOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let Ok(diff) = repo.diff_tree_to_workdir_with_index(Some(&tree), Some(&mut opts)) else {
        return Vec::new();
    };
    let mut paths: Vec<String> = Vec::new();
    for delta in diff.deltas() {
        for file in [delta.new_file(), delta.old_file()] {
            if let Some(path) = file.path() {
                paths.push(path.to_string_lossy().into_owned());
            }
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

/// The working copy the run's stream owns.
pub(super) fn stream_worktree<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
) -> Option<PathBuf> {
    crate::graduation::target_worktree(app, run)
}

/// GRL-FR-ISIL: why this turn is running.
pub(super) fn purpose_for(run: &GraduationRun) -> &'static str {
    if !run.checkpoint.pending_escalation_answers.is_empty() {
        task_input::PURPOSE_ESCALATION_ANSWER
    } else if run.checkpoint.loop_instruction.is_some() {
        task_input::PURPOSE_REVISE
    } else if run.work_turns > 0 {
        task_input::PURPOSE_RESUME
    } else {
        task_input::PURPOSE_GENERATE
    }
}

/// GXD-FR-XPUR: the vendor session a resumed turn continues.
pub(super) fn resume_for(run: &GraduationRun, part: &str) -> Option<SessionRef> {
    if part != phases::PART_WORK {
        return None;
    }
    let resume = run.checkpoint.resume.as_ref()?;
    let session_id = resume.session_id.clone()?;
    Some(SessionRef {
        session_id: Some(session_id),
        continuation_token: resume.continuation_token.clone(),
    })
}

/// GRL-FR-VIAT: the verdict a review turn answered with.
pub(super) fn read_verdict(execution: &AgentExecution) -> Option<ReviewVerdict> {
    let result = execution.response.as_ref()?.result.as_ref()?;
    let verdict: ReviewVerdict =
        serde_json::from_value(serde_json::Value::Object(result.clone())).ok()?;
    if verdict.rationale.trim().is_empty() {
        return None;
    }
    match verdict.verdict {
        // GRL-FR-VIAT: `ready` carries no finding; `revise` carries at least one.
        ReviewOutcome::Ready if !verdict.findings.is_empty() => None,
        ReviewOutcome::Revise if verdict.findings.is_empty() => None,
        _ => Some(verdict),
    }
}

/// GRL-FR-GQAB: the findings, in the order the review returned them.
pub(super) fn compose_instruction(verdict: &ReviewVerdict) -> String {
    let mut out = String::from("The review asked for these changes.\n");
    for (index, finding) in verdict.findings.iter().enumerate() {
        out.push_str(&format!(
            "\n{}. [{}] {}\n   Correction: {}\n",
            index + 1,
            finding.severity.as_str(),
            finding.description,
            finding.correction
        ));
        if !finding.affected_files.is_empty() {
            out.push_str(&format!("   Files: {}\n", finding.affected_files.join(", ")));
        }
    }
    out
}

/// GRD-FR-PQMX: the message of the commit a `ready` verdict makes, and the
/// message an abandoned-turn commit is rewritten to (GRD-FR-WQTN).
///
/// A direct run commits under its own name alone. Every other run commits
/// under the first line of its prompt, with the draft named in the body.
pub(super) fn graduation_message(run: &GraduationRun) -> String {
    const FALLBACK: &str = "Graduation run";
    if run.is_direct() {
        // A control character, a line break above all, would open a body.
        let name: String = run
            .input
            .draft_name
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        let name = name.trim();
        return if name.is_empty() { FALLBACK } else { name }.to_string();
    }
    let title = first_line(&run.input.prompt);
    let title = if title.is_empty() { FALLBACK.to_string() } else { title };
    format!("{title}\n\nGraduated from the draft \"{}\".", run.input.draft_name)
}

pub(super) fn first_line(text: &str) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("Graduation run");
    let line = line.trim_start_matches('#').trim();
    line.chars().take(72).collect()
}
