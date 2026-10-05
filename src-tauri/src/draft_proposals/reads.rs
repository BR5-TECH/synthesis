//! Reading a draft's proposals, and reading what one holds
//! (`DCP-draft-change-proposals.md` DCP-FR-09, DCP-FR-10).
//!
//! Nothing here writes anything, which is what lets a surface ask as often as
//! it needs to. The split that matters is between the **records** — identity,
//! state, and the per-change ledger, which a listing reads — and the
//! **documents** the changes themselves live in, which only a review reads. A
//! draft carrying a long history of proposals therefore costs its tab a list
//! and no document at all.

use super::*;

/// DCP-FR-09: every proposal the draft holds, most recently created first.
///
/// Reads records and never a `.content` file, so a draft carrying a long history
/// of proposals costs a list no more than a draft carrying one.
pub fn list_proposals_impl(root: &fs::RootFs, draft_id: &str) -> Result<Vec<DraftChangeProposal>, String> {
    // A draft that is not there is told apart from one holding no proposals: the
    // first is a caller error and the second is an ordinary empty list.
    drafts::read_draft_record(root, draft_id).map_err(|_| ERR_DRAFT_NOT_FOUND.to_string())?;
    let dir = drafts::draft_proposals_dir(root, draft_id)?;
    let Ok(entries) = root.list_dir(&dir) else {
        // The folder is scaffolded with the draft (DRS-FR-01), but a draft
        // created by an older build has none — which is no proposals rather than
        // an error.
        return Ok(Vec::new());
    };
    let mut out: Vec<DraftChangeProposal> = Vec::new();
    for entry in entries {
        let Some(id) = entry.name.strip_suffix(&format!(".{RECORD_EXT}")) else {
            continue;
        };
        if !is_valid_proposal_id(id) {
            continue;
        }
        // A record that will not read is skipped rather than fatal, on the same
        // principle a damaged comment log line is skipped: one unreadable
        // proposal never costs the author the rest of them.
        if let Ok(record) = root.read_toml::<DraftChangeProposal>(&record_path(&dir, id)) {
            out.push(record);
        }
    }
    // Tie-broken by id so the order is total and a reload never reshuffles two
    // proposals recorded in the same millisecond.
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.id.cmp(&b.id)));
    Ok(out)
}

/// One proposal by id, searched for across the drafts of the worktree.
///
/// A proposal id alone is what a surface holds after reading a comment's
/// attachment, so the lookup cannot require the draft as well.
pub fn find_proposal(root: &fs::RootFs, proposal_id: &str) -> Option<(String, DraftChangeProposal)> {
    if !is_valid_proposal_id(proposal_id) {
        return None;
    }
    for summary in drafts::list_drafts_impl(root).drafts {
        let Ok(dir) = drafts::draft_proposals_dir(root, &summary.id) else {
            continue;
        };
        if let Ok(record) = root.read_toml::<DraftChangeProposal>(&record_path(&dir, proposal_id)) {
            return Some((summary.id, record));
        }
    }
    None
}

/// DCP-FR-10: the candidate of any proposal, decided or not, with the checksum
/// of the bytes served.
pub fn load_content_impl(root: &fs::RootFs, proposal_id: &str) -> Result<ProposalContent, String> {
    let (draft_id, _) = find_proposal(root, proposal_id).ok_or(ERR_PROPOSAL_NOT_FOUND)?;
    let dir = drafts::draft_proposals_dir(root, &draft_id)?;
    let prompt = drafts::require_prompt(root, &draft_id)
        .and_then(|path| drafts::load_draft_file_impl(root, &draft_id, &path))
        .map(|contents| contents.body)
        .unwrap_or_default();
    let (doc, _legacy) = read_hunk_document(root, &dir, proposal_id, &prompt)?;
    // The text every change of this proposal would produce, composed rather than
    // stored: a proposal is a set of changes, and the whole document is what
    // taking all of them arrives at.
    let content = hunks::apply_all(&prompt, &doc).ok_or(ERR_PROPOSAL_NOT_FOUND)?;
    let checksum = crate::fs::sha256_bytes(content.as_bytes());
    Ok(ProposalContent { checksum, content })
}

