//! A prompt proposal's hunk in the shape a draft's hunk renders from
//! (`AGC-agent-conversations.md` AGC-FR-DKHT).

/// AGC-FR-DKHT: a prompt proposal's hunk in the shape a draft's renders from,
/// so the two read identically in a turn's input.
pub(in crate::agent_conversations) fn prompt_change_view(
    change: crate::prompt_proposals::PromptProposalChange,
) -> crate::draft_proposals::ProposalChange {
    use crate::draft_proposals::hunks::HunkState as Draft;
    use crate::prompt_proposals::HunkState as Prompt;
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
        before: change.before,
        after: change.after,
    }
}
