//! The commands a graduation run is managed through (GRD contract surface).

use super::*;

/// GSU-FR-ELZO: start a run, or start nothing.
#[tauri::command]
pub fn start_graduation<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    draft_id: String,
    stream_id: String,
    standing_work: StandingWork,
    standing_work_message: Option<String>,
) -> Result<GraduationRun, String> {
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());

    let project_key = queue::project_key(&app);
    if project_key.is_empty() {
        return Err(ERR_NO_PROJECT_OPEN.to_string());
    }
    // GSU-FR-IJEE: a draft a non-terminal run already holds is refused.
    queue::require_unlocked_draft(&app, &draft_id)?;

    // GSU-FR-JBKZ: the stream must be one the project holds.
    let stream = crate::streams::stream_of(&app, &stream_id)
        .ok_or_else(|| ERR_UNKNOWN_STREAM.to_string())?;

    // GSU-FR-GLVQ: the image preflight probes nothing and refuses before a run
    // exists, so a machine that cannot execute an agent costs no run.
    image_preflight(&app)?;

    // GSU-FR-RWYQ: the prompt is captured whole, once, and never re-read.
    let root = app
        .try_state::<ProjectState>()
        .and_then(|state| state.require_root().ok())
        .ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    let record = crate::drafts::read_draft_prompt(&root, &draft_id)
        .map_err(|reason| {
            if reason == crate::drafts::ERR_NOT_SINGLE_FILE {
                ERR_DRAFT_NOT_SINGLE_FILE.to_string()
            } else {
                ERR_DRAFT_NOT_FOUND.to_string()
            }
        })?;
    let input = CapturedGraduationInput {
        draft_id: draft_id.clone(),
        draft_name: record.draft.name.clone(),
        prompt: record.content.clone(),
        prompt_checksum: crate::fs::sha256_bytes(record.content.as_bytes()),
        captured_at: crate::notes::now_rfc3339(),
    };

    // GSU-FR-MLEJ: the author's choice travels with the run, and the run
    // applies it when its turn comes. The start reads no working copy: what
    // stands in the stream now is not what stands there at the dispatch.
    let mut run = new_run(
        &project_key,
        &stream,
        input,
        standing_work,
        standing_work_message,
    );
    // GRS-FR-KDOY: both streams are created in the write that creates the run,
    // so a run that has emitted nothing still has two readable ones.
    crate::graduation::logs::initialize(&app, &mut run);
    runs::save_run(&app, &mut run)?;
    // GSU-FR-RNOM: the run is enqueued, so the graduation-start draft commit is
    // asked for now and never for a refused start. Nothing waits for it.
    commit_graduation_start(&app, &root, &draft_id, &record.draft.name);
    events::announce_queue(&app, &run);
    let _ = crate::drafts::announce(&app, crate::drafts::DraftChange::to(&draft_id, Vec::new()));
    dispatch_pending(&app);
    Ok(run)
}

/// GRD-FR-LGDV: every run the project has made, in the project's run order.
#[tauri::command]
pub fn list_graduation_queue<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<GraduationQueue, String> {
    // GRD-FR-XVUD: the first read after launch is where a run the application
    // stopped without stopping is found. The queue is loaded once and handed to
    // the sweep, which would otherwise load every run record a second time.
    let queue = queue::project_queue(&app).ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    sweep_abandoned_runs(&app, &queue);
    // A sweep that moved a run changed the answer, so it is read again only
    // where something actually moved.
    if queue
        .runs
        .iter()
        .any(|run| matches!(run.state, GraduationRunState::Working | GraduationRunState::Reviewing))
    {
        return queue::project_queue(&app).ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string());
    }
    Ok(queue)
}

/// GRD-FR-LGDV: one run's whole record.
#[tauri::command]
pub fn get_graduation_run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
) -> Result<GraduationRun, String> {
    runs::load_run(&app, &run_id)
}

/// DRS-FR-18: the run a draft's row names, if it has one.
#[tauri::command]
pub fn get_draft_graduation<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    draft_id: String,
) -> Result<Option<GraduationRun>, String> {
    let Some(queue) = queue::project_queue(&app) else {
        return Ok(None);
    };
    Ok(queue::latest_run_for(&queue, &draft_id).cloned())
}

