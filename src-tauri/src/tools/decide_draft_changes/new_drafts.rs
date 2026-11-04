//! The entries of `decide_draft_changes` that name the pending new-draft
//! proposal of a decomposition (`DDC-decide-draft-changes-tool.md`
//! DDC-FR-GGWO, DDC-FR-ZXML, DDC-FR-YDUQ, DDC-FR-ZWKS, DDC-FR-NWRG).

use super::*;
use crate::new_draft_proposals::{self as nd, NewDraftDecision, NewDraftState};
use crate::tools::decide_changes::{Outcome, RefusalCode};

/// DDC-FR-YDUQ: the opening comment of a new-draft discussion opened by tool.
pub const NEW_DRAFT_DISCUSSION_BODY: &str = "The author asked to discuss this proposed draft.";

/// DCP-FR-TRPW: a new-draft discussion is one whose **opening** comment
/// carries a `new_draft_subject` that names the proposal.
pub fn is_new_draft_discussion(discussion: &comments::Discussion, proposal_id: &str) -> bool {
    discussion.comments.first().is_some_and(|opening| {
        opening.attachments.iter().any(|a| {
            matches!(a, comments::Attachment::NewDraftSubject { proposal_id: p, .. } if p == proposal_id)
        })
    })
}

/// DDC-FR-VEKP: the decomposition of a draft, as a call reads it when it
/// begins.
#[derive(Debug, Clone)]
pub(super) struct Decomposition {
    /// The id of the decomposition: the `set_id` of its proposals.
    pub(super) set_id: String,
    /// DDC-FR-ZGXF: it holds an accepted proposal and is not confirmed.
    pub(super) confirmable: bool,
}

/// DDC-FR-VEKP: the decomposition of `draft_id`, or `None` where the draft
/// holds no new-draft proposal.
pub(super) fn decomposition_of(root: &crate::fs::RootFs, draft_id: &str) -> Option<Decomposition> {
    let all = nd::list_impl(root, draft_id).ok()?;
    let set_id = all.first()?.set_id.clone();
    let confirmed = crate::drafts::read_draft_record(root, draft_id)
        .map(|record| record.superseded.is_some())
        .unwrap_or(false);
    let accepted = all.iter().any(|p| p.state == NewDraftState::Accepted);
    Some(Decomposition { set_id, confirmable: accepted && !confirmed })
}

/// DDC-FR-ZGXF: a pending new-draft proposal, or a decomposition that can be
/// confirmed, lets a call run.
pub(super) fn side_open<R: tauri::Runtime>(backend: &DraftBackend<'_, R>) -> bool
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    backend.new_draft_open.is_some()
        || backend.decomposition.as_ref().is_some_and(|d| d.confirmable)
}

/// DDC-FR-GGWO: run one entry whose `id` names a new-draft proposal of the
/// draft, or answer `None` for an entry about a hunk.
pub(super) fn side_entry<R: tauri::Runtime>(
    backend: &DraftBackend<'_, R>,
    id: &str,
    action: &str,
    place: usize,
    total: usize,
) -> Option<DecisionRow>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let tool = backend.tool;
    // DDC-FR-VEKP: every `confirm` entry, and every entry that names the
    // decomposition, is about the decomposition.
    let names_decomposition = backend.decomposition.as_ref().is_some_and(|d| d.set_id == id);
    if action == "confirm" || names_decomposition {
        return Some(confirm(backend, id, action, place, total, names_decomposition));
    }
    // DDC-FR-GGWO: only the proposal pending when the call began is a new-draft
    // entry; any other id is about a hunk, or unknown.
    if backend.new_draft_open.as_deref() != Some(id) {
        return None;
    }
    let record = nd::list_impl(&tool.roots.worktree, backend.draft_id)
        .ok()?
        .into_iter()
        .find(|p| p.id == id)?;
    let mut row = DecisionRow::new(place, id, action);
    row.new_draft_state = Some(record.state.as_str());
    let decision = match action {
        "accept" => Some(NewDraftDecision::Accept),
        "reject" => Some(NewDraftDecision::Reject),
        "discuss" => None,
        _ => {
            let text = decide_changes::INVALID_DRAFT_ACTION;
            row.refuse(RefusalCode::InvalidAction, total, text, None);
            return Some(row);
        }
    };
    // DDC-FR-ZWKS: an earlier entry of this call decided it.
    if record.state != NewDraftState::Pending {
        let text = "this proposed draft is already decided, so this decision was not made.";
        row.refuse(RefusalCode::NoOpenProposal, total, text, None);
        return Some(row);
    }
    let Some(decision) = decision else {
        discuss(backend, &mut row, total);
        return Some(row);
    };
    // DDC-FR-ZXML / DDC-FR-NWRG: the card's operation, with no feedback, and
    // no agent turn. DDC-FR-JJHZ / DCP-FR-HMUG: the instruction line takes the
    // place feedback takes, so the decision comment ends with it.
    let ran = backend.author.clone().and_then(|by| {
        nd::decide_new_draft_in_process(
            &tool.app,
            &tool.roots.worktree,
            &tool.roots.store,
            id,
            decision,
            Some(&backend.line),
            &by,
        )
    });
    match ran {
        Ok(outcome) => {
            row.outcome = match decision {
                NewDraftDecision::Accept => Outcome::Accepted,
                NewDraftDecision::Reject => Outcome::Rejected,
            };
            row.new_draft_state = Some(outcome.proposal.state.as_str());
            row.created_draft_id = outcome.proposal.created_draft_id;
        }
        Err(error) => {
            let retryable = error == nd::ERR_WRITE_FAILED;
            let text = format!("the decision could not be made ({error}).");
            row.refuse(RefusalCode::DecisionFailed, total, &text, Some(error));
            if let Some(refusal) = row.refusal.as_mut() {
                refusal.retryable = retryable;
            }
        }
    }
    Some(row)
}

