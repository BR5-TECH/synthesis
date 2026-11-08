//! The operations the frontend invokes about a decomposition (DCP contract
//! surface, "New-draft proposals").

use serde::Serialize;
use tauri::State;

use crate::comments;
use crate::global_settings::GlobalSettingsStore;
use crate::project::ProjectState;

use super::*;

/// The answer of `"load new draft proposal body"`.
#[derive(Debug, Clone, Serialize)]
pub struct NewDraftBody {
    pub body: String,
}

/// DCP-FR-ZDSP: every new-draft proposal of the draft, newest first.
#[tauri::command]
pub fn list_new_draft_proposals(
    draft_id: String,
    project: State<'_, ProjectState>,
) -> Result<Vec<NewDraftProposal>, String> {
    let root = project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    list_impl(&root, &draft_id)
}

/// DCP contract surface: the proposed body as it stands.
#[tauri::command]
pub fn load_new_draft_proposal_body(
    proposal_id: String,
    project: State<'_, ProjectState>,
) -> Result<NewDraftBody, String> {
    let root = project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    Ok(NewDraftBody { body: load_body_impl(&root, &proposal_id)? })
}

fn decide_command<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    decision: NewDraftDecision,
    feedback: Option<String>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<NewDraftDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let root = project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    let repo_store = project.require_store()?;
    let by = comments::acting_participant(&store, &project)?;
    decide_new_draft_in_process(
        &app,
        &root,
        &repo_store,
        &proposal_id,
        decision,
        feedback.as_deref(),
        &by,
    )
}

/// DCP-FR-KOOA: accept one new-draft proposal and create its draft.
#[tauri::command(async)]
pub fn accept_new_draft_proposal<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    feedback: Option<String>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<NewDraftDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    // DCP-FR-WGBN: the main branch write check, before the draft is created.
    crate::git::write_guard::commands::require_project_branch_write(
        &app,
        "accept_new_draft_proposal",
    )?;
    decide_command(app, proposal_id, NewDraftDecision::Accept, feedback, store, project)
}

/// DCP-FR-FZEP: reject one new-draft proposal; nothing is created.
#[tauri::command]
pub fn reject_new_draft_proposal<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    feedback: Option<String>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<NewDraftDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    decide_command(app, proposal_id, NewDraftDecision::Reject, feedback, store, project)
}

/// DCP-FR-VLLP: the author ends the decomposition.
#[tauri::command]
pub fn confirm_draft_decomposition<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    draft_id: String,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<drafts::DraftRecord, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let root = project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    let repo_store = project.require_store()?;
    let by = comments::acting_participant(&store, &project)?;
    confirm_decomposition_impl(&app, &root, &repo_store, &draft_id, &by, None)
}
