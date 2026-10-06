//! The one loop: work, then review, at most twice (GRL-FR-VYNX, GRL-FR-ARPX).

use super::stops::Stop;
use super::turns::*;
use super::*;
use crate::graduation::logs::{self, GraduationLogLevel, GraduationLogProducer, StructuredEvent};
use std::path::PathBuf;
use crate::graduation::is_worktree_queue_key;

/// What the loop does after a turn.
pub(super) enum Step {
    /// Go on to the next phase.
    Continue,
    /// The run has come to rest; the loop has already recorded why.
    Stop,
}

/// GRD-FR-EGWS: start a run, on the seams production binds.
///
/// Answers whether the run took its queue and a project slot. A refused claim
/// starts nothing.
pub fn spawn<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run_id: &str, stream_id: &str) -> bool {
    if intercepted(app, run_id, stream_id) {
        return false;
    }
    spawn_with(app, run_id, stream_id, Arc::new(seams::AgentCliDispatch))
}

/// GTE-FR-YAEB: a build under test that installs the tripwire never reaches the
/// production seam from the queue. The attempt is recorded where the loop would
/// have dispatched, with whether the stream was free for it at that moment, so
/// a scenario that let it happen fails and one that expected it can say the
/// stream was free.
#[cfg(test)]
fn intercepted<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run_id: &str, stream_id: &str) -> bool {
    let Some(tripwire) = app.try_state::<SpawnTripwire>() else {
        return false;
    };
    if app
        .try_state::<GraduationState>()
        .is_some_and(|state| state.loop_enabled())
    {
        let unclaimed = app
            .try_state::<GraduationState>()
            .is_some_and(|state| state.holder_of(stream_id).is_none());
        let unmarked = is_worktree_queue_key(stream_id)
            || crate::streams::stream_of(app, stream_id)
                .is_some_and(|stream| stream.busy_run_id.is_none());
        let unreconciled = app
            .try_state::<crate::streams::StreamState>()
            .is_none_or(|state| {
                !(state.has_update_job(stream_id)
                    || state.is_merging(stream_id)
                    || state.is_updating(stream_id))
            });
        tripwire.record(run_id, unclaimed && unmarked && unreconciled);
    }
    true
}

#[cfg(not(test))]
fn intercepted<R: tauri::Runtime>(_app: &tauri::AppHandle<R>, _run_id: &str, _stream_id: &str) -> bool {
    false
}

/// GTE-FR-YAEB: the runs the production queue tried to dispatch in a build
/// under test, each with whether its stream was free at the attempt.
#[cfg(test)]
#[derive(Default)]
pub struct SpawnTripwire {
    attempts: std::sync::Mutex<Vec<(String, bool)>>,
}

#[cfg(test)]
impl SpawnTripwire {
    fn record(&self, run_id: &str, stream_free: bool) {
        let mut attempts = self
            .attempts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        attempts.push((run_id.to_string(), stream_free));
    }

    /// Every attempt recorded so far, and none of them afterwards.
    pub(crate) fn take(&self) -> Vec<(String, bool)> {
        let mut attempts = self
            .attempts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::mem::take(&mut *attempts)
    }
}

/// The same, with the dispatch seam supplied.
///
/// A build under test binds a scripted implementation, so what a test drives is
/// the production loop with the execution agent removed rather than a parallel
/// arrangement.
pub fn spawn_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    stream_id: &str,
    dispatch: Arc<dyn seams::GraduationDispatch<R>>,
) -> bool {
    let Some(cancellation) = claim(app, run_id, stream_id) else {
        return false;
    };
    let app = app.clone();
    let run_id = run_id.to_string();
    let stream_id = stream_id.to_string();
    std::thread::spawn(move || {
        drive_blocking(&app, &run_id, &stream_id, cancellation, dispatch.as_ref());
    });
    true
}

