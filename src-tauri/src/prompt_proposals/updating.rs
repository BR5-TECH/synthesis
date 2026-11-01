//! Updating the open proposal of an artifact (`PCP-prompt-change-proposals.md`
//! PCP-FR-RVSN, PCP-FR-RKJI, PCP-FR-FDBD).
//!
//! The semantics are those of a draft's update (DCP-FR-XDRV through
//! DCP-FR-FKMH). The planning is the neutral `crate::text_hunks::updates`, and
//! everything about this store — its folder, its lock, its ledger — is its own
//! (PCP-FR-29).

use super::*;
use crate::comments::ProposalUpdateCounts;
use crate::text_hunks::hunks::{ContextSide, PlanError};
use crate::text_hunks::updates::{self, Standing, UpdateEntry, UpdateError};

/// What a caller hands [`update_prompt_proposal`].
pub struct PromptProposalUpdate<'a> {
    pub artifact_id: &'a str,
    pub updates: &'a [UpdateEntry],
    pub rationale: &'a str,
    pub agent: &'a Participant,
    /// The conversation of the turn, which the comment is appended into.
    pub thread_id: &'a str,
}

/// Why [`update_prompt_proposal`] wrote nothing. Every refusal about one entry
/// carries its one-based place and the length of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateRefusal {
    NoOpenProposal,
    NoUpdates,
    ArtifactNotFound,
    NotAPromptArtifact,
    HunkNotFound { at: usize, of: usize },
    HunkNotUpdatable { at: usize, of: usize },
    HunkNamedTwice { at: usize, of: usize },
    NoChange { at: usize, of: usize },
    HunkAnchorLost { at: usize, of: usize },
    HunkAmbiguous { at: usize, of: usize },
    HunkOverlap { at: usize, of: usize },
    HunkContextTooLong { at: usize, of: usize, side: ContextSide },
    ThreadResolved,
    NotRecorded,
}

impl UpdateRefusal {
    fn of_plan(error: UpdateError, of: usize) -> Self {
        match error {
            UpdateError::NoUpdates | UpdateError::Plan(PlanError::NoHunks) => Self::NoUpdates,
            UpdateError::UnknownHunk(at) => Self::HunkNotFound { at, of },
            UpdateError::NotUpdatable(at) => Self::HunkNotUpdatable { at, of },
            UpdateError::NamedTwice(at) => Self::HunkNamedTwice { at, of },
            UpdateError::NoChange(at) => Self::NoChange { at, of },
            UpdateError::Plan(PlanError::AnchorLost(at)) => Self::HunkAnchorLost { at, of },
            UpdateError::Plan(PlanError::Ambiguous(at)) => Self::HunkAmbiguous { at, of },
            UpdateError::Plan(PlanError::Overlap(at)) => Self::HunkOverlap { at, of },
            UpdateError::Plan(PlanError::ContextTooLong(at, side)) => {
                Self::HunkContextTooLong { at, of, side }
            }
        }
    }

    /// A stable code for the log record (PCP-FR-28).
    pub fn code(self) -> &'static str {
        match self {
            Self::NoOpenProposal => "no_open_proposal",
            Self::NoUpdates => "no_updates",
            Self::ArtifactNotFound => ERR_ARTIFACT_NOT_FOUND,
            Self::NotAPromptArtifact => ERR_NOT_A_PROMPT_ARTIFACT,
            Self::HunkNotFound { .. } => ERR_HUNK_NOT_FOUND,
            Self::HunkNotUpdatable { .. } => "hunk_not_updatable",
            Self::HunkNamedTwice { .. } => "hunk_named_twice",
            Self::NoChange { .. } => "no_change",
            Self::HunkAnchorLost { .. } => "hunk_anchor_lost",
            Self::HunkAmbiguous { .. } => ERR_HUNK_AMBIGUOUS,
            Self::HunkOverlap { .. } => "hunk_overlap",
            Self::HunkContextTooLong { .. } => ERR_HUNK_CONTEXT_TOO_LONG,
            Self::ThreadResolved => comments::ERR_DISCUSSION_RESOLVED,
            Self::NotRecorded => ERR_WRITE_FAILED,
        }
    }
}

/// PCP-FR-RVSN / PCP-FR-RKJI: apply an ordered list of updates to the
/// artifact's pending proposal, and append one comment, as one act.
///
/// PCP-FR-FDBD: serialised per artifact with every decision, so no acceptance
/// reads a half-written ledger.
pub fn update_prompt_proposal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
    update: PromptProposalUpdate<'_>,
) -> Result<PromptChangeProposal, UpdateRefusal>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let _guard = artifact_lock(update.artifact_id);
    let outcome = update_locked(app, root, store, &update);
    if let Err(refusal) = &outcome {
        log_refusal(app, "update", refusal.code());
    }
    outcome
}

