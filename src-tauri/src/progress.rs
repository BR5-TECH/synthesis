//! Progress reporting (`specifications/core/PRG-progress-reporting.md`).
//!
//! One channel every long-running operation announces itself on, so the status
//! bar (`../ui/STB-status-bar.md`) can render work originating in unrelated
//! modules without knowing which of them exist. The module observes and
//! republishes: it starts nothing, cancels nothing, retries nothing, and keeps
//! no history of what has finished (PRG-FR-09 / PRG-FR-10 / PRG-FR-14).
//!
//! Two seams make the whole thing testable without a Tauri runtime:
//!
//! - [`ProgressRegistry`] holds the in-flight set and decides *what* to emit,
//!   including the coalescing of PRG-FR-07. Its methods return the events due
//!   rather than emitting them.
//! - [`ProgressSink`] is the emitter. `AppHandle` implements it in production;
//!   the tests use a `Vec`-collecting one, so every ordering, coalescing and
//!   terminal-state invariant is exercised without a window.
//!
//! Attribution is opt-in per operation (PRG-FR-11): [`attribute`] wraps a unit
//! of work, and a module that never calls it registers nothing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{Emitter, State};

/// The event every registration, update and terminal state is published on
/// (PRG contract surface).
///
/// Kebab-case rather than the spec's abstract `"operation progress"`: Tauri
/// validates event names and accepts only alphanumerics, `-`, `/`, `:` and `_`,
/// so a name with spaces is rejected by `emit` — which returns an error rather
/// than panicking, and this module discards it (a progress event must never take
/// down the operation it reports on), making the channel silently dead. Matches
/// the constant in `src/events.ts` byte-for-byte, and pinned by
/// `every_event_name_is_one_tauri_will_actually_deliver` in `lib.rs`.
pub const OPERATION_PROGRESS: &str = "operation-progress";

/// PRG-FR-07: intermediate updates within this window collapse to the latest
/// value for their operation. An implementation choice, not a contract — the
/// contract is only that bursts collapse and that registration and terminal
/// events always arrive (PRG non-functional requirements).
pub const COALESCE_WINDOW: Duration = Duration::from_millis(50);

// ---------------------------------------------------------------------------
// Wire shapes (PRG "Payload shapes")
// ---------------------------------------------------------------------------

/// An operation is **in flight** while `Running`, and **terminated** in each of
/// the other three states (PRG contract surface).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationState {
    Running,
    Finished,
    Failed,
    Cancelled,
}

impl OperationState {
    /// Whether this state keeps the operation in the in-flight set (PRG-FR-02).
    pub fn is_in_flight(self) -> bool {
        matches!(self, OperationState::Running)
    }
}

/// PRG-FR-KXQW: where an activity's owning surface is, and which of its targets
/// to select.
///
/// Supplied by the producer and carried unchanged. Nothing here is derived from
/// `kind` or `label` (PRG-FR-TBZN), and a producer with no defined destination
/// registers its operation without one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum Activation {
    /// A graduation run, by its run id (GRD-FR-ZHNV).
    GraduationRun { run_id: String },
    /// A discussion, by its discussion id (AGC-FR-24).
    Discussion { discussion_id: String },
    /// A Git push, by the name of the branch it publishes (GTC-FR-22).
    GitPush { branch: String },
}

/// One reported operation. `completed`/`total` are both present on a determinate
/// operation and both absent on an indeterminate one (PRG-FR-05).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    /// Unique for the lifetime of the running application, never reused
    /// (PRG-FR-03).
    pub id: String,
    /// The producing module, e.g. `"scan"` | `"changes"` | `"git"` (PRG-FR-12).
    pub kind: String,
    /// Short human-readable description the status bar renders.
    pub label: String,
    pub state: OperationState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Monotonically increasing registration ordinal — the ordering key of
    /// PRG-FR-02.
    pub sequence: u64,
    /// PRG-FR-KXQW: the producer's destination for this operation, absent when
    /// it has none (PRG-FR-TBZN).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activation: Option<Activation>,
    /// PRG-FR-13: the content root this operation is scoped to, so closing a
    /// project or changing the active worktree can terminate exactly the
    /// operations running against the outgoing root. `None` for an operation
    /// that is not project-scoped (a plugin install, PRG-FR-15).
    ///
    /// Deliberately off the wire: the contract's payload shape has no scope
    /// field, and no consumer needs one.
    #[serde(skip)]
    pub scope: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// The emitter seam
// ---------------------------------------------------------------------------

/// Where a progress event goes. Implemented by `AppHandle` in production and by
/// a collecting stub in the tests, so the registry's behaviour is exercised
/// without a Tauri runtime.
pub trait ProgressSink {
    fn publish(&self, operation: &Operation);
}

