//! The per-hunk decision ledger of a prompt change proposal
//! (`PCP-prompt-change-proposals.md` PCP-FR-HKLD, PCP-FR-LGXQ).
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
use crate::text_hunks::hunks::{self as text, ProposalHunk};

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
    /// PCP-FR-RVSN (as DCP-FR-IAFV): taken back by its agent's update. It
    /// counts for neither final state.
    Withdrawn,
}

impl HunkState {
    /// A hunk the author has not settled either way.
    pub fn is_undecided(self) -> bool {
        matches!(self, HunkState::Pending | HunkState::Discussing)
    }

    /// PCP-FR-RVSN: a hunk its agent's update may still change or withdraw —
    /// an undecided one, or one the author rejected while the proposal stands.
    pub fn is_updatable(self) -> bool {
        self.is_undecided() || self == HunkState::Rejected
    }

    pub fn as_str(self) -> &'static str {
        match self {
            HunkState::Pending => "pending",
            HunkState::Accepted => "accepted",
            HunkState::Rejected => "rejected",
            HunkState::Discussing => "discussing",
            HunkState::Withdrawn => "withdrawn",
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
    /// PCP-FR-RVSN: bumped by an agent's `change` update, never by an author
    /// edit.
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
    /// PCP-FR-HKLD: absent from a record written before hunks could be
    /// withdrawn, and then zero.
    #[serde(default)]
    pub withdrawn: u32,
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
                HunkState::Withdrawn => c.withdrawn += 1,
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
    fn a_held_hunk_is_undecided_and_a_rejected_one_is_updatable() {
        assert!(HunkState::Discussing.is_undecided());
        assert!(!HunkState::Rejected.is_undecided());
        assert!(HunkState::Rejected.is_updatable());
        assert!(!HunkState::Accepted.is_updatable());
        assert!(!HunkState::Withdrawn.is_updatable());
        assert!(!HunkState::Withdrawn.is_undecided());
    }
}
