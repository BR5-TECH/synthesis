//! MPR-FR-IANE to MPR-FR-VNDV: the follow-up turn. When the author has decided
//! every row of a proposal and accepted at least one, the module gives the
//! agent that proposed one turn with the results, so that it can propose the
//! next step that depends on them, or say that the step is done.
//!
//! The request is one Synthesis comment in the Map discussion and one turn of
//! that agent, on the terms of the re-prepare request (MPR-FR-QIJF,
//! MPR-FR-LOUI). It runs inside the turn of a decision (MPR-FR-HKYH).

use std::collections::HashSet;
use std::sync::Mutex;

use super::errors::{announce, error, store_unavailable};
use super::model::{MapPlanError, MapPlanProposal, MapPlanRow, RowFields, RowState};
use super::recording::open_store;
use super::reprepare::{dispatch_turn, join, proposer, TurnRequest};
use super::store;
use crate::comments::{Participant, Stamp, ThreadRef};
use crate::delivery::github::ERR_CHANGED_ON_TRACKER;
use crate::fs::RootFs;
use crate::logging::{Domain, BUFFER};

/// MPR-FR-BCKW: the proposals whose follow-up waits for a running turn of the
/// agent that proposed. Kept in memory only: a deferral ends with the session.
static DEFERRED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn deferred() -> std::sync::MutexGuard<'static, Option<HashSet<String>>> {
    DEFERRED.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A test sets this state to say whether a turn of the agent runs, without a
/// turn registry. The application never sets it.
#[cfg(test)]
pub struct TurnRunsSeam(pub std::sync::Arc<std::sync::atomic::AtomicBool>);

/// MPR-FR-BCKW: whether a turn of the agent `handle` runs in the discussion.
fn turn_runs<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    discussion_id: &str,
    handle: &str,
) -> bool {
    #[cfg(test)]
    if let Some(seam) = tauri::Manager::try_state::<TurnRunsSeam>(app) {
        return seam.0.load(std::sync::atomic::Ordering::SeqCst);
    }
    crate::agent_conversations::agent_turn_runs(app, discussion_id, handle)
}

/// MPR-FR-IANE: whether the proposal wants its follow-up now. No row waits, a
/// row is accepted, no row failed with `changed_on_tracker`, and the proposal
/// records no re-prepare request and no follow-up request.
pub fn wants_follow_up(proposal: &MapPlanProposal) -> bool {
    let rows = &proposal.rows;
    proposal.reprepare_requested_at.is_none()
        && proposal.follow_up_requested_at.is_none()
        && !rows.iter().any(|r| r.state == RowState::Waiting)
        && rows.iter().any(|r| r.state == RowState::Accepted)
        && !rows.iter().any(|r| {
            r.state == RowState::Failed
                && r.error.as_ref().is_some_and(|e| e.code == ERR_CHANGED_ON_TRACKER)
        })
}

/// The title of the issue of a row, or the name of its draft.
fn title_of(row: &MapPlanRow) -> String {
    match &row.fields {
        RowFields::CreateIssue(fields) => fields.title.clone(),
        RowFields::ModifyIssue(fields) => fields.title.clone(),
        RowFields::NewDraft(fields) => fields.name.clone(),
    }
}

/// MPR-FR-LAAK: the name of one decided row in the comment. An accepted issue
/// row names its number and its URL, so the agent can name it as a parent.
fn subject(row: &MapPlanRow) -> String {
    let title = title_of(row);
    let result = row.result.as_ref();
    match (&row.fields, row.state) {
        (RowFields::NewDraft(_), _) => format!("the new draft \"{title}\""),
        (_, RowState::Accepted) => {
            let number = result.and_then(|r| r.issue_number);
            let url = result.and_then(|r| r.issue_url.clone());
            match (number, url) {
                (Some(n), Some(url)) => format!("#{n} \"{title}\" ({url})"),
                (Some(n), None) => format!("#{n} \"{title}\""),
                _ => format!("\"{title}\""),
            }
        }
        (RowFields::ModifyIssue(fields), _) => format!("#{} \"{title}\"", fields.number),
        (RowFields::CreateIssue(_), _) => format!("the new issue \"{title}\""),
    }
}

/// MPR-FR-LAAK: the text of the follow-up. It names each accepted, rejected,
/// and failed row, and it asks the agent, by its handle, to propose the next
/// step or to say that the step is done.
pub fn follow_up_body(proposal: &MapPlanProposal, handle: &str) -> String {
    let named = |state: RowState| -> Vec<String> {
        proposal.rows.iter().filter(|r| r.state == state).map(subject).collect()
    };
    let failed: Vec<String> = proposal
        .rows
        .iter()
        .filter(|r| r.state == RowState::Failed)
        .map(|r| {
            let code = r.error.as_ref().map_or("failed", |e| e.code.as_str());
            format!("{} ({code})", subject(r))
        })
        .collect();
    let mut sentences = vec!["The author decided every row of the proposal.".to_string()];
    for (label, rows) in [
        ("Accepted", named(RowState::Accepted)),
        ("Rejected", named(RowState::Rejected)),
        ("Failed", failed),
    ] {
        if !rows.is_empty() {
            sentences.push(format!("{label}: {}.", join(&rows)));
        }
    }
    sentences.push(format!(
        "@{handle}, propose the next step that depends on these, or say in one or two sentences that this step is done. You do not have to propose."
    ));
    sentences.join(" ")
}

