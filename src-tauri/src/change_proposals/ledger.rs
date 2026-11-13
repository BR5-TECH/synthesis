//! The per-hunk ledger of a change edit proposal (CEP-FR-HXCC, CEP-FR-IISM).
//!
//! The row is PCP's ledger row with the hunk's change-relative path on it
//! (CEP contract surface `HunkLedgerRow`). The proposal's state is derived from
//! the ledger on every write.

use super::*;
use crate::prompt_proposals::{HunkCounts, HunkState};
use crate::text_hunks::anchors::HunkKind;
use crate::text_hunks::hunks::ProposalHunk;

/// One row of the decision ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeHunkRow {
    pub id: String,
    /// Change-relative path (CEP-FR-WJKY).
    pub path: String,
    pub kind: HunkKind,
    #[serde(default)]
    pub new_file: bool,
    #[serde(default = "pending")]
    pub state: HunkState,
    #[serde(default)]
    pub edited: bool,
    #[serde(default)]
    pub revision: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
    /// OPU-FR-ZNRR: the author's feedback on the decision of this hunk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
}

fn pending() -> HunkState {
    HunkState::Pending
}

impl ChangeHunkRow {
    pub fn new(path: &str, new_file: bool, hunk: &ProposalHunk) -> Self {
        Self {
            id: hunk.id.clone(),
            path: path.to_string(),
            kind: hunk.kind,
            new_file,
            state: HunkState::Pending,
            edited: false,
            revision: hunk.revision,
            decided_at: None,
            feedback: None,
        }
    }
}

/// CEP-FR-ZOIG (as PCP-FR-HKLD): `pending` while any hunk is undecided,
/// `accepted` once all are decided and at least one was accepted, and
/// `rejected` otherwise.
pub fn derive_state(counts: &HunkCounts) -> ChangeProposalState {
    if counts.undecided() > 0 {
        ChangeProposalState::Pending
    } else if counts.accepted > 0 {
        ChangeProposalState::Accepted
    } else {
        ChangeProposalState::Rejected
    }
}

/// Recompute the derived fields from the ledger.
pub(super) fn resettle(record: &mut ChangeEditProposal) {
    record.counts = HunkCounts::of(record.ledger.iter().map(|row| row.state));
    record.hunk_count = record.ledger.len() as u32;
    record.candidate_edited = record.ledger.iter().any(|row| row.edited);
    record.state = derive_state(&record.counts);
    record.decided_at = match record.state {
        ChangeProposalState::Pending => None,
        _ => record.decided_at.clone().or_else(|| Some(now_rfc3339())),
    };
}

/// CEP-FR-RWTQ (as PCP-FR-RSLV): the one decision comment's body — where the
/// proposal ended, the tally, whether the author edited a hunk, and the
/// feedback.
pub(super) fn decision_body(
    change: &str,
    counts: &HunkCounts,
    edited: bool,
    feedback: Option<&str>,
) -> String {
    let sentence = match (derive_state(counts), edited) {
        (ChangeProposalState::Accepted, false) => {
            format!("Accepted the proposed edits to OpenSpec change `{change}`.")
        }
        (ChangeProposalState::Accepted, true) => {
            format!(
                "Accepted an edited version of the proposed edits to OpenSpec change `{change}`."
            )
        }
        (_, false) => format!("Declined the proposed edits to OpenSpec change `{change}`."),
        (_, true) => format!(
            "Declined the proposed edits to OpenSpec change `{change}`, having edited them first."
        ),
    };
    let sentence =
        format!("{sentence} {} accepted, {} rejected.", counts.accepted, counts.rejected);
    match feedback.map(str::trim).filter(|f| !f.is_empty()) {
        Some(feedback) => format!("{sentence}\n\n{feedback}"),
        None => sentence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // CEP-FR-ZOIG / CEP-FR-RWTQ: the state is derived from the ledger, and a
    // proposal closes when no hunk is undecided.
    #[test]
    fn the_proposal_state_follows_its_ledger() {
        use HunkState::*;
        assert_eq!(
            derive_state(&HunkCounts::of([Pending, Accepted])),
            ChangeProposalState::Pending
        );
        assert_eq!(
            derive_state(&HunkCounts::of([Discussing, Rejected])),
            ChangeProposalState::Pending
        );
        assert_eq!(
            derive_state(&HunkCounts::of([Accepted, Rejected])),
            ChangeProposalState::Accepted
        );
        assert_eq!(
            derive_state(&HunkCounts::of([Rejected, Withdrawn])),
            ChangeProposalState::Rejected
        );
    }

    // CEP-FR-RWTQ: the decision comment names the change and the tally, then
    // the feedback.
    #[test]
    fn the_decision_body_names_the_change_and_the_tally() {
        let counts = HunkCounts::of([HunkState::Accepted, HunkState::Rejected]);
        let body = decision_body("add-auth", &counts, false, Some("  thanks  "));
        assert_eq!(
            body,
            "Accepted the proposed edits to OpenSpec change `add-auth`. 1 accepted, 1 rejected.\n\nthanks"
        );
        let declined =
            decision_body("add-auth", &HunkCounts::of([HunkState::Rejected]), true, None);
        assert!(declined.starts_with(
            "Declined the proposed edits to OpenSpec change `add-auth`, having edited"
        ));
    }
}
