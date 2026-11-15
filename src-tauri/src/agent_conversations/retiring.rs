//! Retiring the outstanding turns of a conversation (AGC-FR-31, AGC-FR-TQLC).

use super::*;

/// AGC-FR-31: a later **human** comment in this conversation retires its offer
/// to retry.
///
/// Called from the human write path alone (CMS-FR-11's acting participant), so
/// an agent's own answer retires nothing: an agent answering elsewhere in the
/// thread does not mean the author has stopped wanting the answer that failed.
pub fn retire_recoverable_for_human_comment<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    thread_id: &str,
) {
    use tauri::Manager;
    let Some(turns) = app.try_state::<TurnRegistry>() else {
        return;
    };
    let Some(turn) = turns.retire_recoverable(thread_id) else {
        return;
    };
    logging::log_info(
        app,
        turns.buffer(),
        &[Domain::Ai, Domain::Backend],
        "an offer to retry was retired by a later comment",
        log_fields! {
            "turnId" => &turn.id,
            "agent" => &turn.nickname,
            "discussionId" => thread_id,
        },
    );
}

/// AGC-FR-TQLC: retire the turns a discussion has outstanding, because its
/// question set has been answered.
///
/// A submission retires or discards the asking turn **whether or not that agent
/// is among the agents the submission dispatches to**. The ordinary retirement
/// is per agent at dispatch (`dispatch.rs`), which would leave an agent that
/// asked and was not dispatched to awaiting a reply for good — the answers reach
/// the agents as fresh later turns and never by resuming the turn that asked
/// (ADQ-FR-ZXAF).
///
/// Called from the submission alone, on its success path.
pub fn retire_awaiting_for_thread<R: tauri::Runtime>(app: &tauri::AppHandle<R>, thread_id: &str) {
    use tauri::Manager;
    let Some(turns) = app.try_state::<TurnRegistry>() else {
        return;
    };
    let retired = turns.retire_awaiting_on_thread(thread_id);
    if retired == 0 {
        return;
    }
    logging::log_info(
        app,
        turns.buffer(),
        &[Domain::Ai, Domain::Backend],
        "turns awaiting a question set were retired by its answers",
        log_fields! { "discussionId" => thread_id, "turns" => retired as u64 },
    );
}
