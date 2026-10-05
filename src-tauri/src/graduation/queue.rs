//! Reading a project's run order, and which run each queue starts next
//! (GRD-FR-VLFO, GRD-FR-EGWS, GRD-FR-ZVNO).

use super::*;

/// GRD-FR-LGDV: every run the project has made, in the project's run order.
pub fn load_queue(fs: &fsa::FsAccess, base: &StoreBase, project_key: &str) -> GraduationQueue {
    let index = store::read_queue_index(fs, base, project_key);
    let runs = index
        .runs
        .iter()
        // A run whose record cannot be read is dropped from the answer rather
        // than failing the whole list: the records are written individually so
        // one unreadable file cannot take every other run with it.
        .filter_map(|entry| store::read_run_record(fs, base, &entry.run_id).ok())
        .collect();
    GraduationQueue {
        project_key: project_key.to_string(),
        runs,
    }
}

/// GRD-FR-EGWS: the run a queue starts next.
///
/// The earliest member of that queue that is eligible. A member that is
/// working, waiting, blocked or terminal is passed over rather than waited for,
/// and passing one over moves nothing.
pub fn next_eligible<'a>(queue: &'a GraduationQueue, key: &str) -> Option<&'a GraduationRun> {
    eligible_members(queue, key).into_iter().next()
}

/// GRD-FR-EGWS: every member of a queue that may start, earliest first.
///
/// Empty while a member of the queue holds the queue's stream or worktree. The
/// dispatcher walks the list and passes over a direct run whose pinned branch is
/// not checked out (GRD-FR-XRDY).
pub fn eligible_members<'a>(queue: &'a GraduationQueue, key: &str) -> Vec<&'a GraduationRun> {
    if queue
        .runs
        .iter()
        .any(|run| run.queue_key() == key && run.holds_stream())
    {
        return Vec::new();
    }
    queue
        .runs
        .iter()
        .filter(|run| run.queue_key() == key && run.is_eligible())
        .collect()
}

/// GRD-FR-NYSH: every run the scheduler may start now, oldest first in the
/// project's run order, taken across every queue.
///
/// A queue that a member holds offers nothing. A run that is not eligible is
/// passed over, and it blocks no run of another queue. A queue may list several
/// runs: the scheduler starts the first one it can and then passes over that
/// queue, so each queue still starts one run at a time (GRD-FR-BNTC).
pub fn dispatch_candidates(queue: &GraduationQueue) -> Vec<&GraduationRun> {
    let held: std::collections::BTreeSet<String> = queue
        .runs
        .iter()
        .filter(|run| run.holds_stream())
        .map(|run| run.queue_key())
        .collect();
    queue
        .runs
        .iter()
        .filter(|run| run.is_eligible() && !held.contains(&run.queue_key()))
        .collect()
}

/// GRD-FR-VLFO: one queue, in the project's run order.
pub fn queue_of<'a>(queue: &'a GraduationQueue, key: &str) -> Vec<&'a GraduationRun> {
    queue
        .runs
        .iter()
        .filter(|run| run.queue_key() == key && run.state.waits_in_queue())
        .collect()
}

/// GRD-FR-RHNP: a run's index in its own queue, counted from the earliest
/// member as zero.
pub fn position_in_queue(queue: &GraduationQueue, run_id: &str) -> Option<usize> {
    let run = queue.runs.iter().find(|r| r.id == run_id)?;
    queue_of(queue, &run.queue_key())
        .iter()
        .position(|r| r.id == run_id)
}

/// WKS-FR-YBST: how many non-terminal runs each stream holds.
///
/// Read from the project's run order index alone. Enumerating streams is a
/// listing operation, and opening one record per run to count them would make
/// the cost of the stream list grow with every run the project has ever made.
pub fn queued_counts_by_stream(
    fs: &fsa::FsAccess,
    base: &StoreBase,
    project_key: &str,
) -> std::collections::BTreeMap<String, u32> {
    let mut counts: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    for entry in store::read_queue_index(fs, base, project_key).runs {
        // A direct run on an ordinary worktree names no stream and counts for
        // none; a direct run on a stream's working copy counts for that stream
        // (WKS-FR-YBST).
        if entry.state.is_terminal() || entry.stream_id.is_empty() {
            continue;
        }
        *counts.entry(entry.stream_id).or_insert(0) += 1;
    }
    counts
}

/// WKS-FR-KHJS: each stream's newest merge run that is neither discarded nor
/// archived, with its state.
///
/// Read from the run order index alone, like the depth of a queue
/// (WKS-FR-YBST): a stream listing opens no run record.
pub fn merge_runs_by_stream(
    fs: &fsa::FsAccess,
    base: &StoreBase,
    project_key: &str,
) -> std::collections::BTreeMap<String, (String, GraduationRunState)> {
    let mut found: std::collections::BTreeMap<String, (String, GraduationRunState)> =
        std::collections::BTreeMap::new();
    // The order runs earliest to latest, so the last entry of a stream is its
    // newest.
    for entry in store::read_queue_index(fs, base, project_key).runs {
        if !entry.merge
            || entry.archived
            || entry.stream_id.is_empty()
            || entry.state == GraduationRunState::Discarded
        {
            continue;
        }
        found.insert(entry.stream_id.clone(), (entry.run_id, entry.state));
    }
    found
}

