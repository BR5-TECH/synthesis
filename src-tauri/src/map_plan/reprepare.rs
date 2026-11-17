//! MPR-FR-LJRO to MPR-FR-LOUI: the re-prepare request. Where rows of a
//! proposal failed, the module asks the agent that proposed to read the issues
//! again and to propose again. The request is one Synthesis comment in the Map
//! discussion and one turn of that agent (CTA-FR-YWSU).
//!
//! Every function here runs inside the turn of a decision (MPR-FR-HKYH), so a
//! request and a decision never see different rows.

use super::errors::{
    error, log_operation, store_unavailable, ERR_DISCUSSION_NOT_FOUND, ERR_NOTHING_FAILED,
    ERR_PROPOSAL_OPEN, ERR_REPREPARE_REQUESTED,
};
use super::model::{MapPlanError, MapPlanProposal, MapPlanRow, RowFields, RowState};
use super::{rules, store};
use crate::comments::{Participant, Stamp, ThreadRef};
use crate::delivery::github::ERR_CHANGED_ON_TRACKER;
use crate::fs::RootFs;

/// The one turn that a request starts (MPR-FR-NWEZ).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnRequest {
    pub discussion_id: String,
    /// The handle of the agent that proposed (MPR-FR-QIJF).
    pub nickname: String,
    /// The id of the Synthesis comment of the request.
    pub trigger_comment_id: String,
}

/// What the seam of a test does with the turn of a request.
#[cfg(test)]
pub type DispatchFn = dyn Fn(&TurnRequest) -> Result<(), String> + Send + Sync;

/// A test sets this state on its mock application to see the turn that a
/// request starts, without a model. The application never sets it.
#[cfg(test)]
pub struct DispatchSeam(pub std::sync::Arc<DispatchFn>);

fn failed_rows(proposal: &MapPlanProposal) -> impl Iterator<Item = &MapPlanRow> {
    proposal.rows.iter().filter(|r| r.state == RowState::Failed)
}

fn changed_on_tracker(row: &MapPlanRow) -> bool {
    row.error.as_ref().is_some_and(|e| e.code == ERR_CHANGED_ON_TRACKER)
}

/// MPR-FR-LJRO: whether a decision requests a re-prepare on its own. It does
/// where no row of the proposal waits, at least one row failed with
/// `changed_on_tracker`, and the proposal records no request.
pub fn wants_automatic(proposal: &MapPlanProposal) -> bool {
    proposal.reprepare_requested_at.is_none()
        && !proposal.rows.iter().any(|r| r.state == RowState::Waiting)
        && failed_rows(proposal).any(changed_on_tracker)
}

/// MPR-FR-GIJN: the refusal of a request on the demand of the author, or
/// `None`. `discussion` holds every proposal of the discussion.
pub fn manual_refusal(
    proposal: &MapPlanProposal,
    discussion: &[MapPlanProposal],
) -> Option<MapPlanError> {
    if failed_rows(proposal).next().is_none() {
        return Some(error(ERR_NOTHING_FAILED, "No row of the proposal failed."));
    }
    if rules::has_open_row(discussion) || rules::has_open_row(std::slice::from_ref(proposal)) {
        return Some(error(
            ERR_PROPOSAL_OPEN,
            "The discussion has a row that waits for a decision.",
        ));
    }
    if proposal.reprepare_requested_at.is_some() {
        return Some(error(ERR_REPREPARE_REQUESTED, "The agent was asked already."));
    }
    None
}

/// `a`, `a and b`, `a, b, and c`.
fn join(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} and {second}"),
        [head @ .., last] => format!("{}, and {last}", head.join(", ")),
    }
}

/// The name of a failed row in the comment: the issue number, else the name
/// of what the row was to make.
fn subject(row: &MapPlanRow) -> String {
    match &row.fields {
        RowFields::ModifyIssue(fields) => format!("#{}", fields.number),
        RowFields::CreateIssue(fields) => format!("the new issue \"{}\"", fields.title),
        RowFields::NewDraft(fields) => format!("the new draft \"{}\"", fields.name),
    }
}

/// MPR-FR-NWEZ: the text of the request. It names each failed row, and it
/// asks the agent, by its handle, to read the issues again and to propose
/// again.
pub fn request_body(proposal: &MapPlanProposal, handle: &str) -> String {
    let (changed, other): (Vec<&MapPlanRow>, Vec<&MapPlanRow>) =
        failed_rows(proposal).partition(|r| changed_on_tracker(r));
    let names = |rows: &[&MapPlanRow]| {
        let text = join(&rows.iter().map(|r| subject(r)).collect::<Vec<_>>());
        let mut chars = text.chars();
        chars.next().map(|c| c.to_uppercase().chain(chars).collect::<String>()).unwrap_or_default()
    };
    let mut sentences = Vec::new();
    if !changed.is_empty() {
        sentences.push(format!(
            "{} changed on GitHub after the proposal, so nothing was written.",
            names(&changed)
        ));
    }
    if !other.is_empty() {
        sentences.push(format!("{} could not be applied.", names(&other)));
    }
    let ask = if changed.len() + other.len() == 1 {
        "read this issue again and propose the change again"
    } else {
        "read these issues again and propose the changes again"
    };
    sentences.push(format!("@{handle}, {ask}."));
    sentences.join(" ")
}