impl<R: tauri::Runtime> ProgressSink for tauri::AppHandle<R> {
    fn publish(&self, operation: &Operation) {
        // PRG-FR-06: reporting never blocks the work being reported, and no
        // consumer's absence or failure delays it — an emit that cannot be
        // delivered is dropped rather than propagated. It is still logged: a
        // dropped *terminal* event is not cosmetic, it strands the operation in
        // the status bar's in-flight set (STB-FR-06), which leaves an
        // indeterminate progress bar animating forever on an idle window.
        if let Err(e) = self.emit(OPERATION_PROGRESS, operation) {
            eprintln!(
                "synthesis: failed to emit {OPERATION_PROGRESS} for {}: {e}",
                operation.id
            );
        }
    }
}

/// Emit every event in `events` in order. The registry hands them out already
/// ordered (a coalesced trailing update always precedes its terminal event).
///
/// A failing emit cannot abort the batch, and that is a property of the type
/// rather than of this loop: `ProgressSink::publish` returns `()`, so a sink has
/// no way to report failure upward and no way to stop the iteration. That is
/// also why the `AppHandle` impl above logs rather than returns — and why there
/// is no test for it here. The tests below drive the registry through the
/// `Recorder` stub, which cannot fail; building a failing-sink seam would
/// exercise the loop's `for`, not any behaviour the trait permits to vary.
fn publish_all(sink: &impl ProgressSink, events: &[Operation]) {
    for event in events {
        sink.publish(event);
    }
}

// ---------------------------------------------------------------------------
// The registry
// ---------------------------------------------------------------------------

/// One in-flight operation's bookkeeping, beyond what goes on the wire.
#[derive(Clone, Debug)]
struct Tracked {
    operation: Operation,
    /// When an event for this operation was last emitted, for PRG-FR-07's
    /// coalescing window.
    last_emit: Instant,
    /// Set when an update was coalesced away. The value is already folded into
    /// `operation`; the flag records that it has not yet been *delivered*, so a
    /// trailing emit is owed before the operation terminates.
    pending_update: bool,
}

#[derive(Default)]
struct Inner {
    /// In-flight operations keyed by `sequence`, so iteration is already in
    /// registration order and `rev()` is PRG-FR-02's descending order.
    tracked: BTreeMap<u64, Tracked>,
    /// PRG-FR-03: monotonic, never reset within a run, so no id is ever reused
    /// and no later `sequence` collides with an earlier one.
    next_sequence: u64,
}

/// The in-flight set. Held in memory only and bounded by the number of
/// operations genuinely running (PRG-FR-10).
#[derive(Default)]
pub struct ProgressRegistry {
    inner: Mutex<Inner>,
}

/// A handle on a registered operation, used to report progress and to terminate
/// it. Carries no reference to the registry, so it is cheap to pass around.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationId(u64);

impl ProgressRegistry {
    /// PRG-FR-02 / PRG-FR-04: register an operation and return it together with
    /// the registration event that is due. Registration events are never
    /// coalesced away.
    pub fn register_at(
        &self,
        kind: &str,
        label: &str,
        scope: Option<PathBuf>,
        total: Option<u64>,
        now: Instant,
    ) -> (OperationId, Operation) {
        self.register_with_activation_at(kind, label, scope, total, None, now)
    }

    /// [`register_at`](Self::register_at) for an operation that names a
    /// destination (PRG-FR-KXQW). The destination is fixed at registration and
    /// every later event of the operation carries it unchanged.
    pub fn register_with_activation_at(
        &self,
        kind: &str,
        label: &str,
        scope: Option<PathBuf>,
        total: Option<u64>,
        activation: Option<Activation>,
        now: Instant,
    ) -> (OperationId, Operation) {
        let mut inner = self.lock();
        let sequence = inner.next_sequence;
        inner.next_sequence += 1;
        let operation = Operation {
            // PRG-FR-03: derived from the same monotonic counter as `sequence`,
            // so uniqueness across the run needs no separate guarantee.
            id: format!("op-{sequence}"),
            kind: kind.to_string(),
            label: label.to_string(),
            state: OperationState::Running,
            // PRG-FR-05: determinate iff a total is known; `completed` moves in
            // step with it and is never present on its own.
            completed: total.map(|_| 0),
            total,
            sequence,
            activation,
            scope,
        };
        inner.tracked.insert(
            sequence,
            Tracked {
                operation: operation.clone(),
                last_emit: now,
                pending_update: false,
            },
        );
        (OperationId(sequence), operation)
    }

