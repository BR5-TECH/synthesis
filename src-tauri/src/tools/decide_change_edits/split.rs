//! The entries of `decide_change_edits` that name the pending new-change
//! proposal or the split of the change (`DCE-decide-change-edits-tool.md`
//! DCE-FR-UZDN, DCE-FR-RAWJ, DCE-FR-WMOG, DCE-FR-WRSI, DCE-FR-PHSU).

use super::*;
use crate::new_change_proposals::{self as ncp, NewChangeDecision, NewChangeProposalState};
use crate::tools::decide_changes::{DecisionRow, Outcome, RefusalCode};

/// DCE-FR-WMOG: the opening comment of a new-change discussion opened by tool.
pub const NEW_CHANGE_DISCUSSION_BODY: &str =
    "The author asked to discuss this proposed new change.";

/// The `invalid_action` text of a change entry.
pub const INVALID_CHANGE_ACTION: &str = "the action must be `accept`, `reject`, or `discuss` for a hunk or a proposed new change, and `confirm` for the split.";

/// NCP-FR-LQJK: a new-change discussion is one whose opening comment carries a
/// `new_change_subject` that names the proposal.
pub fn is_new_change_discussion(discussion: &comments::Discussion, proposal_id: &str) -> bool {
    discussion.comments.first().is_some_and(|opening| {
        opening.attachments.iter().any(|a| {
            matches!(a, comments::Attachment::NewChangeSubject { proposal_id: p, .. } if p == proposal_id)
        })
    })
}

/// DCE-FR-UZDN: the split of the change as a call reads it when it begins.
#[derive(Debug, Clone)]
pub(super) struct SplitView {
    pub(super) set_id: String,
    /// DCE-FR-PHSU: it holds an accepted proposal and is not confirmed.
    pub(super) confirmable: bool,
    /// The proposal pending when the call began.
    pub(super) pending: Option<String>,
}

pub(super) fn split_of(
    store: &crate::fs::RootFs,
    stream_id: &str,
    change: &str,
) -> Option<SplitView> {
    let all = ncp::list_proposals_impl(store, stream_id, change);
    let set_id = all.first()?.set_id.clone();
    let confirmed = ncp::read_split(store, stream_id, change).is_some();
    let accepted = all.iter().any(|p| p.state == NewChangeProposalState::Accepted);
    let pending =
        all.iter().find(|p| p.state == NewChangeProposalState::Pending).map(|p| p.id.clone());
    Some(SplitView { set_id, confirmable: accepted && !confirmed, pending })
}

/// DCE-FR-UZDN: run one entry that names the pending new-change proposal or
/// the split, or answer `None` for an entry about a hunk.
pub(super) fn side_entry<R: tauri::Runtime>(
    backend: &ChangeBackend<'_, R>,
    id: &str,
    action: &str,
    place: usize,
    total: usize,
) -> Option<DecisionRow>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let split = backend.split.as_ref()?;
    let names_split = split.set_id == id;
    if action == "confirm" || names_split {
        return Some(confirm(backend, id, action, place, total, names_split));
    }
    if split.pending.as_deref() != Some(id) {
        return None;
    }
    let record = ncp::list_proposals_impl(backend.store(), backend.stream_id, backend.change)
        .into_iter()
        .find(|p| p.id == id)?;
    let mut row = DecisionRow::new(place, id, action);
    let accept = match action {
        "accept" => Some(true),
        "reject" => Some(false),
        "discuss" => None,
        _ => {
            row.refuse(RefusalCode::InvalidAction, total, INVALID_CHANGE_ACTION, None);
            return Some(row);
        }
    };
    if record.state != NewChangeProposalState::Pending {
        let text = "this proposed new change is already decided, so this decision was not made.";
        row.refuse(RefusalCode::NoOpenProposal, total, text, None);
        return Some(row);
    }
    let Some(accept) = accept else {
        discuss(backend, &mut row, total);
        return Some(row);
    };
    // DCE-FR-RAWJ: the card's operation, the instruction line in the place of
    // feedback (DDC-FR-JJHZ).
    let ran = backend.author.clone().and_then(|by| {
        ncp::decide_new_change(
            &backend.tool.app,
            backend.store(),
            backend.sites,
            NewChangeDecision { proposal_id: id, accept, feedback: Some(&backend.line), by: &by },
        )
    });
    match ran {
        Ok(_) => row.outcome = if accept { Outcome::Accepted } else { Outcome::Rejected },
        Err(error) => {
            // DCE-FR-RAWJ: a busy stream is a retryable row, as a write that
            // failed is.
            let retryable = error == ncp::ERR_WRITE_FAILED || error == ncp::ERR_STREAM_BUSY;
            let text = format!("the decision could not be made ({error}).");
            row.refuse(RefusalCode::DecisionFailed, total, &text, Some(error));
            if let Some(refusal) = row.refusal.as_mut() {
                refusal.retryable = retryable;
            }
        }
    }
    Some(row)
}

