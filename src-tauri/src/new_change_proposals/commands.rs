//! The Tauri commands, the event, and the logs of new change proposals.

use super::*;
use crate::change_proposals::AppSites;
use crate::global_settings::GlobalSettingsStore;
use crate::project::ProjectState;
use tauri::{Emitter, State};

/// NCP-FR-VLOO: `"new change proposals changed"`.
pub const NEW_CHANGE_PROPOSALS_CHANGED: &str = "new-change-proposals-changed";

/// NCP-FR-VLOO: what the event is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NewChangeCause {
    Recorded,
    Updated,
    Decided,
    Committed,
    Confirmed,
}

/// NCP-FR-VLOO: the payload.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewChangeProposalsChanged {
    pub stream_id: String,
    pub change: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal: Option<NewChangeProposal>,
    pub cause: NewChangeCause,
}

pub(super) fn announce<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    change: &str,
    proposal: Option<&NewChangeProposal>,
    cause: NewChangeCause,
) {
    let _ = app.emit(
        NEW_CHANGE_PROPOSALS_CHANGED,
        NewChangeProposalsChanged {
            stream_id: stream_id.to_string(),
            change: change.to_string(),
            proposal: proposal.cloned(),
            cause,
        },
    );
}

/// NCP-FR-OLPB: one `INFO` record. Ids, the proposed name, and counts; no
/// file text and no rationale.
pub(super) fn log_info<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    what: &'static str,
    record: &NewChangeProposal,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "new change proposal changed",
        log_fields! {
            "operation" => what,
            "streamId" => &record.stream_id,
            "change" => &record.change,
            "proposalId" => &record.id,
            "name" => &record.name,
            "files" => record.paths.len(),
            "position" => record.position,
            "total" => record.total,
            "state" => record.state.as_str(),
            "commitOwed" => record.commit_owed,
        },
    );
}

/// NCP-FR-OLPB: one `WARN` record for a refusal.
pub(super) fn log_refusal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    operation: &'static str,
    reason: &str,
    (stream_id, change): (&str, &str),
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    logging::log_warn(
        app,
        &BUFFER,
        &[Domain::Backend],
        "new change proposal refused",
        log_fields! {
            "operation" => operation,
            "streamId" => stream_id,
            "change" => change,
            "reason" => reason,
        },
    );
}

fn require_store(project: &ProjectState) -> Result<fs::RootFs, String> {
    project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    project.require_store()
}

/// NCP contract surface: every proposal of the change, newest first, and
/// its confirmed split.
#[tauri::command(async)]
pub fn list_new_change_proposals(
    stream_id: String,
    change: String,
    project: State<'_, ProjectState>,
) -> Result<NewChangeProposals, String> {
    let store = require_store(&project)?;
    Ok(NewChangeProposals {
        proposals: list_proposals_impl(&store, &stream_id, &change),
        split: read_split(&store, &stream_id, &change),
    })
}

/// The proposed files of one proposal.
#[derive(Debug, Clone, Serialize)]
pub struct NewChangeFiles {
    pub files: Vec<ProposedFile>,
}

/// NCP contract surface: `"load new change proposal files"`.
#[tauri::command(async)]
pub fn load_new_change_proposal_files(
    proposal_id: String,
    project: State<'_, ProjectState>,
) -> Result<NewChangeFiles, String> {
    let store = require_store(&project)?;
    let (_, dir) = read_proposal(&store, &proposal_id)?;
    Ok(NewChangeFiles { files: read_files(&store, &dir, &proposal_id)? })
}

fn decide_command<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    proposal_id: &str,
    accept: bool,
    feedback: Option<&str>,
    (settings, project): (&GlobalSettingsStore, &ProjectState),
) -> Result<NewChangeDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let store = require_store(project)?;
    let by = comments::acting_participant(settings, project)?;
    decide_new_change(
        app,
        &store,
        &AppSites { app },
        NewChangeDecision { proposal_id, accept, feedback, by: &by },
    )
}

/// NCP-FR-UWOR: `"accept new change proposal"`.
#[tauri::command(async)]
pub fn accept_new_change_proposal<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    feedback: Option<String>,
    settings: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<NewChangeDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    decide_command(&app, &proposal_id, true, feedback.as_deref(), (&settings, &project))
}

/// NCP-FR-TWDN: `"reject new change proposal"`.
#[tauri::command(async)]
pub fn reject_new_change_proposal<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    feedback: Option<String>,
    settings: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<NewChangeDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    decide_command(&app, &proposal_id, false, feedback.as_deref(), (&settings, &project))
}

/// NCP-FR-KRSA: `"complete new change decision"`.
#[tauri::command(async)]
pub fn complete_new_change_decision<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    project: State<'_, ProjectState>,
) -> Result<NewChangeDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let store = require_store(&project)?;
    complete_new_change(&app, &store, &AppSites { app: &app }, &proposal_id)
}

/// NCP-FR-MQCS: `"confirm change split"`.
#[tauri::command(async)]
pub fn confirm_change_split<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
    change: String,
    settings: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<ChangeSplit, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let store = require_store(&project)?;
    let by = comments::acting_participant(&settings, &project)?;
    confirm_split(&app, &store, &stream_id, &change, &by)
}
