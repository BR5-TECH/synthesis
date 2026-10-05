//! The turn registry: what is in flight, what ended, and what may be retried.
//!
//! [`TurnRegistry`] is the managed state a turn is registered in, and the
//! concurrency bound is a permit it hands out — a bound delays a call and
//! never declines a dispatch (AGC-FR-03).

use super::*;

// ---------------------------------------------------------------------------
// The turn registry
// ---------------------------------------------------------------------------

struct Live {
    pub(super) turn: AgentTurn,
    /// AGC-FR-YQMD: whether one `complete` invocation has been made for this
    /// turn, which is what its lifecycle record reports as `reached_model`.
    ///
    /// Held on the registry rather than read off the loop's own counter,
    /// because the two paths that terminate a turn are not the same path: the
    /// loop knows how many rounds it made, and a cancellation arriving from
    /// another thread does not. A flag both can read is the only thing that
    /// records the same fact either way.
    pub(super) reached_model: Arc<AtomicBool>,
    /// Which project the turn belongs to, so withdrawing an agent from *this*
    /// project cancels only what it is doing here (AGR-FR-15).
    pub(super) project_key: String,
    pub(super) cancelled: Arc<AtomicBool>,
    pub(super) operation: Option<OperationId>,
    /// Set, under the registry lock, once the turn has committed to appending
    /// its answer. Past that point a cancellation is too late to stop it
    /// (AGC-FR-20: "a cancellation racing a delivery never rewrites what was
    /// delivered"), so the two are made mutually exclusive by one lock rather
    /// than by a check that the file write then races.
    pub(super) delivering: bool,
    /// AGC-FR-33: the next **activation sequence** this turn will hand out.
    ///
    /// Ascending and never reused, so a call begun after another is ordered
    /// after it however the two events reach a surface, and a sequence a
    /// finished call held is not given to a later one.
    pub(super) next_seq: u64,
}

/// AGC-FR-23: turn ids, unique for the lifetime of the running application.
///
/// Process-wide rather than per-registry so a `turnId` in the session log names
/// exactly one turn. The records a turn emits are correlated by that id and by
/// nothing else — a counter that restarted per registry would make two turns
/// indistinguishable in the panel, and the production application holds one
/// registry anyway, so nothing outside a test can tell the difference.
pub(super) static NEXT_TURN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[derive(Default)]
struct Inner {
    pub(super) live: HashMap<String, Live>,
    /// The turns that have terminated, so a second cancellation can answer
    /// "that one is already `delivered`" rather than "no such turn"
    /// (AGC-FR-20). Bounded: a terminal record is a handful of short strings,
    /// and the whole map dies with the process (AGC-FR-23).
    pub(super) terminated: HashMap<String, AgentTurn>,
    /// Insertion order of `terminated`, so the oldest is dropped at the cap.
    pub(super) terminated_order: std::collections::VecDeque<String>,
    /// AGC-FR-28: the turns that ended by asking the author something.
    ///
    /// Held apart from `live` because nothing about them is running — no thread,
    /// no slot, no session, no cancellation flag worth setting — and apart from
    /// `terminated` because `list_agent_turns` still returns them (AGC-FR-22).
    /// A registration is the agent, the origin, and the state: it is what tells
    /// the next comment in that conversation who is owed a reply, and it is all
    /// that survives the turn. Unbounded by design and bounded in practice —
    /// one entry per unanswered question, retired by the answer (AGC-FR-29) —
    /// and the whole map dies with the process (AGC-FR-23).
    pub(super) awaiting: HashMap<String, Awaiting>,
    /// AGC-FR-31: the **recovery registry** — at most one entry per
    /// conversation, keyed by the conversation's thread id.
    ///
    /// One entry per conversation is what makes an offer to retry unambiguous:
    /// the author is offered the most recent thing that failed there and never a
    /// choice among several, so a later recoverable failure simply replaces the
    /// one it displaces. The value is the `AgentTurn` and nothing beside it —
    /// the turn's id, the agent, the origin, the trigger comment, the final
    /// failure, `retry_permitted`, and the instants it began and ended, which is
    /// the ordering a later comment or a later failure is compared against.
    /// There is **no** assembled request, no exchange, no tool call or result,
    /// no credential, and no text of any answer in it, so the whole map is safe
    /// to hand to a surface and there is nothing in it to redact.
    ///
    /// Unbounded across conversations by design: one entry per conversation is
    /// small, and a project-wide cap is a rule that would drop the entry the
    /// author was about to use. It is memory on the terms of AGC-FR-23 and dies
    /// with the process.
    pub(super) recoverable: HashMap<String, Recoverable>,
    /// AGC-FR-39: the **notice registry** — per conversation, the most recent
    /// turn that ended with `images_omitted` true.
    ///
    /// It exists so that a surface mounted after a turn ended still knows to say
    /// that its pictures were not sent, exactly as it knows a failure still
    /// stands (AGC-FR-31). Memory of the running session and a contribution to
    /// nothing: it is in no log, in no thread, and in nothing any reader of the
    /// conversation ever sees.
    pub(super) image_notices: HashMap<String, Recoverable>,
}

