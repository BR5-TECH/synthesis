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
    /// GRD-FR-QWLP: the loop this claim was taken for, or [`NO_LOOP`] for a
    /// claim that drives no loop.
    pub(super) loop_id: u64,
}

/// GRD-FR-QWLP: the loop id of a claim that drives no loop.
pub const NO_LOOP: u64 = 0;

/// GRD-FR-QWLP: one loop of this process, from its claim until it returns.
struct LoopEntry {
    queue_key: String,
    loop_id: u64,
    /// GRD-FR-HRXA: why a command stopped the loop. Kept here as well as on the
    /// claim, because the command's rest releases the claim before the loop
    /// returns, and the loop must still know who stopped it.
    stopped_because: Option<GraduationInterruptionReason>,
}

/// EAC-FR-LQRI: the check an idle stop runs again once it holds the VM.
#[cfg(test)]
pub(crate) type IdleRecheck = Box<dyn FnOnce() -> bool + Send>;

/// GRD-FR-UESX: one VM stop that was asked for, as a test reads it.
#[cfg(test)]
struct IdleStop {
    queue_key: String,
    vm: String,
    recheck: Option<IdleRecheck>,
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
    ///
    /// GRD-FR-QWLP: keyed by run id, and naming the queue the loop stands in, so
    /// that no claim is taken in that run or that queue until the loop returns.
    loops: Mutex<BTreeMap<String, LoopEntry>>,
    /// GRD-FR-XVUD: whether the first read of the queue has swept the runs
    /// that no loop stands behind. Later reads sweep nothing.
    swept: std::sync::atomic::AtomicBool,
    /// GRD-FR-QWLP: the id the next loop takes. Never [`NO_LOOP`].
    next_loop_id: std::sync::atomic::AtomicU64,
    /// GRD-FR-ZHNV: the progress operation of each run that is executing, keyed
    /// by run id. At most one for a run.
    operations: Mutex<BTreeMap<String, crate::progress::OperationId>>,
    /// Whether the loop may dispatch at all.
    ///
    /// **On by default**, because the state the application manages is built by
    /// `Default` and a run that cannot be dispatched is a queue that never
    /// moves. Only a test that drives the state machine by hand turns it off.
    loop_enabled: std::sync::atomic::AtomicBool,
    /// GRD-FR-UESX: the VM stops that were asked for, in order, for a test to
    /// read.
    #[cfg(test)]
    idle_stops: Mutex<Vec<IdleStop>>,
}

impl Default for GraduationState {
    fn default() -> Self {
        Self {
            running: Mutex::new(BTreeMap::new()),
            lock: Mutex::new(()),
            dispatch_lock: Mutex::new(()),
            loops: Mutex::new(BTreeMap::new()),
            next_loop_id: std::sync::atomic::AtomicU64::new(NO_LOOP + 1),
            swept: std::sync::atomic::AtomicBool::new(false),
            operations: Mutex::new(BTreeMap::new()),
            root: Mutex::new(None),
            loop_enabled: std::sync::atomic::AtomicBool::new(true),
            #[cfg(test)]
            idle_stops: Mutex::new(Vec::new()),
        }
    }
}

impl GraduationState {
    /// A state whose store stands under a root the caller names.
    pub fn rooted_at(root: PathBuf) -> Self {
        Self { root: Mutex::new(Some(root)), ..Default::default() }
    }

    /// GRD-FR-UESX: note that the VM stop of an idle queue was asked for, with
    /// a copy of the check the stop runs again once it holds the VM.
    #[cfg(test)]
    pub(super) fn record_idle_stop(&self, queue_key: &str, vm: &str, recheck: IdleRecheck) {
        if let Ok(mut stops) = self.idle_stops.lock() {
            stops.push(IdleStop {
                queue_key: queue_key.to_string(),
                vm: vm.to_string(),
                recheck: Some(recheck),
            });
        }
    }

    /// GRD-FR-UESX: the queues whose VM stop was asked for, in order.
    #[cfg(test)]
    pub(crate) fn idle_stops(&self) -> Vec<String> {
        self.idle_stops
            .lock()
            .map(|stops| stops.iter().map(|stop| stop.queue_key.clone()).collect())
            .unwrap_or_default()
    }