/// The hunk document a proposal holds, or the one a legacy proposal is read as
/// (DCP-FR-QLMH).
///
/// A proposal whose folder holds a whole-document candidate and no `.hunks` file
/// is served as one `replace` hunk covering the whole prompt — which is exactly
/// what a whole-document candidate always meant, so every operation above works
/// on it unchanged. Nothing is written back, so a repository checked out by two
/// builds does not corrupt.
pub fn read_hunk_document(
    root: &fs::RootFs,
    dir: &Path,
    proposal_id: &str,
    prompt: &str,
) -> Result<(hunks::HunkDocument, bool), String> {
    if let Ok(text) = root.read_text(hunks_path(dir, proposal_id)) {
        let doc = hunks::HunkDocument::from_toml(&text)
            .map_err(|_| ERR_PROPOSAL_NOT_FOUND.to_string())?;
        return Ok((doc, false));
    }
    let candidate = root
        .read_text(content_path(dir, proposal_id))
        .map_err(|_| ERR_PROPOSAL_NOT_FOUND.to_string())?;
    Ok((hunks::legacy_document(prompt, &candidate, proposal_id), true))
}

/// The draft's prompt, or the empty string where it has gone.
///
/// A draft whose prompt is missing still serves its changes: every one of them
/// resolves as lost, which is the state the author clears by rejecting
/// (DCP-FR-BMLX, DCP-FR-18).
pub(crate) fn prompt_of(root: &fs::RootFs, draft_id: &str) -> String {
    drafts::require_prompt(root, draft_id)
        .and_then(|path| drafts::load_draft_file_impl(root, draft_id, &path))
        .map(|contents| contents.body)
        .unwrap_or_default()
}

/// DCP-FR-QLMH: give a legacy record the one ledger row its one change needs.
///
/// A record written before proposals held changes carries no ledger at all, so
/// every lookup by change id would refuse a proposal that is perfectly
/// decidable — leaving the draft's one pending slot held with no way to release
/// it but declining the whole thing. It is filled in **in memory**, from the
/// change the document synthesises; what persists it is the decision's own
/// write, which was going to write the record anyway.
///
/// Nothing rewrites the candidate itself. A repository checked out by both
/// builds still reads: an older build takes `state` and `decided_at` as it
/// always did and ignores a ledger it does not know about.
pub(crate) fn backfill_legacy_ledger(
    root: &fs::RootFs,
    dir: &Path,
    record: &mut DraftChangeProposal,
    prompt: &str,
) {
    if !record.ledger.is_empty() {
        return;
    }
    let Ok((doc, _legacy)) = read_hunk_document(root, dir, &record.id, prompt) else {
        return;
    };
    record.ledger = doc.hunks.iter().map(hunks::HunkLedgerRow::new).collect();
    record.hunk_count = record.ledger.len() as u32;
    // The record already says where the proposal stands as a whole, and that is
    // what the rows are settled from: a proposal an older build accepted has its
    // one change accepted, and one it rejected has it rejected.
    let settled = match record.state {
        ProposalState::Pending => hunks::HunkState::Pending,
        ProposalState::Accepted => hunks::HunkState::Accepted,
        ProposalState::Rejected => hunks::HunkState::Rejected,
    };
    for row in record.ledger.iter_mut() {
        row.state = settled;
        row.decided_at = record.decided_at.clone();
    }
    resettle(record);
}

