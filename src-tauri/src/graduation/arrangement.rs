//! Moving a run within its own stream's queue (GRD-FR-RHNP).

use super::*;

/// GRD-FR-RHNP: move one run's place in the order of **its own stream's**
/// queue, and nothing else.
///
/// Both positions index that queue from its earliest-dispatching member as
/// zero. Every refusal leaves the order exactly as it was.
#[tauri::command]
pub fn reorder_graduation_run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
    expected_position: usize,
    new_position: usize,
) -> Result<GraduationQueue, String> {
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());

    let queue = queue::project_queue(&app).ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    let run = queue
        .runs
        .iter()
        .find(|r| r.id == run_id)
        .ok_or_else(|| ERR_UNKNOWN_RUN.to_string())?;

    // Accepted for a queued member alone: every other state is a run the author
    // is not arranging.
    if run.state != GraduationRunState::Queued {
        return Err(format!("{ERR_RUN_STATE_NOT_PERMITTED}: {}", run.state.as_str()));
    }
    // A run the author has filed away is not one they are arranging, and
    // declining changes nothing else about it.
    if run.archived {
        return Err(ERR_RUN_ARCHIVED.to_string());
    }
    let queue_key = run.queue_key();
    let announced = run.clone();
    let members: Vec<String> = queue::queue_of(&queue, &queue_key)
        .iter()
        .map(|r| r.id.clone())
        .collect();
    let actual = members
        .iter()
        .position(|id| id == &run_id)
        .ok_or_else(|| ERR_UNKNOWN_RUN.to_string())?;

    // A caller acting on a listing it read before another change landed is told
    // rather than obeyed.
    if actual != expected_position {
        return Err(format!("{ERR_QUEUE_POSITION_STALE}: {actual}"));
    }
    if new_position >= members.len() {
        return Err(format!(
            "{ERR_QUEUE_POSITION_OUT_OF_RANGE}: {}",
            members.len()
        ));
    }
    if actual == new_position {
        return Ok(queue);
    }

    // The move touches this stream's members alone: every other run of the
    // project keeps its place in the order.
    let fs = store::store_fs(&app)?;
    let base = store::store_base(&app)?;
    let mut index = store::read_queue_index(&fs, &base, &queue.project_key);
    let mut ours: Vec<usize> = index
        .runs
        .iter()
        .enumerate()
        .filter(|(_, e)| members.iter().any(|m| m == &e.run_id))
        .map(|(i, _)| i)
        .collect();
    ours.sort_unstable();
    let mut ordered: Vec<store::QueueIndexEntry> =
        ours.iter().map(|i| index.runs[*i].clone()).collect();
    let moved = ordered.remove(actual);
    ordered.insert(new_position, moved);
    for (slot, entry) in ours.iter().zip(ordered.into_iter()) {
        index.runs[*slot] = entry;
    }
    store::write_queue_index(&fs, &base, &index)?;
    events::announce_queue(&app, &announced);
    queue::project_queue(&app).ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())
}
