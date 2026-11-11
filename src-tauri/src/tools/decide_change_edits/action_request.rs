//! The entries of `decide_change_edits` that name the pending change action
//! request of the change (`DCE-decide-change-edits-tool.md` DCE-FR-UZDN,
//! DCE-FR-IZTA, DCE-FR-UVUT, DCE-FR-QUKG).
//!
//! The entry calls the functions behind the card's commands in-process, with
//! no feedback (`CAR-change-action-requests.md` CAR-FR-RKRY). The call is
//! already authorized by the author's comment when an entry runs.

use super::*;
use crate::tools::decide_changes::{DecisionRow, Outcome, RefusalCode};

/// The `invalid_action` text of an action request entry.
pub const INVALID_REQUEST_ACTION: &str =
    "the action must be `confirm` or `reject` for a request to apply or rework the change.";

/// DCE-FR-IZTA / DCE-FR-UVUT: run one entry that names the pending request.
pub(super) fn entry<R: tauri::Runtime>(
    backend: &ChangeBackend<'_, R>,
    id: &str,
    action: &str,
    place: usize,
    total: usize,
) -> DecisionRow
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let mut row = DecisionRow::new(place, id, action);
    let confirm = match action {
        "confirm" => true,
        "reject" => false,
        _ => {
            row.refuse(RefusalCode::InvalidAction, total, INVALID_REQUEST_ACTION, None);
            return row;
        }
    };
    let ran = backend.author.clone().and_then(|by| {
        if confirm {
            car::confirm_action_request(
                &backend.tool.app,
                backend.store(),
                backend.starter,
                id,
                &by,
            )
        } else {
            car::decline_action_request(&backend.tool.app, backend.store(), id, None, &by)
        }
    });
    match ran {
        Ok(outcome) => {
            row.outcome = if confirm { Outcome::Started } else { Outcome::Declined };
            row.run_id = outcome.run.map(|run| run.id);
        }
        Err(refusal) => {
            // DCE-FR-UVUT: the code alone, so no detail of a start reaches the
            // model or the log.
            let code = car::refusal_code(&refusal).to_string();
            let retryable = code != car::ERR_UNKNOWN_STREAM && code != car::ERR_CHANGE_NOT_FOUND;
            let text = format!("the decision could not be made ({code}).");
            row.refuse(RefusalCode::DecisionFailed, total, &text, Some(code));
            if let Some(refusal) = row.refusal.as_mut() {
                refusal.retryable = retryable;
            }
        }
    }
    row
}