/// The counts of accepted, rejected, and failed rows, for the log record.
fn counts(proposal: &MapPlanProposal) -> (usize, usize, usize) {
    let count = |state: RowState| proposal.rows.iter().filter(|r| r.state == state).count();
    (count(RowState::Accepted), count(RowState::Rejected), count(RowState::Failed))
}

/// MPR-FR-VNDV: one record of the follow-up, with its outcome and the counts
/// of the rows. No record carries a title, a body, or a URL.
fn log_follow_up<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    proposal: &MapPlanProposal,
    outcome: &str,
    code: &str,
) {
    let (accepted, rejected, failed) = counts(proposal);
    let fields = crate::log_fields! {
        "operation" => "follow_up",
        "outcome" => outcome,
        "accepted" => accepted,
        "rejected" => rejected,
        "failed" => failed,
        "error" => code,
    };
    if outcome == "failed" {
        crate::logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "map plan follow-up failed",
            fields,
        );
    } else {
        crate::logging::log_info(app, &BUFFER, &[Domain::Backend], "map plan follow-up", fields);
    }
}

/// MPR-FR-LAAK, MPR-FR-LDJH, MPR-FR-LOUI: make the request. It appends the
/// Synthesis comment, dispatches one turn of the agent that proposed with the
/// comment as its trigger, and then records `follow_up_requested_at`. Where
/// the comment cannot be appended or the turn cannot be dispatched, it records
/// nothing. A comment that was appended stays in the log.
fn request<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &RootFs,
    proposal: &mut MapPlanProposal,
    handle: String,
) -> Result<(), MapPlanError> {
    let comment_id = crate::notes::new_note_id();
    let now = crate::notes::now_rfc3339();
    let appended = crate::comments::append_agent_comment_with(
        store,
        ThreadRef::map_discussion(),
        &proposal.discussion_id,
        comment_id.clone(),
        follow_up_body(proposal, &handle),
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
    proposal.follow_up_requested_at = Some(now);
    if let Err(e) = store::write(store, proposal) {
        proposal.follow_up_requested_at = None;
        return Err(e);
    }
    announce(app, &proposal.discussion_id);
    Ok(())
}

/// MPR-FR-IANE, MPR-FR-BCKW: the follow-up of `proposal` where it wants one.
/// A running turn of the agent that proposed defers it. A failure is logged,
/// and the proposal stays as the decision left it.
fn request_if_wanted<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &RootFs,
    proposal: &mut MapPlanProposal,
) {
    if !wants_follow_up(proposal) {
        return;
    }
    let handle = match proposer(store, proposal) {
        Ok(handle) => handle,
        Err(e) => {
            // A proposal with no agent never gets a follow-up, so it is not
            // checked again at the next turn end.
            if let Some(set) = deferred().as_mut() {
                set.remove(&proposal.id);
            }
            return log_follow_up(app, proposal, "failed", &e.code);
        }
    };
    if turn_runs(app, &proposal.discussion_id, &handle) {
        deferred().get_or_insert_with(HashSet::new).insert(proposal.id.clone());
        return log_follow_up(app, proposal, "deferred", "");
    }
    if let Some(set) = deferred().as_mut() {
        set.remove(&proposal.id);
    }
    match request(app, store, proposal, handle) {
        Ok(()) => log_follow_up(app, proposal, "requested", ""),
        Err(e) => log_follow_up(app, proposal, "failed", &e.code),
    }
}

/// MPR-FR-IANE: the follow-up that a decision of the author asks for.
pub(super) fn request_after_decision<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &RootFs,
    proposal: &mut MapPlanProposal,
) {
    request_if_wanted(app, store, proposal);
}

/// MPR-FR-BCKW: a turn of the Map discussion terminated. Each follow-up of the
/// discussion that a running turn deferred is checked again, under the turn of
/// a decision.
pub fn request_deferred<R: tauri::Runtime>(app: &tauri::AppHandle<R>, discussion_id: &str) {
    let waiting: Vec<String> =
        deferred().as_ref().map(|set| set.iter().cloned().collect()).unwrap_or_default();
    if waiting.is_empty() {
        return;
    }
    let Ok(store) = open_store(app) else { return };
    let _turn = super::deciding::lock_decisions();
    for id in waiting {
        let Some(mut proposal) = store::find(&store, &id) else {
            if let Some(set) = deferred().as_mut() {
                set.remove(&id);
            }
            continue;
        };
        if proposal.discussion_id != discussion_id {
            continue;
        }
        if !wants_follow_up(&proposal) {
            if let Some(set) = deferred().as_mut() {
                set.remove(&id);
            }
            continue;
        }
        request_if_wanted(app, &store, &mut proposal);
    }
}

/// Empty the deferrals, for a test that must not see those of another.
#[cfg(test)]
pub fn reset_deferred_for_test() {
    *deferred() = None;
}
