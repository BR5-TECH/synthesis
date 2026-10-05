//! Settling a proposal's record and the rows of its ledger
//! (`DCP-draft-change-proposals.md` DCP-FR-01, DCP-FR-PWSF, DCP-FR-XDRV).
//!
//! Every write here is about **where a proposal and its changes stand**, and
//! none of them touches a file of the draft — that is the transaction
//! `DHS-draft-history.md` owns, and the separation is what keeps "the draft has
//! not changed" true until an acceptance actually lands.
//!
//! The proposal's own state is derived from the ledger on every write and never
//! stored independently, so no reader has to recompute it and none can disagree
//! with it.

use super::*;

/// One proposal's record, by draft and id.
pub(crate) fn read_proposal(
    root: &fs::RootFs,
    draft_id: &str,
    proposal_id: &str,
) -> Result<DraftChangeProposal, String> {
    if !is_valid_proposal_id(proposal_id) {
        return Err(ERR_PROPOSAL_NOT_FOUND.to_string());
    }
    let dir = drafts::draft_proposals_dir(root, draft_id)?;
    root.read_toml::<DraftChangeProposal>(record_path(&dir, proposal_id))
        .map_err(|_| ERR_PROPOSAL_NOT_FOUND.to_string())
}

/// DCP-FR-11 / DHS-FR-15: move a record to its decided state, stamping
/// `decided_at`. The transaction's own step, and the only writer that moves a
/// record of this module's to `accepted`.
pub(crate) fn set_decided(
    root: &fs::RootFs,
    draft_id: &str,
    proposal_id: &str,
    state: ProposalState,
) -> Result<DraftChangeProposal, String> {
    let dir = drafts::draft_proposals_dir(root, draft_id)?;
    let mut record = read_proposal(root, draft_id, proposal_id)?;
    record.state = state;
    record.decided_at = Some(now_rfc3339());
    root.write_toml_atomic(record_path(&dir, proposal_id), &record)
        .map_err(|e| e.to_string())?;
    // DRS-FR-WYIN / DRS-FR-JDRY: a proposal is private draft storage. The
    // ignore file keeps it out of Git, and no commit names it.
    drafts::git_storage::ensure_private_ignored(root);
    Ok(record)
}

