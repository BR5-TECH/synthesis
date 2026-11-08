//! Reads of the registry that only a test needs.

use super::*;

impl TurnRegistry {
    /// The tool calls active in a turn, for a test asserting what a surface
    /// would read at this moment.
    pub fn active_tool_calls(&self, turn_id: &str) -> Vec<ActiveToolCall> {
        self.lock()
            .live
            .get(turn_id)
            .map(|live| live.turn.active_tool_calls.clone())
            .unwrap_or_default()
    }

    /// Whether a turn has committed to appending its answer. Exists so the race
    /// of AGC-FR-20 can be driven deterministically rather than slept at.
    pub fn is_delivering(&self, turn_id: &str) -> bool {
        self.lock().live.get(turn_id).is_some_and(|l| l.delivering)
    }
}
