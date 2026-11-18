//! MPR-FR-SBYA, MPR-FR-RVRN: the in-process half that the tool
//! `propose_plan_changes` calls.

use super::errors::{
    announce, error, log_operation, no_project, store_unavailable, ERR_DISCUSSION_NOT_FOUND,
    ERR_PROPOSAL_OPEN,
};
use super::model::{MapPlanError, MapPlanProposal, MapPlanRow, ProposalComment, RowState};
use super::{rules, store};
use crate::comments::{Stamp, ERR_DISCUSSION_RESOLVED};
use crate::fs::RootFs;
use crate::project::OpenProject;

/// The comments store of the open project.
pub(super) fn open_store<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<RootFs, MapPlanError> {
    let project = app.open_project_state().ok_or_else(no_project)?;
    project.require_root().map_err(|_| no_project())?;
    project.require_store().map_err(|_| store_unavailable())
}

/// MPR-FR-SBYA: whether the Map discussion has an open row, that is a row of
/// any of its proposals in the state `waiting`.
pub fn open_row_exists<R: tauri::Runtime>(app: &tauri::AppHandle<R>, discussion_id: &str) -> bool {
    let Ok(store) = open_store(app) else { return false };
    rules::has_open_row(&store::read_for(&store, discussion_id))
}

/// MPR-FR-TQVZ, MPR-FR-RVRN, MPR-FR-SBYA: records a proposal and appends its one
/// agent comment as one step. Refuses with `proposal_open` where the discussion
/// has an open row. The tool validated the rows already.
pub fn record_map_plan_proposal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    discussion_id: &str,
    rows: Vec<MapPlanRow>,
    comment: ProposalComment,
) -> Result<MapPlanProposal, MapPlanError> {
    let result = open_store(app).and_then(|store| {
        // MPR-FR-HKYH: a recording waits for a decision that runs, so the check
        // of the open row and the write see the same rows.
        let _turn = super::deciding::lock_decisions();
        record_in(&store, discussion_id, rows, comment)
    });
    match &result {
        Ok((proposal, discussion)) => {
            crate::comments::emit_discussion_changed(app, discussion);
            announce(app, discussion_id);
            log_operation(app, false, "record", "recorded", "proposal", "");
            Ok(rules::with_blocks(proposal.clone()))
        }
        Err(e) => {
            log_operation(app, true, "record", "refused", "proposal", &e.code);
            Err(e.clone())
        }
    }
}

/// MPR-FR-RVRN: the record and the comment, or neither. The record is written
/// first, because the comment names it. A comment that cannot be appended takes
/// the record back.
fn record_in(
    store: &RootFs,
    discussion_id: &str,
    rows: Vec<MapPlanRow>,
    comment: ProposalComment,
) -> Result<(MapPlanProposal, crate::comments::Discussion), MapPlanError> {
    let thread = crate::comments::fold_map_discussions(store)
        .into_iter()
        .find(|d| d.id == discussion_id)
        .ok_or_else(|| error(ERR_DISCUSSION_NOT_FOUND, "The Map discussion does not exist."))?;
    if rules::has_open_row(&store::read_for(store, &thread.id)) {
        return Err(error(
            ERR_PROPOSAL_OPEN,
            "The discussion has a row that waits for a decision.",
        ));
    }
    let now = crate::notes::now_rfc3339();
    let proposal = MapPlanProposal {
        id: crate::notes::new_note_id(),
        discussion_id: thread.id.clone(),
        comment_id: crate::notes::new_note_id(),
        created_at: now.clone(),
        reprepare_requested_at: None,
        rows: rows.into_iter().map(as_new_row).collect(),
    };
    store::write(store, &proposal)?;
    let appended = crate::comments::append_map_proposal_comment(
        store,
        &thread.id,
        proposal.comment_id.clone(),
        comment.body,
        comment.spoken_original,
        &proposal.id,
        None,
        Stamp { by: &comment.author, at: &now },
    );
    match appended {
        Ok(discussion) => Ok((proposal, discussion)),
        Err(reason) => {
            store::remove(store, &proposal.id);
            if reason == ERR_DISCUSSION_RESOLVED {
                return Err(error(ERR_DISCUSSION_RESOLVED, "The discussion is resolved."));
            }
            Err(store_unavailable())
        }
    }
}

/// MPR-FR-QUYY: a new row is `waiting`, with no error and no result.
fn as_new_row(mut row: MapPlanRow) -> MapPlanRow {
    row.state = RowState::Waiting;
    row.blocked_by = None;
    row.error = None;
    row.result = None;
    row
}