/// DCP-FR-XDRV: replace one undecided change in place, keeping its id and its
/// position in the order.
///
/// The id is what every reply addressed to that change names, so a revision that
/// minted a new one would orphan the discussion about it.
pub fn revise_hunk_impl(
    root: &fs::RootFs,
    draft_id: &str,
    hunk_id: &str,
    proposed: &hunks::ProposedHunk,
) -> Result<DraftChangeProposal, String> {
    let draft_id = draft_id.to_string();
    // DCP-FR-NKTB: every write against one draft's proposals is serialised, so a
    // revision never races a decision reading the same ledger.
    let _guard = draft_lock(&draft_id);
    // Read inside the lock. The change belongs to whichever proposal of this
    // draft is still standing (a draft holds at most one, DCP-FR-04) — and
    // which that is, and where its changes stand, can both have moved since the
    // caller decided to revise.
    let mut record = pending_for(root, &draft_id).ok_or(ERR_PROPOSAL_NOT_FOUND)?;
    let proposal_id = &record.id.clone();
    let dir = drafts::draft_proposals_dir(root, &draft_id)?;
    // DCP-FR-QLMH: a proposal an older build recorded carries no ledger, and its
    // one change is the row this gives it — the same backfill every other lookup
    // by change id performs before it reads one.
    {
        let prompt = drafts::require_prompt(root, &draft_id)
            .and_then(|path| drafts::load_draft_file_impl(root, &draft_id, &path))
            .map(|contents| contents.body)
            .unwrap_or_default();
        backfill_legacy_ledger(root, &dir, &mut record, &prompt);
    }
    let row = record
        .ledger
        .iter()
        .find(|row| row.id == hunk_id)
        .ok_or(ERR_HUNK_NOT_FOUND)?;
    if !row.state.is_undecided() {
        return Err(ERR_HUNK_ALREADY_DECIDED.to_string());
    }

    let prompt = drafts::require_prompt(root, &draft_id)
        .and_then(|path| drafts::load_draft_file_impl(root, &draft_id, &path))
        .map(|contents| contents.body)
        .unwrap_or_default();
    let (mut doc, _legacy) = read_hunk_document(root, &dir, proposal_id, &prompt)?;
    // DCP-FR-JGCD: the replacement is placed against the prompt exactly as a
    // recording is, so it is refused for the same mistakes **and named by the
    // same errors**. A replacement whose text the prompt holds twice is not a
    // replacement whose text the prompt has lost, and a caller told the one for
    // the other is sent to copy the text again when what it has to do is copy
    // more of the words around it.
    let planned = hunks::plan(&prompt, std::slice::from_ref(proposed), || hunk_id.to_string())
        .map_err(|e| match e {
            hunks::PlanError::Ambiguous(_) => ERR_HUNK_AMBIGUOUS.to_string(),
            // A one-change revision can neither overlap another change nor be
            // empty, so the two remaining errors are one lost anchor.
            _ => ERR_ANCHOR_LOST.to_string(),
        })?;
    let next = planned.hunks.into_iter().next().ok_or(ERR_ANCHOR_LOST)?;
    if !doc.replace_in_place(hunk_id, next) {
        return Err(ERR_HUNK_NOT_FOUND.to_string());
    }
    root.write_text_atomic(hunks_path(&dir, proposal_id), &doc.to_toml())
        .map_err(|_| ERR_WRITE_FAILED.to_string())?;
    // DRS-FR-WYIN / DRS-FR-JDRY: a proposal is private draft storage. The
    // ignore file keeps it out of Git, and no commit names it.
    drafts::git_storage::ensure_private_ignored(root);

    if let Some(row) = record.ledger.iter_mut().find(|row| row.id == hunk_id) {
        // The agent's text is the new origin, so the change reads as unedited
        // again and stays undecided.
        row.edited = false;
        row.revision += 1;
        row.state = hunks::HunkState::Pending;
    }
    record.candidate_edited = record.ledger.iter().any(|row| row.edited);
    resettle(&mut record);
    root.write_toml_atomic(record_path(&dir, proposal_id), &record)
        .map_err(|e| e.to_string())?;
    Ok(record)
}

/// DCP-FR-PWSF: recompute the derived state and counts from the ledger, so no
/// reader has to and none can disagree with it.
pub(crate) fn resettle(record: &mut DraftChangeProposal) {
    record.counts = hunks::HunkCounts::of(record.ledger.iter().map(|row| row.state));
    record.hunk_count = record.ledger.len() as u32;
    record.state = match hunks::derive_state(&record.counts) {
        "accepted" => ProposalState::Accepted,
        "rejected" => ProposalState::Rejected,
        _ => ProposalState::Pending,
    };
    record.decided_at = match record.state {
        ProposalState::Pending => None,
        _ => record.decided_at.clone().or_else(|| Some(now_rfc3339())),
    };
}

