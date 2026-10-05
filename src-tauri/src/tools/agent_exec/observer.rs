//! Watching a run happen (EAC-FR-32), and the small results the launch reads
//! back from it.
//!
//! Split out of `agent_exec.rs` for size alone. The items keep the visibility
//! they had, widened to `pub(super)` only where the module root or a sibling
//! part calls them.

use std::sync::Arc;

use super::*;

// ---------------------------------------------------------------------------
// Watching a run happen (EAC-FR-32)
// ---------------------------------------------------------------------------

/// Turns raw lines into masked, normalized activity, and hands each one to the
/// log and to the caller's sink.
///
/// It sits between the runtime and everyone downstream precisely so that
/// masking happens once, here, on the only path a byte of either stream can
/// take to a reader. Nothing below it is trusted to mask, and nothing above it
/// has to.
pub(super) struct LaunchObserver<'a, S: LogSink + Clone + Send + Sync + 'static> {
    pub(super) descriptor: &'static descriptor::VendorExecutionDescriptor,
    /// Every value this launch handed the container that a record may not
    /// carry, owned because the observer outlives the borrows they came from.
    pub(super) secrets: Vec<String>,
    /// How many bytes an unterminated piece has to hold back so that a secret
    /// split across a delivery boundary is never emitted as a fragment.
    pub(super) carry_len: usize,
    pub(super) sink: Option<&'a Arc<dyn AgentActivitySink>>,
    pub(super) log: S,
    /// EAC-FR-42: whether this execution may write `DEBUG` records into the
    /// session buffer at all.
    ///
    /// False for an execution carrying the semantic-rebase mount, whose task
    /// holds a captured graduation prompt and whose streams hold the
    /// specification text the agent is reading and writing — material a
    /// reconciliation's records may not carry at any level a reader can lower
    /// the floor to (per `../core/GRB-graduation-rebase.md` GRB-FR-OHWT). The
    /// **sink is unaffected**: the turn stays watchable in the run's own
    /// activity stream.
    pub(super) logs_activity: bool,
    pub(super) vendor: String,
    pub(super) container: String,
    /// One per channel, holding the tail of an unterminated piece.
    pub(super) carry: std::sync::Mutex<(Vec<u8>, Vec<u8>)>,
}

impl<'a, S: LogSink + Clone + Send + Sync + 'static> LaunchObserver<'a, S> {
    /// Emit one activity to every reader of this run.
    pub(super) fn emit(&self, event: AgentActivityEvent) {
        // A record per event, at DEBUG. The level is what lets a reader who is
        // chasing something else raise the floor and put a chatty run back in
        // the background, without the run having to be less observable
        // (LOG-FR-07) — except for the one execution kind that may write no
        // such record at all (EAC-FR-42).
        if self.logs_activity {
            logging::log_debug(
            &self.log,
            &BUFFER,
            &[Domain::Ai, Domain::Backend],
            "agent activity",
            log_fields! {
                "vendor" => self.vendor.as_str(),
                "container" => self.container.as_str(),
                "channel" => event.channel,
                "kind" => event.kind,
                "summary" => event.summary.as_str(),
                // The whole event, so a reader who exports or copies a record
                // has what the panel showed rather than a shortened form of it.
                "event" => event.payload.as_str(),
                "event_truncated" => event.payload_truncated,
            },
            );
        }
        if let Some(sink) = self.sink {
            sink.activity(event);
        }
    }

    /// One masked, bounded piece of text, read as an activity and emitted.
    fn observe(&self, channel: runtime::StreamChannel, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        let masked = protocol::mask_secrets(text, &self.secret_refs());
        let read = match channel {
            // Both pinned protocols define stderr as diagnostics and nothing
            // else (CCP-FR-14, CDX-FR-18), so there is no event vocabulary to
            // read it against — what it says is what it says.
            runtime::StreamChannel::Stderr => descriptor::VendorActivity {
                kind: descriptor::ActivityKind::Diagnostic,
                summary: descriptor::summary_line(&masked),
            },
            runtime::StreamChannel::Stdout => self.descriptor.activity(&masked),
        };
        let (payload, payload_truncated) = bounded_payload(&masked);
        self.emit(AgentActivityEvent {
            at: crate::notes::now_rfc3339(),
            channel: channel.as_str(),
            kind: read.kind.as_str(),
            summary: read.summary,
            payload,
            payload_truncated,
        });
    }

    pub(super) fn secret_refs(&self) -> Vec<&str> {
        self.secrets.iter().map(String::as_str).collect()
    }

    /// The same masking every observed line goes through, for the two events
    /// the executor writes about its own invocation.
    pub(super) fn mask(&self, text: &str) -> String {
        protocol::mask_secrets(text, &self.secret_refs())
    }
}

