//! Reading a prompt change proposal's hunks, and the author's edits of them
//! (`PCP-prompt-change-proposals.md` PCP-FR-LDHK, PCP-FR-EDTW, PCP-FR-LGXQ).
//!
//! The reads here write nothing, so a surface or a turn may ask as often as it
//! needs to. The one write is a hunk edit, which changes the hunk document and
//! the record and never the artifact.

use super::*;
use crate::text_hunks::anchors::HunkKind;
use crate::text_hunks::hunks::HunkDocument;

/// The hunk document a proposal holds, or the one a legacy proposal is read as
/// (PCP-FR-LGXQ), and whether it is legacy.
///
/// A proposal whose folder holds a `.content` file and no `.hunks` file is
/// served as one `replace` hunk covering the whole file as it stands now.
/// Nothing is written back.
pub(super) fn read_hunk_document(
    root: &fs::RootFs,
    dir: &Path,
    proposal_id: &str,
    current: &str,
) -> Result<(HunkDocument, bool), String> {
    if let Ok(text) = root.read_text(at(dir, proposal_id, HUNKS_EXT)) {
        let doc = HunkDocument::from_toml(&text).map_err(|_| ERR_PROPOSAL_NOT_FOUND.to_string())?;
        return Ok((doc, false));
    }
    let candidate = root
        .read_text(at(dir, proposal_id, CONTENT_EXT))
        .map_err(|_| ERR_PROPOSAL_NOT_FOUND.to_string())?;
    Ok((legacy_document(current, &candidate), true))
}

/// The artifact's text as it stands, or the empty string where it has gone.
///
/// A proposal whose file is gone still serves its hunks: every one of them
/// resolves as lost, which is the state the author clears by rejecting.
pub(super) fn current_text(root: &fs::RootFs, artifact_id: &str) -> String {
    fs::resolve_under(root.path(), artifact_id)
        .ok()
        .and_then(|path| root.read_text(path).ok())
        .unwrap_or_default()
}

/// PCP-FR-LDHK: the hunks of any proposal, decided or not, each placed against
/// the file **as it stands** rather than as the agent read it.
pub fn load_hunks_impl(
    root: &fs::RootFs,
    proposal_id: &str,
) -> Result<PromptProposalHunks, String> {
    let record = read_proposal(root, proposal_id)?;
    let dir = proposals_dir(root)?;
    let current = current_text(root, &record.artifact_id);
    let (doc, legacy) = read_hunk_document(root, &dir, proposal_id, &current)?;
    let resolutions = doc.resolve_all(&current).into_iter().map(HunkPlacement::from).collect();
    Ok(PromptProposalHunks { checksum: doc.checksum(), hunks: doc.hunks, resolutions, legacy })
}