/// Move one hunk's ledger row and resettle the record around it.
///
/// The row and everything derived from it are one atomic write (DCP-FR-01): a
/// ledger row is tens of bytes, so splitting it out would make every decision
/// two writes with no crash story between them.
pub(crate) fn set_hunk_state(
    root: &fs::RootFs,
    draft_id: &str,
    proposal_id: &str,
    hunk_id: &str,
    state: hunks::HunkState,
) -> Result<DraftChangeProposal, String> {
    let dir = drafts::draft_proposals_dir(root, draft_id)?;
    let mut record = read_proposal(root, draft_id, proposal_id)?;
    // DCP-FR-QLMH: this is the one write every decision about a change goes
    // through, so it is where a record that carries no ledger — one an older
    // build wrote — is given the row its one change is named by. The write
    // below is what persists it, which is the decision's own write.
    backfill_legacy_ledger(root, &dir, &mut record, &prompt_of(root, draft_id));
    let row = record
        .ledger
        .iter_mut()
        .find(|row| row.id == hunk_id)
        .ok_or(ERR_HUNK_NOT_FOUND)?;
    // DCP-FR-PWSF: a decision is made once. Forcing a settled change back to an
    // undecided state would put the whole proposal back to `pending` — with its
    // acceptance already written into the prompt and already recorded as a
    // version — so it is refused here rather than only at the command that
    // usually calls this. Rolling a decision back is `set_hunk_pending`'s,
    // which is part of a history transaction and says so.
    if !row.state.is_undecided() {
        return Err(ERR_HUNK_ALREADY_DECIDED.to_string());
    }
    row.state = state;
    row.decided_at = state.is_undecided().then(|| None).unwrap_or_else(|| Some(now_rfc3339()));
    resettle(&mut record);
    root.write_toml_atomic(record_path(&dir, proposal_id), &record)
        .map_err(|e| e.to_string())?;
    // DRS-FR-WYIN / DRS-FR-JDRY: a proposal is private draft storage. The
    // ignore file keeps it out of Git, and no commit names it.
    drafts::git_storage::ensure_private_ignored(root);
    Ok(record)
}

/// DHS-FR-19: put one hunk's row back to `pending`, leaving every hunk an
/// earlier operation decided exactly as it was.
///
/// That last clause is the whole difference from a whole-proposal rollback: a
/// careless implementation here destroys decisions that already committed.
pub(crate) fn set_hunk_pending(
    root: &fs::RootFs,
    draft_id: &str,
    proposal_id: &str,
    hunk_id: &str,
) -> Result<(), String> {
    let dir = drafts::draft_proposals_dir(root, draft_id)?;
    let mut record = read_proposal(root, draft_id, proposal_id)?;
    // Deliberately not `set_hunk_state`, which refuses to undecide a settled
    // change. This *is* the one thing allowed to: an acceptance that did not
    // reach its commit point must leave the change the author may decide again,
    // and every other change of the proposal exactly as it stands. A record
    // holding no row for this change — one an older build wrote, whose ledger
    // was only ever in memory — has nothing to roll back and is left alone.
    let Some(row) = record.ledger.iter_mut().find(|row| row.id == hunk_id) else {
        return Ok(());
    };
    row.state = hunks::HunkState::Pending;
    row.decided_at = None;
    resettle(&mut record);
    root.write_toml_atomic(record_path(&dir, proposal_id), &record)
        .map_err(|e| e.to_string())?;
    // DRS-FR-WYIN / DRS-FR-JDRY: a proposal is private draft storage. The
    // ignore file keeps it out of Git, and no commit names it.
    drafts::git_storage::ensure_private_ignored(root);
    Ok(())
}

/// DHS-FR-19: put a record back to `pending` with `decided_at` cleared, so a
/// rolled-back acceptance leaves a proposal the author may decide again.
///
/// Idempotent: a record already `pending` is written back unchanged rather than
/// refused, because a reconciliation can itself be interrupted and re-run.
pub(crate) fn set_pending(
    root: &fs::RootFs,
    draft_id: &str,
    proposal_id: &str,
) -> Result<(), String> {
    let dir = drafts::draft_proposals_dir(root, draft_id)?;
    let mut record = read_proposal(root, draft_id, proposal_id)?;
    record.state = ProposalState::Pending;
    record.decided_at = None;
    root.write_toml_atomic(record_path(&dir, proposal_id), &record)
        .map_err(|e| e.to_string())?;
    // DRS-FR-WYIN / DRS-FR-JDRY: a proposal is private draft storage. The
    // ignore file keeps it out of Git, and no commit names it.
    drafts::git_storage::ensure_private_ignored(root);
    Ok(())
}