/// GRD-FR-BNTC: take the stream for this run, or answer that it is not free.
///
/// The claim is the check. A queue already holding a run, and a project with
/// no free slot under its limit, both refuse here (GRD-FR-KKKN).
fn claim<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    stream_id: &str,
) -> Option<CancellationToken> {
    let state = app.try_state::<GraduationState>()?;
    if !state.loop_enabled() {
        return None;
    }
    let limit = crate::graduation::scheduler::concurrency_limit(app);
    let direct = crate::graduation::load_run(app, run_id)
        .map(|run| run.is_direct())
        .unwrap_or(false);
    let cancellation = CancellationToken::new();
    if !state.claim(stream_id, run_id, direct, limit, cancellation.clone()) {
        return None;
    }
    // GRD-FR-ZVNO: an ordinary worktree has no stream record to mark busy; its
    // lock is the claim above and the index entry of a dispatched direct run.
    let marked = if is_worktree_queue_key(stream_id) {
        Ok(())
    } else {
        crate::streams::claim_stream(app, stream_id, run_id)
    };
    if let Err(reason) = marked {
        state.release(stream_id);
        logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation could not mark a stream busy",
            log_fields! { "run_id" => run_id.to_string(), "reason" => reason },
        );
        return None;
    }
    state.begin_loop(run_id);
    Some(cancellation)
}

/// Drive one claimed run to rest, on a runtime of this thread's own.
fn drive_blocking<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    stream_id: &str,
    cancellation: CancellationToken,
    dispatch: &dyn seams::GraduationDispatch<R>,
) {
    if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        runtime.block_on(async {
            drive(app, run_id, cancellation, dispatch).await;
        });
        release_logs(app, run_id);
        release_and_advance(app, run_id, stream_id);
    } else {
        release_and_advance(app, run_id, stream_id);
    }
    // GRD-FR-KZPT: what an ended merge run owns goes once nothing stands in it.
    // The loop is marked finished first, because a run this process still drives
    // is not reclaimed (GRD-FR-PFMD).
    if let Some(state) = app.try_state::<GraduationState>() {
        state.end_loop(run_id);
    }
    // GRD-FR-ZHNV: no operation of a run outlives its loop.
    crate::graduation::end_run_progress(app, run_id);
    if let Ok(run) = crate::graduation::load_run(app, run_id) {
        crate::graduation::reclaim_ended_merge_run(app, &run);
    }
}

/// Forget the shared log indexes of a run this process has stopped driving.
fn release_logs<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run_id: &str) {
    logs::release(app, run_id);
}

/// Give the stream back, and start whatever waits behind this run.
///
/// GRD-FR-BNTC: a run resting `blocked` still holds its stream. The condition is
/// one the author clears, and the work it left standing in the working copy is
/// what they clear it against — so the queue waits rather than starting the run
/// behind it on top of that work.
fn release_and_advance<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    stream_id: &str,
) {
    let holds = crate::graduation::load_run(app, run_id)
        .map(|run| run.holds_stream())
        .unwrap_or(false);
    if holds {
        logging::log_info(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation left a stream held by a run at rest",
            log_fields! { "run_id" => run_id.to_string(), "stream_id" => stream_id.to_string() },
        );
        return;
    }
    if let Some(state) = app.try_state::<GraduationState>() {
        state.release(stream_id);
    }
    if !is_worktree_queue_key(stream_id) {
        let _ = crate::streams::release_stream(app, stream_id, run_id);
    }
    // The stream and its project slot are free, so the oldest eligible run of
    // the project may start, whichever queue it waits in (GRD-FR-NYSH).
    crate::graduation::advance_all_queues(app);
}

/// Drive one run to rest on the calling thread, with the dispatch seam supplied.
///
/// This is the whole of what [`spawn_with`] runs, minus the thread. A test that
/// wants the loop wants it to have finished before the assertions read the
/// record, so it takes this route rather than racing a spawned thread.
#[cfg(test)]
pub(crate) fn drive_for_test<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    stream_id: &str,
    dispatch: Arc<dyn seams::GraduationDispatch<R>>,
) -> bool {
    let Some(cancellation) = claim(app, run_id, stream_id) else {
        return false;
    };
    drive_blocking(app, run_id, stream_id, cancellation, dispatch.as_ref());
    true
}