/// GRD-FR-CYIB: the one operation behind both Continue and Resume.
#[tauri::command]
pub fn continue_graduation_run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
) -> Result<GraduationRun, String> {
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());

    let mut run = runs::load_run(&app, &run_id)?;
    if run.state.is_terminal() {
        return Err(ERR_RUN_STATE_NOT_PERMITTED.to_string());
    }
    // GRD-FR-XHSE: a merge run does not resume over a branch that moved. The
    // check comes first, so a refusal changes nothing.
    refuse_moved_tips(&app, &run)?;
    let paused = matches!(
        run.interruption.as_ref().map(|i| i.reason),
        Some(GraduationInterruptionReason::AuthorPause)
    );
    if paused {
        if let Some(interruption) = run.interruption.as_mut() {
            interruption.resume_requested_at = Some(crate::notes::now_rfc3339());
        }
    }
    // GRD-FR-IKVE: the pending log writes are retried first, before the run is
    // moved or anything else is attempted. A retry that still fails leaves the
    // run interrupted and Continue available.
    if !crate::graduation::logs::retry_pending(&app, &mut run) {
        return Err(format!(
            "{ERR_RUN_STATE_NOT_PERMITTED}: the run's log stream still cannot be written"
        ));
    }
    run.auto_start = true;
    // GOB-FR-BTXN: the return to the queue is a backward move, so the author
    // reads that the run was sent round again rather than started afresh. It
    // appends nothing where the run never left the queue.
    if run.blocker.is_some() {
        let _ = transitions::enter_stage(
            &mut_app(&app),
            &mut run,
            crate::graduation::observability::GraduationVisualStage::Queued,
            crate::graduation::observability::StageReason::BlockedRetry,
        );
    }
    // GRL-FR-QZFB: Continue from `blocked` is the retry the count counts, so
    // the count survives it and a second block on the same code reaches the
    // bound. Continue from `awaiting_author` is the author's own decision to
    // try again, having seen what stopped the run twice, so that one starts the
    // bound over.
    //
    // Do not reset unconditionally here. A reset on every Continue makes every
    // block for ever the first one, and the run continues into the same fault
    // for as long as the author keeps asking — which is the whole defect
    // GRL-FR-QZFB exists to stop.
    let rested_for_author =
        run.state == GraduationRunState::AwaitingAuthor && run.blocker.is_some();
    if rested_for_author {
        transitions::clear_block_count(&mut run);
    }
    // GRL-FR-XBUE: a run resting on a spent budget holds no escalation and no
    // blocker, which is what tells it apart from the other two ways a run rests
    // for the author. Read before the blocker is cleared below.
    // The window is measured from the settings as they stand, so a record
    // written before the window existed — which holds no limit at all — is
    // still recognised as a run whose budget is spent.
    let settings = crate::graduation::driver::LoopSettings::read(&app);
    let window = settings.pass_limit_for(&run);
    let budget_spent = run.state == GraduationRunState::AwaitingAuthor
        && run.escalation.is_none()
        && run.blocker.is_none()
        && run.pass_budget_spent(window);
    run.blocker = None;
    // GRL-FR-TVXI: the author's Continue resets the refusal count, so a run
    // blocked on two unreadable verdicts gets the whole bound again.
    run.checkpoint.verdict_refusals = 0;
    // GRD-FR-CYIB / GRL-FR-XBUE: Continue against a run whose pass budget is
    // spent resets **that budget alone**. The window's floor moves to the next
    // pass and the run stands at that pass, so the passes it already made are
    // not made again and the findings it holds reach the next work turn. Every
    // other checkpoint value — the change set, the instruction, the phase to
    // resume at — is carried through untouched.
    if budget_spent {
        let next = run.pass().saturating_add(1);
        run.checkpoint.pass = next;
        run.checkpoint.pass_floor = next;
        // Written now rather than left at nothing, so a run that waits in its
        // stream's queue between Continue and its next dispatch still reads
        // back as a run with a bound. The next dispatch recomputes it from the
        // settings then held (GRL-FR-KWNP).
        run.checkpoint.pass_limit = settings.pass_limit_from(&run, next);
        crate::logging::log_info(
            &app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Ai, crate::logging::Domain::Backend],
            "graduation reset a spent pass budget",
            crate::log_fields! {
                "run_id" => run.id.clone(),
                "pass" => next as i64,
                "pass_budget" => if run.is_merge() { MERGE_PASS_BUDGET as i64 } else { settings.pass_budget as i64 },
            },
        );
    }
    transitions::transition(&mut_app(&app), &mut run, GraduationRunState::Queued)?;
    dispatch_pending(&app);
    runs::load_run(&app, &run_id)
}

