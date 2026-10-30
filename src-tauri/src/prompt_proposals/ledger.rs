//! The per-hunk decision ledger of a prompt change proposal
//! (`PCP-prompt-change-proposals.md` PCP-FR-HKLD, PCP-FR-RVSN, PCP-FR-LGXQ).
//!
//! The shapes copy the draft ledger's (DCP-FR-PWSF) so a surface reads both
//! alike, and none of them is shared with `crate::draft_proposals`
//! (PCP-FR-29): the two stores hold their own state and their own operations,
//! and only the pure text of `crate::text_hunks` is common.
//!
//! The proposal's own state is derived from the ledger on every write and never
//! stored independently, so no reader has to recompute it and none can
//! disagree with it.

use super::*;
use crate::text_hunks::anchors::HunkKind;
use crate::text_hunks::hunks::{self as text, ProposalHunk, ProposedHunk};

/// PCP-FR-HKLD: where one hunk stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HunkState {
    Pending,
    Accepted,
    Rejected,
    /// PCP-FR-HLDQ: held for discussion. Undecided, so it still holds the
    /// artifact's one slot.
    Discussing,
}

impl HunkState {
    /// A hunk the author has not settled either way.
    pub fn is_undecided(self) -> bool {
        matches!(self, HunkState::Pending | HunkState::Discussing)
    }

    /// PCP-FR-RVSN: a hunk its agent may still replace — an undecided one, or
    /// one the author rejected while the proposal stands.
    pub fn is_revisable(self) -> bool {
        self.is_undecided() || self == HunkState::Rejected
    }

    pub fn as_str(self) -> &'static str {
        match self {
            HunkState::Pending => "pending",
            HunkState::Accepted => "accepted",
            HunkState::Rejected => "rejected",
            HunkState::Discussing => "discussing",
        }
    }
}

/// PCP-FR-HKLD: one row of the decision ledger the record carries.
///
/// It holds the hunk's identity and what the author has done with it, and none
/// of its text — the text is a document and lives in the `.hunks` file, so a
/// list never reads one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HunkLedgerRow {
    pub id: String,
    pub kind: HunkKind,
    #[serde(default = "pending")]
    pub state: HunkState,
    /// PCP-FR-EDTW: true while this hunk's text differs from the agent's own.
    #[serde(default)]
    pub edited: bool,
    /// PCP-FR-RVSN: bumped by an agent revision, never by an author edit.
    #[serde(default)]
    pub revision: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
}

fn pending() -> HunkState {
    HunkState::Pending
}

impl HunkLedgerRow {
    pub fn new(hunk: &ProposalHunk) -> Self {
        Self {
            id: hunk.id.clone(),
            kind: hunk.kind,
            state: HunkState::Pending,
            edited: false,
            revision: hunk.revision,
            decided_at: None,
        }
    }
}

/// PCP-FR-HKLD: how many hunks stand in each state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HunkCounts {
    #[serde(default)]
    pub pending: u32,
    #[serde(default)]
    pub accepted: u32,
    #[serde(default)]
    pub rejected: u32,
    #[serde(default)]
    pub discussing: u32,
}

impl HunkCounts {
    pub fn of(states: impl IntoIterator<Item = HunkState>) -> Self {
        let mut c = Self::default();
        for s in states {
            match s {
                HunkState::Pending => c.pending += 1,
                HunkState::Accepted => c.accepted += 1,
                HunkState::Rejected => c.rejected += 1,
                HunkState::Discussing => c.discussing += 1,
            }
        }
        c
    }

    pub fn undecided(&self) -> u32 {
        self.pending + self.discussing
    }
}

/// PCP-FR-HKLD (as DCP-FR-PWSF): a proposal is `pending` while any hunk is
/// undecided, `accepted` once all are decided and at least one was accepted,
/// and `rejected` otherwise.
pub fn derive_state(counts: &HunkCounts) -> PromptProposalState {
    if counts.undecided() > 0 {
        PromptProposalState::Pending
    } else if counts.accepted > 0 {
        PromptProposalState::Accepted
    } else {
        PromptProposalState::Rejected
    }
}

