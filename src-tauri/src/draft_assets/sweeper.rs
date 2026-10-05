//! The hold registry and the sweep scheduler state.

use super::*;

// ---------------------------------------------------------------------------
// The hold registry (DAS-FR-14) and the sweep scheduler (DAS-FR-15)
// ---------------------------------------------------------------------------

/// DAS-FR-14 / DAS-FR-15: the memory of the running session this module keeps —
/// which assets are held against housekeeping, and which drafts have a pass
/// pending or running.
///
/// Written to no file, surviving no relaunch, and protecting nothing afterwards:
/// the saved prompt is what protects an asset from then on (DAS-FR-13).
#[derive(Default)]
pub struct DraftAssetSweeper {
    holds: Mutex<HashSet<(String, String)>>,
    passes: Mutex<Passes>,
    /// DAS-FR-20: set once the application is shutting down, so a pass still
    /// running is given the chance to finish and is then abandoned where it
    /// stands rather than reporting success for work it did not complete.
    pub(super) shutdown: Arc<AtomicBool>,
    /// DAS-FR-19: whether the application-start pass has been taken. Once, for
    /// the life of the process.
    pub(super) started: AtomicBool,
}

#[derive(Default)]
pub(super) struct Passes {
    pending: HashSet<String>,
    running: HashSet<String>,
}

impl DraftAssetSweeper {
    /// DAS-FR-14: hold an asset this session has just stored, from the moment it
    /// is stored until the insertion that asked for it settles.
    pub fn hold(&self, draft_id: &str, name: &str) {
        if let Ok(mut held) = self.holds.lock() {
            held.insert((draft_id.to_string(), name.to_string()));
        }
    }

    /// DAS-FR-14: release the hold on one asset — `discard_draft_image` removed
    /// it, or it was never stored at all.
    pub fn release(&self, draft_id: &str, name: &str) {
        if let Ok(mut held) = self.holds.lock() {
            held.remove(&(draft_id.to_string(), name.to_string()));
        }
    }

    /// DAS-FR-14: release every hold on one draft — the draft's prompt has been
    /// saved, so every insertion outstanding against it has settled and the
    /// saved prompt is what protects an asset from here on (DAS-FR-13).
    pub fn release_draft(&self, draft_id: &str) {
        if let Ok(mut held) = self.holds.lock() {
            held.retain(|(draft, _)| draft != draft_id);
        }
    }

    pub(super) fn is_held(&self, draft_id: &str, name: &str) -> bool {
        self.holds
            .lock()
            .map(|h| h.contains(&(draft_id.to_string(), name.to_string())))
            .unwrap_or(false)
    }

    /// DAS-FR-20: begin the abandonment of whatever is still running.
    pub fn begin_shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    /// DAS-FR-15: claim a pass for `draft_id`, saying whether this request added
    /// one or joined one already scheduled or already running.
    ///
    /// Returns `(answer, run_now)`: `run_now` is true for exactly the caller
    /// that has to start the worker, so passes for one draft triggered close
    /// together are coalesced into one.
    pub(super) fn claim(&self, draft_id: &str) -> (DraftAssetSweepScheduled, bool) {
        let Ok(mut passes) = self.passes.lock() else {
            return (
                DraftAssetSweepScheduled { scheduled: false, coalesced: false },
                false,
            );
        };
        let busy =
            passes.pending.contains(draft_id) || passes.running.contains(draft_id);
        passes.pending.insert(draft_id.to_string());
        (
            DraftAssetSweepScheduled { scheduled: true, coalesced: busy },
            !busy,
        )
    }

    /// Take the pending claim for `draft_id`, marking it running. `false` means
    /// there is nothing left to do and the worker is finished.
    pub(super) fn take(&self, draft_id: &str) -> bool {
        let Ok(mut passes) = self.passes.lock() else {
            return false;
        };
        if passes.pending.remove(draft_id) {
            passes.running.insert(draft_id.to_string());
            true
        } else {
            passes.running.remove(draft_id);
            false
        }
    }

    /// Release the claim however the worker ended.
    ///
    /// Both sets, because a pass abandoned by a panic may have left a request
    /// pending behind it: leaving that request in `pending` with no worker to
    /// take it would coalesce every later request into a pass nothing is
    /// running. Dropping it means the next trigger schedules a fresh worker,
    /// which is what DAS-FR-15 promises — a pass that did not complete is
    /// simply run again.
    pub(super) fn finish(&self, draft_id: &str) {
        if let Ok(mut passes) = self.passes.lock() {
            passes.running.remove(draft_id);
            passes.pending.remove(draft_id);
        }
    }
}