/// GRL-FR-VYNX: one pass is a work turn followed by a review turn.
async fn drive<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    cancellation: CancellationToken,
    dispatch: &dyn seams::GraduationDispatch<R>,
) {
    let Ok(mut run) = crate::graduation::load_run(app, run_id) else {
        return;
    };
    // GRD-FR-KZPT / GRD-FR-XHSE: a merge run stands in a worktree of its own and
    // is measured from its merge snapshot. It checks its pinned tips before it
    // does anything else, and the stream's working copy is not read at all.
    let merge_worktree = if run.is_merge() {
        match merge_drive::prepare(app, &mut run) {
            Some(path) => Some(path),
            None => return,
        }
    } else {
        None
    };
    // GRD-FR-XRDY: the gate is taken again at the claim, because the branch may
    // have moved between the queue's offer and now. A held run stays queued.
    if let Some(target) = run.direct_target.as_ref() {
        let hold = crate::graduation::dispatch_hold(target);
        if hold.is_some() {
            crate::graduation::record_hold(app, run_id, hold);
            return;
        }
        crate::graduation::record_hold(app, run_id, None);
        run.target_hold = None;
    }
    let worktree = match merge_worktree.clone() {
        Some(path) => Some(path),
        None => stream_worktree(app, &run),
    };
    let Some(worktree) = worktree else {
        // GXD-FR-TJRV: the phase the run stopped in stays the phase it
        // resumes at, so a missing working copy does not move it.
        let resume_part = run
            .checkpoint
            .resume_part
            .clone()
            .unwrap_or_else(|| phases::PART_WORK.to_string());
        let (code, message, clears_by) = if run.is_direct() {
            (
                "direct_worktree_missing",
                "The worktree this run works in is not there.",
                "Restore the worktree, then continue the run.",
            )
        } else {
            (
                "stream_missing",
                "The work stream's working copy is not there.",
                "Re-create the stream, then continue the run.",
            )
        };
        let _ = transitions::block(app, &mut run, code, message, clears_by, &resume_part);
        return;
    };

    // GRD-FR-HQPD / GRD-FR-YBUM: work the author left standing becomes the
    // run's starting point, and the revision it lands on is the base every
    // commit this run makes is measured from.
    if run.base_commit.is_none() && !run.is_merge() {
        if let Err(reason) = establish_base(app, &mut run, &worktree, &cancellation) {
            let _ = transitions::block(
                app,
                &mut run,
                "base_commit_failed",
                reason,
                "Commit or discard the work standing in the stream, then continue.",
                phases::PART_WORK,
            );
            return;
        }
    }

    // GRS-FR-EIXS: a required stream that cannot be written stops the run before
    // anything else happens, so this is the first thing the loop does.
    let dispatched = StructuredEvent::new(GraduationLogProducer::QueueWait, "run dispatched")
        .with("stream", run.target_label())
        .with("pass", run.pass());
    if !logs::emit(app, &mut run, dispatched) {
        return;
    }

    // GXD-FR-TJRV: a run that stopped in the review resumes there. The work did
    // not change while the run rested, so a work turn would spend a container to
    // decide again what nobody reported a problem with — which is the reasoning
    // the verdict-refusal loop below already stands on.
    let mut resume_at_review =
        run.checkpoint.resume_part.as_deref() == Some(phases::PART_REVIEW);
    // GRD-FR-JSBE: a merge run whose apply was blocked resumes at the apply. No
    // turn is dispatched and no pass is spent.
    let mut resume_at_apply = run.is_merge()
        && run.checkpoint.resume_part.as_deref() == Some(phases::PART_APPLY);
    // The pointer is spent the moment it is read. Leaving it set would make a
    // later interruption of this same run skip a work turn it never stopped in.
    if run.checkpoint.resume_part.is_some() {
        run.checkpoint.resume_part = None;
        let _ = crate::graduation::save_run(app, &mut run);
    }
    loop {
        if let Some(reason) = cancellation_reason(app, run_id, &cancellation) {
            abandon(app, &mut run, &worktree, Stop::of_reason(reason));
            return;
        }
        if resume_at_apply {
            resume_at_apply = false;
            match merge_drive::apply_again(app, &mut run, &worktree) {
                Step::Stop => return,
                Step::Continue => continue,
            }
        }
        if !resume_at_review {
            match work_turn(app, &mut run, &worktree, &cancellation, dispatch).await {
                Step::Stop => return,
                Step::Continue => {}
            }
        }
        // Spent. Only the first turn of a resumed run skips the work.
        resume_at_review = false;
        // A run stopped during its work turn does not spend a review container
        // on work nobody is waiting for.
        if let Some(reason) = cancellation_reason(app, run_id, &cancellation) {
            abandon(app, &mut run, &worktree, Stop::of_reason(reason));
            return;
        }
        match review_turn(app, &mut run, &worktree, &cancellation, dispatch).await {
            Step::Stop => return,
            Step::Continue => {}
        }
    }
}

