//! The Tauri commands, the event, and the logs of change edit proposals.

use super::*;
use crate::global_settings::GlobalSettingsStore;
use crate::project::ProjectState;
use tauri::{Emitter, State};

/// CEP-FR-SQCG: `"change edit proposals changed"`, kebab-case as every event.
pub const CHANGE_PROPOSALS_CHANGED: &str = "change-edit-proposals-changed";

/// CEP-FR-SQCG: what the event is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeProposalCause {
    Recorded,
    Updated,
    Decided,
    Held,
    Committed,
    Reconciled,
}

impl ChangeProposalCause {
    fn as_str(self) -> &'static str {
        match self {
            Self::Recorded => "recorded",
            Self::Updated => "updated",
            Self::Decided => "decided",
            Self::Held => "held",
            Self::Committed => "committed",
            Self::Reconciled => "reconciled",
        }
    }
}

/// CEP-FR-SQCG: the payload.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeProposalsChanged {
    pub stream_id: String,
    pub change: String,
    pub proposal: ChangeEditProposal,
    pub cause: ChangeProposalCause,
}

/// CEP-FR-SQCG / CEP-FR-MZFF: emit the event and log the change. The emit is
/// best-effort: a written proposal is never reported as failed because its
/// notification was not delivered.
pub fn announce<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    proposal: &ChangeEditProposal,
    cause: ChangeProposalCause,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let _ = app.emit(
        CHANGE_PROPOSALS_CHANGED,
        ChangeProposalsChanged {
            stream_id: proposal.stream_id.clone(),
            change: proposal.change.clone(),
            proposal: proposal.clone(),
            cause,
        },
    );
    // CEP-FR-MZFF: ids and counts, never hunk text, file text, the rationale,
    // or feedback.
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "change edit proposal changed",
        log_fields! {
            "streamId" => &proposal.stream_id,
            "change" => &proposal.change,
            "proposalId" => &proposal.id,
            "state" => proposal.state.as_str(),
            "pending" => proposal.counts.pending,
            "accepted" => proposal.counts.accepted,
            "rejected" => proposal.counts.rejected,
            "discussing" => proposal.counts.discussing,
            "withdrawn" => proposal.counts.withdrawn,
            "commitOwed" => proposal.commit_owed,
            "cause" => cause.as_str(),
        },
    );
}

pub(super) fn log_recorded<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    record: &ChangeEditProposal,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "change edit proposal recorded",
        log_fields! {
            "streamId" => &record.stream_id,
            "change" => &record.change,
            "proposalId" => &record.id,
            "hunks" => record.hunk_count,
        },
    );
}

/// What a refusal record names (CEP-FR-MZFF): the stream and the change, or
/// the proposal, as far as the refused call knows them.
pub(super) enum Subject<'a> {
    Change { stream_id: &'a str, change: &'a str },
    Proposal(&'a str),
}

/// CEP-FR-MZFF: one `WARN` for each refusal, naming what it is about.
pub(super) fn log_refusal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    operation: &'static str,
    reason: &str,
    subject: Subject<'_>,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let mut fields = log_fields! { "operation" => operation, "reason" => reason };
    match subject {
        Subject::Change { stream_id, change } => {
            fields.insert("streamId".to_string(), serde_json::json!(stream_id));
            fields.insert("change".to_string(), serde_json::json!(change));
        }
        Subject::Proposal(id) => {
            fields.insert("proposalId".to_string(), serde_json::json!(id));
        }
    }
    logging::log_warn(app, &BUFFER, &[Domain::Backend], "change edit proposal refused", fields);
}

/// PCP-FR-28 as applied here: a rollback, a roll-forward, or a comment left
/// owed.
pub(super) fn log_recovery<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    outcome: &'static str,
    record: &ChangeEditProposal,
    reason: &str,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    logging::log_warn(
        app,
        &BUFFER,
        &[Domain::Backend],
        "change edit acceptance reconciled",
        log_fields! {
            "streamId" => &record.stream_id,
            "change" => &record.change,
            "proposalId" => &record.id,
            "outcome" => outcome,
            "reason" => reason,
        },
    );
}

