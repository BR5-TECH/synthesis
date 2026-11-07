//! The end of a run after the vendor's final event
//! (`EAC-execute-agent-cli.md` EAC-FR-HZIE, EAC-FR-QHAY).
//!
//! A vendor CLI can stay alive after it wrote the line that ends its turn. A
//! background task that the agent started and that never ends keeps it alive
//! (`CCP-claude-code-cli-protocol.md` CCP-FR-24), and without this rule only the
//! deadline ends such a run, which discards its answer.
//!
//! The watch reads each stdout line on its way to the caller's observer. A
//! final event starts the grace, and a later stdout line cancels it. Each
//! runtime asks the watch in the poll loop it already has, and ends the process
//! when the grace is over.

use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;

use super::runtime::{StreamChannel, StreamObserver};

/// EAC-FR-HZIE: how long a process may live after its final event.
pub const FINAL_EVENT_GRACE: Duration = Duration::from_secs(30);

/// The longest stdout line the watch puts together from pieces.
///
/// A result line holds the envelope (`LIMIT_ENVELOPE`) and the same document
/// again as text, so it can be longer than the observer's line bound. A line
/// longer than this is not read as a final event.
const LIMIT_FINAL_LINE: usize = 4 * super::protocol::LIMIT_ENVELOPE;

/// EAC-FR-HZIE: a test on one whole stdout line, true where the line ends the
/// vendor's turn.
pub type FinalEvent = fn(&[u8]) -> bool;

/// EAC-FR-HZIE: the final event of a vendor and the grace after it.
#[derive(Clone, Copy, Debug)]
pub struct FinalEventRule {
    pub is_final: FinalEvent,
    pub grace: Duration,
}

impl FinalEventRule {
    /// The rule of a vendor whose descriptor declares a final event, with the
    /// grace of EAC-FR-HZIE. A vendor that declares none gets no rule
    /// (EAC-FR-QHAY).
    pub fn for_vendor(final_event: Option<FinalEvent>) -> Option<Self> {
        final_event.map(|is_final| FinalEventRule { is_final, grace: FINAL_EVENT_GRACE })
    }
}

/// What the watch knows about the stdout lines it has seen.
#[derive(Default)]
struct State {
    /// When the grace ends. `None` where no final event is the latest line.
    grace_ends: Option<Instant>,
    /// The pieces of a stdout line that is longer than the observer's bound.
    pending: Vec<u8>,
    /// The line in `pending` went past `LIMIT_FINAL_LINE`.
    overflowed: bool,
}

/// EAC-FR-HZIE: the observer that watches stdout for the final event and
/// passes every line on to the caller's own observer.
pub struct FinalEventWatch<'a> {
    rule: FinalEventRule,
    inner: Option<&'a dyn StreamObserver>,
    state: Mutex<State>,
}

impl<'a> FinalEventWatch<'a> {
    pub fn new(rule: FinalEventRule, inner: Option<&'a dyn StreamObserver>) -> Self {
        FinalEventWatch { rule, inner, state: Mutex::new(State::default()) }
    }

    /// True where the latest stdout line is a final event and its grace is
    /// over.
    pub fn grace_over(&self) -> bool {
        let state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.grace_ends.is_some_and(|ends| Instant::now() >= ends)
    }

    fn see_stdout(&self, bytes: &[u8], whole: bool) {
        let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        // The end of the stream delivers an empty line. It is not a line the
        // vendor wrote, so it changes nothing. The observer delivers a blank
        // vendor line in the same form, so a blank line does not cancel a
        // grace either; neither pinned CLI writes one. A final event with no
        // line ending is delivered only at the end of the stream, and its
        // grace then starts after stdout has closed.
        if whole && bytes.is_empty() && state.pending.is_empty() {
            return;
        }
        // EAC-FR-HZIE: any later stdout line cancels the grace, also the first
        // piece of a long one.
        state.grace_ends = None;
        if !state.overflowed {
            if state.pending.len() + bytes.len() > LIMIT_FINAL_LINE {
                state.overflowed = true;
                state.pending = Vec::new();
            } else if !whole || !state.pending.is_empty() {
                state.pending.extend_from_slice(bytes);
            }
        }
        if !whole {
            return;
        }
        let is_final = if state.overflowed {
            false
        } else if state.pending.is_empty() {
            (self.rule.is_final)(bytes)
        } else {
            (self.rule.is_final)(&state.pending)
        };
        state.pending.clear();
        state.overflowed = false;
        if is_final {
            state.grace_ends = Some(Instant::now() + self.rule.grace);
        }
    }
}

impl StreamObserver for FinalEventWatch<'_> {
    fn line(&self, channel: StreamChannel, bytes: &[u8], whole: bool) {
        if channel == StreamChannel::Stdout {
            self.see_stdout(bytes, whole);
        }
        if let Some(inner) = self.inner {
            inner.line(channel, bytes, whole);
        }
    }
}

/// The observer a runtime hands its streams: the watch where there is one,
/// and otherwise the caller's own.
pub fn observer_of<'b>(
    watch: &'b Option<FinalEventWatch<'_>>,
    inner: Option<&'b dyn StreamObserver>,
) -> Option<&'b dyn StreamObserver> {
    match watch {
        Some(watch) => Some(watch),
        None => inner,
    }
}

#[cfg(test)]
#[path = "final_event_tests.rs"]
mod tests;