/// GRL-FR-KWNP: the bounds this dispatch runs under, read before it is made.
///
/// Reading here rather than once per run is what makes a correction the author
/// makes while a run rests reach that run. The window of GXD-FR-PWYD is
/// recomputed from what was just read and persisted with it, so what a surface
/// renders as the run's pass bound and what the loop counts against are one
/// value.
fn settings_for_dispatch<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    part: &str,
) -> settings::LoopSettings {
    let settings = refresh_window(app, run);
    logging::log_debug(
        app,
        &crate::logging::BUFFER,
        &[Domain::Ai, Domain::Backend],
        "graduation read the bounds for a turn",
        log_fields! {
            "run_id" => run.id.clone(),
            "part" => part.to_string(),
            "execution_timeout_ms" => settings.execution_timeout_ms as i64,
            "pass_budget" => settings.pass_budget as i64,
            "pass_limit" => run.checkpoint.pass_limit as i64,
        },
    );
    settings
}

/// GXD-FR-PWYD: recompute the run's budget window from the settings as they
/// stand, and persist it with the run.
fn refresh_window<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
) -> settings::LoopSettings {
    // A store that cannot be read leaves the loop at its own bounds rather than
    // stopping a turn that would otherwise have run — but it is reported, since
    // the window it produces is narrower than the one the project configured
    // and nothing else would say why.
    let settings = match settings::LoopSettings::try_read(app) {
        Ok(settings) => settings,
        Err(reason) => {
            logging::log_warn(
                app,
                &crate::logging::BUFFER,
                &[Domain::Ai, Domain::Backend],
                "graduation could not read the project's bounds and used its own",
                log_fields! { "run_id" => run.id.clone(), "reason" => reason },
            );
            settings::LoopSettings::default()
        }
    };
    let floor = run.pass_floor();
    let limit = settings.pass_limit_for(run);
    if run.checkpoint.pass_floor != floor || run.checkpoint.pass_limit != limit {
        run.checkpoint.pass_floor = floor;
        run.checkpoint.pass_limit = limit;
        if let Err(reason) = crate::graduation::save_run(app, run) {
            logging::log_warn(
                app,
                &crate::logging::BUFFER,
                &[Domain::Ai, Domain::Backend],
                "graduation could not persist a run's pass budget window",
                log_fields! { "run_id" => run.id.clone(), "reason" => reason },
            );
        }
    }
    settings
}