/// GRD-FR-MDQZ: the author's own stop, accepted only while the run is doing
/// agent work.
#[tauri::command]
pub fn pause_graduation_run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
) -> Result<GraduationRun, String> {
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());

    let mut run = runs::load_run(&app, &run_id)?;
    if !matches!(
        run.state,
        GraduationRunState::Working | GraduationRunState::Reviewing
    ) {
        return Err(format!("{ERR_RUN_STATE_NOT_PERMITTED}: {}", run.state.as_str()));
    }
    // GRL-FR-ZDKP: the turn is stopped through the one cancellation path, and
    // nothing it already wrote is undone.
    if let Some(state) = app.try_state::<GraduationState>() {
        state.cancel_run(&run_id, GraduationInterruptionReason::AuthorPause);
    }
    // GXD-FR-TJRV: a run paused in the review resumes at the review.
    if run.state == GraduationRunState::Reviewing {
        run.checkpoint.resume_part = Some(driver::phases::PART_REVIEW.to_string());
    }
    transitions::interrupt(
        &app,
        &mut run,
        GraduationInterruptionReason::AuthorPause,
        "You paused this run.",
    )?;
    Ok(run)
}

/// GRD-FR-TKUR: whether a stream's queue may start this run.
#[tauri::command]
pub fn set_graduation_auto_start<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
    enabled: bool,
) -> Result<GraduationRun, String> {
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());

    let mut run = runs::load_run(&app, &run_id)?;
    if run.state != GraduationRunState::Queued {
        return Err(format!("{ERR_RUN_STATE_NOT_PERMITTED}: {}", run.state.as_str()));
    }
    if run.auto_start == enabled {
        return Ok(run);
    }
    run.auto_start = enabled;
    runs::save_run(&app, &mut run)?;
    events::announce_queue(&app, &run);
    if enabled {
        dispatch_pending(&app);
    }
    Ok(run)
}

/// GRD-FR-JOFE: where the author filed a run. Independent of its lifecycle.
#[tauri::command]
pub fn archive_graduation_run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
) -> Result<GraduationRun, String> {
    set_archived(&app, &run_id, true)
}

#[tauri::command]
pub fn unarchive_graduation_run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
) -> Result<GraduationRun, String> {
    set_archived(&app, &run_id, false)
}

fn set_archived<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    archived: bool,
) -> Result<GraduationRun, String> {
    let mut run = runs::load_run(app, run_id)?;
    // Idempotent: archiving an archived run writes nothing and emits nothing.
    if run.archived == archived {
        return Ok(run);
    }
    run.archived = archived;
    run.archived_at = archived.then(crate::notes::now_rfc3339);
    runs::save_run(app, &mut run)?;
    Ok(run)
}

