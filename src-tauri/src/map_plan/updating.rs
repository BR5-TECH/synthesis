//! MPR-FR-TMTI to MPR-FR-AESZ: the update of the open proposal of a
//! discussion (MPR-FR-KDTQ), the in-process half that the tool `propose_plan_changes` calls
//! with `updates`. An update changes, adds, and withdraws rows of the proposal
//! that holds a `waiting` row, and appends one comment, as one step. A later
//! call of a turn updates with no comment (PPT-FR-IRFE).

use std::collections::HashSet;

use super::errors::{
    announce, error, log_operation, store_unavailable, ERR_KEY_TAKEN, ERR_KIND_CHANGED,
    ERR_NO_OPEN_PROPOSAL, ERR_NO_ROW_LEFT, ERR_PARENT_WITHDRAWN, ERR_ROW_DECIDED,
    ERR_ROW_NOT_FOUND,
};
use super::model::{
    MapPlanError, MapPlanProposal, MapPlanRow, ProposalComment, RowFields, RowState,
};
use super::owner::ProposalDiscussion;
use super::recording::open_store;
use super::{rules, store};
use crate::comments::{ProposalUpdateCounts, Stamp, ERR_DISCUSSION_RESOLVED};
use crate::fs::RootFs;

/// MPR contract: one update of the open proposal.
#[derive(Clone, Debug, PartialEq)]
pub enum MapPlanUpdate {
    /// A new version of the row with the same `key` and the same kind.
    Change(MapPlanRow),
    /// A new row at the end of the proposal.
    Add(MapPlanRow),
    /// The key of a row that leaves the proposal.
    Withdraw(String),
}

impl MapPlanUpdate {
    fn action(&self) -> &'static str {
        match self {
            MapPlanUpdate::Change(_) => "change",
            MapPlanUpdate::Add(_) => "add",
            MapPlanUpdate::Withdraw(_) => "withdraw",
        }
    }
}

/// MPR-FR-SBYA: the open proposal of the discussion: the latest one that holds
/// a `waiting` row.
pub fn open_proposal_of(proposals: Vec<MapPlanProposal>) -> Option<MapPlanProposal> {
    proposals.into_iter().rev().find(|p| p.rows.iter().any(|r| r.state == RowState::Waiting))
}

/// The open proposal of the discussion, with `blocked_by` computed, or `None`.
pub fn read_open_proposal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    discussion_id: &str,
) -> Option<MapPlanProposal> {
    let store = open_store(app).ok()?;
    open_proposal_of(store::read_for(&store, discussion_id)).map(rules::with_blocks)
}

/// MPR-FR-TMTI, MPR-FR-EAAJ, MPR-FR-AESZ: apply `updates` to the open proposal
/// of the discussion and append the comment of the update, where it carries
/// one, as one step. `discussion` is a Map thread id or the origin of a turn
/// of any discussion (MPR-FR-KDTQ).
pub fn update_map_plan_proposal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    discussion: impl Into<ProposalDiscussion>,
    updates: Vec<MapPlanUpdate>,
    comment: Option<ProposalComment>,
) -> Result<MapPlanProposal, MapPlanError> {
    let discussion = discussion.into();
    let discussion_id = discussion.id.as_str();
    let result = open_store(app).and_then(|store| {
        // MPR-FR-AESZ: an update and a decision never run at the same time.
        let _turn = super::deciding::lock_decisions();
        update_in(app, &store, &discussion, updates, comment)
    });
    match &result {
        Ok((proposal, discussion)) => {
            if let Some(discussion) = discussion {
                crate::comments::emit_discussion_changed(app, discussion);
            }
            announce(app, discussion_id);
            log_operation(app, false, "update", "updated", "proposal", "");
            Ok(rules::with_blocks(proposal.clone()))
        }
        Err(e) => {
            log_operation(app, true, "update", "refused", "proposal", &e.code);
            Err(e.clone())
        }
    }
}