    /// EAC-FR-LQRI: the names of the VMs whose stop was asked for, in order.
    #[cfg(test)]
    pub(crate) fn idle_stop_vms(&self) -> Vec<String> {
        self.idle_stops
            .lock()
            .map(|stops| stops.iter().map(|stop| stop.vm.clone()).collect())
            .unwrap_or_default()
    }

    /// EAC-FR-LQRI: the check that the stop at `index` runs again once it
    /// holds the VM. Each check is given out once.
    #[cfg(test)]
    pub(crate) fn take_idle_recheck(&self, index: usize) -> Option<IdleRecheck> {
        self.idle_stops.lock().ok()?.get_mut(index)?.recheck.take()
    }

    /// Whether a run may be dispatched at all.
    pub(super) fn loop_enabled(&self) -> bool {
        self.loop_enabled.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Turn dispatch off, or back on. A test that settles the state machine by
    /// hand uses this so no loop starts underneath it.
    #[cfg(test)]
    pub(crate) fn set_loop_enabled(&self, enabled: bool) {
        self.loop_enabled.store(enabled, std::sync::atomic::Ordering::SeqCst);
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
    /// atomically, where the stored runs hold no slot.
    pub fn claim(
        &self,
        stream_id: &str,
        run_id: &str,
        direct: bool,
        limit: crate::project_settings::GraduationConcurrency,
        cancellation: CancellationToken,
    ) -> bool {
        self.claim_with_holders(
            stream_id,
            run_id,
            direct,
            limit,
            &std::collections::BTreeSet::new(),
            cancellation,
        )
    }

    /// GRD-FR-BNTC, GRD-FR-KKKN: claim a queue and a project slot for a run,
    /// atomically.
    ///
    /// `holders` names the runs the stored records say hold a project slot. A
    /// run at rest holds one as a run at work does, so the count is the claims of
    /// this process together with those stored holders no claim names.
    ///
    /// Refuses when the queue's stream or worktree already holds a run, when
    /// this run already holds another queue, and when the project's limit has no
    /// free slot for a run that holds none. A run that already holds its slot
    /// takes no new claim on the limit (GRD-FR-KKKN), so a limit lowered below
    /// the slots held refuses the claims of other runs until enough are released
    /// (GRD-FR-IJKV). A refused claim changes nothing. `direct` marks a run that
    /// works directly in a worktree (WTC-FR-FBJQ).
    pub fn claim_with_holders(
        &self,
        stream_id: &str,
        run_id: &str,
        direct: bool,
        limit: crate::project_settings::GraduationConcurrency,
        holders: &std::collections::BTreeSet<String>,
        cancellation: CancellationToken,
    ) -> bool {
        self.claim_inner(stream_id, run_id, direct, limit, holders, cancellation, false).is_some()
    }

    /// GRD-FR-BNTC, GRD-FR-QWLP: claim a queue and a project slot for a loop
    /// that drives the run, and register that loop, in one step.
    ///
    /// Answers the id of the loop, or `None` where the claim is refused. A loop
    /// that has not returned holds its run and its queue, so a second loop
    /// cannot start in the same working copy while the first one stops.
    pub fn claim_loop(
        &self,
        stream_id: &str,
        run_id: &str,
        direct: bool,
        limit: crate::project_settings::GraduationConcurrency,
        holders: &std::collections::BTreeSet<String>,
        cancellation: CancellationToken,
    ) -> Option<u64> {
        self.claim_inner(stream_id, run_id, direct, limit, holders, cancellation, true)
    }

    #[allow(clippy::too_many_arguments)] // The claim's inputs, plus whether a loop is registered.
    fn claim_inner(
        &self,
        stream_id: &str,
        run_id: &str,
        direct: bool,
        limit: crate::project_settings::GraduationConcurrency,
        holders: &std::collections::BTreeSet<String>,
        cancellation: CancellationToken,
        register_loop: bool,
    ) -> Option<u64> {
        // The lock order is `running`, then `loops`, everywhere both are held.
        let mut running = self.running.lock().ok()?;
        if running.contains_key(stream_id) {
            return None;
        }
        if running.values().any(|held| held.run_id == run_id) {
            return None;
        }
        let holds_already = holders.contains(run_id);
        if !holds_already && !limit.permits(Self::slots_among(&running, holders)) {
            return None;
        }
        let mut loops = self.loops.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        // GRD-FR-QWLP: a loop that a command stopped still stands in its run and
        // its queue until it returns.
        if loops.contains_key(run_id) || loops.values().any(|entry| entry.queue_key == stream_id) {
            return None;
        }
        let loop_id = if register_loop {
            let loop_id = self.next_loop_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            loops.insert(
                run_id.to_string(),
                LoopEntry { queue_key: stream_id.to_string(), loop_id, stopped_because: None },
            );
            loop_id
        } else {
            NO_LOOP
        };
        running.insert(
            stream_id.to_string(),
            RunningRun {
                run_id: run_id.to_string(),
                cancellation,
                stopped_because: None,
                direct,
                loop_id,
            },
        );
        Some(loop_id)
    }

    /// WTC-FR-FBJQ: the run of a direct claim held right now, if any.
    ///
    /// Covers the window between the claim and the save of the run as
    /// dispatched, which the run order index does not yet show.
    pub fn direct_claim(&self) -> Option<String> {
        self.running.lock().ok().and_then(|running| {
            running.values().find(|held| held.direct).map(|held| held.run_id.clone())
        })
    }

    /// Release a stream, whatever run held it.
    pub fn release(&self, stream_id: &str) {
        if let Ok(mut running) = self.running.lock() {
            running.remove(stream_id);
        }
    }

    /// Release a stream where this run is the one that holds the claim on it, so
    /// a loop that is ending never drops the claim of another run.
    pub fn release_run(&self, stream_id: &str, run_id: &str) {
        if let Ok(mut running) = self.running.lock() {
            if running.get(stream_id).is_some_and(|held| held.run_id == run_id) {
                running.remove(stream_id);
            }
        }
    }

    /// GRD-FR-QWLP: release the claim one loop took, and no other claim, also
    /// not a later claim of the same run.
    pub(super) fn release_loop_claim(&self, stream_id: &str, run_id: &str, loop_id: u64) {
        if let Ok(mut running) = self.running.lock() {
            if running
                .get(stream_id)
                .is_some_and(|held| held.run_id == run_id && held.loop_id == loop_id)
            {
                running.remove(stream_id);
            }
        }
    }

    /// The run holding a stream right now.
    pub fn holder_of(&self, stream_id: &str) -> Option<String> {
        self.running.lock().ok().and_then(|r| r.get(stream_id).map(|held| held.run_id.clone()))
    }

    /// GRD-FR-XVUD: whether a loop of this process is behind the named run.
    ///
    /// A run the store holds as working that this answers `false` for was held
    /// when the application stopped.
    pub fn is_driving(&self, run_id: &str) -> bool {
        self.running.lock().map(|r| r.values().any(|held| held.run_id == run_id)).unwrap_or(false)
    }

    /// GRD-FR-ZHNV: register the run's operation unless it has one. Answers
    /// whether this call registered it. The hold spans the check and the
    /// registration, so two changes that arrive together register one.
    pub(super) fn ensure_operation(
        &self,
        run_id: &str,
        register: impl FnOnce() -> Option<crate::progress::OperationId>,
    ) -> bool {
        let mut operations =
            self.operations.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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
        self.operations.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(run_id)
    }

    /// The loop of this process has returned from a run. Only the entry of
    /// that loop is removed (GRD-FR-QWLP).
    pub(super) fn end_loop(&self, run_id: &str, loop_id: u64) {
        if let Ok(mut loops) = self.loops.lock() {
            if loops.get(run_id).is_some_and(|entry| entry.loop_id == loop_id) {
                loops.remove(run_id);
            }
        }
    }

    /// Whether a loop of this process is still inside the named run, whether or
    /// not it still holds the run's claim.
    pub fn is_looping(&self, run_id: &str) -> bool {
        self.loops.lock().map(|loops| loops.contains_key(run_id)).unwrap_or(false)
    }

    /// GTE-FR-DQLR: the process stops and starts again. No claim and no loop of
    /// the old process stands, and the next read of the queue is the first.
    #[cfg(test)]
    pub(crate) fn relaunch_for_test(&self) {
        if let Ok(mut running) = self.running.lock() {
            running.clear();
        }
        if let Ok(mut loops) = self.loops.lock() {
            loops.clear();
        }
        self.swept.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// GRD-FR-XVUD: answers `true` once, for the first read of the queue.
    pub fn take_first_read(&self) -> bool {
        !self.swept.swap(true, std::sync::atomic::Ordering::SeqCst)
    }

    /// GRL-FR-WCZE: whether the loop that stands in the named run was stopped by
    /// a command. No other loop can stand in the run until that one returns.
    pub fn loop_stopped(&self, run_id: &str) -> bool {
        self.loops
            .lock()
            .map(|loops| loops.get(run_id).is_some_and(|entry| entry.stopped_because.is_some()))
            .unwrap_or(false)
    }

    /// GRD-FR-QWLP: whether a loop of this process still stands in the named
    /// queue, whether or not it still holds the queue's claim.
    pub fn is_loop_in_queue(&self, queue_key: &str) -> bool {
        self.loops
            .lock()
            .map(|loops| loops.values().any(|entry| entry.queue_key == queue_key))
            .unwrap_or(false)
    }

    /// How many runs of this process hold a claim right now.
    pub fn working_count(&self) -> usize {
        self.running.lock().map(|r| r.len()).unwrap_or(0)
    }

    /// GRD-FR-KKKN: how many project slots are held, given the runs the stored
    /// records say hold one.
    pub fn slots_in_use(&self, holders: &std::collections::BTreeSet<String>) -> usize {
        self.running
            .lock()
            .map(|running| Self::slots_among(&running, holders))
            .unwrap_or(holders.len())
    }

    /// The claims of this process together with the stored holders no claim
    /// names.
    fn slots_among(
        running: &BTreeMap<String, RunningRun>,
        holders: &std::collections::BTreeSet<String>,
    ) -> usize {
        let unclaimed =
            holders.iter().filter(|id| !running.values().any(|held| &held.run_id == *id)).count();
        running.len() + unclaimed
    }

    /// GRD-FR-MDQZ: stop the turn one run is making, and say why.
    pub fn cancel_run(&self, run_id: &str, reason: GraduationInterruptionReason) -> bool {
        let Ok(mut running) = self.running.lock() else {
            return false;
        };
        let mut loops = self.loops.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        for held in running.values_mut() {
            if held.run_id == run_id {
                held.stopped_because = Some(reason);
                held.cancellation.cancel();
                if let Some(entry) = loops.get_mut(run_id).filter(|e| e.loop_id == held.loop_id) {
                    entry.stopped_because = Some(reason);
                }
                return true;
            }
        }
        false
    }

    /// GRD-FR-TWMA: a project change and a shutdown stop every running turn.
    pub fn cancel_all(&self, reason: GraduationInterruptionReason) {
        if let Ok(mut running) = self.running.lock() {
            let mut loops = self.loops.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            for held in running.values_mut() {
                held.stopped_because = Some(reason);
                held.cancellation.cancel();
                if let Some(entry) =
                    loops.get_mut(&held.run_id).filter(|e| e.loop_id == held.loop_id)
                {
                    entry.stopped_because = Some(reason);
                }
            }
        }
    }

    /// Why the turn of a run was stopped, where something stopped it.
    ///
    /// GRD-FR-HRXA: the claim answers first. Where a command's rest already
    /// released it, the loop that still stands in the run answers.
    pub fn cancellation_reason(&self, run_id: &str) -> Option<GraduationInterruptionReason> {
        let claimed = self
            .running
            .lock()
            .ok()
            .and_then(|running| {
                running.values().find(|held| held.run_id == run_id).map(|h| h.stopped_because)
            })
            .flatten();
        claimed
            .or_else(|| self.loops.lock().ok().and_then(|loops| loops.get(run_id)?.stopped_because))
    }
}
