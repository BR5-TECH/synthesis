//! MPR-FR-QUYY to MPR-FR-SKLA, MPR-FR-HKYH: the decision on one row. A decision
//! changes that row alone (MPR-FR-MQEQ). One decision runs at a time for the
//! open project, in the order the decisions arrive.

use super::applying::{apply_row_kind, Applied};
use super::errors::{
    announce, error, log_operation, no_project, ERR_PROPOSAL_NOT_FOUND, ERR_ROW_DECIDED,
    ERR_ROW_NOT_FOUND,
};
use super::model::{MapPlanError, MapPlanProposal, RowState};
use super::recording::open_store;
use super::{rules, store};
use crate::fs::RootFs;
use crate::project::OpenProject;

/// MPR-FR-HKYH: one decision at a time. The lock is held across the write to
/// GitHub, so a second decision on the same row waits, and then finds it decided.
static DECISIONS: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// MPR-FR-HKYH: take the turn of a decision. The lock guards no state in memory,
/// because the files are the state, so a lock that a panic poisoned is safe to
/// take again.
pub(super) fn lock_decisions() -> std::sync::MutexGuard<'static, ()> {
    DECISIONS.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The decision that the author makes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Decision {
    Accept,
    Reject,
}

impl Decision {
    fn name(self) -> &'static str {
        match self {
            Decision::Accept => "accept",
            Decision::Reject => "reject",
        }
    }
}

/// MPR-FR-YCHW to MPR-FR-VMXC: accept one row. The row is applied, and it
/// becomes `accepted` or `failed`. A refusal changes nothing.
pub fn accept_row<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    discussion_id: &str,
    proposal_id: &str,
    row_key: &str,
) -> Result<MapPlanProposal, MapPlanError> {
    decide(app, discussion_id, proposal_id, row_key, Decision::Accept)
}

/// MPR-FR-SKLA: reject one row. It writes nothing to GitHub, creates no draft,
/// and makes no network request.
pub fn reject_row<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    discussion_id: &str,
    proposal_id: &str,
    row_key: &str,
) -> Result<MapPlanProposal, MapPlanError> {
    decide(app, discussion_id, proposal_id, row_key, Decision::Reject)
}

fn decide<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    discussion_id: &str,
    proposal_id: &str,
    row_key: &str,
    decision: Decision,
) -> Result<MapPlanProposal, MapPlanError> {
    let mut kind = "";
    let result = open_store(app).and_then(|store| {
        let _turn = lock_decisions();
        decide_in(app, &store, discussion_id, proposal_id, row_key, decision, &mut kind)
    });
    match &result {
        Ok(proposal) => {
            let row = proposal.rows.iter().find(|r| r.key == row_key);
            let outcome = match row.map(|r| r.state) {
                Some(RowState::Accepted) => "accepted",
                Some(RowState::Rejected) => "rejected",
                Some(RowState::Failed) => "failed",
                _ => "waiting",
            };
            let code = row.and_then(|r| r.error.as_ref()).map(|e| e.code.as_str()).unwrap_or("");
            log_operation(app, false, decision.name(), outcome, kind, code);
            announce(app, discussion_id);
        }
        Err(e) => log_operation(app, true, decision.name(), "refused", kind, &e.code),
    }
    result.map(rules::with_blocks)
}

fn decide_in<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &RootFs,
    discussion_id: &str,
    proposal_id: &str,
    row_key: &str,
    decision: Decision,
    kind: &mut &'static str,
) -> Result<MapPlanProposal, MapPlanError> {
    let mut proposal = store::find(store, proposal_id)
        .filter(|p| p.discussion_id == discussion_id)
        .ok_or_else(|| error(ERR_PROPOSAL_NOT_FOUND, "The proposal does not exist."))?;
    let at = proposal
        .rows
        .iter()
        .position(|r| r.key == row_key)
        .ok_or_else(|| error(ERR_ROW_NOT_FOUND, "The proposal has no such row."))?;
    *kind = rules::kind_name(&proposal.rows[at].fields);
    // MPR-FR-QUYY: a decided row never changes state again.
    if proposal.rows[at].state != RowState::Waiting {
        return Err(error(ERR_ROW_DECIDED, "The row is decided already."));
    }
    match decision {
        Decision::Reject => proposal.rows[at].state = RowState::Rejected,
        Decision::Accept => {
            // MPR-FR-YLKL: a blocked row is refused, and nothing changes.
            if let Some(block) = rules::block_of(&proposal.rows, at) {
                return Err(super::errors::parent_not_accepted(block.reason));
            }
            let project = app.open_project_state().ok_or_else(no_project)?;
            match apply_row_kind(app, &project, store, &proposal, at) {
                Applied::Refused(refusal) => return Err(refusal),
                Applied::Done(result) => {
                    let row = &mut proposal.rows[at];
                    row.state = RowState::Accepted;
                    row.result = Some(result);
                }
                Applied::Failed(failure) => {
                    let row = &mut proposal.rows[at];
                    row.state = RowState::Failed;
                    row.error = Some(failure);
                }
            }
        }
    }
    store::write(store, &proposal)?;
    Ok(proposal)
}
