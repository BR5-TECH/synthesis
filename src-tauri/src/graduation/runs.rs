//! Reading and writing one run's record (GRD-FR-LGDV, GXD-FR-QGYA).

use super::*;

/// Read one run, wherever the project's store holds it.
pub fn load_run<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
) -> Result<GraduationRun, String> {
    let fs = store::store_fs(app)?;
    let base = store::store_base(app)?;
    let mut run =
        store::read_run_record(&fs, &base, run_id).map_err(|_| ERR_UNKNOWN_RUN.to_string())?;
    // GRS-FR-ZTCF: a run this process writes is read with its live log indexes.
    logs::overlay_live(app, &mut run);
    Ok(run)
}

/// GXD-FR-QGYA: write one run durably, and tell every reader.
///
/// Nothing is reported before it is durable: the state a caller reads back is
/// what the store holds.
pub fn save_run<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
) -> Result<(), String> {
    run.updated_at = crate::notes::now_rfc3339();
    let fs = store::store_fs(app)?;
    let base = store::store_base(app)?;
    store::write_run_record(&fs, &base, run)?;
    // The order's index carries a summary of each run, so a reader that needs
    // the order or a draft's lock opens one file rather than every record.
    let mut index = store::read_queue_index(&fs, &base, &run.project_key);
    index.project_key = run.project_key.clone();
    let entry = store::QueueIndexEntry {
        run_id: run.id.clone(),
        stream_id: run.stream_id.clone(),
        draft_id: run.input.draft_id.clone(),
        state: run.state,
        worktree_path: run
            .direct_target
            .as_ref()
            .map(|target| target.worktree_path.clone())
            .unwrap_or_default(),
        dispatched_direct: run.is_dispatched_direct(),
        merge: run.merge.is_some(),
        archived: run.archived,
    };
    match index.runs.iter_mut().find(|e| e.run_id == run.id) {
        Some(existing) => *existing = entry,
        // GRD-FR-VLFO: a new run is inserted at the latest end of the order.
        None => index.runs.push(entry),
    }
    store::write_queue_index(&fs, &base, &index)?;
    events::announce_run(app, run);
    Ok(())
}

/// GXD-FR-HGSU: record what an agent stopped to ask.
pub fn record_escalation<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    reason: String,
    questions: Vec<GraduationEscalationQuestion>,
    origin: GraduationEscalationOrigin,
    resume: Option<GraduationResumeRef>,
) -> Result<(), String> {
    run.escalation = Some(GraduationEscalation {
        reason,
        questions,
        origin,
        raised_at: crate::notes::now_rfc3339(),
        resume,
    });
    save_run(app, run)
}

/// GXD-FR-GMDI: record what a stopped run is continued from.
pub fn record_checkpoint<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    checkpoint: GraduationCheckpoint,
) -> Result<(), String> {
    run.checkpoint = checkpoint;
    save_run(app, run)
}

/// ESU-FR-19: the rules an `escalate_to_user` request must satisfy.
///
/// One to eight questions, each with at most three proposed responses, and no
/// blank text anywhere. A request that breaks one of them records nothing.
pub fn validate_escalation_request(
    reason: &str,
    questions: &[GraduationEscalationQuestion],
) -> Result<(), String> {
    if reason.trim().is_empty() {
        return Err("escalation reason is blank".to_string());
    }
    if questions.is_empty() || questions.len() > 8 {
        return Err("an escalation asks between one and eight questions".to_string());
    }
    for question in questions {
        if question.question.trim().is_empty() {
            return Err("an escalation question is blank".to_string());
        }
        if question.options.len() > 3 {
            return Err("a question offers at most three responses".to_string());
        }
        for option in &question.options {
            if option.answer.trim().is_empty() || option.summary.trim().is_empty() {
                return Err("a proposed response is blank".to_string());
            }
        }
    }
    Ok(())
}