fn update_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
    update: &PromptProposalUpdate<'_>,
) -> Result<PromptChangeProposal, UpdateRefusal>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let artifact_path =
        require_prompt_artifact(root, update.artifact_id).map_err(|e| match e.as_str() {
            ERR_NOT_A_PROMPT_ARTIFACT => UpdateRefusal::NotAPromptArtifact,
            _ => UpdateRefusal::ArtifactNotFound,
        })?;
    // PCP-FR-14: an interrupted acceptance is settled before the update plans.
    if let Some(open) = pending_for(root, update.artifact_id) {
        reconcile_locked(app, root, store, &open.id).map_err(|_| UpdateRefusal::NotRecorded)?;
    }
    // PCP-FR-RVSN: only the pending proposal is updated, and a closed one never.
    let mut record = pending_for(root, update.artifact_id).ok_or(UpdateRefusal::NoOpenProposal)?;
    if update.updates.is_empty() {
        return Err(UpdateRefusal::NoUpdates);
    }
    let current = root.read_text(&artifact_path).map_err(|_| UpdateRefusal::ArtifactNotFound)?;
    let dir = ensure_folder(root).map_err(|_| UpdateRefusal::NotRecorded)?;
    let proposal_id = record.id.clone();

    // PCP-FR-RKJI: the bytes a failed append puts back.
    let prior_hunks = root.read_text(at(&dir, &proposal_id, HUNKS_EXT)).ok();
    let prior_record = root.read_text(at(&dir, &proposal_id, RECORD_EXT)).ok();

    // PCP-FR-FDBD: a legacy proposal names its one hunk `legacy`.
    ledger::backfill_legacy_ledger(&mut record);
    let (doc, _legacy) = read_hunk_document(root, &dir, &proposal_id, &current)
        .map_err(|_| UpdateRefusal::NotRecorded)?;

    let standing = |id: &str| {
        record.ledger.iter().find(|row| row.id == id).map(|row| {
            if row.state.is_updatable() {
                Standing::Live
            } else {
                Standing::Settled
            }
        })
    };
    let of = update.updates.len();
    let plan = updates::plan_update(&current, &doc, standing, update.updates, new_note_id)
        .map_err(|error| UpdateRefusal::of_plan(error, of))?;

    // CMS-FR-17: a resolved conversation takes no comment, so nothing is written.
    let (thread_artifact, discussion) =
        locate_thread(root, store, update.thread_id).map_err(|_| UpdateRefusal::NotRecorded)?;
    let target = thread_ref(&thread_artifact, discussion);
    if comments::thread_at(store, target, update.thread_id)
        .map_err(|_| UpdateRefusal::NotRecorded)?
        .resolved
    {
        return Err(UpdateRefusal::ThreadResolved);
    }

    // PCP-FR-RVSN (as DCP-FR-TQBO and DCP-FR-IAFV): the ledger follows the plan.
    for row in record.ledger.iter_mut() {
        if plan.changed.contains(&row.id) {
            row.edited = false;
            row.revision += 1;
            row.state = HunkState::Pending;
            row.decided_at = None;
        } else if plan.withdrawn.contains(&row.id) {
            row.state = HunkState::Withdrawn;
            row.decided_at = None;
            // The author's edit of a withdrawn hunk is no longer part of it.
            row.edited = false;
        }
    }
    for id in plan.changed.iter().chain(plan.added.iter()) {
        if let Some(hunk) = plan.doc.find(id) {
            record
                .origin_checksums
                .insert(id.clone(), fs::sha256_bytes(hunk.after_text().as_bytes()));
            if plan.added.contains(id) {
                record.ledger.push(HunkLedgerRow::new(hunk));
            }
        }
    }
    record.legacy = false;
    ledger::resettle(&mut record);

    let restore = || {
        match &prior_hunks {
            Some(text) => {
                let _ = root.write_text_atomic(at(&dir, &proposal_id, HUNKS_EXT), text);
            }
            None => {
                let _ = root.delete_under(&dir, format!("{proposal_id}.{HUNKS_EXT}"), false);
            }
        }
        if let Some(text) = &prior_record {
            let _ = root.write_text_atomic(at(&dir, &proposal_id, RECORD_EXT), text);
        }
    };
    if root.write_text_atomic(at(&dir, &proposal_id, HUNKS_EXT), &plan.doc.to_toml()).is_err()
        || root.write_toml_atomic(at(&dir, &proposal_id, RECORD_EXT), &record).is_err()
    {
        restore();
        return Err(UpdateRefusal::NotRecorded);
    }

    // PCP-FR-RKJI: one agent comment, carrying the counts of this update.
    let attachment = Attachment::PromptProposal {
        proposal_id: proposal_id.clone(),
        artifact_id: record.artifact_id.clone(),
        path: record.path.clone(),
        update: Some(ProposalUpdateCounts {
            changed: plan.changed.len() as u32,
            added: plan.added.len() as u32,
            withdrawn: plan.withdrawn.len() as u32,
        }),
    };
    let now = now_rfc3339();
    let appended = match comments::append_agent_comment_with(
        store,
        target,
        update.thread_id,
        new_note_id(),
        update.rationale.to_string(),
        vec![attachment],
        comments::Stamp { by: update.agent, at: &now },
    ) {
        Ok(thread) => thread,
        Err(error) => {
            restore();
            return Err(if error == comments::ERR_DISCUSSION_RESOLVED {
                UpdateRefusal::ThreadResolved
            } else {
                UpdateRefusal::NotRecorded
            });
        }
    };
    comments::emit_discussion_changed(app, &appended);

    // PCP-FR-28: an update is a milestone, so it is INFO. Counts only.
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "prompt change proposal updated",
        log_fields! {
            "artifactId" => &record.artifact_id,
            "proposalId" => &proposal_id,
            "changed" => plan.changed.len(),
            "added" => plan.added.len(),
            "withdrawn" => plan.withdrawn.len(),
            "state" => record.state.as_str(),
        },
    );
    // PCP-FR-17: once per update, after its comment.
    announce(app, &BUFFER, &record, PromptProposalChangeCause::Updated);
    Ok(record)
}