    /// PRG-FR-05 / PRG-FR-07: advance an operation's progress and/or label,
    /// returning the event due — `None` when the update fell inside the
    /// coalescing window and was folded into the operation without being
    /// delivered.
    ///
    /// `completed` never exceeds `total` and neither value decreases: an update
    /// that would move either backwards is clamped rather than rejected, so a
    /// mis-reporting producer cannot make the bar jump about. An operation that
    /// starts indeterminate becomes determinate the first time a total arrives
    /// and never reverts.
    pub fn update_at(
        &self,
        id: OperationId,
        completed: Option<u64>,
        total: Option<u64>,
        label: Option<&str>,
        now: Instant,
    ) -> Option<Operation> {
        let mut inner = self.lock();
        let tracked = inner.tracked.get_mut(&id.0)?;
        // PRG-FR-08: an operation that has already terminated is gone from the
        // map entirely, so no post-terminal event can be produced here.
        if let Some(total) = total {
            let previous = tracked.operation.total.unwrap_or(0);
            tracked.operation.total = Some(total.max(previous));
        }
        if let Some(completed) = completed {
            let previous = tracked.operation.completed.unwrap_or(0);
            let ceiling = tracked.operation.total.unwrap_or(u64::MAX);
            tracked.operation.completed = Some(completed.max(previous).min(ceiling));
        }
        // PRG-FR-05: an operation is determinate iff it carries BOTH fields, so
        // neither may travel alone. A total arriving after registration brings
        // `completed` with it; a `completed` reported against no total is a
        // reporter that does not yet know its total, and is dropped rather than
        // sent as a third state the consumer has no rendering for (STB-FR-08
        // branches on exactly this pair).
        match (tracked.operation.total, tracked.operation.completed) {
            (Some(_), None) => tracked.operation.completed = Some(0),
            (None, Some(_)) => tracked.operation.completed = None,
            _ => {}
        }
        if let Some(label) = label {
            tracked.operation.label = label.to_string();
        }
        if now.duration_since(tracked.last_emit) < COALESCE_WINDOW {
            tracked.pending_update = true;
            return None;
        }
        tracked.last_emit = now;
        tracked.pending_update = false;
        Some(tracked.operation.clone())
    }

    /// PRG-FR-08 / PRG-FR-09: move an operation to a terminal state and return
    /// the events due, in order. A coalesced-but-undelivered update is flushed
    /// first, so the last delivered update carries the latest values; the
    /// terminal event follows and is emitted exactly once, after which the
    /// operation is absent from the registry entirely.
    ///
    /// A terminal state carries no error detail — reporting a failure belongs to
    /// the module that owns the operation (PRG-FR-09).
    pub fn terminate(&self, id: OperationId, state: OperationState) -> Vec<Operation> {
        let mut inner = self.lock();
        let Some(mut tracked) = inner.tracked.remove(&id.0) else {
            // Already terminated: exactly one terminal event per operation.
            return Vec::new();
        };
        let mut events = Vec::new();
        if tracked.pending_update {
            events.push(tracked.operation.clone());
        }
        tracked.operation.state = if state.is_in_flight() {
            // Terminating *into* a running state is a caller error; treat it as
            // a plain finish rather than stranding the operation as permanently
            // running (PRG-FR-09).
            OperationState::Finished
        } else {
            state
        };
        events.push(tracked.operation);
        events
    }

    /// PRG-FR-02: the operations in flight at the moment of the call, ordered by
    /// descending `sequence` so the first element is the most recently started.
    /// Empty when none is in flight; never an error.
    pub fn in_flight(&self) -> Vec<Operation> {
        self.lock()
            .tracked
            .values()
            .rev()
            .map(|t| t.operation.clone())
            .collect()
    }

    /// PRG-FR-13: terminate every operation scoped to `root`, returning the
    /// terminal events due. Operations with no scope, and those scoped to a
    /// different root, are left running — a plugin install is not a project's
    /// business (PRG-FR-15).
    pub fn terminate_scope(&self, root: &Path) -> Vec<Operation> {
        let ids: Vec<OperationId> = {
            let inner = self.lock();
            inner
                .tracked
                .values()
                .filter(|t| t.operation.scope.as_deref() == Some(root))
                .map(|t| OperationId(t.operation.sequence))
                .collect()
        };
        ids.into_iter()
            .flat_map(|id| self.terminate(id, OperationState::Cancelled))
            .collect()
    }

