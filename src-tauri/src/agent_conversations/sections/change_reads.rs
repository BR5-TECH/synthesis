//! The reads of a change edit proposal for a turn's input (AGC-FR-RVQP,
//! AGC-FR-DKHT, AGC-FR-EPNB applied to a change target by CEP-FR-XXMW).
//!
//! Each hunk reads in the shape a draft's hunk renders from, so the three
//! proposal kinds read alike. The hunk's change-relative path leads its text,
//! because one change edit proposal spans several files.

use super::*;

/// The label a change edit proposal is named by in a turn's input.
pub(in crate::agent_conversations) fn change_label(change: &str) -> String {
    format!("OpenSpec change {change}")
}

/// One hunk of a change edit proposal, as a draft hunk reads.
pub(in crate::agent_conversations) fn change_hunk_view(
    change: crate::change_proposals::ChangeProposalChange,
) -> crate::draft_proposals::ProposalChange {
    use crate::draft_proposals::hunks::HunkState as Draft;
    use crate::prompt_proposals::HunkState as Prompt;
    let in_file = |text: String| {
        if text.is_empty() {
            text
        } else {
            format!("({}) {text}", change.path)
        }
    };
    let new_file = change.new_file;
    let path = change.path.clone();
    crate::draft_proposals::ProposalChange {
        id: change.id,
        kind: change.kind,
        state: match change.state {
            Prompt::Pending => Draft::Pending,
            Prompt::Accepted => Draft::Accepted,
            Prompt::Rejected => Draft::Rejected,
            Prompt::Discussing => Draft::Discussing,
            Prompt::Withdrawn => Draft::Withdrawn,
        },
        before: in_file(change.before),
        after: if new_file {
            format!("(new file {path}) {}", change.after)
        } else {
            in_file(change.after)
        },
    }
}

/// AGC-FR-RVQP: the digest of one change edit proposal, read from the store.
pub(in crate::agent_conversations) fn change_digest(
    store: &crate::fs::RootFs,
    proposal_id: &str,
) -> Option<(String, ProposalDigest)> {
    let proposal = crate::change_proposals::read_change_proposal_changes(store, proposal_id)?;
    let counts = proposal.counts;
    Some((
        change_label(&proposal.change),
        ProposalDigest {
            counts: crate::draft_proposals::hunks::HunkCounts {
                pending: counts.pending,
                accepted: counts.accepted,
                rejected: counts.rejected,
                discussing: counts.discussing,
                withdrawn: counts.withdrawn,
            },
            changes: Some(proposal.changes.into_iter().map(change_hunk_view).collect()),
        },
    ))
}

/// AGC-FR-DKHT: one hunk of one change edit proposal.
pub(in crate::agent_conversations) fn change_hunk(
    store: &crate::fs::RootFs,
    proposal_id: &str,
    hunk_id: &str,
) -> Option<crate::draft_proposals::ProposalChange> {
    crate::change_proposals::read_change_proposal_change(store, proposal_id, hunk_id)
        .map(change_hunk_view)
}