/// AGC-FR-31: what is left of a turn the author may ask again.
struct Recoverable {
    pub(super) turn: AgentTurn,
    /// The project it belongs to, so closing a project or withdrawing an agent
    /// from one discards only what belongs there (AGR-FR-15).
    pub(super) project_key: String,
}

/// AGC-FR-28: what is left of a turn that ended awaiting a reply.
struct Awaiting {
    pub(super) turn: AgentTurn,
    /// The project it belongs to, so withdrawing an agent from *this* project
    /// discards only what it is owed here (AGR-FR-15).
    pub(super) project_key: String,
}

/// AGC-FR-23: the turns in flight, held in memory and for the lifetime of the
/// running application alone. Nothing about a turn is written to disk, because
/// the reply delivered into the conversation is the whole of the record and a
/// turn interrupted by a relaunch left nothing to resume.
pub struct TurnRegistry {
    inner: Mutex<Inner>,
    pub(super) seam: Box<dyn CompletionSeam>,
    /// AGC-FR-25: the concurrency bound, as a counting semaphore. A turn
    /// dispatched beyond it is still registered, still running, and still
    /// reported — it begins its call as a slot frees.
    pub(super) permits: Arc<(Mutex<usize>, Condvar)>,
    /// The bound the permits were built with, kept so a turn waiting on one can
    /// say what it is waiting behind.
    pub(super) concurrency: usize,
    pub(super) timeout: Duration,
    /// CVL-FR-19: where a backoff delay's jitter comes from. A collaborator like
    /// the seam beside it, so a test asserts a schedule exactly rather than
    /// tolerating a margin.
    pub(super) jitter: Box<dyn JitterSource>,
    /// CVL-FR-19: the retry schedule. The application always builds it from the
    /// module's own constants (CVL-FR-22).
    pub(super) retry: RetryPolicy,
    /// Where this registry's turns report themselves (LGC-FR-01).
    ///
    /// A collaborator like the seam beside it, rather than a global reached for
    /// at each emit site — the same reason `logging::log` takes the buffer as an
    /// argument. The application has exactly one and passes it; a test passes a
    /// buffer of its own, which matters because the session buffer is *cleared*
    /// when a project closes or a worktree changes (LGC-FR-15) and the tests of
    /// those paths run alongside these.
    pub(super) buffer: &'static LogBuffer,
}

/// CVL-FR-PDXK: what one turn is bounded by, read before that turn runs.
#[derive(Clone, Copy, Debug)]
pub struct TurnBounds {
    /// CVL-FR-16: the bound on the whole turn.
    pub timeout: Duration,
    /// CVL-FR-17 / CVL-FR-19: the deadline on one provider call, and how many
    /// attempts one logical invocation may spend.
    pub retry: RetryPolicy,
}

impl Default for TurnRegistry {
    fn default() -> Self {
        Self::new(Box::new(RigCompletion), DEFAULT_CONCURRENCY, DEFAULT_TIMEOUT)
    }
}

