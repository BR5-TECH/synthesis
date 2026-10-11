//! The one funnel a run's state changes through (GRD-FR-QJHM).

use super::*;

/// Move a run to `next`, durably, and tell every reader.
///
/// Every state change goes through here, so the stage, the stream hold, the
/// draft lock and the events are settled in one place rather than at each call
/// site.
pub fn transition<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    next: GraduationRunState,
) -> Result<(), String> {
    // A run that has ended has ended: nothing moves it again. The **store** is
    // what answers that, not the copy in hand: a turn is driven from a thread of
    // its own, so a run the author discarded while it ran is terminal on disk
    // and still `working` here (GRD-FR-EWTN). A `failed` run alone moves once
    // more, to `discarded`, because it holds its queue until the author discards
    // it (GRD-FR-RKWQ).
    if has_ended(run.state, next) {
        return Ok(());
    }
    if runs::load_run(app, &run.id).is_ok_and(|stored| has_ended(stored.state, next)) {
        return Ok(());
    }
    let held_queue = run.holds_queue();
    let paused = matches!(
        run.interruption.as_ref().map(|i| i.reason),
        Some(GraduationInterruptionReason::AuthorPause)
    );

    run.state = next;
    // GRD-FR-BNTC: the run was dispatched, so it holds its queue from here until
    // it is completed or discarded, also while it waits in `queued` again.
    if next.holds_queue() {
        run.dispatched = true;
    }
    // GOB-FR-HXUZ: the stage and the condition are persisted in the same
    // durable write as the change that set them.
    run.observability.apply_state(next, paused);

    // GRD-FR-BNTC: a run that has stopped holding its queue releases it, and
    // the stream's record is cleared with it (WKS-FR-JQJA). A run that rests
    // keeps both: only `completed` and `discarded` free a queue.
    if held_queue && !run.holds_queue() {
        release_stream(app, run);
    } else if !next.is_agent_work() {
        // The loop that drove the run is ending. Its claim is not the hold, which
        // the stored run carries (GRD-FR-BSWV), so a Continue that follows finds
        // no stale claim in its way.
        if let Some(state) = app.try_state::<GraduationState>() {
            state.release_run(&run.queue_key(), &run.id);
        }
    }
    // GRD-FR-GMTX: a run reaching a terminal state reclaims nothing of the
    // stream. No branch is deleted, no working copy removed, no commit undone.

    runs::save_run(app, run)?;
    events::announce_queue(app, run);
    // GRD-FR-ZHNV: the run is reported while it executes and in no other state.
    run_progress::sync(app, run);

    // DRS-FR-KQTW: the draft's status follows the runs, so a run that ends
    // releases the lock it held. A merge run holds no draft.
    if next.is_terminal() && !run.input.draft_id.is_empty() {
        crate::drafts::announce(
            app,
            crate::drafts::DraftChange::to(&run.input.draft_id, Vec::new()),
        );
    }
    // WKS-FR-WULF: a stream's row names its merge run and that run's state, so a
    // change of the run is a change of the listing.
    if run.is_merge() {
        crate::streams::announce_changed(app, &run.project_key);
        // GRD-FR-KZPT: a merge run that ended and is not driven by this process
        // has what it owns removed now. A driven one is removed by its own loop.
        if next.is_terminal() {
            crate::graduation::reclaim_ended_merge_run(app, run);
        }
    }
    Ok(())
}

/// Whether a run in `state` can no longer move to `next`.
///
/// `completed` and `discarded` end a run for good. `failed` ends it too, except
/// that the author's discard of it is accepted (GRD-FR-RKWQ).
fn has_ended(state: GraduationRunState, next: GraduationRunState) -> bool {
    match state {
        GraduationRunState::Completed | GraduationRunState::Discarded => true,
        GraduationRunState::Failed => next != GraduationRunState::Discarded,
        _ => false,
    }
}

/// GOB-FR-VVNI: move the run to a stage and record why.
pub fn enter_stage<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    stage: observability::GraduationVisualStage,
    reason: observability::StageReason,
) -> Result<(), String> {
    let pass = run.pass();
    run.observability.note_stage(stage, pass, crate::notes::now_rfc3339(), reason);
    runs::save_run(app, run)
}

/// GRD-FR-BNTC / WKS-FR-JQJA: release the stream this run was holding, in
/// memory and on the stream's own record.
pub(super) fn release_stream<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &GraduationRun) {
    if let Some(state) = app.try_state::<GraduationState>() {
        state.release_run(&run.queue_key(), &run.id);
    }
    // An ordinary worktree has no stream record to clear (GRD-FR-ZVNO).
    let released = if run.stream_id.is_empty() {
        Ok(())
    } else {
        crate::streams::release_stream(app, &run.stream_id, &run.id)
    };
    if let Err(reason) = released {
        logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation could not clear a stream's busy mark",
            log_fields! {
                "run_id" => run.id.clone(),
                "stream_id" => run.stream_id.clone(),
                "reason" => reason,
            },
        );
    }
}

/// Rest a run on a stop it may be continued from (GRD-FR-XVUD).
pub fn interrupt<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    reason: GraduationInterruptionReason,
    detail: impl Into<String>,
) -> Result<(), String> {
    // GRD-FR-BNTC: `interrupted` holds the queue and the project slot, whatever
    // stopped the run, so no interruption gives either back.
    run.interruption = Some(GraduationInterruption {
        reason,
        at: crate::notes::now_rfc3339(),
        detail: detail.into(),
        resume_requested_at: None,
    });
    transition(app, run, GraduationRunState::Interrupted)
}

/// Rest a run on a condition the author clears (GRD-FR-QJHM).
///
/// GRL-FR-QZFB: the same condition twice in a row is not a condition Continue
/// clears. The first one rests the run `blocked`, so the author clears it
/// against the work standing in the working copy. A second one carrying the
/// same code rests the run `awaiting_author` instead, and the author decides
/// rather than continuing into it again. Both states hold the queue and the
/// project slot (GRD-FR-BNTC).
///
/// `resume_part` is recorded either way, so whichever state the run rests in it
/// resumes at the phase it stopped in (GXD-FR-TJRV).
pub fn block<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    code: impl Into<String>,
    message: impl Into<String>,
    clears_by: impl Into<String>,
    resume_part: &str,
) -> Result<(), String> {
    let code = code.into();
    let attempt = if run.checkpoint.blocked_code.as_deref() == Some(code.as_str()) {
        run.checkpoint.block_attempts.saturating_add(1)
    } else {
        1
    };
    run.checkpoint.blocked_code = Some(code.clone());
    run.checkpoint.block_attempts = attempt;
    run.checkpoint.resume_part = Some(resume_part.to_string());
    run.blocker = Some(GraduationBlocker {
        code,
        message: message.into(),
        clears_by: clears_by.into(),
        attempt,
    });
    // GRD-FR-WKMD: the run keeps its blocker in either state. What tells the
    // two apart is the state, not the absence of the record.
    let next = if attempt >= driver::BLOCK_ATTEMPT_BOUND {
        GraduationRunState::AwaitingAuthor
    } else {
        GraduationRunState::Blocked
    };
    transition(app, run, next)
}

/// GRL-FR-QZFB: a phase that got past what blocked it starts the count again.
///
/// Without this a run that blocked once, was continued, and then worked for an
/// hour before blocking on the same code much later would read as two
/// consecutive blocks and rest for the author over nothing.
pub fn clear_block_count(run: &mut GraduationRun) {
    run.checkpoint.blocked_code = None;
    run.checkpoint.block_attempts = 0;
}
