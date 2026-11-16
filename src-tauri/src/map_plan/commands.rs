//! The Tauri commands of the plan proposals (`MPR-map-plan-proposals.md`,
//! Contract surface). Each command body is a function of this module that a
//! test calls on a mock application.

use super::errors::error;
use super::model::{MapPlanError, MapPlanProposal};
use super::recording::open_store;
use super::{deciding, rules, store};

/// MPR-FR-TQVZ, `"list map plan proposals (discussion id)"`: the proposals of the
/// Map discussion, oldest first, each row with its `blocked_by` (MPR-FR-YLKL).
pub fn list_proposals<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    discussion_id: &str,
) -> Result<Vec<MapPlanProposal>, MapPlanError> {
    let store = open_store(app).inspect_err(|e| {
        super::errors::log_operation(app, true, "list", "refused", "", &e.code);
    })?;
    Ok(store::read_for(&store, discussion_id).into_iter().map(rules::with_blocks).collect())
}

/// MPR-FR-NXDX: the proposals of a Map discussion from a comments store, oldest
/// first, each row with its `blocked_by`. A store that holds none gives none.
pub fn proposals_in(store: &crate::fs::RootFs, discussion_id: &str) -> Vec<MapPlanProposal> {
    store::read_for(store, discussion_id).into_iter().map(rules::with_blocks).collect()
}

/// `"list map plan proposals (discussion id)"`.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn list_map_plan_proposals(
    app: tauri::AppHandle,
    discussion_id: String,
) -> Result<Vec<MapPlanProposal>, MapPlanError> {
    list_proposals(&app, &discussion_id)
}

/// `"accept map plan row (discussion id, proposal id, row key)"`. The body can
/// make GitHub requests, so it runs off the window thread.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub async fn accept_map_plan_row(
    app: tauri::AppHandle,
    discussion_id: String,
    proposal_id: String,
    row_key: String,
) -> Result<MapPlanProposal, MapPlanError> {
    run_off_thread(move || deciding::accept_row(&app, &discussion_id, &proposal_id, &row_key)).await
}

/// `"reject map plan row (discussion id, proposal id, row key)"`.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub async fn reject_map_plan_row(
    app: tauri::AppHandle,
    discussion_id: String,
    proposal_id: String,
    row_key: String,
) -> Result<MapPlanProposal, MapPlanError> {
    run_off_thread(move || deciding::reject_row(&app, &discussion_id, &proposal_id, &row_key)).await
}

async fn run_off_thread<F>(work: F) -> Result<MapPlanProposal, MapPlanError>
where
    F: FnOnce() -> Result<MapPlanProposal, MapPlanError> + Send + 'static,
{
    crate::project_scope::context::spawn_blocking(work)
        .await
        .map_err(|_| error("store_unavailable", "The operation stopped unexpectedly."))?
}