/// The queues the project's runs belong to, in the order they first appear.
pub fn streams_with_runs(queue: &GraduationQueue) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for run in &queue.runs {
        let key = run.queue_key();
        if !seen.iter().any(|s| s == &key) {
            seen.push(key);
        }
    }
    seen
}

/// GRD-FR-OYPY: the dispatched direct run of the open project, if one stands.
///
/// Read from the run order index alone, so a switching route checks it without
/// opening a run record (WTC-FR-FBJQ). Returns the run's id.
pub fn dispatched_direct_run<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<String> {
    let key = project_key(app);
    if key.is_empty() {
        return None;
    }
    let fs = store::store_fs(app).ok()?;
    let base = store::store_base(app).ok()?;
    store::read_queue_index(&fs, &base, &key)
        .runs
        .into_iter()
        .find(|entry| entry.dispatched_direct && !entry.state.is_terminal())
        .map(|entry| entry.run_id)
}

/// GRD-FR-DXWL: the draft's non-terminal run, if it has one.
pub(super) fn non_terminal_run_for_draft<'a>(
    queue: &'a GraduationQueue,
    draft_id: &str,
) -> Option<&'a GraduationRun> {
    queue
        .runs
        .iter()
        .find(|r| r.input.draft_id == draft_id && !r.state.is_terminal())
}

/// The most recent run for a draft, terminal or not.
pub fn latest_run_for<'a>(
    queue: &'a GraduationQueue,
    draft_id: &str,
) -> Option<&'a GraduationRun> {
    queue.runs.iter().rev().find(|r| r.input.draft_id == draft_id)
}

/// DRS-FR-KQTW: whether the draft's `graduated` status stands.
///
/// It stands while any run of that draft has committed its work onto a stream
/// and is not `discarded`.
pub fn draft_graduation_stands(queue: &GraduationQueue, draft_id: &str) -> bool {
    queue.runs.iter().any(|run| {
        run.input.draft_id == draft_id
            && !run.commits.is_empty()
            && run.state != GraduationRunState::Discarded
    })
}

/// DRS-FR-18: one draft's row in the Drafts panel.
pub fn draft_graduation(queue: &GraduationQueue, draft_id: &str) -> Option<DraftGraduation> {
    let run = latest_run_for(queue, draft_id)?;
    Some(DraftGraduation {
        run_id: run.id.clone(),
        state: run.state,
        locked: run.locks_draft(),
        graduated: draft_graduation_stands(queue, draft_id),
    })
}

/// The active project's run order, or `None` where no project is open.
pub fn project_queue<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<GraduationQueue> {
    let key = project_key(app);
    if key.is_empty() {
        return None;
    }
    let fs = store::store_fs(app).ok()?;
    let base = store::store_base(app).ok()?;
    let mut queue = load_queue(&fs, &base, &key);
    // GRS-FR-ZTCF: every run this process writes is listed with its live log
    // indexes, so a stage is activatable from its first durable record.
    for run in &mut queue.runs {
        logs::overlay_live(app, run);
    }
    Some(queue)
}

/// The open project's key, or an empty string where no project is open.
pub(super) fn project_key<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> String {
    app.try_state::<ProjectState>()
        .map(|state| state.slot_key())
        .unwrap_or_default()
}

/// GHP-FR-SRDL: refuse a publication against a draft a **non-terminal** run
/// holds.
///
/// Weaker than [`require_unlocked_draft`] on purpose: a `graduated` draft is
/// publishable, because publication changes the draft's metadata and not its
/// prompt or its local graduation. What it refuses is a draft an agent is
/// working from right now.
pub fn require_no_running_graduation<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    draft_id: &str,
) -> Result<(), String> {
    let Some(queue) = project_queue(app) else {
        return Ok(());
    };
    if non_terminal_run_for_draft(&queue, draft_id).is_some() {
        return Err(ERR_DRAFT_LOCKED.to_string());
    }
    Ok(())
}

/// DRS-FR-19: refuse a draft-storage write against a draft a run holds.
pub fn require_unlocked_draft<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    draft_id: &str,
) -> Result<(), String> {
    let Some(queue) = project_queue(app) else {
        return Ok(());
    };
    if non_terminal_run_for_draft(&queue, draft_id).is_some()
        || draft_graduation_stands(&queue, draft_id)
    {
        return Err(ERR_DRAFT_LOCKED.to_string());
    }
    Ok(())
}
