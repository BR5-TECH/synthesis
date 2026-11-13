//! The split of an OpenSpec change in `discussion_history`
//! (`AGC-agent-conversations.md` AGC-FR-JNOS, AGC-FR-LTLT).
//!
//! An agent that proposes new changes one at a time needs to know which parts
//! the author already accepted or rejected, and the files of the one that is
//! still open, so it can change it or propose the next one.

use std::collections::HashMap;

use super::*;
use crate::new_change_proposals::{NewChangeProposal, NewChangeProposalState};

/// AGC-FR-LTLT: the new-change proposals of a thread's change, by id.
pub(in crate::agent_conversations) type NewChangeReads = HashMap<String, NewChangeProposal>;

/// AGC-FR-LTLT: one read of the change's new-change records, for a thread on a
/// change that names one. Any other thread reads nothing.
pub(in crate::agent_conversations) fn new_change_reads(
    roots: Roots<'_>,
    thread: &comments::Discussion,
) -> NewChangeReads {
    let Some((stream_id, change)) = thread.target.change_owner() else {
        return NewChangeReads::new();
    };
    let has_reference = thread.comments.iter().flat_map(|c| &c.attachments).any(|a| {
        matches!(
            a,
            comments::Attachment::NewChangeProposal { .. }
                | comments::Attachment::NewChangeSubject { .. }
        )
    });
    if !has_reference {
        return NewChangeReads::new();
    }
    crate::new_change_proposals::list_proposals_impl(roots.store, stream_id, change)
        .into_iter()
        .map(|p| (p.id.clone(), p))
        .collect()
}

/// One proposal named by its id, its place, its name, its files, and its state.
fn summary(p: &NewChangeProposal) -> String {
    let created = match &p.created_change {
        Some(name) => format!(", created {name}"),
        None => String::new(),
    };
    format!(
        "{} — {} of {} — {} — files {} — {}{created}",
        p.id,
        p.position,
        p.total,
        p.name,
        p.paths.join(", "),
        p.state.as_str()
    )
}

/// AGC-FR-LTLT: the line a `new_change_proposal` or `new_change_subject`
/// attachment contributes. It names the proposal as it stands, and never its
/// files' text.
pub(in crate::agent_conversations) fn new_change_line(
    attachment: &comments::Attachment,
    reads: &NewChangeReads,
) -> String {
    match attachment {
        comments::Attachment::NewChangeProposal { proposal_id, update, .. } => {
            let verb = match update {
                Some(comments::NewDraftUpdateAction::Change) => "changed the proposed new change",
                Some(comments::NewDraftUpdateAction::Withdraw) => {
                    "withdrew the proposed new change"
                }
                None => "proposed a new OpenSpec change",
            };
            match reads.get(proposal_id) {
                Some(p) => format!("[{verb}: {}]", summary(p)),
                None => format!("[{verb} {proposal_id}]"),
            }
        }
        comments::Attachment::NewChangeSubject { proposal_id, .. } => {
            match reads.get(proposal_id) {
                Some(p) => format!("[about the proposed new change: {}]", summary(p)),
                None => format!("[about the proposed new change {proposal_id}]"),
            }
        }
        _ => String::new(),
    }
}

/// AGC-FR-JNOS: the split that ends `discussion_history`, before an open edit
/// proposal, or `None` where the target is no change or holds no new-change
/// proposal.
pub(in crate::agent_conversations) fn split_text(
    roots: Roots<'_>,
    target: &DiscussionTarget,
) -> Option<String> {
    let (stream_id, change) = target.change_owner()?;
    let split =
        crate::new_change_proposals::read_new_change_proposals(roots.store, stream_id, change)?;
    let status = if split.confirmed { "confirmed" } else { "not confirmed" };
    let mut text = format!(
        "\nThe split {} of this change, {status}, every proposed new change oldest first:\n",
        split.set_id
    );
    for p in &split.proposals {
        text.push_str(&format!("  {}\n", summary(p)));
    }
    if let Some(open) = split.proposals.iter().find(|p| p.state == NewChangeProposalState::Pending)
    {
        text.push_str(&format!(
            "The proposed new change {} waits on the author. Its files:\n",
            open.id
        ));
        for file in &split.pending_files {
            text.push_str(&format!("--- {} ---\n{}\n", file.path, file.text));
        }
    }
    Some(text)
}