/// DCE-FR-WMOG: open a new discussion of the change about the proposal, or
/// refuse where one exists.
fn discuss<R: tauri::Runtime>(backend: &ChangeBackend<'_, R>, row: &mut DecisionRow, total: usize)
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    use crate::tools::decide_changes::ProposalBackend;
    let id = row.id.clone();
    if backend.discussions().iter().any(|d| is_new_change_discussion(d, &id)) {
        let text = "this proposed new change already has a discussion, so nothing changed.";
        row.refuse(RefusalCode::AlreadyInState, total, text, None);
        return;
    }
    let attachment = comments::AttachmentInput::NewChangeSubject {
        proposal_id: id,
        stream_id: backend.stream_id.to_string(),
        change: backend.change.to_string(),
    };
    match comments::open_discussion_in(
        backend.store(),
        &backend.tool.roots.worktree,
        &backend.target(),
        None,
        NEW_CHANGE_DISCUSSION_BODY.to_string(),
        vec![attachment],
        comments::Stamp { by: &backend.tool.agent, at: &crate::notes::now_rfc3339() },
    ) {
        Ok(opened) => {
            comments::emit_discussion_changed(&backend.tool.app, &opened);
            row.outcome = Outcome::DiscussionOpened;
            row.discussion_id = Some(opened.id);
        }
        Err(error) => {
            let text = format!("the decision could not be made ({error}).");
            row.refuse(RefusalCode::DecisionFailed, total, &text, Some(error));
        }
    }
}

/// DCE-FR-WRSI: one entry about the split.
fn confirm<R: tauri::Runtime>(
    backend: &ChangeBackend<'_, R>,
    id: &str,
    action: &str,
    place: usize,
    total: usize,
    names_split: bool,
) -> DecisionRow
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let mut row = DecisionRow::new(place, id, action);
    if action != "confirm" {
        row.refuse(RefusalCode::InvalidAction, total, INVALID_CHANGE_ACTION, None);
        return row;
    }
    if !names_split {
        let text =
            "the id you gave is not a change of the open proposal. Use an id from your input.";
        row.refuse(RefusalCode::UnknownHunk, total, text, None);
        return row;
    }
    let ran = backend.author.clone().and_then(|by| {
        ncp::confirm_split(
            &backend.tool.app,
            backend.store(),
            backend.stream_id,
            backend.change,
            &by,
        )
    });
    match ran {
        Ok(_) => row.outcome = Outcome::Confirmed,
        Err(error) => {
            let retryable = error == ncp::ERR_WRITE_FAILED;
            let text = format!("the decision could not be made ({error}).");
            row.refuse(RefusalCode::DecisionFailed, total, &text, Some(error));
            if let Some(refusal) = row.refusal.as_mut() {
                refusal.retryable = retryable;
            }
        }
    }
    row
}
