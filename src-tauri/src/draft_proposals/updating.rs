//! Updating the open proposal of a draft (`DCP-draft-change-proposals.md`
//! DCP-FR-XDRV, DCP-FR-TQBO, DCP-FR-IAFV, DCP-FR-BFDR, DCP-FR-FKMH,
//! DCP-FR-PHUB, DCP-FR-VTYX).
//!
//! An update changes hunks of the open proposal in place, adds hunks, and
//! withdraws hunks, in one call that appends one agent comment. It is all or
//! nothing: a refused entry writes nothing, and a comment that could not be
//! appended puts both files back as they were.

use super::*;
use crate::comments::ProposalUpdateCounts;
use crate::text_hunks::updates::{self, Standing, UpdateEntry, UpdateError};

/// What a caller hands `update_proposal`.
pub struct ProposalUpdate<'a> {
    pub draft_id: &'a str,
    pub updates: &'a [UpdateEntry],
    pub rationale: &'a str,
    pub agent: &'a Participant,
    /// The conversation of the turn, which the comment is appended into.
    pub target: ThreadRef<'a>,
    pub thread_id: &'a str,
}

/// Why `update_proposal` wrote nothing (DCP-FR-XDRV, DCP-FR-BFDR,
/// DCP-FR-FKMH). Every refusal about one entry carries its one-based place
/// and the length of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateRefusal {
    NoOpenProposal,
    NoUpdates,
    PathMissing,
    HunkNotFound { at: usize, of: usize },
    HunkNotUpdatable { at: usize, of: usize },
    HunkNamedTwice { at: usize, of: usize },
    NoChange { at: usize, of: usize },
    HunkAnchorLost { at: usize, of: usize },
    HunkAmbiguous { at: usize, of: usize },
    HunkOverlap { at: usize, of: usize },
    HunkContextTooLong { at: usize, of: usize, side: hunks::ContextSide },
    ThreadResolved,
    NotRecorded,
}

impl UpdateRefusal {
    /// The refusal for a plan error, with the length of the list.
    fn of_plan(error: UpdateError, of: usize) -> Self {
        match error {
            UpdateError::NoUpdates => UpdateRefusal::NoUpdates,
            UpdateError::UnknownHunk(at) => UpdateRefusal::HunkNotFound { at, of },
            UpdateError::NotUpdatable(at) => UpdateRefusal::HunkNotUpdatable { at, of },
            UpdateError::NamedTwice(at) => UpdateRefusal::HunkNamedTwice { at, of },
            UpdateError::NoChange(at) => UpdateRefusal::NoChange { at, of },
            UpdateError::Plan(hunks::PlanError::AnchorLost(at)) => {
                UpdateRefusal::HunkAnchorLost { at, of }
            }
            UpdateError::Plan(hunks::PlanError::Ambiguous(at)) => {
                UpdateRefusal::HunkAmbiguous { at, of }
            }
            UpdateError::Plan(hunks::PlanError::Overlap(at)) => {
                UpdateRefusal::HunkOverlap { at, of }
            }
            UpdateError::Plan(hunks::PlanError::ContextTooLong(at, side)) => {
                UpdateRefusal::HunkContextTooLong { at, of, side }
            }
            UpdateError::Plan(hunks::PlanError::NoHunks) => UpdateRefusal::NoUpdates,
        }
    }

    /// A stable code for the log record (DCP-FR-24).
    pub fn code(self) -> &'static str {
        match self {
            UpdateRefusal::NoOpenProposal => "no_open_proposal",
            UpdateRefusal::NoUpdates => "no_updates",
            UpdateRefusal::PathMissing => ERR_PATH_MISSING,
            UpdateRefusal::HunkNotFound { .. } => ERR_HUNK_NOT_FOUND,
            UpdateRefusal::HunkNotUpdatable { .. } => "hunk_not_updatable",
            UpdateRefusal::HunkNamedTwice { .. } => "hunk_named_twice",
            UpdateRefusal::NoChange { .. } => "no_change",
            UpdateRefusal::HunkAnchorLost { .. } => "hunk_anchor_lost",
            UpdateRefusal::HunkAmbiguous { .. } => ERR_HUNK_AMBIGUOUS,
            UpdateRefusal::HunkOverlap { .. } => "hunk_overlap",
            UpdateRefusal::HunkContextTooLong { .. } => ERR_HUNK_CONTEXT_TOO_LONG,
            UpdateRefusal::ThreadResolved => comments::ERR_DISCUSSION_RESOLVED,
            UpdateRefusal::NotRecorded => ERR_WRITE_FAILED,
        }
    }
}

