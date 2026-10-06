//! The process cache and the gate that lets one initialization attempt run at a
//! time (`specifications/core/ASV-application-secret-vault.md` ASV-FR-ZUGZ,
//! ASV-FR-ZRWU, ASV-FR-CIDB, ASV-FR-FTFU, ASV-FR-SUXZ).
//!
//! Neither type reads the keyring. The vault decides when to fill the cache;
//! this module only holds what it was given and coordinates who may try.

use std::sync::{Condvar, Mutex, MutexGuard};

use serde_json::{Map, Value};

use super::VaultError;

/// The decoded `AppSecrets` object, and the serialized value the entry held
/// when the object last agreed with it.
///
/// `serialized` is `None` where the entry does not exist. A mutation whose
/// verification fails restores it, so a rollback needs no second keyring read
/// (ASV-FR-12).
///
/// `Debug` is hand-rolled: the object is secret material (ASV-FR-SUXZ).
pub(super) struct Cached {
    pub(super) object: Map<String, Value>,
    pub(super) serialized: Option<String>,
}

impl std::fmt::Debug for Cached {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Cached { object: <redacted>, serialized: <redacted> }")
    }
}

/// What the gate remembers about initialization attempts.
struct GateState {
    running: bool,
    /// Counts finished attempts. A waiter wakes when it changes.
    epoch: u64,
    /// The result of the latest finished attempt.
    last: Result<(), VaultError>,
    /// How many requests wait for the running attempt. Read by tests only.
    waiting: usize,
}

/// Allows one initialization attempt at a time (ASV-FR-CIDB).
///
/// A request that arrives while an attempt runs waits for it and takes its
/// result, so concurrent requests never cause parallel bulk reads. A failed
/// attempt is reported to every request that waited for it and is not kept:
/// the next request starts a new attempt (ASV-FR-FTFU).
pub(super) struct InitGate {
    state: Mutex<GateState>,
    finished: Condvar,
}

/// Ends an attempt even where the attempt panics, so a waiter is never left
/// waiting for an attempt that no longer runs.
struct Attempt<'a> {
    gate: &'a InitGate,
    result: Option<Result<(), VaultError>>,
}

impl Drop for Attempt<'_> {
    fn drop(&mut self) {
        let result = self.result.take().unwrap_or(Err(VaultError::Unavailable));
        let mut state = self.gate.lock();
        state.running = false;
        state.epoch += 1;
        state.last = result;
        self.gate.finished.notify_all();
    }
}

impl InitGate {
    pub(super) fn new() -> Self {
        Self {
            state: Mutex::new(GateState {
                running: false,
                epoch: 0,
                last: Ok(()),
                waiting: 0,
            }),
            finished: Condvar::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, GateState> {
        match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// How many requests wait for the running attempt.
    #[cfg(test)]
    pub(super) fn waiting(&self) -> usize {
        self.lock().waiting
    }

    /// Run `attempt` unless another request already runs one. `needs_work` is
    /// asked while the gate is held, so two requests never both decide to
    /// start. Returns `Ok` at once where there is nothing to do.
    pub(super) fn run(
        &self,
        needs_work: impl FnOnce() -> bool,
        attempt: impl FnOnce() -> Result<(), VaultError>,
    ) -> Result<(), VaultError> {
        let mut state = self.lock();
        if !state.running && !needs_work() {
            return Ok(());
        }
        if state.running {
            let epoch = state.epoch;
            state.waiting += 1;
            while state.epoch == epoch {
                state = match self.finished.wait(state) {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
            }
            state.waiting -= 1;
            return state.last;
        }
        state.running = true;
        drop(state);

        let mut guard = Attempt {
            gate: self,
            result: None,
        };
        let result = attempt();
        guard.result = Some(result);
        drop(guard);
        result
    }
}