/// PCP-FR-HKLD: recompute the derived state and counts from the ledger.
pub(super) fn resettle(record: &mut PromptChangeProposal) {
    record.counts = HunkCounts::of(record.ledger.iter().map(|row| row.state));
    record.hunk_count = record.ledger.len() as u32;
    record.candidate_edited = record.ledger.iter().any(|row| row.edited);
    record.state = derive_state(&record.counts);
    record.decided_at = match record.state {
        PromptProposalState::Pending => None,
        _ => record.decided_at.clone().or_else(|| Some(now_rfc3339())),
    };
}

/// PCP-FR-LGXQ: give a legacy record the one ledger row its one hunk needs.
///
/// A record an older build wrote carries no ledger. Its one hunk is the fixed
/// id [`LEGACY_HUNK_ID`], settled from the record's own state. It is filled in
/// **in memory**; what persists it is a decision's own write. The candidate
/// itself is never rewritten.
pub(super) fn backfill_legacy_ledger(record: &mut PromptChangeProposal) {
    if !record.ledger.is_empty() {
        return;
    }
    let state = match record.state {
        PromptProposalState::Pending => HunkState::Pending,
        PromptProposalState::Accepted => HunkState::Accepted,
        PromptProposalState::Rejected => HunkState::Rejected,
    };
    record.legacy = true;
    record.ledger = vec![HunkLedgerRow {
        id: LEGACY_HUNK_ID.to_string(),
        kind: HunkKind::Replace,
        state,
        edited: record.candidate_edited,
        revision: 0,
        decided_at: if state.is_undecided() { None } else { record.decided_at.clone() },
    }];
    record.counts = HunkCounts::of(record.ledger.iter().map(|row| row.state));
    record.hunk_count = 1;
}

/// PCP-FR-LGXQ: a whole candidate read as one `replace` hunk covering the whole
/// file as it stands now.
pub(super) fn legacy_document(current: &str, candidate: &str) -> text::HunkDocument {
    let current = anchors::normalise_newlines(current);
    text::HunkDocument::new(vec![ProposalHunk {
        id: LEGACY_HUNK_ID.to_string(),
        kind: HunkKind::Replace,
        revision: 0,
        note: None,
        before: Some(current.clone()),
        after: Some(anchors::normalise_newlines(candidate)),
        anchor: anchors::HunkAnchor {
            lead: String::new(),
            trail: String::new(),
            hint_start: 0,
            hint_end: current.len(),
        },
    }])
}

/// What a change id a model named can mean on the artifact as it stands
/// (PPC-FR-RVKS, on the terms of PDC-FR-GMWR).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionTarget {
    /// The hunk is one of the standing proposal's, undecided or rejected.
    Revisable,
    /// A proposal stands and holds no such hunk that can still be revised.
    NoSuchChange,
    /// The artifact holds no standing proposal, so no name could have meant one.
    NothingStanding,
}

/// PPC-FR-RVKS: whether a hunk id names something its agent can still revise on
/// this artifact.
pub fn revision_target(root: &fs::RootFs, artifact_id: &str, hunk_id: &str) -> RevisionTarget {
    let Some(record) = pending_for(root, artifact_id) else {
        return RevisionTarget::NothingStanding;
    };
    if record.ledger.iter().any(|row| row.id == hunk_id && row.state.is_revisable()) {
        RevisionTarget::Revisable
    } else {
        RevisionTarget::NoSuchChange
    }
}