impl<'a, S: LogSink + Clone + Send + Sync + 'static> runtime::StreamObserver for LaunchObserver<'a, S> {
    fn line(&self, channel: runtime::StreamChannel, bytes: &[u8], whole: bool) {
        let mut held = match self.carry.lock() {
            Ok(held) => held,
            // A poisoned lock means a previous call panicked mid-observation.
            // The run itself is unaffected and the carry is only ever a tail of
            // text, so it is taken as it stands rather than propagating the
            // panic into the pipe pump and killing the capture.
            Err(poisoned) => poisoned.into_inner(),
        };
        let carry = match channel {
            runtime::StreamChannel::Stdout => &mut held.0,
            runtime::StreamChannel::Stderr => &mut held.1,
        };

        let mut piece = std::mem::take(carry);
        piece.extend_from_slice(bytes);

        // A piece that was cut at the reassembly bound rather than at a line
        // ending may have been cut through a credential. Holding back the last
        // `carry_len` bytes means the two halves are rejoined before anything
        // is masked, so no fragment of a secret is ever emitted (EAC-FR-29).
        let emit_upto = if whole {
            piece.len()
        } else {
            piece.len().saturating_sub(self.carry_len)
        };
        if !whole {
            carry.extend_from_slice(&piece[emit_upto..]);
        }
        drop(held);

        if emit_upto == 0 {
            return;
        }
        let text = String::from_utf8_lossy(&piece[..emit_upto]);
        self.observe(channel, &text);
    }
}

/// The verbatim event, bounded (EAC-FR-32).
///
/// Cut on a character boundary so a reader is never handed half a character,
/// and said to be cut, because an event silently shortened reads as a complete
/// one that happened to be small.
pub(super) fn bounded_payload(text: &str) -> (String, bool) {
    if text.len() <= protocol::LIMIT_ACTIVITY_EVENT {
        return (text.to_string(), false);
    }
    let end = (0..=protocol::LIMIT_ACTIVITY_EVENT)
        .rev()
        .find(|i| text.is_char_boundary(*i))
        .unwrap_or(0);
    (text[..end].to_string(), true)
}

pub(super) fn merge_session(
    reported: Option<protocol::SessionRef>,
    run_session_id: Option<String>,
) -> Option<protocol::SessionRef> {
    // EAC-FR-21: the identifier is the one the *run* reported at the position
    // its descriptor names, not the one the agent wrote into its envelope. The
    // precedence is the whole point of the rule — an agent does not reliably
    // know the id its CLI assigned, and for a vendor whose identity the
    // executor assigns, the run's value is the one already asserted against
    // what was handed to it. Letting the envelope win would discard a verified
    // identifier in favour of a string a model composed, and hand the caller a
    // reference that resumes nothing.
    //
    // The envelope's own value survives only where the run reported none, which
    // is a vendor with no session to report rather than a disagreement.
    let mut session = reported.unwrap_or_default();
    if let Some(id) = run_session_id {
        session.session_id = Some(id);
    }
    // The envelope's fields were bounds-checked during validation; the run's was
    // not, and it is grafted on here — so the merged value is sanitized rather
    // than either half.
    let session = session.sanitized();
    if session.is_empty() {
        None
    } else {
        Some(session)
    }
}

pub(super) fn directory_problem(error: &crate::fs::FsError) -> DirectoryProblem {
    use crate::fs::FsError;
    match error {
        FsError::NotFound { .. } => DirectoryProblem::Missing,
        FsError::NotADirectory { .. } => DirectoryProblem::NotADirectory,
        _ => DirectoryProblem::Inaccessible,
    }
}
