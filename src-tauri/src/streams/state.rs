//! Which stream a run holds, and how many streams may work at once
//! (WKS-FR-CYAG, WKS-FR-NDSV).
//!
//! The busy mark is durable, on the stream's own record, because a stream a
//! run held when the application stopped must be recognisable at the next
//! launch. This in-memory state is what serialises the claim itself.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::tools::agent_exec::runtime::CancellationToken;

/// WKS-FR-NDSV: the streams that hold a working run right now.
///
/// One run of a stream works at a time, and streams work at the same time up
/// to the project-wide graduation concurrency limit (GRD-FR-KKKN).
#[derive(Default)]
pub struct StreamState {
    /// stream id → the run holding it.
    holders: Mutex<BTreeMap<String, String>>,
    /// One exclusive hold over the stream store, so two writers cannot write
    /// the same record over each other.
    lock: Mutex<()>,
    /// WKS-FR-HLGN: the **repository update guard** — the one reconciliation
    /// running right now, with the stream it belongs to and the cancellation
    /// that stops it (GRB-FR-MWTC, GRB-FR-TXVL).
    ///
    /// One entry at most. A merge writes the base branch and an update stands on
    /// a pinned revision of it, so two of them at once in one repository would
    /// write one worktree from two directions (GRB-FR-JIRD). The token is held
    /// beside the hold rather than by the work alone, because the request that
    /// cancels it is a different call from the one running it.
    ///
    /// One `StreamState` serves one repository: the application holds a state
    /// for the open project, and a test roots one at a directory of its own.
    reconciling: Mutex<Option<Reconciling>>,
    /// The attempts this process has a turn behind, so a reclaim of what a
    /// killed process stranded never removes a live one.
    attempts: Mutex<BTreeSet<String>>,
    /// WKS-FR-QFTH: the streams this process has an update **job** behind, on
    /// the same terms as `jobs` (WKS-FR-DAKP).
    update_jobs: Mutex<BTreeSet<String>>,
    /// The store root, where it is not the application's own.
    ///
    /// Production leaves it unset and the store stands under
    /// `short_data_dir()`. A test roots it at a temporary directory, because a
    /// test that wrote to the real root would write beside every stream this
    /// machine holds.
    root: Mutex<Option<PathBuf>>,
}

impl StreamState {
    /// A state whose store stands under a root the caller names.
    pub fn rooted_at(root: PathBuf) -> Self {
        Self {
            root: Mutex::new(Some(root)),
            ..Default::default()
        }
    }

    /// The store root this state reads and writes under.
    pub fn root(&self) -> Option<PathBuf> {
        self.root.lock().ok().and_then(|guard| guard.clone())
    }

    /// Take the exclusive hold every write of this module takes.
    pub fn write_guard(&self) -> std::sync::MutexGuard<'_, ()> {
        match self.lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// WKS-FR-NDSV: claim a stream for a run.
    ///
    /// Refuses when the stream already holds a run, and when the project is
    /// already holding every slot of the project's limit. A claim that is
    /// refused changes nothing.
    pub fn claim(
        &self,
        stream_id: &str,
        run_id: &str,
        limit: crate::project_settings::GraduationConcurrency,
    ) -> bool {
        let Ok(mut holders) = self.holders.lock() else {
            return false;
        };
        if holders.contains_key(stream_id) {
            return false;
        }
        if !limit.permits(holders.len()) {
            return false;
        }
        holders.insert(stream_id.to_string(), run_id.to_string());
        true
    }

    /// Release a stream, whatever run held it.
    pub fn release(&self, stream_id: &str) {
        if let Ok(mut holders) = self.holders.lock() {
            holders.remove(stream_id);
        }
    }

    /// The run holding a stream right now, if one does.
    pub fn holder_of(&self, stream_id: &str) -> Option<String> {
        self.holders
            .lock()
            .ok()
            .and_then(|holders| holders.get(stream_id).cloned())
    }

    /// How many streams are working.
    pub fn working_count(&self) -> usize {
        self.holders.lock().map(|h| h.len()).unwrap_or(0)
    }

