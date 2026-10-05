//! The operations the frontend invokes about a draft's proposed changes
//! (`DCP-draft-change-proposals.md` contract surface).
//!
//! Every one of these is argument-shuffling around the decision engine in the
//! parent module: resolve the project's root, resolve the acting participant,
//! call in, log what refused. They are here rather than there so the engine —
//! the anchoring, the ledger, the transaction it delegates — is read without
//! the Tauri plumbing wrapped around it.

use tauri::State;

use crate::comments;
use crate::drafts;
use crate::fs;
use crate::global_settings::GlobalSettingsStore;
use crate::logging::LogSink;
use crate::project::ProjectState;

use super::*;

/// DCP-FR-09 / DCP-FR-28: every proposal the draft holds, after the draft's
/// acceptance journal has been reconciled — so no proposal is ever reported in a
/// state the journal has not yet settled.
#[tauri::command]
pub fn list_draft_change_proposals<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    draft_id: String,
    project: State<'_, ProjectState>,
) -> Result<Vec<DraftChangeProposal>, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let root = project.require_root()?;
    let repo_store = project.require_store()?;
    crate::draft_history::reconcile(&app, &root, &repo_store, &draft_id)?;
    list_proposals_impl(&root, &draft_id)
}

/// DCP-FR-10: the proposal's hunks and their placements, for a review about to
/// render them.
#[tauri::command]
pub fn load_draft_change_proposal_hunks(
    proposal_id: String,
    project: State<'_, ProjectState>,
) -> Result<ProposalHunks, String> {
    let root = project.require_root()?;
    load_hunks_impl(&root, &proposal_id)
}

/// a turn (DCR-FR-15).
#[tauri::command]
pub fn accept_draft_change_hunk<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    hunk_id: String,
    feedback: Option<String>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<DecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    decide_hunk(app, proposal_id, Some(hunk_id), Decision::Accept, feedback, store, project)
}

/// DCP-FR-14: reject one change, writing nothing into the draft.
#[tauri::command]
pub fn reject_draft_change_hunk<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    hunk_id: String,
    feedback: Option<String>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<DecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    decide_hunk(app, proposal_id, Some(hunk_id), Decision::Decline, feedback, store, project)
}

/// The shared body of the two hunk decisions.
fn decide_hunk<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    hunk_id: Option<String>,
    decision: Decision,
    feedback: Option<String>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<DecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    // DRS-FR-19: a draft a graduation run holds takes no write by any route.
    if let Some((draft_id, _)) = find_proposal(&project.require_root()?, &proposal_id) {
        // DRS-FR-QPSC: an accepted change is a write of a GitHub-shadow
        // draft's prompt, refused with its own reason before the lock.
        if decision == Decision::Accept {
            drafts::require_not_github_shadow(&project.require_root()?, &draft_id)?;
        }
        crate::graduation::require_unlocked_draft(&app, &draft_id)?;
    }
    let root = project.require_root()?;
    let repo_store = project.require_store()?;
    let by = comments::acting_participant(&store, &project)?;
    let operation = if decision == Decision::Accept { "accept_hunk" } else { "reject_hunk" };
    let Decided {
        proposal,
        comment_id,
        origin_kind,
        comment_failed,
    } = decide(
        &app,
        &root,
        &repo_store,
        &proposal_id,
        decision,
        hunk_id.as_deref(),
        feedback.as_deref(),
        &by,
    )
    .inspect_err(|e| log_refusal(&app, operation, e))?;
    if let Some(reason) = comment_failed {
        log_refusal(&app, "hunk_comment", &reason);
    }
    // DCP-FR-16: emitted when a change is decided, after any transaction it
    // delegated has committed.
    announce(&app, &BUFFER, &proposal);
    Ok(DecisionOutcome { proposal, comment_id, origin_kind })
}

/// DCP-FR-PWSF: hold one change for discussion, or release it.
///
/// A held change is **undecided**, so it still occupies the draft's one slot —
/// a change the author is talking about is one they have not decided.
#[tauri::command]
pub fn set_draft_change_hunk_discussing<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    hunk_id: String,
    discussing: bool,
    project: State<'_, ProjectState>,
) -> Result<DraftChangeProposal, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let root = project.require_root()?;
    // The draft is found before the lock, because the lock is per draft and
    // there is no way to name it otherwise. Everything the decision is *checked*
    // against is read again below, inside the lock.
    let (draft_id, _) = find_proposal(&root, &proposal_id).ok_or(ERR_PROPOSAL_NOT_FOUND)?;
    let _guard = draft_lock(&draft_id);
    // DCP-FR-NKTB: read again inside the lock. A decision that landed on
    // another change between the read above and this line has already settled
    // this proposal, and holding a settled change for discussion would put the
    // whole proposal back to `pending` after its acceptance had written the
    // prompt and recorded a version.
    let mut record =
        find_proposal(&root, &proposal_id).map(|(_, r)| r).ok_or(ERR_PROPOSAL_NOT_FOUND)?;
    // DCP-FR-QLMH: as in `decide`, a legacy record is given the row its one
    // change is named by.
    let dir = drafts::draft_proposals_dir(&root, &draft_id)?;
    backfill_legacy_ledger(&root, &dir, &mut record, &prompt_of(&root, &draft_id));
    let row = record
        .ledger
        .iter()
        .find(|row| row.id == hunk_id)
        .ok_or(ERR_HUNK_NOT_FOUND)?;
    if !row.state.is_undecided() {
        return Err(ERR_HUNK_ALREADY_DECIDED.to_string());
    }
    let next = if discussing { hunks::HunkState::Discussing } else { hunks::HunkState::Pending };
    let proposal = set_hunk_state(&root, &draft_id, &proposal_id, &hunk_id, next)
        .inspect_err(|e| log_refusal(&app, "hold_hunk", e))?;
    announce(&app, &BUFFER, &proposal);
    Ok(proposal)
}

