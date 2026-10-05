//! The project-wide scheduler: which queued run starts next, and how many runs
//! the project works at once (GRD-FR-KKKN, GRD-FR-NYSH, GRD-FR-IJKV,
//! GRD-FR-KPET, GRD-FR-GRHC).

use std::collections::BTreeSet;

use super::*;
use crate::project_settings::GraduationConcurrency;

/// GRD-FR-GRHC: what the project's slots hold now, and which queued runs wait
/// for a slot alone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationCapacity {
    /// PSS-FR-JRWC: an integer of one or more, or the text `unlimited`.
    pub limit: GraduationConcurrency,
    /// GRD-FR-KKKN: how many slots are held.
    pub in_use: u32,
    /// GRD-FR-GRHC: the queued runs that wait for a project slot alone.
    pub waiting_for_slot: Vec<String>,
}

/// PSS-FR-JRWC: the open project's limit, one where it cannot be read.
pub(super) fn concurrency_limit<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> GraduationConcurrency {
    app.try_state::<ProjectState>()
        .and_then(|state| state.require_root().ok())
        .and_then(|root| crate::project_settings::load_project_config_from(&root).ok())
        .and_then(|config| config.graduation_concurrency_limit)
        .unwrap_or_default()
}

/// GRD-FR-NYSH: offer every free slot to the oldest eligible queued run of the
/// whole project.
///
/// The pass walks the project's run order once. A run is started when its queue
/// is free and the project has a free slot. A direct run whose pinned worktree
/// is not on its pinned branch is passed over and records why (GRD-FR-XRDY), so
/// the run behind it may start. The pass ends as soon as no slot is free, which
/// is how a limit lowered below the slots held starts nothing (GRD-FR-IJKV).
pub(super) fn dispatch_pending<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    dispatch_with(app, |run_id, key| driver::spawn(app, run_id, key));
}

/// [`dispatch_pending`], with the act of starting one run supplied.
///
/// `start` answers whether the run took its queue and a project slot. A test
/// binds a starter that claims without driving a turn, so the order and the
/// count of starts are observable without an execution agent.
pub(super) fn dispatch_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    mut start: impl FnMut(&str, &str) -> bool,
) {
    let Some(state) = app.try_state::<GraduationState>() else {
        return;
    };
    // One pass at a time: two ends of runs must not each offer a slot to a
    // different queue.
    let _pass = state.dispatch_guard();
    let Some(queue) = queue::project_queue(app) else {
        return;
    };
    let limit = concurrency_limit(app);
    let mut passed_over: BTreeSet<String> = BTreeSet::new();
    for run in queue::dispatch_candidates(&queue) {
        if !limit.permits(state.working_count()) {
            logging::log_debug(
                app,
                &BUFFER,
                &[Domain::Backend],
                "graduation found no free project slot",
                log_fields! { "in_use" => state.working_count() as i64 },
            );
            return;
        }
        let key = run.queue_key();
        if passed_over.contains(&key) || state.holder_of(&key).is_some() {
            continue;
        }
        if let Some(target) = run.direct_target.as_ref() {
            let hold = direct::dispatch_hold(target);
            let held = hold.is_some();
            direct::record_hold(app, &run.id, hold);
            if held {
                continue;
            }
        }
        if start(&run.id, &key) {
            logging::log_info(
                app,
                &BUFFER,
                &[Domain::Backend],
                "graduation started a queued run",
                log_fields! {
                    "run_id" => run.id.clone(),
                    "in_use" => state.working_count() as i64,
                },
            );
        }
        // The queue has now started a run, or its claim was refused. Either
        // way no later member of it may start in this pass (GRD-FR-BNTC).
        passed_over.insert(key);
    }
}

/// GRD-FR-GRHC: the project's capacity.
///
/// A run waits for a slot alone when it is the earliest eligible member of its
/// queue, the queue holds no run, and the limit is full. A direct run held for
/// its branch is not eligible (GRD-FR-XRDY) and is never listed.
pub fn graduation_capacity_of<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> GraduationCapacity {
    let limit = concurrency_limit(app);
    let in_use = app
        .try_state::<GraduationState>()
        .map(|state| state.working_count())
        .unwrap_or(0);
    let mut waiting_for_slot = Vec::new();
    if limit.is_full(in_use) {
        if let Some(queue) = queue::project_queue(app) {
            let mut seen: BTreeSet<String> = BTreeSet::new();
            for run in queue::dispatch_candidates(&queue) {
                // GRD-FR-XRDY: read from the worktree now, because the hold a
                // record carries is written by a pass that reached the run.
                let held = run
                    .direct_target
                    .as_ref()
                    .is_some_and(|target| direct::dispatch_hold(target).is_some());
                // A run claimed a moment ago still reads `queued` until its first
                // turn is recorded, so the claim in memory names the queue too.
                let claimed = app
                    .try_state::<GraduationState>()
                    .is_some_and(|state| state.holder_of(&run.queue_key()).is_some());
                if held || claimed || !seen.insert(run.queue_key()) {
                    continue;
                }
                waiting_for_slot.push(run.id.clone());
            }
        }
    }
    GraduationCapacity {
        limit,
        in_use: in_use as u32,
        waiting_for_slot,
    }
}

/// GRD-FR-GRHC: what the project's slots hold now, and which queued runs wait
/// for a slot alone.
#[tauri::command]
pub fn get_graduation_capacity<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<GraduationCapacity, String> {
    Ok(graduation_capacity_of(&app))
}
