//! Settling a proposal's record and the rows of its ledger
//! (`DCP-draft-change-proposals.md` DCP-FR-01, DCP-FR-PWSF).
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
    root.write_toml_atomic(record_path(&dir, proposal_id), &record).map_err(|e| e.to_string())?;
    // DRS-FR-WYIN / DRS-FR-JDRY: a proposal is private draft storage. The
    // ignore file keeps it out of Git, and no commit names it.
    drafts::git_storage::ensure_private_ignored(root);
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
    let row = record.ledger.iter_mut().find(|row| row.id == hunk_id).ok_or(ERR_HUNK_NOT_FOUND)?;
    // DCP-FR-PWSF: a decision is made once. Forcing a settled change back to an
    // undecided state would put the whole proposal back to `pending` — with its
    // acceptance already written into the prompt and already recorded as a
    // version — so it is refused here rather than only at the command that
    // usually calls this. The one move out of a settled state is the
    // acceptance of a rejected change (DCP-FR-11). Rolling a decision back is
    // `restore_hunk_state`'s, which is part of a history transaction and says so.
    let open = row.state.is_undecided()
        || (row.state == hunks::HunkState::Rejected && state == hunks::HunkState::Accepted);
    if !open {
        return Err(ERR_HUNK_ALREADY_DECIDED.to_string());
    }
    row.state = state;
    row.decided_at = if state.is_undecided() { None } else { Some(now_rfc3339()) };
    resettle(&mut record);
    root.write_toml_atomic(record_path(&dir, proposal_id), &record).map_err(|e| e.to_string())?;
    // DRS-FR-WYIN / DRS-FR-JDRY: a proposal is private draft storage. The
    // ignore file keeps it out of Git, and no commit names it.
    drafts::git_storage::ensure_private_ignored(root);
    Ok(record)
}

/// DHS-FR-19 / DCP-FR-13: put one hunk's row back to the state it held before
/// an acceptance that did not commit, leaving every hunk an earlier operation
/// decided exactly as it was.
///
/// That last clause is the whole difference from a whole-proposal rollback: a
/// careless implementation here destroys decisions that already committed. The
/// prior state is the journal's (`pending` for a journal an older build wrote),
/// so a rolled-back acceptance of a rejected change leaves it rejected.
pub(crate) fn restore_hunk_state(
    root: &fs::RootFs,
    draft_id: &str,
    proposal_id: &str,
    hunk_id: &str,
    prior: hunks::HunkState,
    prior_decided_at: Option<String>,
) -> Result<(), String> {
    let dir = drafts::draft_proposals_dir(root, draft_id)?;
    let mut record = read_proposal(root, draft_id, proposal_id)?;
    // Deliberately not `set_hunk_state`, which refuses to undecide a settled
    // change. This *is* the one thing allowed to: an acceptance that did not
    // reach its commit point must leave the change as it stood before, and
    // every other change of the proposal exactly as it stands. A record
    // holding no row for this change — one an older build wrote, whose ledger
    // was only ever in memory — has nothing to roll back and is left alone.
    let Some(row) = record.ledger.iter_mut().find(|row| row.id == hunk_id) else {
        return Ok(());
    };
    row.state = prior;
    row.decided_at = if prior.is_undecided() { None } else { prior_decided_at };
    resettle(&mut record);
    root.write_toml_atomic(record_path(&dir, proposal_id), &record).map_err(|e| e.to_string())?;
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
    root.write_toml_atomic(record_path(&dir, proposal_id), &record).map_err(|e| e.to_string())?;
    // DRS-FR-WYIN / DRS-FR-JDRY: a proposal is private draft storage. The
    // ignore file keeps it out of Git, and no commit names it.
    drafts::git_storage::ensure_private_ignored(root);
    Ok(())
}
