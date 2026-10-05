//! What the application holds about the runs that are working right now
//! (GRD-FR-BNTC, GRD-FR-WSXA).

use std::collections::BTreeMap;

use super::*;
use crate::tools::agent_exec::runtime::CancellationToken;

/// One run the application is driving.
pub(super) struct RunningRun {
    pub(super) run_id: String,
    pub(super) cancellation: CancellationToken,
    /// Why the turn was stopped, once something stopped it.
    ///
    /// The token carries no reason of its own, and the reason is what the
    /// author is shown: a run they paused and a run a shutdown stopped ask
    /// different things of them (GRD-FR-MDQZ, GRD-FR-TWMA).
    pub(super) stopped_because: Option<GraduationInterruptionReason>,
    /// WTC-FR-FBJQ: whether the run works directly in a worktree, so a switch
    /// guard sees it from the claim, before its record is saved as dispatched.
    pub(super) direct: bool,
}

/// GRD-FR-BNTC / GRD-FR-WSXA: which stream each working run holds, and the one
/// exclusive hold every write of this module takes.
pub struct GraduationState {
    /// stream id → the run driving it. One run of a stream works at a time.
    running: Mutex<BTreeMap<String, RunningRun>>,
    /// GRD-FR-WSXA: order, membership and eligibility are settled under one
    /// hold, so no reorder interleaves with a dispatch.
    lock: Mutex<()>,
    /// GRD-FR-NYSH: one dispatch pass at a time, so two runs that end together
    /// cannot each offer a slot to a different queue and break project order.
    dispatch_lock: Mutex<()>,
    /// The store root, where it is not the application's own. A test roots it
    /// at a temporary directory.
    root: Mutex<Option<PathBuf>>,
    /// The runs a loop of this process is driving, from the claim until the loop
    /// has returned.
    ///
    /// Wider than the claim map above. A discard or a pause releases the claim
    /// while the turn it cancelled is still stopping, and what a run owns must not
    /// be reclaimed under that turn (GRD-FR-KZPT).
    loops: Mutex<std::collections::BTreeSet<String>>,
    /// GRD-FR-ZHNV: the progress operation of each run that is executing, keyed
    /// by run id. At most one for a run.
    operations: Mutex<BTreeMap<String, crate::progress::OperationId>>,
    /// Whether the loop may dispatch at all.
    ///
    /// **On by default**, because the state the application manages is built by
    /// `Default` and a run that cannot be dispatched is a queue that never
    /// moves. Only a test that drives the state machine by hand turns it off.
    loop_enabled: std::sync::atomic::AtomicBool,
}

impl Default for GraduationState {
    fn default() -> Self {
        Self {
            running: Mutex::new(BTreeMap::new()),
            lock: Mutex::new(()),
            dispatch_lock: Mutex::new(()),
            loops: Mutex::new(std::collections::BTreeSet::new()),
            operations: Mutex::new(BTreeMap::new()),
            root: Mutex::new(None),
            loop_enabled: std::sync::atomic::AtomicBool::new(true),
        }
    }
}

impl GraduationState {
    /// A state whose store stands under a root the caller names.
    pub fn rooted_at(root: PathBuf) -> Self {
        Self {
            root: Mutex::new(Some(root)),
            ..Default::default()
        }
    }