/// GRL-FR-DXLU: the turn that does the work.
async fn work_turn<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &PathBuf,
    cancellation: &CancellationToken,
    dispatch: &dyn seams::GraduationDispatch<R>,
) -> Step {
    // GRD-FR-XHSE: a merge run checks its pinned tips before every dispatch.
    if merge_drive::moved(app, run) {
        return Step::Stop;
    }
    let settings = settings_for_dispatch(app, run, phases::PART_WORK);
    // GRL-FR-ARPX / GRL-FR-XBUE: the window was just recomputed from the
    // settings as they stand, so a budget the author lowered rests the run here
    // rather than reaching the task validation below as a fault.
    if run.pass_beyond_window() {
        rest_for_author(app, run);
        return Step::Stop;
    }
    let purpose = purpose_for(run);
    let input = task_input::compose_input(run, phases::PART_WORK, purpose);
    if let Err(reason) = input.validate(run.checkpoint.pass_limit) {
        note_blocked(app, run, "task_invalid");
        let _ = transitions::block(app, run, "task_invalid", reason, "Continue the run.", phases::PART_WORK);
        return Step::Stop;
    }
    // GRL-FR-ISIL: a resumed turn continues the pass it was in, so a pass
    // already open is kept rather than opened again.
    run.observability.open_pass(
        run.pass(),
        pass_task(run),
        crate::notes::now_rfc3339(),
    );
    let _ = transitions::enter_stage(
        app,
        run,
        observability::GraduationVisualStage::Working,
        observability::StageReason::WorkStarted,
    );
    if transitions::transition(app, run, GraduationRunState::Working).is_err() {
        return Step::Stop;
    }
    run.work_turns += 1;
    if !logs::emit(
        app,
        run,
        StructuredEvent::new(GraduationLogProducer::WorkTurn, "work turn started")
            .with("purpose", purpose)
            .with("turn", run.work_turns),
    ) {
        return Step::Stop;
    }

    let limit_ms = settings.execution_timeout_ms;
    let outcome = execute(
        app,
        run,
        worktree,
        phases::PART_WORK,
        input,
        settings,
        cancellation,
        dispatch,
    )
    .await;
    let step = settle_turn(app, run, worktree, outcome, GraduationEscalationOrigin::Work, limit_ms);
    if matches!(step, Step::Continue)
        && !logs::emit(
            app,
            run,
            StructuredEvent::new(GraduationLogProducer::WorkTurn, "work turn ended")
                .with("changed_paths", run.checkpoint.changed_paths.len() as u64)
                .with("hidden_paths", run.checkpoint.hidden_paths.len() as u64),
        )
    {
        return Step::Stop;
    }
    step
}

/// GOB-FR-XYCY: the prompt this pass is asked to answer.
///
/// The captured prompt on the first pass, and the review's own instruction on a
/// later one. What the pass record must not hold is `work.md`: that instruction
/// is identical for every run and every project (GRL-FR-DXLU), so a record
/// carrying it would say nothing about this pass.
fn pass_task(run: &GraduationRun) -> String {
    run.checkpoint
        .loop_instruction
        .clone()
        .unwrap_or_else(|| run.input.prompt.clone())
}