    /// WKS-FR-HLGN / WKS-FR-TSOA: take the repository update guard.
    ///
    /// The typed refusal of whichever reconciliation already holds it, where one
    /// does. The hold is released when the returned guard is dropped, so every
    /// early return releases it, and it carries the running work's own
    /// cancellation (GRB-FR-MWTC, GRB-FR-TXVL).
    pub fn begin_repository_update(
        &self,
        stream_id: &str,
        kind: Reconciliation,
    ) -> Result<RepositoryUpdateHold<'_>, String> {
        let mut reconciling = self.reconciling();
        if let Some(held) = reconciling.as_ref() {
            return Err(held.kind.in_progress_error().to_string());
        }
        let cancellation = CancellationToken::new();
        *reconciling = Some(Reconciling {
            stream_id: stream_id.to_string(),
            kind,
            cancellation: cancellation.clone(),
        });
        Ok(RepositoryUpdateHold {
            state: self,
            cancellation,
        })
    }

    /// GRB-FR-TXVL / WKS-FR-WEJK: stop the update of one stream.
    pub fn cancel_update(&self, stream_id: &str) -> bool {
        self.cancel_reconciliation(stream_id, Reconciliation::Update)
    }

    fn cancel_reconciliation(&self, stream_id: &str, kind: Reconciliation) -> bool {
        let reconciling = self.reconciling();
        match reconciling.as_ref() {
            Some(held) if held.stream_id == stream_id && held.kind == kind => {
                held.cancellation.cancel();
                true
            }
            _ => false,
        }
    }

    /// The attempt ids of every update this process is running.
    ///
    /// What the reclaim of a stranded attempt must not touch: an attempt with a
    /// live turn behind it is the one attempt that is not stranded.
    pub fn attempts_running(&self) -> Vec<String> {
        self.attempts
            .lock()
            .map(|attempts| attempts.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Record that this process is running `attempt_id` for an update.
    pub fn begin_attempt(&self, attempt_id: &str) {
        if let Ok(mut attempts) = self.attempts.lock() {
            attempts.insert(attempt_id.to_string());
        }
    }

    /// The attempt has completed, whatever it decided.
    pub fn end_attempt(&self, attempt_id: &str) {
        if let Ok(mut attempts) = self.attempts.lock() {
            attempts.remove(attempt_id);
        }
    }

    /// Whether a merge of this stream is running.
    pub fn is_merging(&self, stream_id: &str) -> bool {
        self.is_reconciling(stream_id, Reconciliation::Merge)
    }

    /// Whether an update of this stream is running.
    pub fn is_updating(&self, stream_id: &str) -> bool {
        self.is_reconciling(stream_id, Reconciliation::Update)
    }

    fn is_reconciling(&self, stream_id: &str, kind: Reconciliation) -> bool {
        self.reconciling()
            .as_ref()
            .is_some_and(|held| held.stream_id == stream_id && held.kind == kind)
    }

    fn end_reconciliation(&self) {
        *self.reconciling() = None;
    }

    /// The repository update guard's own hold, recovered from a poisoning.
    ///
    /// Every reader and writer of it recovers the same way. A lock that stayed
    /// poisoned would leave the guard held by a reconciliation that has gone,
    /// and every merge and every update of the process refused for ever with no
    /// cancellation able to clear it.
    fn reconciling(&self) -> std::sync::MutexGuard<'_, Option<Reconciling>> {
        match self.reconciling.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// WKS-FR-QFTH: claim the update job of one stream for this process.
    ///
    /// `false` where a job of that stream is already claimed. The claim is held
    /// from before the `running` record is written until after the outcome is
    /// written, so a sweep never calls a live update stranded.
    pub fn claim_update_job(&self, stream_id: &str) -> bool {
        self.update_jobs
            .lock()
            .map(|mut jobs| jobs.insert(stream_id.to_string()))
            .unwrap_or(false)
    }

    /// Whether this process has an update job behind the named stream.
    pub fn has_update_job(&self, stream_id: &str) -> bool {
        self.update_jobs
            .lock()
            .map(|jobs| jobs.contains(stream_id))
            .unwrap_or(false)
    }

    /// WKS-FR-QFTH: release an update job's claim.
    pub fn release_update_job(&self, stream_id: &str) {
        if let Ok(mut jobs) = self.update_jobs.lock() {
            jobs.remove(stream_id);
        }
    }

    /// Whether this process holds a loop behind the named stream.
    ///
    /// WKS-FR-LWEI reads it at launch: a stream the store marks busy that no
    /// live loop is behind was held when the application stopped.
    pub fn is_held_here(&self, stream_id: &str) -> bool {
        self.holder_of(stream_id).is_some()
    }
}

/// WKS-FR-TSOA: which of the two reconciliations holds the repository update
/// guard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reconciliation {
    /// The stream branch into its base branch.
    Merge,
    /// A pinned revision of the base branch into the stream branch.
    Update,
}

impl Reconciliation {
    /// The typed refusal a request made while this one holds the guard answers with.
    fn in_progress_error(self) -> &'static str {
        match self {
            Self::Merge => super::ERR_MERGE_IN_PROGRESS,
            Self::Update => super::ERR_UPDATE_IN_PROGRESS,
        }
    }
}

/// What one reconciliation holds while it runs.
struct Reconciling {
    stream_id: String,
    kind: Reconciliation,
    cancellation: CancellationToken,
}

/// WKS-FR-HLGN: what holds the repository update guard, for as long as one
/// merge or one update runs.
pub struct RepositoryUpdateHold<'a> {
    state: &'a StreamState,
    /// GRB-FR-MWTC / GRB-FR-TXVL: what the cancellation request trips, and what
    /// the reconciliation passes to every turn it dispatches.
    cancellation: CancellationToken,
}

impl RepositoryUpdateHold<'_> {
    /// The cancellation this reconciliation runs under.
    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }
}

impl Drop for RepositoryUpdateHold<'_> {
    fn drop(&mut self) {
        self.state.end_reconciliation();
    }
}
