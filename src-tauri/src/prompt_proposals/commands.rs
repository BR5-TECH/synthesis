//! The Tauri commands, the event, and the logging of prompt change proposals.

use super::*;

// ---------------------------------------------------------------------------
// Tauri commands, the event, and logging
// ---------------------------------------------------------------------------

/// PCP-FR-17: a proposal was recorded, a hunk of it decided, held, or revised.
///
/// Kebab-case for the reason every event in this application is: Tauri rejects
/// the rest of the character set at `emit`, leaving a channel that is silently
/// dead in production.
pub const PROMPT_PROPOSALS_CHANGED: &str = "prompt-change-proposals-changed";

/// What the event carries (PCP-FR-17): the artifact, and the whole proposal as
/// it then stood, so a consumer redraws from the payload without a read of its
/// own.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptProposalsChanged {
    pub artifact_id: String,
    pub proposal: PromptChangeProposal,
}

/// PCP-FR-17 / PCP-FR-28: announce the change and record it.
///
/// Best-effort on the emit: a proposal that was written must not be reported as
/// failed because the notification of it could not be delivered.
pub fn announce<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    proposal: &PromptChangeProposal,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let _ = app.emit(
        PROMPT_PROPOSALS_CHANGED,
        PromptProposalsChanged {
            artifact_id: proposal.artifact_id.clone(),
            proposal: proposal.clone(),
        },
    );
    // PCP-FR-28: the artifact, the proposal, the path, and the outcome — and
    // never the proposed text, the artifact's current text, the rationale, or
    // any feedback, all of which are material a model or an author composed.
    logging::log_info(
        app,
        buffer,
        &[Domain::Backend],
        "prompt change proposal changed",
        log_fields! {
            "artifactId" => &proposal.artifact_id,
            "proposalId" => &proposal.id,
            "path" => &proposal.path,
            "state" => proposal.state.as_str(),
            "pending" => proposal.counts.pending,
            "accepted" => proposal.counts.accepted,
            "rejected" => proposal.counts.rejected,
            "discussing" => proposal.counts.discussing,
        },
    );
}

/// PCP-FR-28: one `WARN` naming what refused and why.
pub(super) fn log_refusal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    operation: &'static str,
    reason: &str,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    logging::log_warn(
        app,
        &BUFFER,
        &[Domain::Backend],
        "prompt change proposal refused",
        log_fields! { "operation" => operation, "reason" => reason },
    );
}

/// PCP-FR-28: a `WARN` for each acceptance outcome a reader has to be able to
/// find afterwards — a rollback, a reconciliation that settled a transaction it
/// found unfinished, and a comment left owed.
///
/// The reason is this module's own fixed prose or a typed error, never the
/// proposed text, the artifact's text, the rationale, or any feedback.
pub(super) fn log_recovery<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    outcome: &'static str,
    record: &PromptChangeProposal,
    reason: &str,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    logging::log_warn(
        app,
        &BUFFER,
        &[Domain::Backend],
        "prompt change acceptance reconciled",
        log_fields! {
            "artifactId" => &record.artifact_id,
            "proposalId" => &record.id,
            "path" => &record.path,
            "outcome" => outcome,
            "reason" => reason,
        },
    );
}

/// The repository machine store the conversation half of a decision is written
/// into (`CMS-comments-storage.md` CMS-FR-01).
/// RMS-FR-HAJC: the store's own refusal, passed through rather than rewritten
/// as "no project open" — a project *is* open, and the two are different things
/// for the surface to say.
pub(super) fn require_store(project: &ProjectState) -> Result<fs::RootFs, String> {
    project.require_store()
}

pub(super) fn require_root(project: &ProjectState) -> Result<fs::RootFs, String> {
    project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())
}

/// PCP-FR-10 / PCP-FR-14: every proposal the artifact holds, after any
/// interrupted transaction has been settled — so no proposal is ever reported in
/// a state a journal has not yet resolved.
#[tauri::command]
pub fn list_prompt_change_proposals<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    artifact_id: String,
    project: State<'_, ProjectState>,
) -> Result<Vec<PromptChangeProposal>, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let root = require_root(&project)?;
    let repo_store = require_store(&project)?;
    // Best-effort: a reconciliation that cannot settle leaves the record reading
    // whatever it reads, which is `pending` for an uncommitted transaction —
    // and a list that refused would leave every surface with no proposals at all
    // (PCR-FR-27).
    for record in list_proposals_impl(&root, &artifact_id) {
        let _ = reconcile(&app, &root, &repo_store, &record.id);
    }
    Ok(list_proposals_impl(&root, &artifact_id))
}

/// PCP-FR-LDHK: the proposal's hunks and their placements against the file as
/// it stands, for a review about to render them.
#[tauri::command]
pub fn load_prompt_change_proposal_hunks(
    proposal_id: String,
    project: State<'_, ProjectState>,
) -> Result<PromptProposalHunks, String> {
    let root = require_root(&project)?;
    load_hunks_impl(&root, &proposal_id)
}

