//! Cancellation: one turn, an agent's turns, a note's turns, or all of them.

use super::*;

/// AGC-FR-20: abandon the model call, terminate the turn `cancelled`, and append
/// nothing. Succeeds against a turn that has already terminated without changing
/// it.
pub fn cancel_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    progress_registry: &ProgressRegistry,
    turn_id: &str,
) -> Result<AgentTurn, String> {
    let Some(flag) = turns.flag(turn_id) else {
        // AGC-FR-20: succeeds against a turn that has already terminated,
        // without changing it. A cancellation racing a delivery therefore reads
        // back `delivered` rather than as an error, and a second cancel of the
        // same turn is a no-op rather than a failure.
        return turns
            .terminated(turn_id)
            .ok_or_else(|| ERR_TURN_NOT_FOUND.to_string());
    };
    flag.store(true, Ordering::SeqCst);
    let Some((turn, operation, _project_key, reached_model)) =
        turns.terminate(turn_id, AgentTurnState::Cancelled, None)
    else {
        // The turn committed to delivering between the two lines above. It is
        // still in flight and will terminate itself; nothing here changes it.
        return turns
            .terminated(turn_id)
            .or_else(|| turns.in_flight(None).into_iter().find(|t| t.id == turn_id))
            .ok_or_else(|| ERR_TURN_NOT_FOUND.to_string());
    };
    log_turn_ended(app, turns.buffer(), &turn, TurnCounts::default());
    // AGC-FR-YQMD: a cancellation terminates the turn here rather than through
    // `finish`, so the one lifecycle line the turn leaves is recorded here too.
    // Whether it reached the model is read from the registry's own flag rather
    // than assumed: a turn abandoned mid-call did reach one.
    record_refinement_lifecycle(app, &turn, reached_model);
    if let Some(operation) = operation {
        progress::terminate_and_publish(
            app,
            progress_registry,
            operation,
            OperationState::Cancelled,
        );
    }
    publish(app, &turn);
    Ok(turn)
}

/// AGR-FR-11 / AGR-FR-15: an agent deleted from the registry, or withdrawn from
/// a project, stops answering. `project_key` narrows the cancellation to one
/// project — a withdrawal is from this conversation, not from the machine.
pub fn cancel_turns_for_agent<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    progress_registry: &ProgressRegistry,
    agent_id: &str,
    project_key: Option<&str>,
) {
    let turn_ids = turns.cancellation_flags_for_agent(agent_id, project_key);
    // The per-turn records below say only that a turn was cancelled; this is the
    // one that says why an agent stopped answering everywhere at once. Silent
    // when nothing was in flight, which is the common case for a registry edit.
    if !turn_ids.is_empty() {
        logging::log_info(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "cancelling turns for a withdrawn agent",
            log_fields! { "agentId" => agent_id, "turns" => turn_ids.len() },
        );
    }
    for turn_id in turn_ids {
        let _ = cancel_impl(app, turns, progress_registry, &turn_id);
    }
    // AGC-FR-28: an agent that is gone is owed nothing, so what this project was
    // holding for it goes with it. **After** the cancellations rather than
    // before: a turn already inside its ask terminates `awaiting_reply` on its
    // own thread, and a discard that ran first would be overtaken by the
    // registration it was meant to prevent. Cancelling first makes that
    // impossible — a cancelled turn is out of the live set, so its own
    // termination finds nothing to park — and this sweeps whatever was already
    // waiting.
    let discarded = turns.discard_awaiting_for_agent(agent_id, project_key);
    // AGC-FR-31: an agent that is gone is offered nothing either, so its
    // recovery entries go with its awaiting registrations.
    turns.discard_recoverable_for_agent(agent_id, project_key);
    if discarded > 0 {
        logging::log_info(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "discarding questions a withdrawn agent was owed answers to",
            log_fields! { "agentId" => agent_id, "awaiting" => discarded },
        );
    }
}

/// The note a conversation belongs to, for the sweeps of AGC-FR-30.
pub(super) fn origin_note_id(origin: &ConversationOrigin) -> Option<&str> {
    origin.note_id()
}

/// AGC-FR-30: a turn whose note has been deleted is cancelled and contributes
/// nothing, on exactly the terms a turn is cancelled when the project closes.
///
/// Called after the deletion transaction has committed (NTC-FR-21), so a
/// cleanup that failed cancels nothing and the conversation is still there to
/// answer in. The conversation this turn would have written into is gone with
/// the note, so an answer arriving afterwards has nowhere to land — cancelling
/// is what stops it trying and logging a failure the author cannot act on.
pub fn cancel_turns_for_note<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    progress_registry: &ProgressRegistry,
    note_id: &str,
) {
    let turn_ids = turns.cancellation_flags_for_note(note_id);
    if !turn_ids.is_empty() {
        logging::log_info(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "cancelling turns for a deleted note",
            log_fields! { "noteId" => note_id, "turns" => turn_ids.len() },
        );
    }
    for turn_id in turn_ids {
        let _ = cancel_impl(app, turns, progress_registry, &turn_id);
    }
    // After the cancellations, for the reason `cancel_turns_for_agent` sweeps
    // after its own: a turn already inside its ask parks a registration on its
    // own thread, and a discard that ran first would be overtaken by it.
    let discarded = turns.discard_awaiting_for_note(note_id);
    // AGC-FR-31: an offer to retry in a conversation gone with its note is an
    // offer nothing could take.
    turns.discard_recoverable_for_note(note_id);
    if discarded > 0 {
        logging::log_info(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "discarding questions owed in a deleted note's conversation",
            log_fields! { "noteId" => note_id, "awaiting" => discarded },
        );
    }
}

/// AGC-FR-20: a turn in flight when the project closes terminates `cancelled`.
pub fn cancel_all_turns<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    progress_registry: &ProgressRegistry,
) {
    let turn_ids = turns.all_turn_ids();
    if !turn_ids.is_empty() {
        // Emitted before the clear that a project close performs (LGC-FR-15) is
        // not a concern here: this runs during the close, so the record lands in
        // the fresh buffer and explains why a conversation stopped mid-answer.
        logging::log_info(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "cancelling every agent turn in flight",
            log_fields! { "turns" => turn_ids.len() },
        );
    }
    for turn_id in turn_ids {
        let _ = cancel_impl(app, turns, progress_registry, &turn_id);
    }
    // AGC-FR-28: the registrations die with the project, which holds no
    // conversation for one to be owed in. Swept after the cancellations for the
    // reason `cancel_turns_for_agent` sweeps after its own: a turn mid-ask would
    // otherwise park a registration behind an earlier clear, and
    // `list_agent_turns` would go on returning it for a project that is closed.
    let discarded = turns.clear_awaiting();
    // AGC-FR-31: a closing project holds no conversation for an offer to be
    // made in.
    turns.clear_recoverable();
    if discarded > 0 {
        logging::log_info(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "discarding questions awaiting an answer",
            log_fields! { "awaiting" => discarded },
        );
    }
}