/// GRL-FR-OTRH / GRL-FR-YKRI: the turn that judges what the work turn wrote,
/// in a throwaway checkout of its own.
async fn review_turn<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &PathBuf,
    cancellation: &CancellationToken,
    dispatch: &dyn seams::GraduationDispatch<R>,
) -> Step {
    // GRD-FR-XHSE: a merge run checks its pinned tips before every dispatch.
    if merge_drive::moved(app, run) {
        return Step::Stop;
    }
    let _ = transitions::enter_stage(
        app,
        run,
        observability::GraduationVisualStage::Review,
        observability::StageReason::ReviewStarted,
    );
    if transitions::transition(app, run, GraduationRunState::Reviewing).is_err() {
        return Step::Stop;
    }

    // GRL-FR-TVXI: a verdict the application cannot read is refused whole and
    // **the review is asked again**. Asking is what the loop does here rather
    // than falling out to a further work turn: the work has not changed, so a
    // work turn would spend a container and a pass re-deciding work nobody
    // reported a problem with.
    let verdict = loop {
        let settings = settings_for_dispatch(app, run, phases::PART_REVIEW);
        // A budget the author lowered while the work turn ran rests the run,
        // exactly as it does before a work turn. The review's own bound below
        // is about what the verdict may start, not about what may run now.
        if run.pass_beyond_window() {
            rest_for_author(app, run);
            return Step::Stop;
        }
        let mut input =
            task_input::compose_input(run, phases::PART_REVIEW, task_input::PURPOSE_GENERATE);
        if run.checkpoint.verdict_refusals > 0 {
            // The turn is told what could not be read and what to send instead,
            // so it is given the correction rather than the fault.
            input.correction = prompts::prompt_section(&prompts::REFUSALS, REFUSAL_REVIEW_RESULT);
        }
        if let Err(reason) = input.validate(run.checkpoint.pass_limit) {
            note_blocked(app, run, "task_invalid");
        let _ = transitions::block(app, run, "task_invalid", reason, "Continue the run.", phases::PART_REVIEW);
            return Step::Stop;
        }
        run.review_turns += 1;
        if !logs::emit(
            app,
            run,
            StructuredEvent::new(GraduationLogProducer::ReviewTurn, "review turn started")
                .with("turn", run.review_turns)
                .with("corrected", run.checkpoint.verdict_refusals > 0),
        ) {
            return Step::Stop;
        }

        // GRL-FR-YKRI: the reviewer stands in a throwaway checkout holding the
        // run's change set, removed when the turn ends. Nothing it writes
        // reaches the stream, so it may run the project's own build and test
        // commands freely.
        let checkout = match review_checkout::create(app, run, worktree) {
            Ok(path) => path,
            Err(reason) => {
                note_blocked(app, run, "review_checkout_failed");
                let _ = transitions::block(
                    app,
                    run,
                    "review_checkout_failed",
                    reason,
                    "Continue the run to try the review again.",
                    phases::PART_REVIEW,
                );
                return Step::Stop;
            }
        };
        let limit_ms = settings.execution_timeout_ms;
        let outcome = execute(
            app,
            run,
            &checkout,
            phases::PART_REVIEW,
            input,
            settings,
            cancellation,
            dispatch,
        )
        .await;
        // GRL-FR-YKRI: removed when the turn ends, whatever it decided.
        //
        // A release that does not finish is reported rather than dropped. What
        // it leaves standing is what the next turn's `create` fails on, so a
        // silent failure here is read as a failure one turn later and in
        // another place.
        if let Err(reason) = review_checkout::release(app, run, worktree) {
            logging::log_warn(
                app,
                &crate::logging::BUFFER,
                &[Domain::Ai, Domain::Backend],
                "graduation could not remove a review checkout",
                log_fields! {
                    "run_id" => run.id.clone(),
                    "reason" => reason,
                },
            );
        }

        let execution =
            match settle_execution(app, run, worktree, outcome, GraduationEscalationOrigin::Review, limit_ms)
        {
            Some(execution) => execution,
            None => return Step::Stop,
        };
        if let Some(verdict) = read_verdict(&execution) {
            break verdict;
        }
        run.checkpoint.verdict_refusals += 1;
        if !logs::emit(
            app,
            run,
            StructuredEvent::new(GraduationLogProducer::ReviewTurn, "review verdict refused")
                .at_level(GraduationLogLevel::Warn)
                .with("refusals", run.checkpoint.verdict_refusals),
        ) {
            return Step::Stop;
        }
        logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Ai, Domain::Backend],
            "graduation could not read a review verdict",
            log_fields! {
                "run_id" => run.id.clone(),
                "refusals" => run.checkpoint.verdict_refusals as i64,
            },
        );
        let _ = crate::graduation::save_run(app, run);
        if run.checkpoint.verdict_refusals >= phases::VERDICT_REFUSAL_BOUND {
            note_blocked(app, run, "review_verdict_invalid");
            let _ = transitions::block(
                app,
                run,
                "review_verdict_invalid",
                "The review did not answer with a verdict this application can read.",
                "Continue the run to ask for the review again.",
                phases::PART_REVIEW,
            );
            return Step::Stop;
        }
    };
    run.checkpoint.verdict_refusals = 0;
    // GXD-FR-XPUR: answers a review asked for were delivered into this review,
    // and it returned a verdict, so no later turn carries them again.
    run.checkpoint.pending_escalation_answers.clear();
    // GRL-FR-QZFB: the review got past what blocked it, so a later block on
    // another condition is a first attempt rather than a second.
    transitions::clear_block_count(run);
    if !logs::emit(
        app,
        run,
        StructuredEvent::new(GraduationLogProducer::ReviewTurn, "review verdict")
            .with(
                "verdict",
                match verdict.verdict {
                    ReviewOutcome::Ready => "ready",
                    ReviewOutcome::Revise => "revise",
                },
            )
            .with("findings", verdict.findings.len() as u64),
    ) {
        return Step::Stop;
    }

    // GRL-FR-UQNV: two findings or fewer, all minor, end the loop as `ready`
    // does. They are recorded as advisory remarks and no further turn is
    // composed.
    //
    // GRL-FR-MRVK: the rule does not apply to a merge review. Every `revise`
    // verdict of one starts another pass or rests the run for the author.
    let advisory = !run.is_merge()
        && verdict.verdict == ReviewOutcome::Revise
        && verdict.findings.len() <= 2
        && verdict.findings.iter().all(|f| f.severity == ReviewSeverity::Minor);

    if verdict.verdict == ReviewOutcome::Ready || advisory {
        run.observability
            .settle_pass(&verdict, None, crate::notes::now_rfc3339());
        // GRD-FR-AQNW: the counterpart of the commit of a draft run, for a merge
        // run, is the apply of the merge onto the base branch.
        if run.is_merge() {
            return merge_drive::finish_merge(app, run, worktree);
        }
        finish(app, run, worktree);
        return Step::Stop;
    }
    revise_or_rest(app, run, &verdict)
}