/// DCP-FR-25: the author's own text over one undecided change.
///
/// It writes exactly one file — the hunk document — and is not a change to the
/// draft: nothing under `files/` moves and `updated_at` stays where it was.
/// DCP-FR-25: the author's own text over one undecided change.
///
/// It writes exactly one file — the hunk document — and is not a change to the
/// draft: nothing under `files/` moves and `updated_at` stays where it was.
pub fn edit_hunk_impl(
    root: &fs::RootFs,
    proposal_id: &str,
    hunk_id: &str,
    after: &str,
    baseline_checksum: &str,
) -> Result<CandidateSaved, String> {
    // The draft is named before the lock, because the lock is per draft and
    // there is no way to name it otherwise. Everything the write is checked
    // against is read again inside it.
    let (draft_id, _) = find_proposal(root, proposal_id).ok_or(ERR_PROPOSAL_NOT_FOUND)?;
    let _guard = draft_lock(&draft_id);
    let dir = drafts::draft_proposals_dir(root, &draft_id)?;
    // DCP-FR-NKTB: the record this write builds on is the record as it stands
    // now. Written back from a copy read before the lock, an edit would silently
    // discard a decision recorded against a sibling change in the meantime.
    let mut record =
        find_proposal(root, proposal_id).map(|(_, r)| r).ok_or(ERR_PROPOSAL_NOT_FOUND)?;
    let prompt = prompt_of(root, &draft_id);
    backfill_legacy_ledger(root, &dir, &mut record, &prompt);

    let (mut doc, legacy) = read_hunk_document(root, &dir, proposal_id, &prompt)?;
    // DCP-FR-QLMH: a legacy proposal's text is not editable — the surface says
    // so rather than offering an edit that could not be written back. Checked
    // before the ledger, so it is refused as what it is rather than as a change
    // nobody can find.
    if legacy {
        return Err(ERR_WRITE_FAILED.to_string());
    }
    let row = record
        .ledger
        .iter()
        .find(|row| row.id == hunk_id)
        .ok_or(ERR_HUNK_NOT_FOUND)?;
    // DCP-FR-26: a change is the author's to shape only while the decision it is
    // for is still owed.
    if !row.state.is_undecided() {
        return Err(ERR_HUNK_ALREADY_DECIDED.to_string());
    }
    // DCP-FR-27: checked before anything is written, so a stale edit leaves the
    // change byte-for-byte as it was and the caller resolves the divergence.
    if doc.checksum() != baseline_checksum {
        return Err(ERR_CANDIDATE_STALE.to_string());
    }
    let slot = doc
        .hunks
        .iter_mut()
        .find(|h| h.id == hunk_id)
        .ok_or(ERR_HUNK_NOT_FOUND)?;
    slot.after = (!after.is_empty()).then(|| after.to_string());
    let checksum = doc.checksum();
    root.write_text_atomic(hunks_path(&dir, proposal_id), &doc.to_toml())
        .map_err(|_| ERR_WRITE_FAILED.to_string())?;
    // DRS-FR-WYIN / DRS-FR-JDRY: a proposal is private draft storage. The
    // ignore file keeps it out of Git, and no commit names it.
    drafts::git_storage::ensure_private_ignored(root);

    // DCP-FR-25: the flag describes the text, so an edit restoring the agent's
    // own words clears it again.
    if let Some(row) = record.ledger.iter_mut().find(|row| row.id == hunk_id) {
        row.edited = true;
    }
    record.candidate_edited = record.ledger.iter().any(|row| row.edited);
    let _ = root.write_toml_atomic(record_path(&dir, proposal_id), &record);
    // DCP-FR-26: no event — the proposal's state is exactly what it was.
    Ok(CandidateSaved { checksum })
}

#[tauri::command]
pub fn edit_draft_change_hunk<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    hunk_id: String,
    after: String,
    baseline_checksum: String,
    project: State<'_, ProjectState>,
) -> Result<CandidateSaved, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let root = project.require_root()?;
    edit_hunk_impl(&root, &proposal_id, &hunk_id, &after, &baseline_checksum)
        .inspect_err(|e| log_refusal(&app, "edit_hunk", e))
}

#[tauri::command]
pub fn decline_draft_change_proposal<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    proposal_id: String,
    feedback: Option<String>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<DecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let root = project.require_root()?;
    let repo_store = project.require_store()?;
    let by = comments::acting_participant(&store, &project)?;
    let Decided {
        proposal,
        comment_id,
        origin_kind,
        comment_failed,
    } = decide(
        &app,
        &root,
        &repo_store,
        &proposal_id,
        Decision::Decline,
        None,
        feedback.as_deref(),
        &by,
    )
    .inspect_err(|e| log_refusal(&app, "decline", e))?;
    if let Some(reason) = comment_failed {
        log_refusal(&app, "decline_comment", &reason);
    }
    // DCP-FR-14: nothing was written into the draft, so no `drafts-changed`.
    announce(&app, &BUFFER, &proposal);
    Ok(DecisionOutcome {
        proposal,
        comment_id,
        origin_kind,
    })
}