/// CEP-FR-TFOY: the repository machine store of the open project.
pub(super) fn require_store(project: &ProjectState) -> Result<fs::RootFs, String> {
    project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    project.require_store()
}

/// CEP-FR-HUFE: every proposal of the change, most recently created first,
/// after any interrupted acceptance has been settled.
// Async: it takes the change lock, which a commit can hold for a while,
// so it never blocks the main thread.
#[tauri::command(async)]
pub fn list_change_edit_proposals<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
    change: String,
    project: State<'_, ProjectState>,
) -> Result<Vec<ChangeEditProposal>, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let store = require_store(&project)?;
    let sites = AppSites { app: &app };
    for record in list_proposals_impl(&store, &stream_id, &change) {
        let _guard = change_lock(&stream_id, &change);
        let _ = reconcile_locked(&app, &store, &sites, &record.id);
    }
    Ok(list_proposals_impl(&store, &stream_id, &change))
}

/// CEP-FR-HUFE: the hunks of a proposal and their placements.
// Async: it takes the change lock, which a commit can hold for a while,
// so it never blocks the main thread.
#[tauri::command(async)]
pub fn load_change_edit_proposal_hunks<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    project: State<'_, ProjectState>,
) -> Result<ChangeProposalHunks, String> {
    let store = require_store(&project)?;
    load_hunks_impl(&store, &AppSites { app: &app }, &proposal_id)
}

/// The shared body of the decisions, once the author is known.
pub(super) fn decide_command<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    operation: &'static str,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    request: DecisionRequest<'_>,
) -> Result<ChangeDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let proposal_id = request.proposal_id;
    let hunk = request.hunk;
    let outcome = decide(app, store, sites, request)
        .inspect_err(|e| log_refusal(app, operation, e, Subject::Proposal(proposal_id)))?;
    logging::log_debug(
        app,
        &BUFFER,
        &[Domain::Backend],
        "change edit hunk decided",
        log_fields! {
            "operation" => operation,
            "proposalId" => proposal_id,
            "hunkId" => hunk.unwrap_or(""),
            "resolved" => outcome.resolved,
            "committed" => outcome.committed,
        },
    );
    Ok(outcome)
}

fn decide_from_command<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    operation: &'static str,
    request: (&str, Option<&str>, Decision, Option<&str>),
    settings: &GlobalSettingsStore,
    project: &ProjectState,
) -> Result<ChangeDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let (proposal_id, hunk, decision, feedback) = request;
    let store = require_store(project)?;
    let by = comments::acting_participant(settings, project)?;
    let request = DecisionRequest { proposal_id, decision, hunk, feedback, by: &by };
    decide_command(app, operation, &store, &AppSites { app }, request)
}

/// CEP-FR-OPGW: accept one hunk, writing its file in the stream's working copy.
#[tauri::command(async)]
pub fn accept_change_edit_hunk<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    hunk_id: String,
    feedback: Option<String>,
    settings: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<ChangeDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let request =
        (proposal_id.as_str(), Some(hunk_id.as_str()), Decision::Accept, feedback.as_deref());
    decide_from_command(&app, "accept_hunk", request, &settings, &project)
}

/// CEP-FR-UCVO: reject one hunk. It writes nothing into the working copy.
#[tauri::command(async)]
pub fn reject_change_edit_hunk<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    hunk_id: String,
    feedback: Option<String>,
    settings: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<ChangeDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let request =
        (proposal_id.as_str(), Some(hunk_id.as_str()), Decision::Decline, feedback.as_deref());
    decide_from_command(&app, "reject_hunk", request, &settings, &project)
}

/// CEP-FR-UCVO: decline the proposal — reject every undecided hunk.
#[tauri::command(async)]
pub fn decline_change_edit_proposal<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    feedback: Option<String>,
    settings: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<ChangeDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let request = (proposal_id.as_str(), None, Decision::Decline, feedback.as_deref());
    decide_from_command(&app, "decline", request, &settings, &project)
}

/// CEP-FR-UCVO: hold one hunk for discussion, or release it.
// Async: it takes the change lock, which a commit can hold for a while,
// so it never blocks the main thread.
#[tauri::command(async)]
pub fn set_change_edit_hunk_discussing<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    hunk_id: String,
    discussing: bool,
    project: State<'_, ProjectState>,
) -> Result<ChangeEditProposal, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let store = require_store(&project)?;
    hold_in_process(&app, &store, &proposal_id, &hunk_id, discussing)
}