    /// A poisoned registry must not take the application down with it: progress
    /// is peripheral to every operation it reports on (PRG-FR-14).
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

// ---------------------------------------------------------------------------
// Attribution (PRG-FR-11)
// ---------------------------------------------------------------------------

/// Run `work` with progress attribution: register an operation, run it, and
/// terminate it either way.
///
/// The terminal event is emitted on the failure path too, so a failure never
/// strands an operation as permanently running (PRG-FR-09) — that is the whole
/// reason the work goes *through* this function rather than being bracketed by
/// two calls at each site.
///
/// Attribution is opt-in (PRG-FR-11): a module that never calls this registers
/// nothing, and adding a call later needs no change to the contract.
pub fn attribute<T, E>(
    sink: &impl ProgressSink,
    registry: &ProgressRegistry,
    kind: &str,
    label: &str,
    scope: Option<PathBuf>,
    work: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    attribute_with_activation(sink, registry, kind, label, scope, None, work)
}

/// [`attribute`] for work that names a destination (PRG-FR-KXQW).
pub fn attribute_with_activation<T, E>(
    sink: &impl ProgressSink,
    registry: &ProgressRegistry,
    kind: &str,
    label: &str,
    scope: Option<PathBuf>,
    activation: Option<Activation>,
    work: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    let (id, registration) = registry.register_with_activation_at(
        kind,
        label,
        scope,
        None,
        activation,
        Instant::now(),
    );
    sink.publish(&registration);
    // A panic in attributed work — git2 hitting a repository race, a scan
    // tripping an unwrap — must terminate the operation too, or it is stranded
    // as permanently running for the rest of the session, which is exactly what
    // PRG-FR-09 forbids. `?`-style early returns are already covered by the
    // `Result`, but an unwind would skip the terminate entirely.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
    let state = match &outcome {
        Ok(Ok(_)) => OperationState::Finished,
        _ => OperationState::Failed,
    };
    publish_all(sink, &registry.terminate(id, state));
    match outcome {
        Ok(result) => result,
        // The operation is out of the in-flight set; the panic itself belongs to
        // the caller, so it is re-raised rather than swallowed into an error
        // this module has no vocabulary for (PRG-FR-09: no error detail here).
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

/// Register an operation and publish its registration event, returning the
/// handle the caller advances and terminates it with.
///
/// The counterpart to [`attribute`] for work that does not fit inside a single
/// closure: a search (`SCC-search.md` SCC-FR-18) registers here, streams for as
/// long as it runs on its own threads, and terminates from wherever it ends.
/// The caller owns the guarantee `attribute` makes for free — that every path
/// out reaches [`terminate_and_publish`] (PRG-FR-09).
pub fn register_and_publish(
    sink: &impl ProgressSink,
    registry: &ProgressRegistry,
    kind: &str,
    label: &str,
    scope: Option<PathBuf>,
    total: Option<u64>,
) -> OperationId {
    register_and_publish_with_activation(sink, registry, kind, label, scope, total, None)
}

/// [`register_and_publish`] for an operation that names a destination
/// (PRG-FR-KXQW).
pub fn register_and_publish_with_activation(
    sink: &impl ProgressSink,
    registry: &ProgressRegistry,
    kind: &str,
    label: &str,
    scope: Option<PathBuf>,
    total: Option<u64>,
    activation: Option<Activation>,
) -> OperationId {
    let (id, registration) = registry.register_with_activation_at(
        kind,
        label,
        scope,
        total,
        activation,
        Instant::now(),
    );
    sink.publish(&registration);
    id
}

/// PRG-FR-05 / PRG-FR-07: advance an operation, publishing the event if one is
/// due. An update that falls inside the coalescing window publishes nothing.
pub fn update_and_publish(
    sink: &impl ProgressSink,
    registry: &ProgressRegistry,
    id: OperationId,
    completed: Option<u64>,
    total: Option<u64>,
) {
    if let Some(event) = registry.update_at(id, completed, total, None, Instant::now()) {
        sink.publish(&event);
    }
}

/// PRG-FR-08 / PRG-FR-09: move an operation to a terminal state and publish the
/// events due (a coalesced trailing update, then exactly one terminal event).
pub fn terminate_and_publish(
    sink: &impl ProgressSink,
    registry: &ProgressRegistry,
    id: OperationId,
    state: OperationState,
) {
    publish_all(sink, &registry.terminate(id, state));
}

/// PRG-FR-13: terminate and publish every operation scoped to the outgoing
/// content root, as part of a project close or worktree change.
pub fn terminate_scope_and_publish(
    sink: &impl ProgressSink,
    registry: &ProgressRegistry,
    root: &Path,
) {
    publish_all(sink, &registry.terminate_scope(root));
}

// ---------------------------------------------------------------------------
// Tauri command (PRG contract surface)
// ---------------------------------------------------------------------------

/// PRG-FR-02 / PRG-FR-15: the operations in flight right now, most-recently
/// started first. Answers normally with no project open, because an operation
/// need not be project-scoped. Never returns an error.
#[tauri::command]
pub fn list_in_flight_operations(registry: State<'_, ProgressRegistry>) -> Vec<Operation> {
    registry.in_flight()
}

#[cfg(test)]
mod tests;