/// The shared body of the hunk decisions and the decline.
#[allow(clippy::too_many_arguments)] // Each is a Tauri state or an argument of the command.
fn decide_command<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    operation: &'static str,
    proposal_id: &str,
    hunk: Option<&str>,
    decision: Decision,
    feedback: Option<&str>,
    store: &GlobalSettingsStore,
    project: &ProjectState,
    tracker: &ContentTracker,
) -> Result<PromptDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let root = require_root(project)?;
    let repo_store = require_store(project)?;
    // CMS-FR-12 / PCP-FR-16: resolved before anything is written, so a decision
    // that cannot be attributed changes neither the file nor the record.
    let by = comments::acting_participant(store, project)?;
    let request = DecisionRequest { proposal_id, decision, hunk, feedback, by: &by };
    let outcome = decide(app, &root, &repo_store, request, tracker)
        .inspect_err(|e| log_refusal(app, operation, e))?;
    // PCP-FR-28: the decision and its shape, never the feedback. DEBUG, because
    // `announce` has already recorded the proposal's new state at INFO.
    logging::log_debug(
        app,
        &BUFFER,
        &[Domain::Backend],
        "prompt change hunk decided",
        log_fields! {
            "operation" => operation,
            "proposalId" => proposal_id,
            "hunkId" => hunk.unwrap_or(""),
            "resolved" => outcome.resolved,
            "state" => outcome.proposal.state.as_str(),
        },
    );
    Ok(outcome)
}

/// PCP-FR-ZQAH: accept one hunk, writing the file through the artifact's own
/// save path inside the one-commit-point transaction.
#[tauri::command]
pub fn accept_prompt_change_hunk<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    hunk_id: String,
    feedback: Option<String>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
    tracker: State<'_, ContentTracker>,
) -> Result<PromptDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    decide_command(
        &app,
        "accept_hunk",
        &proposal_id,
        Some(&hunk_id),
        Decision::Accept,
        feedback.as_deref(),
        &store,
        &project,
        &tracker,
    )
}

/// PCP-FR-RJHN: reject one hunk, writing nothing into the file.
#[tauri::command]
pub fn reject_prompt_change_hunk<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    hunk_id: String,
    feedback: Option<String>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
    tracker: State<'_, ContentTracker>,
) -> Result<PromptDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    decide_command(
        &app,
        "reject_hunk",
        &proposal_id,
        Some(&hunk_id),
        Decision::Decline,
        feedback.as_deref(),
        &store,
        &project,
        &tracker,
    )
}

/// PCP-FR-HLDQ: hold one hunk for discussion, or release it. It decides
/// nothing.
#[tauri::command]
pub fn set_prompt_change_hunk_discussing<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    hunk_id: String,
    discussing: bool,
    project: State<'_, ProjectState>,
) -> Result<PromptChangeProposal, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let root = require_root(&project)?;
    let proposal = hold_hunk_impl(&root, &proposal_id, &hunk_id, discussing)
        .inspect_err(|e| log_refusal(&app, "hold_hunk", e))?;
    announce(&app, &BUFFER, &proposal);
    Ok(proposal)
}

/// PCP-FR-EDTW: the author's own text over one undecided hunk. It is not a
/// change to the file and emits no event.
#[tauri::command]
pub fn edit_prompt_change_hunk<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    hunk_id: String,
    after: String,
    baseline_checksum: String,
    project: State<'_, ProjectState>,
) -> Result<PromptCandidateSaved, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let root = require_root(&project)?;
    let saved = edit_hunk_impl(&root, &proposal_id, &hunk_id, &after, &baseline_checksum)
        .inspect_err(|e| log_refusal(&app, "edit_hunk", e))?;
    // PCP-FR-28: the proposal, the hunk, and the shape of the write — never
    // the author's text.
    logging::log_debug(
        &app,
        &BUFFER,
        &[Domain::Backend],
        "prompt change hunk edited",
        log_fields! {
            "proposalId" => &proposal_id,
            "hunkId" => &hunk_id,
            "bytes" => after.len(),
        },
    );
    Ok(saved)
}

/// PCP-FR-RJHN: decline the proposal — reject every undecided hunk at once,
/// writing nothing into the file.
#[tauri::command]
pub fn decline_prompt_change_proposal<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    feedback: Option<String>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
    tracker: State<'_, ContentTracker>,
) -> Result<PromptDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    decide_command(
        &app,
        "decline",
        &proposal_id,
        None,
        Decision::Decline,
        feedback.as_deref(),
        &store,
        &project,
        &tracker,
    )
}

/// PCP-FR-14: finish an acceptance whose decision comment is still owed.
///
/// It decides nothing itself: against a proposal that owes no comment it is a
/// no-op returning that proposal as it stands, and against one that was rolled
/// back it returns it `pending`. It is the operation the review's retry invokes
/// (PCR-FR-15), and it exists so an owed comment is reachable by a call rather
/// than only by whatever command the author happens to make next.
#[tauri::command]
pub fn complete_prompt_change_decision<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    project: State<'_, ProjectState>,
) -> Result<PromptDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let root = require_root(&project)?;
    let repo_store = require_store(&project)?;
    let held = read_proposal(&root, &proposal_id)?;
    let _guard = artifact_lock(&held.artifact_id);
    let reconciled = reconcile_locked(&app, &root, &repo_store, &proposal_id)
        .inspect_err(|e| log_refusal(&app, "complete", e))?;
    let record = read_proposal(&root, &proposal_id)?;
    let (_, discussion) = locate_thread(&root, &repo_store, &record.thread_id)?;
    let comment_id = match &reconciled {
        Reconciliation::Settled { proposal_id: completed, comment_id }
            if completed == &proposal_id =>
        {
            comment_id.clone()
        }
        _ => None,
    };
    let resolved = record.state != PromptProposalState::Pending;
    Ok(PromptDecisionOutcome {
        proposal: record,
        resolved,
        comment_id,
        origin_kind: origin_kind_of(discussion),
    })
}