/// DCP-FR-XDRV: apply an ordered list of updates to the draft's **open**
/// proposal, and append one comment, as one act.
///
/// DCP-FR-VTYX: the files reach disk before the comment is appended, and a
/// failure to append puts both back to the bytes they held. The whole update
/// runs under the draft's lock, so no decision reads a half-written ledger.
pub fn update_proposal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
    update: ProposalUpdate<'_>,
) -> Result<DraftChangeProposal, UpdateRefusal>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let _guard = draft_lock(update.draft_id);
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
    update: &ProposalUpdate<'_>,
) -> Result<DraftChangeProposal, UpdateRefusal>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let draft_id = update.draft_id;
    drafts::read_draft_record(root, draft_id).map_err(|_| UpdateRefusal::NotRecorded)?;
    // DHS-FR-18: an interrupted acceptance is settled before the update plans
    // against the prompt and the ledger.
    crate::draft_history::reconcile_locked(app, root, store, draft_id)
        .map_err(|_| UpdateRefusal::NotRecorded)?;
    // DCP-FR-XDRV: only the open proposal is updated, and a closed one never.
    let mut record = pending_for(root, draft_id).ok_or(UpdateRefusal::NoOpenProposal)?;
    if update.updates.is_empty() {
        return Err(UpdateRefusal::NoUpdates);
    }
    let prompt_path =
        drafts::require_prompt(root, draft_id).map_err(|_| UpdateRefusal::PathMissing)?;
    if record.path != prompt_path {
        return Err(UpdateRefusal::PathMissing);
    }
    let prompt = drafts::load_draft_file_impl(root, draft_id, &prompt_path)
        .map(|contents| contents.body)
        .map_err(|_| UpdateRefusal::PathMissing)?;
    let dir =
        drafts::draft_proposals_dir(root, draft_id).map_err(|_| UpdateRefusal::NotRecorded)?;
    let proposal_id = record.id.clone();

    // DCP-FR-FKMH: the bytes the update would have to put back.
    let prior_hunks = root.read_text(hunks_path(&dir, &proposal_id)).ok();
    let prior_record = root.read_text(record_path(&dir, &proposal_id)).ok();

    // DCP-FR-QLMH: a legacy proposal is given the row its one change is named by.
    backfill_legacy_ledger(root, &dir, &mut record, &prompt);
    let (doc, _legacy) = read_hunk_document(root, &dir, &proposal_id, &prompt)
        .map_err(|_| UpdateRefusal::NotRecorded)?;

    // DCP-FR-BFDR: where each hunk stands, from the ledger.
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
    let plan = updates::plan_update(&prompt, &doc, standing, update.updates, new_note_id)
        .map_err(|error| UpdateRefusal::of_plan(error, of))?;

    // DCP-FR-TQBO / DCP-FR-IAFV: the ledger follows the plan.
    for row in record.ledger.iter_mut() {
        if plan.changed.contains(&row.id) {
            row.edited = false;
            row.revision += 1;
            row.state = hunks::HunkState::Pending;
            row.decided_at = None;
        } else if plan.withdrawn.contains(&row.id) {
            row.state = hunks::HunkState::Withdrawn;
            row.decided_at = None;
            // The author's edit of a withdrawn hunk is no longer part of it.
            row.edited = false;
        }
    }
    for id in &plan.added {
        if let Some(hunk) = plan.doc.find(id) {
            record.ledger.push(hunks::HunkLedgerRow::new(hunk));
        }
    }
    record.candidate_edited = record.ledger.iter().any(|row| row.edited);
    record.legacy = false;
    resettle(&mut record);

    let restore = || {
        match &prior_hunks {
            Some(text) => {
                let _ = root.write_text_atomic(hunks_path(&dir, &proposal_id), text);
            }
            None => {
                let _ = root.delete_under(&dir, format!("{proposal_id}.{HUNKS_EXT}"), false);
            }
        }
        if let Some(text) = &prior_record {
            let _ = root.write_text_atomic(record_path(&dir, &proposal_id), text);
        }
    };
    if root.write_text_atomic(hunks_path(&dir, &proposal_id), &plan.doc.to_toml()).is_err() {
        restore();
        return Err(UpdateRefusal::NotRecorded);
    }
    if root.write_toml_atomic(record_path(&dir, &proposal_id), &record).is_err() {
        restore();
        return Err(UpdateRefusal::NotRecorded);
    }
    // DRS-FR-WYIN / DRS-FR-JDRY: a proposal is private draft storage.
    drafts::git_storage::ensure_private_ignored(root);

    // DCP-FR-PHUB: one agent comment, carrying the counts of this update.
    let attachment = Attachment::Proposal {
        proposal_id: proposal_id.clone(),
        draft_id: draft_id.to_string(),
        path: record.path.clone(),
        update: Some(ProposalUpdateCounts {
            changed: plan.changed.len() as u32,
            added: plan.added.len() as u32,
            withdrawn: plan.withdrawn.len() as u32,
        }),
    };
    let now = now_rfc3339();
    let appended = comments::append_agent_comment_with(
        store,
        update.target,
        update.thread_id,
        new_note_id(),
        update.rationale.to_string(),
        vec![attachment],
        comments::Stamp { by: update.agent, at: &now },
    );
    let appended = match appended {
        Ok(thread) => thread,
        Err(error) => {
            // DCP-FR-VTYX: the proposal is byte-for-byte what it was.
            restore();
            return Err(if error == comments::ERR_DISCUSSION_RESOLVED {
                UpdateRefusal::ThreadResolved
            } else {
                UpdateRefusal::NotRecorded
            });
        }
    };
    comments::emit_discussion_changed(app, &appended);

    // DCP-FR-24: an update is a milestone, so it is INFO. Counts only.
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "draft change proposal updated",
        log_fields! {
            "draftId" => draft_id,
            "proposalId" => &proposal_id,
            "changed" => plan.changed.len(),
            "added" => plan.added.len(),
            "withdrawn" => plan.withdrawn.len(),
            "state" => record.state.as_str(),
        },
    );
    // DCP-FR-16: once per update, after its comment.
    announce(app, &BUFFER, &record, ProposalChangeCause::Updated);
    Ok(record)
}