/// GXD-FR-BJYT: the author's answers, covering every recorded question.
#[tauri::command]
pub fn answer_graduation_escalation<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
    answers: Vec<GraduationEscalationAnswer>,
) -> Result<GraduationRun, String> {
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());

    let mut run = runs::load_run(&app, &run_id)?;
    // GXD-FR-BJYT: a run that has ended takes no answers, whatever escalation
    // its record still holds.
    if run.state.is_terminal() {
        logging::log_warn(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "graduation refused answers to a run that has ended",
            log_fields! { "run_id" => run.id.clone(), "state" => run.state.as_str().to_string() },
        );
        return Err(format!("{ERR_RUN_STATE_NOT_PERMITTED}: {}", run.state.as_str()));
    }
    let Some(escalation) = run.escalation.clone() else {
        return Err(ERR_RUN_STATE_NOT_PERMITTED.to_string());
    };
    // GRD-FR-XHSE: answering resumes the run, so a moved tip refuses it first.
    refuse_moved_tips(&app, &run)?;
    // An ordered set covering every question and nothing else. A set that does
    // not cover them all records nothing.
    let mut positions: Vec<u32> = answers.iter().map(|a| a.position).collect();
    positions.sort_unstable();
    let expected: Vec<u32> = escalation.questions.iter().map(|q| q.position).collect();
    if positions != expected || answers.iter().any(|a| a.answer.trim().is_empty()) {
        return Err("the answers do not cover every question".to_string());
    }
    run.checkpoint.pending_escalation_answers = answers;
    run.checkpoint.resume = escalation.resume.clone();
    run.escalation = None;
    transitions::transition(&app, &mut run, GraduationRunState::Queued)?;
    dispatch_pending(&app);
    runs::load_run(&app, &run_id)
}

/// GRD-FR-BLCR: revert the commits one completed run made, as new commits.
#[tauri::command]
pub fn revert_graduation_run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
) -> Result<GraduationRun, String> {
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());

    let mut run = runs::load_run(&app, &run_id)?;
    // GRD-FR-YTCR: a merge run holds a merge, and a merge commit is not undone
    // by replaying commits in reverse.
    if run.is_merge() {
        return Err(ERR_RUN_NOT_REVERTABLE.to_string());
    }
    // A run still doing agent work holds its stream, and its working copy holds
    // uncommitted work. Reverting under it would rewrite the tree a turn is
    // writing into. A queued run is refused too: the queue may start it on the
    // reverted branch at any moment (GRD-FR-BLCR).
    if run.holds_stream() || run.state == GraduationRunState::Queued {
        return Err(format!("{ERR_RUN_STATE_NOT_PERMITTED}: {}", run.state.as_str()));
    }
    if run.commits.is_empty() {
        return Err(ERR_RUN_NOT_REVERTABLE.to_string());
    }
    let worktree = direct::target_worktree(&app, &run).ok_or_else(|| {
        if run.is_direct() {
            ERR_DIRECT_WORKTREE_MISSING.to_string()
        } else {
            ERR_UNKNOWN_STREAM.to_string()
        }
    })?;
    // GRD-FR-VAUE: a revert is a commit, so it lands on the pinned branch alone.
    if !direct::commit_target_ok(&run, &worktree) {
        return Err(ERR_DIRECT_TARGET_CHANGED.to_string());
    }
    commit::revert_run_commits(&worktree, &run.commits)?;
    logging::log_info(
        &app,
        &BUFFER,
        &[Domain::Backend],
        "graduation run reverted",
        log_fields! { "run_id" => run.id.clone(), "commits" => run.commits.len() as i64 },
    );
    // GRD-FR-BLCR: a run that has not ended ends with its work reverted, so no
    // turn continues on a branch that no longer holds that work.
    if run.state.is_terminal() {
        runs::save_run(&app, &mut run)?;
    } else {
        transitions::transition(&app, &mut run, GraduationRunState::Discarded)?;
        // GRD-FR-NYSH: a run that rested `blocked` held a slot and has no loop
        // behind it to offer the slot on, so the revert offers it.
        dispatch_pending(&app);
    }
    Ok(run)
}