/// PCP-FR-EDTW: the author's own text over one undecided hunk.
///
/// It writes the hunk document and the record, and is not a change to the
/// file: the artifact is not touched and no event is emitted. A legacy
/// proposal's hunk is not editable (PCP-FR-LGXQ).
pub fn edit_hunk_impl(
    root: &fs::RootFs,
    proposal_id: &str,
    hunk_id: &str,
    after: &str,
    baseline_checksum: &str,
) -> Result<PromptCandidateSaved, String> {
    // The artifact is named before the lock, because the lock is per artifact.
    // Everything the write is checked against is read again inside it.
    let held = read_proposal(root, proposal_id)?;
    let _guard = artifact_lock(&held.artifact_id);
    let mut record = read_proposal(root, proposal_id)?;
    let dir = proposals_dir(root)?;
    let current = current_text(root, &record.artifact_id);
    let (mut doc, legacy) = read_hunk_document(root, &dir, proposal_id, &current)?;
    // PCP-FR-LGXQ: checked first, so it is refused as what it is.
    if legacy {
        return Err(ERR_WRITE_FAILED.to_string());
    }
    let row = record.ledger.iter().find(|row| row.id == hunk_id).ok_or(ERR_HUNK_NOT_FOUND)?;
    if !row.state.is_undecided() {
        return Err(ERR_HUNK_ALREADY_DECIDED.to_string());
    }
    // PCP-FR-EDTW: checked before anything is written, so a stale edit leaves
    // the hunk byte-for-byte as it was.
    if doc.checksum() != baseline_checksum {
        return Err(ERR_CANDIDATE_STALE.to_string());
    }
    let slot = doc.hunks.iter_mut().find(|h| h.id == hunk_id).ok_or(ERR_HUNK_NOT_FOUND)?;
    slot.after = (!after.is_empty()).then(|| after.to_string());
    let checksum = doc.checksum();
    root.write_text_atomic(at(&dir, proposal_id, HUNKS_EXT), &doc.to_toml())
        .map_err(|_| ERR_WRITE_FAILED.to_string())?;

    // PCP-FR-EDTW: the flag describes the text, so an edit that restores the
    // agent's own words clears it again.
    let edited = record
        .origin_checksums
        .get(hunk_id)
        .map(|origin| *origin != fs::sha256_bytes(after.as_bytes()))
        .unwrap_or(true);
    if let Some(row) = record.ledger.iter_mut().find(|row| row.id == hunk_id) {
        row.edited = edited;
    }
    ledger::resettle(&mut record);
    // A record that will not write leaves the hunk written and the flag stale.
    // That is the harmless way round: the flag drives a line of prose, while
    // the hunk is the author's work.
    let _ = root.write_toml_atomic(at(&dir, proposal_id, RECORD_EXT), &record);
    Ok(PromptCandidateSaved { checksum })
}

// ---------------------------------------------------------------------------
// The read a turn assembles its input from (AGC-FR-DKHT)
// ---------------------------------------------------------------------------

/// One hunk of a prompt change proposal, as a turn assembling its input reads
/// it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptProposalChange {
    /// AGC-FR-DKHT: the hunk's id, which is the value an agent passes as
    /// `revises` (PPC-FR-RVKS).
    pub id: String,
    pub kind: HunkKind,
    pub state: HunkState,
    /// The exact text the hunk replaces or deletes. Empty for an insertion.
    pub before: String,
    /// The exact text the hunk offers. Empty for a deletion.
    pub after: String,
}

/// AGC-FR-DKHT: one hunk of one proposal, for a turn's input.
///
/// It runs no reconciliation and writes nothing: a turn assembling its input
/// must read a file without changing one (CVL-FR-09). Any failure is `None`,
/// which the caller renders as the bare reference.
pub fn read_proposal_change(
    root: &fs::RootFs,
    proposal_id: &str,
    hunk_id: &str,
) -> Option<PromptProposalChange> {
    let record = read_proposal(root, proposal_id).ok()?;
    let dir = proposals_dir(root).ok()?;
    let current = current_text(root, &record.artifact_id);
    let (doc, _legacy) = read_hunk_document(root, &dir, proposal_id, &current).ok()?;
    let hunk = doc.find(hunk_id)?;
    let state = record
        .ledger
        .iter()
        .find(|row| row.id == hunk_id)
        .map(|row| row.state)
        .unwrap_or(HunkState::Pending);
    Some(PromptProposalChange {
        id: hunk.id.clone(),
        kind: hunk.kind,
        state,
        before: hunk.before_text().to_string(),
        after: hunk.after_text().to_string(),
    })
}

/// The text every hunk of the proposal would produce against the file as it
/// stands, for a test that checks what a recording holds.
#[cfg(test)]
pub fn candidate_for_test(root: &fs::RootFs, proposal_id: &str) -> Result<String, String> {
    let record = read_proposal(root, proposal_id)?;
    let dir = proposals_dir(root)?;
    let current = current_text(root, &record.artifact_id);
    let (doc, legacy) = read_hunk_document(root, &dir, proposal_id, &current)?;
    if legacy {
        return root
            .read_text(at(&dir, proposal_id, CONTENT_EXT))
            .map_err(|_| ERR_PROPOSAL_NOT_FOUND.to_string());
    }
    let built = crate::text_hunks::hunks::apply_all(&current, &doc).ok_or(ERR_ANCHOR_LOST)?;
    Ok(normalise_for_write(root, &built))
}
