//! The queued runs a discarded run takes with it (GRD-FR-QCZF, GRD-FR-EPWW).

use super::*;

/// GRD-FR-QCZF: the runs queued behind `run` in its own queue, in run order.
///
/// Read from the run order's index alone. A run counts when it comes later in
/// the order than `run`, belongs to the same queue, and waits in `queued`
/// without ever having been dispatched. A run ahead of `run`, and a run that
/// has ended, are history and stay.
pub(super) fn queued_behind(
    fs: &fsa::FsAccess,
    base: &StoreBase,
    run: &GraduationRun,
) -> Vec<QueueIndexEntry> {
    let index = store::read_queue_index(fs, base, &run.project_key);
    let key = run.queue_key();
    let Some(position) = index.runs.iter().position(|entry| entry.run_id == run.id) else {
        return Vec::new();
    };
    index.runs[position + 1..]
        .iter()
        .filter(|entry| {
            entry.queue_key() == key
                && entry.state == GraduationRunState::Queued
                && !entry.holds_queue()
        })
        .cloned()
        .collect()
}

/// GRD-FR-EPWW: delete queued runs from the run history and from the run order.
///
/// A removed run does not become `discarded`, is not archived, and leaves no
/// record. What a merge run owns is reclaimed with it (GRD-FR-KZPT). The caller
/// holds the store hold (GRD-FR-WSXA), and calls [`announce_removed`] once the
/// removal is done. Answers the ids that were removed.
pub(super) fn remove_runs<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    holder: &GraduationRun,
    removed: &[QueueIndexEntry],
) -> Result<Vec<GraduationRun>, String> {
    if removed.is_empty() {
        return Ok(Vec::new());
    }
    let fs = store::store_fs(app)?;
    let base = store::store_base(app)?;
    // The records are read before anything is deleted, because the draft of each
    // run and the merge data it owns are named there.
    let records: Vec<GraduationRun> = removed
        .iter()
        .filter_map(|entry| store::read_run_record(&fs, &base, &entry.run_id).ok())
        .collect();
    // The order is written first. A crash after it leaves a directory nothing
    // lists, where the other order would leave a listed run with no record.
    let mut index = store::read_queue_index(&fs, &base, &holder.project_key);
    index
        .runs
        .retain(|entry| !removed.iter().any(|gone| gone.run_id == entry.run_id));
    store::write_queue_index(&fs, &base, &index)?;
    for entry in removed {
        let record = records.iter().find(|run| run.id == entry.run_id);
        if let Some(run) = record.filter(|run| run.is_merge()) {
            if driver::merge_workspace::stands(app, run) {
                if let Err(reason) = driver::merge_workspace::release(app, run) {
                    logging::log_warn(
                        app,
                        &BUFFER,
                        &[Domain::Backend],
                        "graduation could not reclaim what a removed merge run owned",
                        log_fields! { "run_id" => entry.run_id.clone(), "reason" => reason },
                    );
                }
            }
        }
        logs::release(app, &entry.run_id);
        if let Err(reason) = store::remove_run_home(&fs, &base, &entry.run_id) {
            logging::log_warn(
                app,
                &BUFFER,
                &[Domain::Backend],
                "graduation could not delete the directory of a removed run",
                log_fields! { "run_id" => entry.run_id.clone(), "reason" => reason },
            );
        }
    }
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "graduation removed the runs queued behind a discarded run",
        log_fields! {
            "run_id" => holder.id.clone(),
            "removed" => removed.len() as i64,
        },
    );
    Ok(records)
}

/// GRD-FR-EPWW: tell every reader of a removal, after it is durable.
///
/// The queue of the discarded run changed. Each removed run's draft is an active
/// draft again, so the Drafts panel and every open tab re-read it
/// (DRS-FR-CFJJ). A removed merge run leaves its stream's row (WKS-FR-KHJS). No
/// notification is raised: a removed run enters no state.
pub(super) fn announce_removed<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    holder: &GraduationRun,
    removed: &[GraduationRun],
) {
    events::announce_queue(app, holder);
    for run in removed {
        if !run.input.draft_id.is_empty() {
            crate::drafts::announce_drafts_changed(app, &run.input.draft_id);
        }
    }
    if removed.iter().any(|run| run.is_merge()) {
        crate::streams::announce_changed(app, &holder.project_key);
    }
}

/// GRD-FR-QCZF: the removal that goes with the discard of a run that held its
/// queue. A run that never held it removes nothing. The caller announces the
/// removal with [`announce_removed`] after the discard is written.
///
/// `held` is whether the run held its queue before it ended, which the caller
/// reads before the transition because the transition moves the run out of the
/// state that answers it.
pub(super) fn remove_behind_discarded<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
    held: bool,
) -> Result<Vec<GraduationRun>, String> {
    if !held {
        return Ok(Vec::new());
    }
    let fs = store::store_fs(app)?;
    let base = store::store_base(app)?;
    let behind = queued_behind(&fs, &base, run);
    remove_runs(app, run, &behind)
}