/// GRL-FR-GQAB / GRL-FR-ARPX: a review that asked for changes starts the next
/// pass while the budget holds one, and rests the run for the author where it
/// does not.
///
/// A merge run reaches this from a `ready` review too, where the apply found a
/// conflict marker the review missed (GRL-FR-NDHW).
pub(super) fn revise_or_rest<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    verdict: &ReviewVerdict,
) -> Step {
    // GRL-FR-GQAB: the findings travel whole into the next work turn, in the
    // order the review returned them.
    let instruction = compose_instruction(verdict);
    run.observability.settle_pass(
        verdict,
        Some(instruction.clone()),
        crate::notes::now_rfc3339(),
    );
    run.checkpoint.loop_instruction = Some(instruction);

    if run.pass() >= run.checkpoint.pass_limit.max(1) {
        rest_for_author(app, run);
        return Step::Stop;
    }
    // GRL-FR-ARPX: this is the one place a pass advances.
    run.checkpoint.pass = run.pass() + 1;
    let _ = crate::graduation::save_run(app, run);
    let _ = transitions::enter_stage(
        app,
        run,
        observability::GraduationVisualStage::Working,
        observability::StageReason::ReviewRevision,
    );
    Step::Continue
}

/// GXD-FR-KXXB: one turn is one call through the dispatch seam.
async fn execute<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
    execution_directory: &PathBuf,
    part: &str,
    input: task_input::GraduationTaskInput,
    settings: settings::LoopSettings,
    cancellation: &CancellationToken,
    dispatch: &dyn seams::GraduationDispatch<R>,
) -> Result<AgentExecution, AgentExecutionError> {
    let task = AgentTaskRequest {
        protocol_version: PROTOCOL_VERSION,
        instruction: prompts::instruction_for_run(run, part).to_string(),
        input: Some(input.to_map()),
        // GRL-FR-VIAT: the review names the shape its own answer must take.
        result_contract: (part == phases::PART_REVIEW).then_some(ResultContract::ReviewVerdict),
        resume: resume_for(run, part),
        // GXD-FR-MTVR: the controls are composed from the settings read for
        // this dispatch, and nothing of an earlier turn's controls is reused.
        execution: ExecutionControls {
            cancellation: Cancellation::CallerControlled,
            timeout_ms: settings.execution_timeout_ms,
        },
    };
    let request = AgentExecutionRequest {
        execution_directory: execution_directory.clone(),
        task,
        supplementary_mount: None,
        turn_kind: task_input::turn_kind_for_run(run, part),
        cancellation: cancellation.clone(),
        // GXD-FR-IOZU: what the agent does reaches the run it is doing it for.
        // A turn with no sink is a run the author watches an empty panel for.
        activity: activity_sink(app, run),
        // GXD-FR-IOZU / GLG-FR-QKVI: the record of what the agent did, which
        // the executor must deliver durably and acknowledged.
        durable_output: Some(durable_sink(app, run, part)),
    };
    // GXD-FR-CYIW / GRD-FR-NHRY: the interval is the turn's own execution, from
    // the moment it is dispatched to the moment it returns. Held on this stack,
    // so it can span no queue wait and no author wait.
    let recorder = crate::graduation::statistics::OperationRecorder::begin(
        crate::graduation::statistics::bucket_for_part(part),
    );
    // GLG-FR-QNLC: the dispatch of each merge turn is a record of the run.
    if run.is_merge() {
        crate::graduation::log_merge_boundary(
            app,
            run,
            crate::graduation::MergeBoundary {
                level: crate::logging::LogLevel::Info,
                domains: &[Domain::Ai, Domain::Backend],
                message: "graduation dispatched a merge turn",
                boundary: "dispatch",
                outcome: "dispatched",
                pass: Some(run.pass()),
                paths: None,
                extra: log_fields! { "turn_kind" => request.turn_kind.as_str() },
            },
        );
    }
    let outcome = dispatch
        .dispatch_graduation_turn(app, &run.project_key, request)
        .await;
    recorder.finish(app, run);
    outcome
}