/// PCP-FR-RVSN: replace one undecided or rejected hunk of the artifact's
/// standing proposal in place, keeping its id and its position. The hunk is
/// `pending` after it, and reads as unedited.
///
/// The replacement is placed against the file as it stands, exactly as a
/// recording is, so it refuses with the same typed errors.
pub fn revise_prompt_change_hunk(
    root: &fs::RootFs,
    artifact_id: &str,
    hunk_id: &str,
    proposed: &ProposedHunk,
) -> Result<PromptChangeProposal, String> {
    let _guard = artifact_lock(artifact_id);
    // Read inside the lock: which proposal stands, and where its hunks stand,
    // can both have moved since the caller decided to revise.
    let mut record = pending_for(root, artifact_id).ok_or(ERR_PROPOSAL_NOT_FOUND)?;
    let row = record.ledger.iter().find(|row| row.id == hunk_id).ok_or(ERR_HUNK_NOT_FOUND)?;
    if !row.state.is_revisable() {
        return Err(ERR_HUNK_ALREADY_DECIDED.to_string());
    }
    let dir = ensure_folder(root)?;
    let artifact_path = require_prompt_artifact(root, artifact_id)?;
    let current = root.read_text(&artifact_path).map_err(|_| ERR_ARTIFACT_NOT_FOUND.to_string())?;
    let (mut doc, _legacy) = read_hunk_document(root, &dir, &record.id, &current)?;
    let planned = text::plan(&current, std::slice::from_ref(proposed), || hunk_id.to_string())
        .map_err(|e| match e {
            text::PlanError::Ambiguous(_) => ERR_HUNK_AMBIGUOUS.to_string(),
            text::PlanError::ContextTooLong(_, side) => {
                format!("{ERR_HUNK_CONTEXT_TOO_LONG}:{}", side.as_str())
            }
            // A one-hunk revision can neither overlap another hunk of its own
            // nor be empty, so the remaining errors are one lost anchor.
            _ => ERR_ANCHOR_LOST.to_string(),
        })?;
    let next = planned.hunks.into_iter().next().ok_or(ERR_ANCHOR_LOST)?;
    let origin = fs::sha256_bytes(next.after_text().as_bytes());
    if !doc.replace_in_place(hunk_id, next) {
        return Err(ERR_HUNK_NOT_FOUND.to_string());
    }
    // PCP-FR-RVSN: a revised legacy proposal is a hunk proposal from here on.
    // The `.content` file is left exactly as it is (PCP-FR-LGXQ).
    root.write_text_atomic(at(&dir, &record.id, HUNKS_EXT), &doc.to_toml())
        .map_err(|_| ERR_WRITE_FAILED.to_string())?;
    record.legacy = false;
    if let Some(row) = record.ledger.iter_mut().find(|row| row.id == hunk_id) {
        row.edited = false;
        row.revision += 1;
        row.state = HunkState::Pending;
        row.decided_at = None;
    }
    record.origin_checksums.insert(hunk_id.to_string(), origin);
    resettle(&mut record);
    root.write_toml_atomic(at(&dir, &record.id, RECORD_EXT), &record).map_err(|e| e.to_string())?;
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    // PCP-FR-HKLD: the proposal's state is derived from the ledger, as
    // DCP-FR-PWSF derives a draft proposal's.
    #[test]
    fn the_proposal_state_is_derived_from_its_ledger() {
        use HunkState::*;
        assert_eq!(
            derive_state(&HunkCounts::of([Pending, Accepted])),
            PromptProposalState::Pending
        );
        assert_eq!(
            derive_state(&HunkCounts::of([Discussing, Accepted])),
            PromptProposalState::Pending
        );
        assert_eq!(
            derive_state(&HunkCounts::of([Accepted, Rejected])),
            PromptProposalState::Accepted
        );
        assert_eq!(
            derive_state(&HunkCounts::of([Rejected, Rejected])),
            PromptProposalState::Rejected
        );
        assert_eq!(HunkCounts::of([Pending, Discussing, Accepted]).undecided(), 2);
    }

    // PCP-FR-HLDQ: a held hunk is undecided; PCP-FR-RVSN: a rejected one is
    // still revisable and an accepted one is not.
    #[test]
    fn a_held_hunk_is_undecided_and_a_rejected_one_is_revisable() {
        assert!(HunkState::Discussing.is_undecided());
        assert!(!HunkState::Rejected.is_undecided());
        assert!(HunkState::Rejected.is_revisable());
        assert!(!HunkState::Accepted.is_revisable());
    }
}