/// MPR-FR-QIJF: the handle of the agent that proposed: the author of the
/// comment of the proposal.
fn proposer(store: &RootFs, proposal: &MapPlanProposal) -> Result<String, MapPlanError> {
    let missing = || error(ERR_DISCUSSION_NOT_FOUND, "The comment of the proposal has no agent.");
    let thread = crate::comments::fold_map_discussions(store)
        .into_iter()
        .find(|d| d.id == proposal.discussion_id)
        .ok_or_else(|| error(ERR_DISCUSSION_NOT_FOUND, "The Map discussion does not exist."))?;
    let comment =
        thread.comments.iter().find(|c| c.id == proposal.comment_id).ok_or_else(missing)?;
    match &comment.author {
        Participant::Agent { handle, .. } => Ok(handle.clone()),
        _ => Err(missing()),
    }
}

/// MPR-FR-NWEZ, MPR-FR-QIJF, MPR-FR-LOUI: make the request. It appends the
/// Synthesis comment, dispatches one turn of the agent that proposed with the
/// comment as its trigger, and then records `reprepare_requested_at` in the
/// proposal and writes it. Where the comment cannot be appended or the turn
/// cannot be dispatched, it records no request. A comment that was appended
/// stays in the log.
pub fn request<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &RootFs,
    proposal: &mut MapPlanProposal,
) -> Result<(), MapPlanError> {
    let handle = proposer(store, proposal)?;
    let comment_id = crate::notes::new_note_id();
    let now = crate::notes::now_rfc3339();
    let appended = crate::comments::append_agent_comment_with(
        store,
        ThreadRef::map_discussion(),
        &proposal.discussion_id,
        comment_id.clone(),
        request_body(proposal, &handle),
        Vec::new(),
        Stamp { by: &Participant::synthesis(), at: &now },
    )
    .map_err(|reason| match reason.as_str() {
        crate::comments::ERR_DISCUSSION_RESOLVED => {
            error(crate::comments::ERR_DISCUSSION_RESOLVED, "The discussion is resolved.")
        }
        _ => store_unavailable(),
    })?;
    crate::comments::emit_discussion_changed(app, &appended);
    let turn = TurnRequest {
        discussion_id: proposal.discussion_id.clone(),
        nickname: handle,
        trigger_comment_id: comment_id,
    };
    dispatch_turn(app, &turn)
        .map_err(|failure| error(&failure, "The agent turn did not start."))?;
    proposal.reprepare_requested_at = Some(now);
    if let Err(e) = store::write(store, proposal) {
        proposal.reprepare_requested_at = None;
        return Err(e);
    }
    Ok(())
}

/// MPR-FR-LJRO: the request that a decision makes on its own. It never turns
/// the decision into an error: a failure is logged, and the proposal stays as
/// the decision left it.
pub fn request_automatic<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &RootFs,
    proposal: &mut MapPlanProposal,
) {
    if !wants_automatic(proposal) {
        return;
    }
    match request(app, store, proposal) {
        Ok(()) => log_operation(app, false, "reprepare", "requested", "automatic", ""),
        Err(e) => log_operation(app, true, "reprepare", "failed", "automatic", &e.code),
    }
}

/// MPR-FR-NWEZ: start the turn of the agent in the Map discussion, as a
/// comment that addressed the agent does.
#[cfg(not(test))]
fn dispatch_turn<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turn: &TurnRequest,
) -> Result<(), String> {
    dispatch_agent(app, turn)
}

/// [`dispatch_turn`] through the seam of the test, where the test set one.
#[cfg(test)]
fn dispatch_turn<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turn: &TurnRequest,
) -> Result<(), String> {
    use tauri::Manager;
    match app.try_state::<DispatchSeam>() {
        Some(seam) => (seam.0)(turn),
        None => dispatch_agent(app, turn),
    }
}

fn dispatch_agent<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turn: &TurnRequest,
) -> Result<(), String> {
    use crate::project::OpenProject;
    let no_project = || super::errors::ERR_NO_PROJECT_OPEN.to_string();
    let project = app.open_project_state().ok_or_else(no_project)?;
    let key = project.anchor().ok_or_else(no_project)?;
    let root = project.require_root().map_err(|_| no_project())?;
    let store = project.require_store()?;
    let origin = crate::agent_conversations::ConversationOrigin {
        discussion_id: turn.discussion_id.clone(),
        target: crate::comments::DiscussionTarget::Map,
        fragment_target: None,
    };
    crate::agent_conversations::dispatch_impl(
        app,
        crate::agent_conversations::Roots { worktree: &root, store: &store },
        &key,
        &turn.nickname,
        origin,
        &turn.trigger_comment_id,
    )
    .map(|_| ())
}