/// GXD-FR-IOZU: the live activity sink one turn of this run reports through.
///
/// `None` where the application holds no activity store, which costs the live
/// panel and nothing else: no turn is refused over it.
fn activity_sink<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
) -> Option<Arc<dyn crate::tools::agent_exec::AgentActivitySink>> {
    // AGV: the live panel, which is bounded and evicted.
    let store = app
        .try_state::<Arc<crate::agent_activity::ActivityStore>>()
        .map(|state| state.inner().clone())?;
    let fs = crate::graduation::store_fs(app).ok();
    Some(Arc::new(crate::agent_activity::RunActivitySink::new(
        app, &run.id, store, fs,
    )))
}

/// GXD-FR-IOZU / GLG-FR-QKVI: the durable activity sink one turn of this run
/// reports through.
///
/// GRS-FR-JQOO: the record is retained for the lifetime of the run record and
/// is what the author reads a finished run back from.
fn durable_sink<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
    part: &str,
) -> Arc<dyn crate::tools::agent_exec::DurableOutputSink> {
    Arc::new(logs::GraduationActivitySink::new(
        app,
        run,
        source_producer(part),
    ))
}

/// GRS-FR-KJVN: which producer a turn's activity is attributed to.
fn source_producer(part: &str) -> GraduationLogProducer {
    match part {
        phases::PART_REVIEW => GraduationLogProducer::ReviewTurn,
        phases::PART_SEMANTIC_MERGE => GraduationLogProducer::SemanticMergeTurn,
        _ => GraduationLogProducer::WorkTurn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GOB-FR-XYCY: what a pass records as its task is the prompt it answers.
    ///
    /// The three shapes a checkpoint takes, rather than the call graph that
    /// produces them: what a pass holds must follow from the run's own record,
    /// so a later path that sets an instruction earlier cannot quietly relabel
    /// the first pass.
    #[test]
    fn a_pass_answers_the_captured_prompt_until_a_review_says_otherwise() {
        let mut run = GraduationRun::new_for_test("g1", "d1", "2026-09-06T09:00:00Z");
        run.input.prompt = "Add the empty state to the panel.".into();

        assert_eq!(pass_task(&run), "Add the empty state to the panel.");

        run.checkpoint.loop_instruction = Some("Put the empty state back.".into());
        assert_eq!(pass_task(&run), "Put the empty state back.");

        // An instruction is what a pass answers whatever pass it is on, so a
        // resumed run reads back the instruction it was given.
        run.checkpoint.pass = 2;
        assert_eq!(pass_task(&run), "Put the empty state back.");

        run.checkpoint.loop_instruction = None;
        assert_eq!(pass_task(&run), "Add the empty state to the panel.");
    }
}