/// DCP-FR-10: the hunks of any proposal, decided or not, each placed against the
/// prompt **as it stands** rather than as the agent read it (DCP-FR-06).
pub fn load_hunks_impl(root: &fs::RootFs, proposal_id: &str) -> Result<ProposalHunks, String> {
    let (draft_id, _record) = find_proposal(root, proposal_id).ok_or(ERR_PROPOSAL_NOT_FOUND)?;
    let dir = drafts::draft_proposals_dir(root, &draft_id)?;
    // The prompt is the text every anchor resolves against. A draft whose prompt
    // has gone still serves its hunks — every one of them resolves as lost, which
    // is the state the author clears by rejecting (DCP-FR-BMLX, DCP-FR-18).
    let prompt = drafts::require_prompt(root, &draft_id)
        .and_then(|path| drafts::load_draft_file_impl(root, &draft_id, &path))
        .map(|contents| contents.body)
        .unwrap_or_default();

    let (doc, legacy) = read_hunk_document(root, &dir, proposal_id, &prompt)?;
    let resolutions = doc
        .resolve_all(&prompt)
        .into_iter()
        .map(HunkPlacement::from)
        .collect();
    Ok(ProposalHunks {
        checksum: doc.checksum(),
        hunks: doc.hunks,
        resolutions,
        legacy,
    })
}


// ---------------------------------------------------------------------------
// The read a turn assembles its input from (DCP-FR-HNWD)
// ---------------------------------------------------------------------------

/// One change of a proposal, as a turn assembling its input reads it.
///
/// It carries the text the change names rather than where that text now sits: a
/// turn wants to know which passages a conversation has already settled, and a
/// placement against the prompt would cost a resolution this read is forbidden
/// to take (DCP-FR-HNWD).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalChange {
    pub kind: anchors::HunkKind,
    pub state: hunks::HunkState,
    /// The exact text the change replaces or deletes. Empty for an insertion.
    pub before: String,
    /// The exact text the change offered. Empty for a deletion.
    pub after: String,
}

/// One proposal of a draft, with its changes and how each was decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalChanges {
    pub id: String,
    pub path: String,
    pub counts: hunks::HunkCounts,
    pub changes: Vec<ProposalChange>,
}

/// DCP-FR-HNWD: every proposal of the draft, most recently created first, each
/// with its changes in proposal order.
///
/// It runs no reconciliation and writes nothing, which is the whole reason it
/// exists beside the listing of DCP-FR-28: a turn assembling its input must read
/// a draft without changing one (`../ai/CVL-conversation-loop.md` CVL-FR-09),
/// and a reconciliation completes an interrupted acceptance, which writes the
/// prompt the turn is reading.
pub fn read_proposal_changes(
    root: &fs::RootFs,
    draft_id: &str,
) -> Result<Vec<ProposalChanges>, String> {
    let records = list_proposals_impl(root, draft_id)?;
    let dir = drafts::draft_proposals_dir(root, draft_id)?;
    // Read once for the whole draft rather than once per proposal: only a legacy
    // record needs it, and every one of them needs the same bytes.
    let prompt = prompt_of(root, draft_id);

    let mut out = Vec::with_capacity(records.len());
    for mut record in records {
        // DCP-FR-QLMH: a record written before proposals held changes carries no
        // ledger and no counts, so its one change would read as undecided however
        // the author decided it. Settling it from the record's own state is what
        // every other reader of a legacy proposal does, and it is load-bearing
        // here: an agent told a settled passage is still open proposes into it
        // again (AGC-FR-RVQP).
        backfill_legacy_ledger(root, &dir, &mut record, &prompt);
        // A document that will not read is skipped rather than fatal, on the
        // principle list_proposals_impl already applies to a damaged record: one
        // unreadable proposal never costs the turn the rest of them.
        let Ok((doc, _legacy)) = read_hunk_document(root, &dir, &record.id, &prompt) else {
            continue;
        };
        let changes = doc
            .hunks
            .iter()
            .map(|hunk| ProposalChange {
                kind: hunk.kind,
                // A change with no ledger row is undecided, which is what a row
                // absent from an older record already means (DCP-FR-QLMH).
                state: record
                    .ledger
                    .iter()
                    .find(|row| row.id == hunk.id)
                    .map(|row| row.state)
                    .unwrap_or(hunks::HunkState::Pending),
                before: hunk.before_text().to_string(),
                after: hunk.after_text().to_string(),
            })
            .collect();
        out.push(ProposalChanges {
            id: record.id,
            path: record.path,
            counts: record.counts,
            changes,
        });
    }
    Ok(out)
}