    /// Whether a run may be dispatched at all.
    pub(super) fn loop_enabled(&self) -> bool {
        self.loop_enabled.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Turn dispatch off, or back on. A test that settles the state machine by
    /// hand uses this so no loop starts underneath it.
    #[cfg(test)]
    pub(crate) fn set_loop_enabled(&self, enabled: bool) {
        self.loop_enabled
            .store(enabled, std::sync::atomic::Ordering::SeqCst);
    }

    /// The store root this state reads and writes under.
    pub fn root(&self) -> Option<PathBuf> {
        self.root.lock().ok().and_then(|guard| guard.clone())
    }

    /// GRD-FR-WSXA: take the exclusive hold.
    pub fn write_guard(&self) -> std::sync::MutexGuard<'_, ()> {
        match self.lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// GRD-FR-NYSH: take the hold of one dispatch pass.
    pub fn dispatch_guard(&self) -> std::sync::MutexGuard<'_, ()> {
        match self.dispatch_lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// GRD-FR-BNTC, GRD-FR-KKKN: claim a queue and a project slot for a run,
    /// atomically.
    ///
    /// Refuses when the queue's stream or worktree already holds a run, when
    /// this run already holds another queue, and when the project's limit has no
    /// free slot. A limit lowered below the slots held refuses every claim until
    /// enough are released (GRD-FR-IJKV). A refused claim changes nothing.
    /// `direct` marks a run that works directly in a worktree (WTC-FR-FBJQ).
    pub fn claim(
        &self,
        stream_id: &str,
        run_id: &str,
        direct: bool,
        limit: crate::project_settings::GraduationConcurrency,
        cancellation: CancellationToken,
    ) -> bool {
        let Ok(mut running) = self.running.lock() else {
            return false;
        };
        if running.contains_key(stream_id) {
            return false;
        }
        if running.values().any(|held| held.run_id == run_id) {
            return false;
        }
        if !limit.permits(running.len()) {
            return false;
        }
        running.insert(
            stream_id.to_string(),
            RunningRun {
                run_id: run_id.to_string(),
                cancellation,
                stopped_because: None,
                direct,
            },
        );
        true
    }

    /// WTC-FR-FBJQ: the run of a direct claim held right now, if any.
    ///
    /// Covers the window between the claim and the save of the run as
    /// dispatched, which the run order index does not yet show.
    pub fn direct_claim(&self) -> Option<String> {
        self.running.lock().ok().and_then(|running| {
            running
                .values()
                .find(|held| held.direct)
                .map(|held| held.run_id.clone())
        })
    }

    /// Release a stream, whatever run held it.
    pub fn release(&self, stream_id: &str) {
        if let Ok(mut running) = self.running.lock() {
            running.remove(stream_id);
        }
    }

    /// The run holding a stream right now.
    pub fn holder_of(&self, stream_id: &str) -> Option<String> {
        self.running
            .lock()
            .ok()
            .and_then(|r| r.get(stream_id).map(|held| held.run_id.clone()))
    }

    /// GRD-FR-XVUD: whether a loop of this process is behind the named run.
    ///
    /// A run the store holds as working that this answers `false` for was held
    /// when the application stopped.
    pub fn is_driving(&self, run_id: &str) -> bool {
        self.running
            .lock()
            .map(|r| r.values().any(|held| held.run_id == run_id))
            .unwrap_or(false)
    }

    /// A loop of this process begins driving a run.
    pub(super) fn begin_loop(&self, run_id: &str) {
        if let Ok(mut loops) = self.loops.lock() {
            loops.insert(run_id.to_string());
        }
    }

    /// GRD-FR-ZHNV: register the run's operation unless it has one. Answers
    /// whether this call registered it. The hold spans the check and the
    /// registration, so two changes that arrive together register one.
    pub(super) fn ensure_operation(
        &self,
        run_id: &str,
        register: impl FnOnce() -> Option<crate::progress::OperationId>,
    ) -> bool {
        let mut operations = self
            .operations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if operations.contains_key(run_id) {
            return false;
        }
        match register() {
            Some(operation) => {
                operations.insert(run_id.to_string(), operation);
                true
            }
            None => false,
        }
    }

    /// GRD-FR-ZHNV: take the run's operation, so it is ended exactly once.
    pub(super) fn take_operation(&self, run_id: &str) -> Option<crate::progress::OperationId> {
        self.operations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(run_id)
    }

    /// The loop of this process has returned from a run.
    pub(super) fn end_loop(&self, run_id: &str) {
        if let Ok(mut loops) = self.loops.lock() {
            loops.remove(run_id);
        }
    }

    /// Whether a loop of this process is still inside the named run, whether or
    /// not it still holds the run's claim.
    pub fn is_looping(&self, run_id: &str) -> bool {
        self.loops
            .lock()
            .map(|loops| loops.contains(run_id))
            .unwrap_or(false)
    }

    /// GRD-FR-KKKN: how many project slots are held.
    pub fn working_count(&self) -> usize {
        self.running.lock().map(|r| r.len()).unwrap_or(0)
    }

    /// GRD-FR-MDQZ: stop the turn one run is making, and say why.
    pub fn cancel_run(&self, run_id: &str, reason: GraduationInterruptionReason) -> bool {
        let Ok(mut running) = self.running.lock() else {
            return false;
        };
        for held in running.values_mut() {
            if held.run_id == run_id {
                held.stopped_because = Some(reason);
                held.cancellation.cancel();
                return true;
            }
        }
        false
    }

    /// GRD-FR-TWMA: a project change and a shutdown stop every running turn.
    pub fn cancel_all(&self, reason: GraduationInterruptionReason) {
        if let Ok(mut running) = self.running.lock() {
            for held in running.values_mut() {
                held.stopped_because = Some(reason);
                held.cancellation.cancel();
            }
        }
    }

    /// Why the turn of a run was stopped, where something stopped it.
    pub fn cancellation_reason(&self, run_id: &str) -> Option<GraduationInterruptionReason> {
        self.running
            .lock()
            .ok()?
            .values()
            .find(|held| held.run_id == run_id)
            .and_then(|held| held.stopped_because)
    }
}
