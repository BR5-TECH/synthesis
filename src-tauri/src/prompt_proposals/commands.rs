//! The Tauri commands, the event, and the logging of prompt change proposals.

use super::*;

// ---------------------------------------------------------------------------
// Tauri commands, the event, and logging
// ---------------------------------------------------------------------------

/// PCP-FR-17: a proposal was recorded or decided.
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
pub(super) fn announce<R: tauri::Runtime>(
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
        },
    );
}

/// PCP-FR-28: one `WARN` naming what refused and why.
pub(super) fn log_refusal<R: tauri::Runtime>(app: &tauri::AppHandle<R>, operation: &'static str, reason: &str)
where
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
    project
        .require_root()
        .map_err(|_| ERR_NO_PROJECT_OPEN.to_string())
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

#[tauri::command]
pub fn load_prompt_change_proposal_content(
    proposal_id: String,
    project: State<'_, ProjectState>,
) -> Result<PromptProposalContent, String> {
    let root = require_root(&project)?;
    load_content_impl(&root, &proposal_id)
}

/// PCP-FR-22 / PCP-FR-23 / PCP-FR-24: the author's own text over a pending
/// proposal's candidate, and nothing else anywhere.
///
/// Emits no `"prompt change proposals changed"` on success — the proposal's
/// state is exactly what it was, and the surfaces that care read
/// `candidate_edited` from the record they already hold or from their next list.
#[tauri::command]
pub fn save_prompt_change_proposal_candidate<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    content: String,
    baseline_checksum: String,
    project: State<'_, ProjectState>,
) -> Result<PromptCandidateSaved, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let root = require_root(&project)?;
    let saved = save_candidate_impl(&root, &proposal_id, &content, &baseline_checksum)
        .inspect_err(|e| log_refusal(&app, "save_candidate", e))?;
    // PCP-FR-28: a DEBUG record naming the artifact and the proposal — and never
    // the author's rewrite, which is material they composed. The byte count is
    // the shape of the write rather than any of its content.
    logging::log_debug(
        &app,
        &BUFFER,
        &[Domain::Backend],
        "prompt change proposal candidate saved",
        log_fields! {
            "proposalId" => &proposal_id,
            "bytes" => content.len(),
        },
    );
    Ok(saved)
}

/// PCP-FR-22: write the author's own text over a **pending** proposal's
/// candidate, and nothing else anywhere.
///
/// The whole reason the review surface can promise that a candidate under
/// revision has cost the project nothing (PCR-FR-22): no file of the project is
/// created, modified, or deleted, no `"artifact changed externally"` follows,
/// nothing is recorded in the recently-edited list, and neither decision
/// operation is called. It writes exactly one file.
pub fn save_candidate_impl(
    root: &fs::RootFs,
    proposal_id: &str,
    content: &str,
    baseline_checksum: &str,
) -> Result<PromptCandidateSaved, String> {
    let mut record = read_proposal(root, proposal_id)?;
    // PCP-FR-23: a candidate is the author's to shape only while the decision it
    // is for is still owed.
    if record.state != PromptProposalState::Pending {
        return Err(ERR_ALREADY_DECIDED.to_string());
    }
    let dir = proposals_dir(root)?;
    let path = at(&dir, proposal_id, CONTENT_EXT);

    // PCP-FR-24: checked before anything is written, so a stale save leaves the
    // candidate byte-for-byte as it was and the caller resolves the divergence
    // explicitly rather than discovering it after an overwrite.
    let current = root
        .sha256_file(&path)
        .map_err(|_| ERR_PROPOSAL_NOT_FOUND.to_string())?;
    if current != baseline_checksum {
        return Err(ERR_CANDIDATE_STALE.to_string());
    }

    // PCP-FR-09: written atomically and verbatim. A candidate is never
    // re-indented, normalised, re-encoded, or trimmed.
    root.write_text_atomic(&path, content)
        .map_err(|_| ERR_WRITE_FAILED.to_string())?;
    let checksum = root
        .sha256_file(&path)
        .map_err(|_| ERR_WRITE_FAILED.to_string())?;

    // PCP-FR-22: the flag describes the *text*, so an edit that restores the
    // agent's own bytes exactly clears it again.
    if let Some(origin) = record.origin_checksum.as_deref() {
        let edited = checksum != origin;
        if edited != record.candidate_edited {
            record.candidate_edited = edited;
            // A record that will not write leaves the candidate written and the
            // flag stale. That is the harmless way round — the flag drives a line
            // of prose, while the candidate is the author's work.
            let _ = root.write_toml_atomic(at(&dir, proposal_id, RECORD_EXT), &record);
        }
    }

    Ok(PromptCandidateSaved { checksum })
}

#[tauri::command]
pub fn apply_prompt_change_proposal<R: tauri::Runtime>(
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
    let root = require_root(&project)?;
    let repo_store = require_store(&project)?;
    // CMS-FR-12 / PCP-FR-16: resolved before anything is written, so a decision
    // that cannot be attributed changes neither the file nor the record.
    let by = comments::acting_participant(&store, &project)?;
    decide(
        &app,
        &root,
        &repo_store,
        &proposal_id,
        Decision::Accept,
        feedback.as_deref(),
        &by,
        &tracker,
    )
    .inspect_err(|e| log_refusal(&app, "apply", e))
}

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
    let root = require_root(&project)?;
    let repo_store = require_store(&project)?;
    let by = comments::acting_participant(&store, &project)?;
    decide(
        &app,
        &root,
        &repo_store,
        &proposal_id,
        Decision::Decline,
        feedback.as_deref(),
        &by,
        &tracker,
    )
    .inspect_err(|e| log_refusal(&app, "decline", e))
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
        Reconciliation::Settled {
            proposal_id: completed,
            comment_id,
        } if completed == &proposal_id => comment_id.clone(),
        _ => None,
    };
    Ok(PromptDecisionOutcome {
        proposal: record,
        comment_id,
        origin_kind: origin_kind_of(discussion),
    })
}