/// CEP-FR-UCVO: the hold a command and `decide_change_edits` share.
pub(crate) fn hold_in_process<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    proposal_id: &str,
    hunk_id: &str,
    discussing: bool,
) -> Result<ChangeEditProposal, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    // CEP-FR-KGOL: the plan proposal waits while its questions stand.
    if (AppSites { app }).plan_questions_pending(proposal_id) {
        log_refusal(app, "hold_hunk", ERR_PLAN_QUESTIONS_PENDING, Subject::Proposal(proposal_id));
        return Err(ERR_PLAN_QUESTIONS_PENDING.to_string());
    }
    let proposal = hold_hunk_impl(store, proposal_id, hunk_id, discussing)
        .inspect_err(|e| log_refusal(app, "hold_hunk", e, Subject::Proposal(proposal_id)))?;
    announce(app, &proposal, ChangeProposalCause::Held);
    Ok(proposal)
}

/// CEP-FR-UCVO: the author's own text over one undecided hunk.
// Async: it takes the change lock, which a commit can hold for a while,
// so it never blocks the main thread.
#[tauri::command(async)]
pub fn edit_change_edit_hunk<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    hunk_id: String,
    after: String,
    baseline_checksum: String,
    project: State<'_, ProjectState>,
) -> Result<ChangeCandidateSaved, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let store = require_store(&project)?;
    // CEP-FR-KGOL: the plan proposal waits while its questions stand.
    if (AppSites { app: &app }).plan_questions_pending(&proposal_id) {
        log_refusal(&app, "edit_hunk", ERR_PLAN_QUESTIONS_PENDING, Subject::Proposal(&proposal_id));
        return Err(ERR_PLAN_QUESTIONS_PENDING.to_string());
    }
    let saved = edit_hunk_impl(&store, &proposal_id, &hunk_id, &after, &baseline_checksum)
        .inspect_err(|e| log_refusal(&app, "edit_hunk", e, Subject::Proposal(&proposal_id)))?;
    logging::log_debug(
        &app,
        &BUFFER,
        &[Domain::Backend],
        "change edit hunk edited",
        log_fields! { "proposalId" => &proposal_id, "hunkId" => &hunk_id, "bytes" => after.len() },
    );
    Ok(saved)
}

/// CEP-FR-IYHP: finish a decision: settle an interrupted acceptance, append an
/// owed comment, and commit every file the change still owes.
#[tauri::command(async)]
pub fn complete_change_edit_decision<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    project: State<'_, ProjectState>,
) -> Result<ChangeDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let store = require_store(&project)?;
    complete_in_process(&app, &store, &AppSites { app: &app }, &proposal_id)
}

/// CEP-FR-IYHP: the completion a command and the decide tool share.
pub(crate) fn complete_in_process<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    proposal_id: &str,
) -> Result<ChangeDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let (held, _) = read_proposal(store, proposal_id)?;
    let _guard = change_lock(&held.stream_id, &held.change);
    reconcile_locked(app, store, sites, proposal_id)
        .inspect_err(|e| log_refusal(app, "complete", e, Subject::Proposal(proposal_id)))?;
    let (record, _) = read_proposal(store, proposal_id)?;
    let owes = read_change(store, &record.stream_id, &record.change).iter().any(|p| p.commit_owed);
    let mut committed = false;
    if owes && record.state != ChangeProposalState::Pending {
        let site = sites.site(&record.stream_id, &record.change).ok();
        committed = commit_close(app, store, site.as_ref(), proposal_id);
    }
    let (record, _) = read_proposal(store, proposal_id)?;
    let origin_kind = locate_thread(store, &record.stream_id, &record.change, &record.thread_id)
        .map(|t| origin_kind_of(&t))
        .unwrap_or(ORIGIN_CHANGE_DISCUSSION);
    let resolved = record.state != ChangeProposalState::Pending;
    Ok(ChangeDecisionOutcome {
        proposal: record,
        committed,
        resolved,
        comment_id: None,
        origin_kind,
    })
}