/// DDC-FR-YDUQ / DDC-FR-ZWKS: open a new discussion about the proposal, or
/// refuse where one exists.
fn discuss<R: tauri::Runtime>(backend: &DraftBackend<'_, R>, row: &mut DecisionRow, total: usize)
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let tool = backend.tool;
    let id = row.id.clone();
    if backend.discussions().iter().any(|d| is_new_draft_discussion(d, &id)) {
        let text = "this proposed draft already has a discussion, so nothing changed.";
        row.refuse(RefusalCode::AlreadyInState, total, text, None);
        return;
    }
    let target = backend.target();
    let attachment = comments::AttachmentInput::NewDraftSubject {
        proposal_id: id,
        draft_id: backend.draft_id.to_string(),
    };
    match comments::open_discussion_in(
        &tool.roots.store,
        &tool.roots.worktree,
        &target,
        None,
        NEW_DRAFT_DISCUSSION_BODY.to_string(),
        vec![attachment],
        comments::Stamp { by: &tool.agent, at: &crate::notes::now_rfc3339() },
    ) {
        Ok(opened) => {
            comments::emit_discussion_changed(&tool.app, &opened);
            row.outcome = Outcome::DiscussionOpened;
            row.discussion_id = Some(opened.id);
        }
        Err(error) => {
            let text = format!("the decision could not be made ({error}).");
            row.refuse(RefusalCode::DecisionFailed, total, &text, Some(error));
        }
    }
}

/// DDC-FR-VEKP, DDC-FR-PEJA, DDC-FR-WITR: one entry about the decomposition.
///
/// A `confirm` entry with the id of the decomposition invokes "confirm draft
/// decomposition" in-process (DCP-FR-IKTU). A `confirm` entry with any other
/// id is `unknown_hunk`, and any other action with that id is
/// `invalid_action`. No agent turn is dispatched.
fn confirm<R: tauri::Runtime>(
    backend: &DraftBackend<'_, R>,
    id: &str,
    action: &str,
    place: usize,
    total: usize,
    names_decomposition: bool,
) -> DecisionRow
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let tool = backend.tool;
    let mut row = DecisionRow::new(place, id, action);
    if action != "confirm" {
        row.refuse(RefusalCode::InvalidAction, total, decide_changes::INVALID_DRAFT_ACTION, None);
        return row;
    }
    if !names_decomposition {
        // DDC contract surface: the one `unknown_hunk` text.
        let text =
            "the id you gave is not a change of the open proposal. Use an id from your input.";
        row.refuse(RefusalCode::UnknownHunk, total, text, None);
        return row;
    }
    let ran = backend.author.clone().and_then(|by| {
        nd::confirm_decomposition_impl(
            &tool.app,
            &tool.roots.worktree,
            &tool.roots.store,
            backend.draft_id,
            &by,
            Some(&backend.line),
        )
    });
    match ran {
        Ok(_) => row.outcome = Outcome::Confirmed,
        Err(error) => {
            // DDC-FR-PEJA: retryable for `write_failed` alone.
            let retryable = error == nd::ERR_WRITE_FAILED;
            let text = format!("the decision could not be made ({error}).");
            row.refuse(RefusalCode::DecisionFailed, total, &text, Some(error));
            if let Some(refusal) = row.refusal.as_mut() {
                refusal.retryable = retryable;
            }
        }
    }
    row
}

#[cfg(test)]
mod tests;