impl TurnRegistry {
    pub fn new(seam: Box<dyn CompletionSeam>, concurrency: usize, timeout: Duration) -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            seam,
            permits: Arc::new((Mutex::new(concurrency.max(1)), Condvar::new())),
            concurrency: concurrency.max(1),
            timeout,
            jitter: Box::new(SystemJitter),
            retry: RetryPolicy::default(),
            buffer: &logging::BUFFER,
        }
    }

    /// A registry whose backoff jitter is scripted. Test-only: the application
    /// wants a decorrelated delay and `new` already gives it one.
    #[cfg(test)]
    pub fn with_jitter(mut self, jitter: Box<dyn JitterSource>) -> Self {
        self.jitter = jitter;
        self
    }

    /// A registry whose retry schedule is not the module's own. Test-only, for
    /// the same reason `with_jitter` is: what most of the suite wants is the
    /// retry *behaviour* without paying for the waits, and one test asserts the
    /// default schedule is exactly what CVL-FR-19 says.
    #[cfg(test)]
    pub fn with_retry_policy(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// The retry schedule this registry runs under before the project's own
    /// settings are read over it.
    pub fn retry_policy(&self) -> &RetryPolicy {
        &self.retry
    }

    /// CVL-FR-PDXK: the bounds one turn runs under, read before that turn.
    ///
    /// CVL-FR-16: the whole-turn timeout is the turn timeout the turn's provider
    /// stores (`AAP-ai-api-integrations.md` AAP-FR-TXNM), passed in as
    /// `provider_timeout_ms`; where it stores none, the project's execution
    /// timeout (`PSS-project-settings-storage.md` PSS-FR-TQMV); and where that
    /// is unset too, this registry's own value. The provider-call deadline and
    /// the retry budget are the project's own settings (PSS-FR-HDBN,
    /// PSS-FR-WPKS), and each unset one leaves this registry's own value in
    /// place. The waits between attempts are constants of this module and are
    /// not read from anywhere (CVL-FR-22).
    pub fn bounds_for_turn<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        provider_timeout_ms: Option<u64>,
    ) -> TurnBounds {
        let mut bounds = TurnBounds {
            timeout: self.timeout,
            retry: self.retry,
        };
        use tauri::Manager;
        let root = app
            .try_state::<crate::project::ProjectState>()
            .and_then(|state| state.require_root().ok());
        if let Some(root) = root {
            let stored = crate::project_settings::loop_settings::SharedLoopSettings::read(&root);
            if let Some(ms) = stored.execution_timeout_ms {
                bounds.timeout = Duration::from_millis(ms);
            }
            if let Some(ms) = stored.provider_call_deadline_ms {
                bounds.retry.call_timeout = Duration::from_millis(ms);
            }
            if let Some(budget) = stored.retry_budget {
                bounds.retry.max_attempts = budget.max(1) as usize;
            }
        }
        if let Some(ms) = provider_timeout_ms {
            bounds.timeout = Duration::from_millis(ms);
        }
        bounds
    }

    /// A registry reporting into a buffer other than the session's. Test-only:
    /// the application has one buffer and `new` already names it.
    #[cfg(test)]
    pub fn reporting_into(mut self, buffer: &'static LogBuffer) -> Self {
        self.buffer = buffer;
        self
    }

    /// Where this registry's turns report themselves.
    pub(super) fn buffer(&self) -> &'static LogBuffer {
        self.buffer
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub(super) fn register(
        &self,
        agent: &Agent,
        origin: ConversationOrigin,
        trigger_comment_id: &str,
        project_key: &str,
    ) -> (AgentTurn, Arc<AtomicBool>) {
        let mut inner = self.lock();
        let turn = AgentTurn {
            id: format!("turn-{}", NEXT_TURN.fetch_add(1, Ordering::Relaxed)),
            agent_id: agent.id.clone(),
            nickname: agent.nickname.clone(),
            origin,
            trigger_comment_id: trigger_comment_id.to_string(),
            state: AgentTurnState::Running,
            failure: None,
            // AGC-FR-31: set only where the turn terminates on a recoverable
            // failure and becomes its conversation's registry entry.
            retry_permitted: false,
            started_at: now_rfc3339(),
            ended_at: None,
            // AGC-FR-33: a turn that has made no tool call carries an empty
            // list, which is what a registration event carries.
            active_tool_calls: Vec::new(),
            // AGC-FR-36: decided once the endpoint is resolved, which is after
            // the turn is registered — so a turn is registered carrying the
            // answer for a turn that sent no picture, and the decision writes it
            // through before any payload but the registration itself.
            images_omitted: false,
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        inner.live.insert(
            turn.id.clone(),
            Live {
                turn: turn.clone(),
                reached_model: Arc::new(AtomicBool::new(false)),
                project_key: project_key.to_string(),
                cancelled: cancelled.clone(),
                operation: None,
                delivering: false,
                next_seq: 1,
            },
        );
        (turn, cancelled)
    }

    /// AGC-FR-33: begin a tool call in a running turn, publish it, and answer
    /// with the whole turn as it now stands.
    ///
    /// The id and the sequence are handed out under the one lock that appends
    /// the call, so two threads beginning calls in the same turn cannot be
    /// given the same sequence and the list is ordered by activation and by
    /// nothing else. A turn that has already terminated answers `None`: there
    /// is nothing to be active in, and the call's result will be discarded
    /// (CVL-FR-25).
    ///
    /// **The event is emitted under that same lock**, and AGC-FR-21 is why. A
    /// turn is cancelled from a different thread than the one driving its loop,
    /// and `terminate` removes the turn from `live` under this lock before its
    /// terminal event is published. Emitting outside the lock would leave a
    /// window in which this thread reads a `running` snapshot, the cancelling
    /// thread terminates and publishes, and this thread then publishes its
    /// stale snapshot **after** the terminal event — which a consumer reading
    /// the state it was sent would answer by resurrecting a pending
    /// contribution for a turn that has ended, and nothing would ever remove
    /// it. Holding the lock across the emit orders the two: either this runs
    /// first and the terminal event follows it, or the termination runs first
    /// and there is no live turn here to report. The critical section is one
    /// serialise and one queued dispatch, and no listener re-enters this
    /// registry.
    pub(super) fn begin_tool_call<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        turn_id: &str,
        tool: &str,
    ) -> Option<(String, AgentTurn)> {
        let mut inner = self.lock();
        let live = inner.live.get_mut(turn_id)?;
        let activation_seq = live.next_seq;
        live.next_seq += 1;
        let id = format!("call-{activation_seq}");
        live.turn.active_tool_calls.push(ActiveToolCall {
            id: id.clone(),
            tool: tool.to_string(),
            activation_seq,
        });
        let turn = live.turn.clone();
        publish(app, &turn);
        Some((id, turn))
    }

    /// AGC-FR-33: a call leaves the list the moment it succeeds, refuses, or is
    /// abandoned. Publishes and answers the whole turn as it now stands, or
    /// `None` where the call was not active — a turn already terminated, or a
    /// call already finished by another path, neither of which is a second
    /// event.
    ///
    /// Published under the lock for the reason
    /// [`begin_tool_call`](Self::begin_tool_call) gives.
    pub(super) fn finish_tool_call<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        turn_id: &str,
        call_id: &str,
    ) -> Option<AgentTurn> {
        let mut inner = self.lock();
        let live = inner.live.get_mut(turn_id)?;
        let before = live.turn.active_tool_calls.len();
        live.turn.active_tool_calls.retain(|call| call.id != call_id);
        if live.turn.active_tool_calls.len() == before {
            return None;
        }
        let turn = live.turn.clone();
        publish(app, &turn);
        Some(turn)
    }

    /// The tool calls active in a turn, for a test asserting what a surface
    /// would read at this moment.
    #[cfg(test)]
    pub fn active_tool_calls(&self, turn_id: &str) -> Vec<ActiveToolCall> {
        self.lock()
            .live
            .get(turn_id)
            .map(|live| live.turn.active_tool_calls.clone())
            .unwrap_or_default()
    }

    /// AGC-FR-24: record the progress operation of a turn that now holds a slot.
    /// Answers `false` where the turn has already terminated, so the caller ends
    /// the operation it just registered rather than leaving it in flight.
    pub(super) fn attach_operation(&self, turn_id: &str, operation: OperationId) -> bool {
        match self.lock().live.get_mut(turn_id) {
            Some(live) => {
                live.operation = Some(operation);
                true
            }
            None => false,
        }
    }

    /// AGC-FR-21: a terminal state is reached exactly once per turn. The turn is
    /// removed from the in-flight set in the same lock that stamps its terminal
    /// state, so a cancellation racing a delivery cannot rewrite what was
    /// delivered — the loser simply finds nothing to terminate.
    pub(super) fn terminate(
        &self,
        turn_id: &str,
        state: AgentTurnState,
        failure: Option<&str>,
    ) -> Option<(AgentTurn, Option<OperationId>, String, bool)> {
        let mut inner = self.lock();
        // AGC-FR-20: a turn that has committed to appending cannot be cancelled
        // out from under the append. Its own `finish` still terminates it,
        // because that call comes from the delivering thread itself.
        if state == AgentTurnState::Cancelled
            && inner.live.get(turn_id).is_some_and(|l| l.delivering)
        {
            return None;
        }
        let live = inner.live.remove(turn_id)?;
        let project_key = live.project_key.clone();
        let reached_model = live.reached_model.load(Ordering::SeqCst);
        let mut turn = live.turn;
        turn.state = state;
        turn.failure = failure.map(str::to_string);
        turn.ended_at = Some(now_rfc3339());
        // AGC-FR-34: a terminal event carries an empty list whatever was active
        // when the turn ended, so a terminating turn leaves no call behind for
        // a surface to keep reading a status from (CVL-FR-33).
        turn.active_tool_calls.clear();
        if state == AgentTurnState::AwaitingReply {
            // AGC-FR-28: terminal, so it leaves `live` with the rest of the
            // turn's working state — but still outstanding, so it is parked
            // where `in_flight` can find it rather than filed with the turns
            // nothing is owed on.
            inner.awaiting.insert(
                turn.id.clone(),
                Awaiting {
                    turn: turn.clone(),
                    project_key: live.project_key,
                },
            );
        } else {
            inner.remember_terminated(turn.clone());
        }
        Some((turn, live.operation, project_key, reached_model))
    }

    /// AGC-FR-YQMD: record that this turn has made a `complete` invocation.
    ///
    /// Called by the loop the moment a physical attempt goes out, so a turn
    /// cancelled mid-call is still recorded as having reached the model.
    pub(super) fn mark_reached_model(&self, turn_id: &str) {
        if let Some(live) = self.lock().live.get(turn_id) {
            live.reached_model.store(true, Ordering::SeqCst);
        }
    }

    /// AGC-FR-20: claim the turn for delivery, or report that a cancellation got
    /// there first.
    ///
    /// The check and the claim happen under one lock, which is what makes the
    /// race unwinnable in either direction: a cancellation that arrives before
    /// this returns `true` stops the append, and one that arrives after it is
    /// refused by `terminate` above.
    pub(super) fn begin_delivery(&self, turn_id: &str) -> bool {
        let mut inner = self.lock();
        let Some(live) = inner.live.get_mut(turn_id) else {
            return false;
        };
        if live.cancelled.load(Ordering::SeqCst) {
            return false;
        }
        live.delivering = true;
        true
    }

    /// A turn that has already terminated, for a cancellation that arrived late.
    ///
    /// An `awaiting_reply` turn answers here too: it has terminated, so
    /// AGC-FR-20's "succeeds against a turn that has already terminated without
    /// changing it" applies to it unchanged — an explicit cancel leaves it as it
    /// stands, and its question where it was posted.
    pub(super) fn terminated(&self, turn_id: &str) -> Option<AgentTurn> {
        let inner = self.lock();
        inner
            .terminated
            .get(turn_id)
            .cloned()
            .or_else(|| inner.awaiting.get(turn_id).map(|a| a.turn.clone()))
    }

    /// AGC-FR-29: retire what this conversation owed `agent_id`, because a fresh
    /// turn for it has just been dispatched here.
    ///
    /// The awaiting turn becomes `delivered` — it did deliver, its contribution
    /// being the question — and stops being outstanding. **No event is
    /// emitted**: a terminal event was already emitted for this turn when it
    /// ended awaiting a reply, and AGC-FR-21 allows exactly one. A consumer
    /// stays in step because it is itself the cause, having issued the dispatch
    /// that retired it.
    pub(super) fn retire_awaiting(&self, agent_id: &str, origin: &ConversationOrigin) -> usize {
        let mut inner = self.lock();
        let ids: Vec<String> = inner
            .awaiting
            .values()
            .filter(|a| a.turn.agent_id == agent_id && &a.turn.origin == origin)
            .map(|a| a.turn.id.clone())
            .collect();
        for id in &ids {
            if let Some(awaiting) = inner.awaiting.remove(id) {
                let mut turn = awaiting.turn;
                turn.state = AgentTurnState::Delivered;
                inner.remember_terminated(turn);
            }
        }
        ids.len()
    }

    /// AGC-FR-TQLC: retire every registration outstanding on one **thread**,
    /// whichever agent holds it.
    ///
    /// [`retire_awaiting`](Self::retire_awaiting) retires the agent being
    /// dispatched to, which is right for an answer addressed to that agent. A
    /// question-set submission is not that: it may dispatch to a different agent
    /// than the one that asked, or to none at all, and the turn that asked is
    /// retired either way — it ended before there was an answer and is never
    /// resumed, re-entered, or continued.
    ///
    /// Silent for the reason its sibling is: the turn's terminal event has
    /// already been emitted, so this changes what the registry is holding rather
    /// than what any surface has been told.
    pub(super) fn retire_awaiting_on_thread(&self, thread_id: &str) -> usize {
        let mut inner = self.lock();
        let ids: Vec<String> = inner
            .awaiting
            .values()
            .filter(|a| a.turn.origin.discussion_id() == thread_id)
            .map(|a| a.turn.id.clone())
            .collect();
        for id in &ids {
            if let Some(awaiting) = inner.awaiting.remove(id) {
                let mut turn = awaiting.turn;
                turn.state = AgentTurnState::Delivered;
                inner.remember_terminated(turn);
            }
        }
        ids.len()
    }

    /// AGC-FR-28: discard the registrations an agent is owed, because the agent
    /// is gone — deleted, or withdrawn from this project (AGR-FR-11, AGR-FR-15).
    ///
    /// An agent that is gone is owed nothing. Silent for the same reason
    /// [`retire_awaiting`](Self::retire_awaiting) is: the turn's terminal event
    /// has already been emitted.
    pub(super) fn discard_awaiting_for_agent(&self, agent_id: &str, project_key: Option<&str>) -> usize {
        let mut inner = self.lock();
        let before = inner.awaiting.len();
        inner.awaiting.retain(|_, a| {
            a.turn.agent_id != agent_id || project_key.is_some_and(|key| a.project_key != key)
        });
        before - inner.awaiting.len()
    }

    /// AGC-FR-28: discard every registration, because the project is closing and
    /// holds no conversation for one to be owed in.
    pub(super) fn clear_awaiting(&self) -> usize {
        let mut inner = self.lock();
        let count = inner.awaiting.len();
        inner.awaiting.clear();
        count
    }

    /// Whether a turn has committed to appending its answer. Exists so the race
    /// of AGC-FR-20 can be driven deterministically rather than slept at.
    #[cfg(test)]
    pub fn is_delivering(&self, turn_id: &str) -> bool {
        self.lock().live.get(turn_id).is_some_and(|l| l.delivering)
    }

    /// AGC-FR-22: the turns still **outstanding**, most recently started first
    /// — those `running` and those `awaiting_reply`.
    ///
    /// The second kind is why this is not simply the live set: a turn that asked
    /// the author something is over, but the conversation still owes it an
    /// answer, and a surface mounted a week later has no other way to learn
    /// that. A turn that is `delivered`, `failed`, or `cancelled` is absent
    /// under either form.
    pub(super) fn in_flight(&self, origin: Option<&ConversationOrigin>) -> Vec<AgentTurn> {
        let inner = self.lock();
        let mut turns: Vec<AgentTurn> = inner
            .live
            .values()
            .map(|live| &live.turn)
            .chain(inner.awaiting.values().map(|awaiting| &awaiting.turn))
            .filter(|turn| origin.is_none_or(|o| &turn.origin == o))
            .cloned()
            .collect();
        // Ids are `turn-<n>` from a monotonic counter, so the numeric suffix is
        // the registration order. `started_at` has one-second resolution and
        // would tie for turns dispatched together by one post.
        turns.sort_by(|a, b| ordinal(&b.id).cmp(&ordinal(&a.id)));
        turns
    }

    /// AGC-FR-30: the live turns belonging to one note's conversation.
    pub(super) fn cancellation_flags_for_note(&self, note_id: &str) -> Vec<String> {
        self.lock()
            .live
            .values()
            .filter(|live| origin_note_id(&live.turn.origin) == Some(note_id))
            .map(|live| live.turn.id.clone())
            .collect()
    }

    /// AGC-FR-31: enter a terminal recoverable failure as its conversation's
    /// current entry, displacing whatever stood there.
    ///
    /// Returns the turn as the registry now holds it — carrying
    /// `retry_permitted` — so the terminal event of AGC-FR-21 publishes the same
    /// thing `list_recoverable_agent_turn_failures` would return, and a consumer
    /// watching events and one reading the query can never disagree.
    pub(super) fn remember_recoverable(&self, mut turn: AgentTurn, project_key: String) -> AgentTurn {
        turn.retry_permitted = true;
        let mut inner = self.lock();
        let key = turn.origin.discussion_id().to_string();
        // The displaced entry keeps its terminal state; only the offer moves.
        inner.recoverable.insert(
            key,
            Recoverable {
                turn: turn.clone(),
                project_key,
            },
        );
        turn
    }

    /// AGC-FR-31: the entries, most recently failed first.
    ///
    /// An `origin` narrows to that conversation; `None` returns every entry
    /// anywhere. A turn returned here is not outstanding, so `list_agent_turns`
    /// returns it under neither form.
    pub(super) fn recoverable_failures(&self, origin: Option<&ConversationOrigin>) -> Vec<AgentTurn> {
        let inner = self.lock();
        let mut turns: Vec<AgentTurn> = inner
            .recoverable
            .values()
            .map(|entry| &entry.turn)
            .filter(|turn| origin.is_none_or(|o| &turn.origin == o))
            .cloned()
            .collect();
        // Registration order, as `in_flight` sorts: `ended_at` has one-second
        // resolution and would tie for two turns that failed together.
        turns.sort_by(|a, b| ordinal(&b.id).cmp(&ordinal(&a.id)));
        turns
    }

    /// AGC-FR-36 / AGC-FR-37: record the decision about this turn's images on
    /// the turn itself, from the moment it is taken.
    ///
    /// Written through to the live record so every payload the turn appears in
    /// from here on carries it — its terminal event among them.
    pub(super) fn note_images_omitted(&self, turn_id: &str, omitted: bool) {
        if let Some(live) = self.lock().live.get_mut(turn_id) {
            live.turn.images_omitted = omitted;
        }
    }

    /// AGC-FR-39: enter, replace, or retire this conversation's notice entry.
    ///
    /// A turn that omitted images **replaces** whatever stood there; one that
    /// carried its images successfully or carried none at all **retires** the
    /// entry. Either way this emits nothing of its own, on the same terms the
    /// recovery registry's own transitions emit nothing (AGC-FR-21).
    pub(super) fn record_image_notice(&self, turn: &AgentTurn, project_key: &str) {
        let mut inner = self.lock();
        let key = turn.origin.discussion_id().to_string();
        if turn.images_omitted {
            inner.image_notices.insert(
                key,
                Recoverable {
                    turn: turn.clone(),
                    project_key: project_key.to_string(),
                },
            );
        } else {
            inner.image_notices.remove(&key);
        }
    }

    /// AGC-FR-39: the notice entries, most recent first.
    ///
    /// An `origin` narrows to that conversation; `None` returns every entry
    /// anywhere. What a surface reads is the **current state** of a conversation
    /// rather than a history of it: at most one entry per conversation.
    pub(super) fn image_notices(&self, origin: Option<&ConversationOrigin>) -> Vec<AgentTurn> {
        let inner = self.lock();
        let mut turns: Vec<AgentTurn> = inner
            .image_notices
            .values()
            .map(|entry| &entry.turn)
            .filter(|turn| origin.is_none_or(|o| &turn.origin == o))
            .cloned()
            .collect();
        turns.sort_by(|a, b| ordinal(&b.id).cmp(&ordinal(&a.id)));
        turns
    }

    /// AGC-FR-32: take the entry this turn id names, but **only** where it is
    /// still its conversation's current one.
    ///
    /// The check and the removal happen under one lock, so two retries issued
    /// together leave exactly one new turn: the loser finds nothing to take and
    /// is refused, exactly as a retry of a displaced turn is.
    pub(super) fn take_recoverable(&self, turn_id: &str) -> Option<(AgentTurn, String)> {
        let mut inner = self.lock();
        let key = inner
            .recoverable
            .iter()
            .find(|(_, entry)| entry.turn.id == turn_id)
            .map(|(key, _)| key.clone())?;
        inner
            .recoverable
            .remove(&key)
            .map(|entry| (entry.turn, entry.project_key))
    }

    /// AGC-FR-31: retire the entry a conversation holds, because a later
    /// **human** comment was appended to it.
    ///
    /// An agent-authored comment retires nothing, which is why this is called
    /// from the human write path alone: an agent answering elsewhere in the
    /// thread does not mean the author has stopped wanting the answer that
    /// failed.
    pub(super) fn retire_recoverable(&self, thread_id: &str) -> Option<AgentTurn> {
        let mut inner = self.lock();
        inner.recoverable.remove(thread_id).map(|entry| entry.turn)
    }

    /// AGC-FR-31: discard the entries belonging to an agent that is gone, on the
    /// terms its awaiting registrations are discarded (AGR-FR-11, AGR-FR-15).
    pub(super) fn discard_recoverable_for_agent(&self, agent_id: &str, project_key: Option<&str>) -> usize {
        let mut inner = self.lock();
        let before = inner.recoverable.len();
        inner.recoverable.retain(|_, entry| {
            entry.turn.agent_id != agent_id
                || project_key.is_some_and(|key| entry.project_key != key)
        });
        before - inner.recoverable.len()
    }

    /// AGC-FR-31: an offer to retry in a conversation that is gone with its note
    /// is an offer nothing could take (AGC-FR-30).
    pub(super) fn discard_recoverable_for_note(&self, note_id: &str) -> usize {
        let mut inner = self.lock();
        let before = inner.recoverable.len();
        inner
            .recoverable
            .retain(|_, entry| origin_note_id(&entry.turn.origin) != Some(note_id));
        before - inner.recoverable.len()
    }

    /// AGC-FR-31: every entry, because the project is closing and holds no
    /// conversation for one to be offered in.
    pub(super) fn clear_recoverable(&self) -> usize {
        let mut inner = self.lock();
        let count = inner.recoverable.len();
        inner.recoverable.clear();
        // AGC-FR-23 / AGC-FR-39: the notice registry is memory of the running
        // session on the same terms, and a project closing holds no conversation
        // for a notice to be read in.
        inner.image_notices.clear();
        count
    }

    /// AGC-FR-30: an author who deleted the note owes no answer, so what was
    /// parked for that conversation goes with it.
    pub(super) fn discard_awaiting_for_note(&self, note_id: &str) -> usize {
        let mut inner = self.lock();
        let before = inner.awaiting.len();
        inner
            .awaiting
            .retain(|_, a| origin_note_id(&a.turn.origin) != Some(note_id));
        before - inner.awaiting.len()
    }

    pub(super) fn cancellation_flags_for_agent(
        &self,
        agent_id: &str,
        project_key: Option<&str>,
    ) -> Vec<String> {
        self.lock()
            .live
            .values()
            .filter(|live| live.turn.agent_id == agent_id)
            .filter(|live| project_key.is_none_or(|key| live.project_key == key))
            .map(|live| live.turn.id.clone())
            .collect()
    }

    pub(super) fn flag(&self, turn_id: &str) -> Option<Arc<AtomicBool>> {
        self.lock().live.get(turn_id).map(|l| l.cancelled.clone())
    }

    pub(super) fn all_turn_ids(&self) -> Vec<String> {
        self.lock().live.keys().cloned().collect()
    }

    /// How many of the AGC-FR-25 slots are free right now.
    ///
    /// A hint rather than a reservation — it is read to explain a wait in the
    /// log, never to decide one, which is [`acquire`](Self::acquire)'s business
    /// under the condvar.
    pub(super) fn permits_available(&self) -> usize {
        let (lock, _) = &*self.permits;
        *lock.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The AGC-FR-25 bound this registry was built with, for the record that
    /// reports a turn waiting on it.
    pub(super) fn concurrency(&self) -> usize {
        self.concurrency
    }

    /// Take one of the AGC-FR-25 slots, blocking until one frees. The turn is
    /// already registered and already reported by the time this is reached, so
    /// waiting here delays a call and never refuses a dispatch.
    pub(super) fn acquire(&self) -> PermitGuard {
        let (lock, cvar) = &*self.permits;
        let mut available = lock.lock().unwrap_or_else(|e| e.into_inner());
        while *available == 0 {
            available = cvar.wait(available).unwrap_or_else(|e| e.into_inner());
        }
        *available -= 1;
        PermitGuard {
            permits: self.permits.clone(),
        }
    }
}

/// How many terminated turns are remembered for AGC-FR-20's idempotent cancel.
/// Large enough that a late cancellation of anything a surface could still be
/// showing finds its turn, small enough to be a rounding error in memory.
const TERMINATED_MEMORY: usize = 256;

impl Inner {
    fn remember_terminated(&mut self, turn: AgentTurn) {
        self.terminated_order.push_back(turn.id.clone());
        self.terminated.insert(turn.id.clone(), turn);
        while self.terminated_order.len() > TERMINATED_MEMORY {
            if let Some(oldest) = self.terminated_order.pop_front() {
                self.terminated.remove(&oldest);
            }
        }
    }
}

fn ordinal(turn_id: &str) -> u64 {
    turn_id
        .rsplit('-')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

pub(super) struct PermitGuard {
    pub(super) permits: Arc<(Mutex<usize>, Condvar)>,
}

impl Drop for PermitGuard {
    fn drop(&mut self) {
        let (lock, cvar) = &*self.permits;
        if let Ok(mut available) = lock.lock() {
            *available += 1;
            cvar.notify_one();
        }
    }
}
