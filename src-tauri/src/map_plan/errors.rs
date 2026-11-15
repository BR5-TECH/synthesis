//! The typed errors of the plan proposals (`MPR-map-plan-proposals.md`, Contract
//! surface) and the events and logs of the module.

use serde_json::json;
use tauri::Emitter;

use super::model::{MapPlanError, RowState};
use crate::logging::{Domain, BUFFER};

pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";
pub const ERR_PROPOSAL_NOT_FOUND: &str = "proposal_not_found";
pub const ERR_ROW_NOT_FOUND: &str = "row_not_found";
pub const ERR_ROW_DECIDED: &str = "row_decided";
pub const ERR_PARENT_NOT_ACCEPTED: &str = "parent_not_accepted";
pub const ERR_PROPOSAL_OPEN: &str = "proposal_open";
pub const ERR_DRAFT_CREATE_FAILED: &str = "draft_create_failed";
pub const ERR_DISCUSSION_NOT_FOUND: &str = "discussion_not_found";
pub const ERR_STORE_UNAVAILABLE: &str = "store_unavailable";
/// A stored row whose change cannot be read as a write.
pub const ERR_ROW_INVALID: &str = "row_invalid";

/// MPR-FR-SGYA: the Tauri event of a change of a proposal. Kebab-case, because
/// Tauri refuses a space in an event name at `emit`.
pub const MAP_PLAN_PROPOSALS_CHANGED: &str = "map-plan-proposals-changed";

pub fn error(code: &str, message: &str) -> MapPlanError {
    MapPlanError { code: code.to_string(), message: message.to_string() }
}

pub fn no_project() -> MapPlanError {
    error(ERR_NO_PROJECT_OPEN, "No project is open.")
}

pub fn store_unavailable() -> MapPlanError {
    error(ERR_STORE_UNAVAILABLE, "The store of the plan proposals is not available.")
}

/// MPR-FR-YLKL: the refusal of a row whose parent row is not accepted. The
/// message names the reason: `waiting`, `rejected`, or `failed`.
pub fn parent_not_accepted(reason: RowState) -> MapPlanError {
    let word = match reason {
        RowState::Waiting => "waiting",
        RowState::Rejected => "rejected",
        RowState::Failed => "failed",
        RowState::Accepted => "accepted",
    };
    error(ERR_PARENT_NOT_ACCEPTED, &format!("The parent row is {word}."))
}

/// MPR-FR-SGYA: announce a change of a proposal of the discussion. A dropped
/// event costs a redraw, and the next read corrects it.
pub fn announce<R: tauri::Runtime>(app: &tauri::AppHandle<R>, discussion_id: &str) {
    let _ = app.emit(MAP_PLAN_PROPOSALS_CHANGED, json!({ "discussionId": discussion_id }));
}

/// MPR-FR-KZRN: one record of an operation. It names the operation, the
/// outcome, the row kind, and the typed error. It never carries a title, a
/// body, a label name, a draft name, or a token.
pub fn log_operation<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    refused: bool,
    operation: &str,
    outcome: &str,
    kind: &str,
    code: &str,
) {
    let fields = crate::log_fields! {
        "operation" => operation,
        "outcome" => outcome,
        "kind" => kind,
        "error" => code,
    };
    if refused {
        crate::logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "map plan operation refused",
            fields,
        );
    } else {
        crate::logging::log_info(
            app,
            &BUFFER,
            &[Domain::Backend],
            "map plan operation done",
            fields,
        );
    }
}