fn update_in<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &RootFs,
    discussion: &ProposalDiscussion,
    updates: Vec<MapPlanUpdate>,
    comment: Option<ProposalComment>,
) -> Result<(MapPlanProposal, Option<crate::comments::Discussion>), MapPlanError> {
    let owner = discussion.owner.as_ref();
    let thread = super::owner::find_thread(store, owner, &discussion.id)?;
    // MPR-FR-NSEP.
    let before = open_proposal_of(store::read_for(store, &thread.id))
        .ok_or_else(|| error(ERR_NO_OPEN_PROPOSAL, "The discussion has no open proposal."))?;
    let (after, counts) = apply_updates(&before, updates)?;
    // MPR-FR-NSEP: a resolved discussion takes no update, also one with no
    // comment.
    if thread.resolved {
        return Err(error(ERR_DISCUSSION_RESOLVED, "The discussion is resolved."));
    }
    store::write(store, &after)?;
    let Some(comment) = comment else { return Ok((after, None)) };
    let now = crate::notes::now_rfc3339();
    let appended = crate::comments::append_map_proposal_comment(
        store,
        super::owner::log_of(owner),
        &thread.id,
        crate::notes::new_note_id(),
        comment.body,
        comment.spoken_original,
        &after.id,
        Some(counts),
        Stamp { by: &comment.author, at: &now },
    );
    match appended {
        Ok(discussion) => Ok((after, Some(discussion))),
        Err(reason) => {
            // MPR-FR-EAAJ: a comment that cannot be appended takes the update back.
            if let Err(e) = store::write(store, &before) {
                log_operation(app, true, "update rollback", "failed", "proposal", &e.code);
            }
            if reason == ERR_DISCUSSION_RESOLVED {
                return Err(error(ERR_DISCUSSION_RESOLVED, "The discussion is resolved."));
            }
            Err(store_unavailable())
        }
    }
}

fn kind_of(fields: &RowFields) -> &'static str {
    rules::kind_name(fields)
}

/// MPR-FR-KSRG: a changed or added row is `waiting`, with no error and no result.
fn as_waiting(mut row: MapPlanRow) -> MapPlanRow {
    row.key = row.key.trim().to_string();
    row.state = RowState::Waiting;
    row.blocked_by = None;
    row.error = None;
    row.result = None;
    row
}

/// MPR-FR-KSRG to MPR-FR-EAAJ: the proposal after `updates`, and the counts of
/// the update. Pure. All or nothing: the first refusal returns, and `before`
/// stays as it was.
pub fn apply_updates(
    before: &MapPlanProposal,
    updates: Vec<MapPlanUpdate>,
) -> Result<(MapPlanProposal, ProposalUpdateCounts), MapPlanError> {
    let mut after = before.clone();
    let mut counts = ProposalUpdateCounts::default();
    let mut withdrawn: HashSet<String> = HashSet::new();
    for update in updates {
        let action = update.action();
        match update {
            MapPlanUpdate::Change(row) => {
                let row = as_waiting(row);
                let slot = after
                    .rows
                    .iter_mut()
                    .find(|r| r.key == row.key)
                    .ok_or_else(|| row_not_found(action))?;
                if !matches!(slot.state, RowState::Waiting | RowState::Rejected) {
                    return Err(row_decided());
                }
                if kind_of(&slot.fields) != kind_of(&row.fields) {
                    return Err(error(ERR_KIND_CHANGED, "A change keeps the kind of the row."));
                }
                *slot = row;
                counts.changed += 1;
            }
            MapPlanUpdate::Add(row) => {
                let row = as_waiting(row);
                if after.rows.iter().any(|r| r.key == row.key) || withdrawn.contains(&row.key) {
                    return Err(error(ERR_KEY_TAKEN, "A row of the proposal holds the key."));
                }
                after.rows.push(row);
                counts.added += 1;
            }
            MapPlanUpdate::Withdraw(key) => {
                let key = key.trim().to_string();
                let at = after
                    .rows
                    .iter()
                    .position(|r| r.key == key)
                    .ok_or_else(|| row_not_found(action))?;
                if !matches!(after.rows[at].state, RowState::Waiting | RowState::Rejected) {
                    return Err(row_decided());
                }
                after.rows.remove(at);
                withdrawn.insert(key);
                counts.withdrawn += 1;
            }
        }
    }
    // MPR-FR-HQXX.
    if after.rows.is_empty() {
        return Err(error(ERR_NO_ROW_LEFT, "The update leaves no row."));
    }
    let names_withdrawn = |row: &MapPlanRow| {
        super::model::parent_row_of(&row.fields).is_some_and(|key| withdrawn.contains(key))
    };
    if after.rows.iter().any(names_withdrawn) {
        return Err(error(ERR_PARENT_WITHDRAWN, "A row names a withdrawn row as its parent."));
    }
    Ok((after, counts))
}

fn row_not_found(action: &str) -> MapPlanError {
    error(ERR_ROW_NOT_FOUND, &format!("The {action} names no row of the proposal."))
}

fn row_decided() -> MapPlanError {
    error(ERR_ROW_DECIDED, "The row is accepted or failed.")
}
