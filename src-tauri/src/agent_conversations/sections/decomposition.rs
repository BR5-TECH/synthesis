//! The decomposition in `discussion_history` (`AGC-agent-conversations.md`
//! AGC-FR-TIGM, AGC-FR-ZZWP).
//!
//! An agent that proposes new drafts one at a time needs to know which parts
//! the author already accepted or rejected, and the text of the one that is
//! still open, so it can change it or propose the next one.

use std::collections::HashMap;

use super::*;
use crate::new_draft_proposals::{NewDraftProposal, NewDraftState};

/// AGC-FR-ZZWP: the new-draft proposals of a thread's draft, by id.
pub(in crate::agent_conversations) type NewDraftReads = HashMap<String, NewDraftProposal>;

/// AGC-FR-ZZWP: one read of the draft's new-draft records, for a thread on a
/// draft. Any other thread reads nothing.
pub(in crate::agent_conversations) fn new_draft_reads(
    roots: Roots<'_>,
    thread: &comments::Discussion,
) -> NewDraftReads {
    let DiscussionTarget::Draft { draft_id } = &thread.target else {
        return NewDraftReads::new();
    };
    let has_reference = thread.comments.iter().flat_map(|c| &c.attachments).any(|a| {
        matches!(
            a,
            comments::Attachment::NewDraftProposal { .. }
                | comments::Attachment::NewDraftSubject { .. }
        )
    });
    if !has_reference {
        return NewDraftReads::new();
    }
    crate::new_draft_proposals::list_impl(roots.worktree, draft_id)
        .unwrap_or_default()
        .into_iter()
        .map(|p| (p.id.clone(), p))
        .collect()
}

fn summary(p: &NewDraftProposal) -> String {
    let mut text = format!(
        "id {} — part {} of {} — “{}” — {}",
        p.id,
        p.position,
        p.total,
        p.name,
        p.state.as_str()
    );
    if let Some(created) = &p.created_draft_id {
        text.push_str(&format!(" — created draft {created}"));
    }
    text
}

/// AGC-FR-ZZWP: the line a `new_draft_proposal` or `new_draft_subject`
/// attachment contributes. It names the proposal as it stands, and never its
/// body.
pub(in crate::agent_conversations) fn new_draft_line(
    attachment: &comments::Attachment,
    reads: &NewDraftReads,
) -> String {
    match attachment {
        comments::Attachment::NewDraftProposal { proposal_id, update, .. } => {
            let verb = match update {
                Some(comments::NewDraftUpdateAction::Change) => "changed the proposed new draft",
                Some(comments::NewDraftUpdateAction::Withdraw) => "withdrew the proposed new draft",
                None => "proposed a new draft",
            };
            match reads.get(proposal_id) {
                Some(p) => format!("[{verb}: {}]", summary(p)),
                None => format!("[{verb} {proposal_id}]"),
            }
        }
        comments::Attachment::NewDraftSubject { proposal_id, .. } => match reads.get(proposal_id) {
            Some(p) => format!("[about the proposed new draft: {}]", summary(p)),
            None => format!("[about the proposed new draft {proposal_id}]"),
        },
        _ => String::new(),
    }
}

/// AGC-FR-TIGM: the decomposition that ends `discussion_history`, before an
/// open proposal, or `None` where the target is no draft or holds no
/// new-draft proposal.
pub(in crate::agent_conversations) fn decomposition_text(
    roots: Roots<'_>,
    target: &DiscussionTarget,
) -> Option<String> {
    let DiscussionTarget::Draft { draft_id } = target else {
        return None;
    };
    let (all, body) =
        crate::new_draft_proposals::read_new_draft_proposals(roots.worktree, draft_id)?;
    // AGC-FR-TIGM: the id of the decomposition and whether it is confirmed,
    // so the agent can confirm it on the author's instruction (DDC-FR-VEKP).
    let set_id = all.first().map(|p| p.set_id.as_str()).unwrap_or_default();
    let confirmed = crate::drafts::read_draft_record(roots.worktree, draft_id)
        .map(|record| record.superseded.is_some())
        .unwrap_or(false);
    let status = if confirmed { "confirmed" } else { "not confirmed" };
    let mut text = format!(
        "\nThe decomposition {set_id} of this draft, {status}, every proposed new draft oldest first:\n"
    );
    for p in &all {
        text.push_str(&format!("  {}\n", summary(p)));
    }
    if let (Some(open), Some(body)) = (all.iter().find(|p| p.state == NewDraftState::Pending), body)
    {
        text.push_str(&format!(
            "The proposed new draft {} waits on the author. Its body:\n{body}\n",
            open.id
        ));
    }
    Some(text)
}

#[cfg(test)]
mod tests;
