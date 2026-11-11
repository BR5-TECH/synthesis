//! The action requests of an OpenSpec change in `discussion_history`
//! (`AGC-agent-conversations.md` AGC-FR-KRMV).
//!
//! An agent that asked the author to apply or rework the change needs to know
//! whether the author decided, and what became of the run, so it does not ask
//! again and can name the request when the author tells it to decide.

use super::*;
use crate::change_action_requests::RequestForTurn;

/// AGC-FR-KRMV: the most requests the section lists.
pub(in crate::agent_conversations) const ACTION_REQUESTS_LISTED: usize = 10;

/// One request: its id, its action, its state, and its run where one started.
fn summary(entry: &RequestForTurn) -> String {
    let request = &entry.request;
    let run = match (&request.run_id, &entry.run_state) {
        (Some(run_id), Some(state)) => format!(" — run {run_id} {state}"),
        (Some(run_id), None) => format!(" — run {run_id}"),
        (None, _) => String::new(),
    };
    format!("{} — {} — {}{run}", request.id, request.action.as_str(), request.state.as_str())
}

/// AGC-FR-KRMV: the action requests that end `discussion_history`, before the
/// split, or `None` where the target is no change or holds no request.
pub(in crate::agent_conversations) fn action_requests_text(
    roots: Roots<'_>,
    target: &DiscussionTarget,
) -> Option<String> {
    let (stream_id, change) = target.change_owner()?;
    let requests =
        crate::change_action_requests::read_change_action_requests(roots.store, stream_id, change);
    if requests.is_empty() {
        return None;
    }
    let mut text = String::from(
        "\nThe requests to apply or rework this change, the ten newest, newest first:\n",
    );
    for entry in requests.iter().take(ACTION_REQUESTS_LISTED) {
        text.push_str(&format!("  {}\n", summary(entry)));
    }
    Some(text)
}

/// The line a `change_action_request` attachment contributes: the action it
/// asked for and the id of the request.
pub(in crate::agent_conversations) fn action_request_line(
    attachment: &comments::Attachment,
) -> String {
    match attachment {
        comments::Attachment::ChangeActionRequest { request_id, action, .. } => {
            format!("[asked the author to {action} this change: request {request_id}]")
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests;