/// GRD-FR-EWTN: end a run without reverting anything it committed.
#[tauri::command]
pub fn discard_graduation_run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
) -> Result<GraduationRun, String> {
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());

    let mut run = runs::load_run(&app, &run_id)?;
    if run.state.is_terminal() {
        return Err(format!("{ERR_RUN_STATE_NOT_PERMITTED}: {}", run.state.as_str()));
    }
    if let Some(state) = app.try_state::<GraduationState>() {
        state.cancel_run(&run_id, GraduationInterruptionReason::AuthorPause);
    }
    transitions::transition(&app, &mut run, GraduationRunState::Discarded)?;
    // GRD-FR-NYSH: a run that rested `blocked` held a slot and has no loop
    // behind it to offer the slot on, so the discard offers it. A discarded
    // working run offers it again from its loop's end, which a pass answers
    // with no start.
    dispatch_pending(&app);
    Ok(run)
}

/// GRD-FR-ZAMI: a new run over a discarded run's captured prompt.
#[tauri::command]
pub fn restart_graduation_run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
    stream_id: String,
    standing_work: StandingWork,
    standing_work_message: Option<String>,
) -> Result<GraduationRun, String> {
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());

    let discarded = runs::load_run(&app, &run_id)?;
    // GRD-FR-YTCR: a merge run has no captured draft input to run again.
    if discarded.is_merge() {
        return Err(format!(
            "{ERR_RUN_STATE_NOT_PERMITTED}: a merge run is not restarted"
        ));
    }
    if discarded.state != GraduationRunState::Discarded {
        return Err(format!(
            "{ERR_RUN_STATE_NOT_PERMITTED}: {}",
            discarded.state.as_str()
        ));
    }
    queue::require_unlocked_draft(&app, &discarded.input.draft_id)?;
    // GSU-FR-NPFB / GRD-FR-PNMU: a discarded direct run restarts on the worktree
    // and branch it pinned. The stream, the standing-work choice and the message
    // the caller names belong to a stream run and are not read.
    if let Some(target) = discarded.direct_target.clone() {
        image_preflight(&app)?;
        if !std::path::Path::new(&target.worktree_path).is_dir() {
            return Err(ERR_DIRECT_WORKTREE_MISSING.to_string());
        }
        let stream_name = crate::streams::stream_of(&app, &discarded.stream_id)
            .map(|stream| stream.name)
            .unwrap_or_else(|| discarded.stream_name.clone());
        let mut run = build_run(
            &discarded.project_key,
            discarded.stream_id.clone(),
            stream_name,
            Some(target),
            discarded.input.clone(),
            StandingWork::Keep,
            None,
        );
        run.restarted_from_run_id = Some(discarded.id.clone());
        crate::graduation::logs::initialize(&app, &mut run);
        runs::save_run(&app, &mut run)?;
        events::announce_queue(&app, &run);
        dispatch_pending(&app);
        return Ok(run);
    }
    let stream = crate::streams::stream_of(&app, &stream_id)
        .ok_or_else(|| ERR_UNKNOWN_STREAM.to_string())?;
    image_preflight(&app)?;

    // The captured input travels whole: the run answers the prompt as it stood
    // when the author judged it finished, and the draft's current prompt is a
    // different prompt. Everything else starts empty.
    // GSU-FR-IRAC: a restart takes its own standing-work choice, against the
    // stream it names, rather than the choice the discarded run carried.
    let mut run = new_run(
        &discarded.project_key,
        &stream,
        discarded.input.clone(),
        standing_work,
        standing_work_message,
    );
    run.restarted_from_run_id = Some(discarded.id.clone());
    crate::graduation::logs::initialize(&app, &mut run);
    runs::save_run(&app, &mut run)?;
    // GSU-FR-RNOM: a restart that enqueues a run asks for the same commit a
    // start does, against the worktree of the run's own project. A restart
    // taken while no project or another project is open has no worktree of
    // that project to commit into, and the run it enqueued stands without it.
    // PST-FR-YWXF: the message carries the draft's current name, which a
    // rename after the discard may have changed from the captured one.
    let root = app
        .try_state::<ProjectState>()
        .and_then(|state| state.require_root().ok())
        .filter(|_| queue::project_key(&app) == run.project_key);
    if let Some(root) = root {
        let name = crate::drafts::draft_record(&root, &run.input.draft_id)
            .map(|record| record.name)
            .unwrap_or_else(|_| run.input.draft_name.clone());
        commit_graduation_start(&app, &root, &run.input.draft_id, &name);
    }
    events::announce_queue(&app, &run);
    dispatch_pending(&app);
    Ok(run)
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// GRD-FR-XHSE: refuse to resume a merge run whose pinned tip moved.
///
/// Read-only: nothing is written to a branch, a worktree or the run.
fn refuse_moved_tips<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
) -> Result<(), String> {
    let Some(data) = run.merge.as_ref() else {
        return Ok(());
    };
    // GLG-FR-QNLC: the tip check of a Continue or of an answer is a record,
    // whether it lets the request through or refuses it.
    // A merge already applied moved the base branch itself, and the run is
    // completed by its next dispatch rather than refused.
    let moved = data.result.is_none() && crate::streams::merge_tips_moved(app, data);
    log_merge_boundary(
        app,
        run,
        MergeBoundary {
            level: if moved {
                crate::logging::LogLevel::Warn
            } else {
                crate::logging::LogLevel::Info
            },
            domains: &[Domain::Ai, Domain::Backend],
            message: if moved {
                "graduation refused to resume a merge run: a pinned tip moved"
            } else {
                "graduation checked the pinned tips of a merge run"
            },
            boundary: "tip_check",
            outcome: if moved {
                crate::streams::ERR_MERGE_BRANCH_MOVED
            } else {
                "ok"
            },
            pass: Some(run.pass()),
            paths: None,
            extra: log_fields! {
                "checked_at" => "resume",
                "state" => run.state.as_str().to_string(),
            },
        },
    );
    if !moved {
        return Ok(());
    }
    Err(crate::streams::ERR_MERGE_BRANCH_MOVED.to_string())
}

