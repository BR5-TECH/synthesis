//! How a turn tells a surface what it is doing.
//!
//! The state event every surface listens for, and the progress operations a
//! tool call opens and closes around itself.

use super::*;

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// AGC-FR-21: publish a turn's state. Emission failure is discarded for the
/// reason every other channel discards it — an event must never take down the
/// work it reports on.
pub(super) fn publish<R: tauri::Runtime>(app: &tauri::AppHandle<R>, turn: &AgentTurn) {
    let _ = app.emit(AGENT_TURN_STATE_CHANGED, turn);
}

/// CVL-FR-33 / AGC-FR-34: report that a tool call has **begun** in this turn,
/// and answer with the id it was given so its finish can be reported against it.
///
/// What a report carries is the tool's name and its order and nothing else: no
/// argument, no result, and no part of the exchange leaves the loop by this
/// route (CVL-FR-26, CVL-FR-27). `None` where the turn has already terminated
/// — there is nothing to be active in, and nothing to publish.
pub(super) fn report_tool_begun<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    turn_id: &str,
    tool: &str,
) -> Option<String> {
    let (id, turn) = turns.begin_tool_call(app, turn_id, tool)?;
    // Emitted by the registry under its own lock (AGC-FR-21); the record is
    // written here, after it, so the lock is not held across a log emit.
    //
    // The one record that says which status the author was shown and when.
    // The tool's name and the shape of the list around it; never an argument.
    logging::log_debug(
        app,
        turns.buffer(),
        &[Domain::Ai, Domain::Backend],
        "agent tool call begun",
        log_fields! {
            "turnId" => turn_id,
            "tool" => tool,
            "activationSeq" => turn
                .active_tool_calls
                .last()
                .map(|call| call.activation_seq)
                .unwrap_or(0),
            "activeCalls" => turn.active_tool_calls.len(),
        },
    );
    Some(id)
}

/// CVL-FR-33 / AGC-FR-34: report that a tool call has **finished** — its result
/// produced, its refusal returned, or the call abandoned by cancellation, by the
/// whole-turn deadline, or by the turn ending on the call itself.
///
/// Silent where the call was not active: a turn that has already terminated
/// carries no list to leave, and its terminal event already said so.
pub(super) fn report_tool_finished<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    turn_id: &str,
    call_id: &str,
) {
    let _ = turns.finish_tool_call(app, turn_id, call_id);
}

/// CVL-FR-33: the calls one model reply asked for, reported begun in the order
/// the reply asked for them and finished one by one as the loop is done with
/// each.
///
/// Held as a unit rather than reported call by call at each dispatch site
/// because the whole reply's calls are begun **before any of their results is
/// appended**: calls made in one round carry an order of their own rather than
/// arriving as a set. It also makes "no call is left active after the loop has
/// stopped working on it" a single call to [`ToolActivity::finish_all`] on
/// every path out of the round, including the ones that end the turn.
pub(super) struct ToolActivity {
    /// One entry per call the reply asked for, positionally. `None` once the
    /// call has been reported finished, or where the turn had already
    /// terminated when it was begun.
    pub(super) ids: Vec<Option<String>>,
}

impl ToolActivity {
    pub(super) fn begin<R: tauri::Runtime>(
        app: &tauri::AppHandle<R>,
        turns: &TurnRegistry,
        turn_id: &str,
        calls: &[rig::completion::message::ToolCall],
    ) -> Self {
        Self {
            ids: calls
                .iter()
                .map(|call| report_tool_begun(app, turns, turn_id, &call.function.name))
                .collect(),
        }
    }

    pub(super) fn finish<R: tauri::Runtime>(
        &mut self,
        app: &tauri::AppHandle<R>,
        turns: &TurnRegistry,
        turn_id: &str,
        index: usize,
    ) {
        if let Some(id) = self.ids.get_mut(index).and_then(Option::take) {
            report_tool_finished(app, turns, turn_id, &id);
        }
    }

    pub(super) fn finish_all<R: tauri::Runtime>(
        &mut self,
        app: &tauri::AppHandle<R>,
        turns: &TurnRegistry,
        turn_id: &str,
    ) {
        for index in 0..self.ids.len() {
            self.finish(app, turns, turn_id, index);
        }
    }
}