/// GSU-FR-RNOM / `PST-project-storage.md` PST-FR-DQZT: ask for the
/// graduation-start draft commit of the run's draft, against the active
/// worktree. Returns at once and never fails the start.
pub(super) fn commit_graduation_start<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &crate::fs::RootFs,
    draft_id: &str,
    draft_name: &str,
) where
    tauri::AppHandle<R>: crate::logging::LogSink + Clone + Send + 'static,
{
    crate::storage_floor::commit::offer_sink(app);
    crate::drafts::git_storage::commit_event(
        root,
        draft_id,
        draft_name,
        crate::storage_floor::commit::DraftEvent::GraduationStarted,
    );
}

pub(super) fn new_run(
    project_key: &str,
    stream: &crate::streams::WorkStream,
    input: CapturedGraduationInput,
    standing_work: StandingWork,
    standing_work_message: Option<String>,
) -> GraduationRun {
    build_run(
        project_key,
        stream.id.clone(),
        stream.name.clone(),
        None,
        input,
        standing_work,
        standing_work_message,
    )
}

/// GRD-FR-PZAK / GRD-FR-BSNI: a run in the state a fresh enqueue leaves it in,
/// placed in a stream, in a pinned worktree, or both.
pub(crate) fn build_run(
    project_key: &str,
    stream_id: String,
    stream_name: String,
    direct_target: Option<DirectTarget>,
    input: CapturedGraduationInput,
    standing_work: StandingWork,
    standing_work_message: Option<String>,
) -> GraduationRun {
    let now = crate::notes::now_rfc3339();
    let mut observability = GraduationObservability::default();
    observability.stage_history.push(StageTransition {
        from: GraduationVisualStage::Queued,
        to: GraduationVisualStage::Queued,
        pass: 1,
        at: now.clone(),
        reason: StageReason::Enqueued,
    });
    GraduationRun {
        id: store::new_run_id(),
        stream_id,
        stream_name,
        direct_target,
        target_hold: None,
        project_key: project_key.to_string(),
        state: GraduationRunState::Queued,
        // GRD-FR-HQPD / GRD-FR-RJFC: recorded once, and read at the dispatch.
        standing_work,
        standing_work_message,
        standing_work_outcome: None,
        input,
        base_commit: None,
        commits: Vec::new(),
        // GRD-FR-TKUR: every run entering the queue starts eligible.
        auto_start: true,
        archived: false,
        archived_at: None,
        work_turns: 0,
        review_turns: 0,
        logs: crate::graduation::logs::GraduationLogIndexes::default(),
        checkpoint: GraduationCheckpoint {
            // GRL-FR-ARPX: every run starts at its first pass.
            pass: 1,
            ..GraduationCheckpoint::default()
        },
        observability,
        escalation: None,
        blocker: None,
        interruption: None,
        restarted_from_run_id: None,
        failure: None,
        merge: None,
        created_at: now.clone(),
        updated_at: now,
    }
}

/// GSU-FR-GLVQ: the machine must be able to execute an agent before a run
/// exists. It probes nothing: configuration and verification state alone.
pub(crate) fn image_preflight<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<(), String> {
    let root = app
        .try_state::<ProjectState>()
        .and_then(|state| state.require_root().ok())
        .ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    let store = app
        .try_state::<crate::global_settings::GlobalSettingsStore>()
        .ok_or_else(|| ERR_VENDOR_EXECUTION_UNSUPPORTED.to_string())?;
    let integrations = app
        .try_state::<crate::agentic::AgenticIntegrations>()
        .ok_or_else(|| ERR_VENDOR_EXECUTION_UNSUPPORTED.to_string())?;
    // GSU-FR-GLVQ: which vendor a turn would resolve to, read from
    // configuration rather than by launching anything.
    let invocation = crate::agentic::resolve_agentic_invocation(
        &store,
        &integrations,
        &queue::project_key(app),
        None,
    )
    .map_err(|_| ERR_VENDOR_EXECUTION_UNSUPPORTED.to_string())?;
    // EAC-FR-04: only a CLI-kind vendor is executable in a container, so an
    // API-kind one is refused here rather than at launch.
    let vendor = match &invocation {
        crate::agentic::AgenticInvocation::Cli { vendor, .. } => vendor.clone(),
        crate::agentic::AgenticInvocation::Api { .. } => {
            return Err(ERR_VENDOR_EXECUTION_UNSUPPORTED.to_string())
        }
    };
    // PSS-FR-30: the image the open project commits for that vendor. Read
    // without probing a daemon or a registry.
    crate::project_settings::resolve_project_vendor_image(&root, &vendor)
        .map_err(|_| ERR_VENDOR_IMAGE_UNCONFIGURED.to_string())?;
    // GSS-FR-40: the machine's Docker backend must say verified.
    crate::docker::resolve_docker_backend(&store)
        .map_err(|_| ERR_DOCKER_BACKEND_UNVERIFIED.to_string())?;
    Ok(())
}

/// A borrow the transition helper can take without moving the handle.
fn mut_app<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::AppHandle<R> {
    app.clone()
}

/// GRS-FR-RZXA: one page of one stream for one run, phase, and pass scope.
///
/// Read-only. It writes nothing to either stream, and the only thing it may
/// change about the run is the record of a damaged file (GRS-FR-EYNU).
#[tauri::command]
pub fn read_graduation_logs<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
    phase_id: String,
    pass: serde_json::Value,
    stream: String,
    cursor: Option<logs::GraduationLogCursor>,
    limit: Option<u32>,
    query: Option<String>,
) -> Result<logs::GraduationLogPage, String> {
    // Every refusal is reported, so a window that says it could not read the
    // log is answerable from the Logs panel. The values named here are the
    // caller's own identifiers and carry no record content.
    let refuse = |code: &str| {
        logging::log_warn(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "graduation refused a run log read",
            log_fields! {
                "run_id" => run_id.clone(),
                "phase_id" => phase_id.clone(),
                "stream" => stream.clone(),
                "code" => code.to_string(),
            },
        );
    };
    // GRS-FR-KJVN: every phase is one of the progress-bar ids, so a value
    // outside them is named back rather than answered from the wrong scope.
    if !GraduationVisualStage::from_id(&phase_id) {
        refuse(logs::read::ERR_UNKNOWN_PHASE);
        return Err(format!("{}: {phase_id}", logs::read::ERR_UNKNOWN_PHASE));
    }
    let selected = match stream.as_str() {
        "source" => GraduationLogStream::Source,
        "structured" => GraduationLogStream::Structured,
        _ => {
            refuse(ERR_UNKNOWN_STREAM);
            return Err(ERR_UNKNOWN_STREAM.to_string());
        }
    };
    let scope: logs::GraduationLogPassScope = match serde_json::from_value(pass) {
        Ok(scope) => scope,
        Err(_) => {
            refuse(logs::read::ERR_UNKNOWN_SCOPE);
            return Err(logs::read::ERR_UNKNOWN_SCOPE.to_string());
        }
    };

    let run = runs::load_run(&app, &run_id).inspect_err(|reason| {
        refuse(reason);
    })?;
    let fs = store_fs(&app)?;
    let paths = logs::paths_for(&app, &run_id)?;

    let page = logs::read::read_page(
        &fs,
        &paths,
        &run.logs,
        &run_id,
        &phase_id,
        &scope,
        selected,
        cursor,
        limit,
        query,
    )
    .inspect_err(|reason| {
        refuse(reason);
    })?;

    // The shape of the answer, and no part of what it holds: no decoded output,
    // no structured field, and no query text reaches a diagnostic record.
    logging::log_debug(
        &app,
        &BUFFER,
        &[Domain::Backend],
        "graduation read one page of a run log stream",
        log_fields! {
            "run_id" => run_id.clone(),
            "phase_id" => phase_id.clone(),
            "stream" => stream.clone(),
            "status" => page.status.clone(),
            "search" => page.search.clone(),
            "entries" => page.entries.len() as i64,
            "matched_total" => page.matched_total as i64,
        },
    );

    // GRS-FR-EYNU / GRS-FR-MRKO: a damaged file is recorded on the run and
    // leaves the persistence status alone. It interrupts no run, cancels no
    // agent action, and moves no run state.
    let met = page
        .failure
        .clone()
        .filter(|_| page.status == logs::read::STATUS_UNAVAILABLE);
    if let Some(failure) = met.as_ref() {
        logging::log_warn(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "graduation could not read a run log stream whole",
            log_fields! {
                "run_id" => run_id.clone(),
                "stream" => failure.stream.as_str().to_string(),
                "code" => failure.code.clone(),
            },
        );
    }
    record_read_failure(&app, &run_id, selected, met);
    Ok(page)
}

/// Whether two read failures name the same damage.
///
/// Compared on what the damage **is** rather than on the whole record: a
/// failure carries the instant it was met, so two reads of one damaged file are
/// never equal and comparing them whole would rewrite the run record once per
/// scroll.
fn same_damage(held: &logs::GraduationLogFailure, met: &logs::GraduationLogFailure) -> bool {
    held.code == met.code
        && held.stream == met.stream
        && held.stopped_sequence == met.stopped_sequence
        && held.byte_offset == met.byte_offset
}

/// GRS-FR-EYNU: keep the damage a read met on the run record, and clear it once
/// a later read of that stream succeeds.
///
/// The run record is written only where the value actually changes, and under
/// the same write guard the run's own operations take, so a read never overtakes
/// the thread driving the run.
fn record_read_failure<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    stream: GraduationLogStream,
    met: Option<logs::GraduationLogFailure>,
) {
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());
    // Read the record back inside the guard: the copy this command read before
    // the file may be behind what the run's own thread has since written.
    let Ok(mut run) = runs::load_run(app, run_id) else {
        return;
    };
    let held = run.logs.last_read_failure.clone();
    let settled = match (&held, &met) {
        (Some(held), Some(met)) if same_damage(held, met) => return,
        (None, None) => return,
        // A read of this stream that met nothing clears a failure of this
        // stream alone, and leaves one the other stream is still damaged by.
        (Some(held), None) if held.stream != stream => return,
        (_, met) => met.clone(),
    };
    run.logs.last_read_failure = settled;
    if let Err(reason) = runs::save_run(app, &mut run) {
        logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation could not record what a run log read met",
            log_fields! { "run_id" => run_id.to_string(), "reason" => reason },
        );
    }
}
