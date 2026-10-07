//! The fake keyring the vault suite and the owning modules' suites both drive.
//!
//! Test-only. It stands in for the platform keyring at the `VaultBackend` seam
//! so the whole of the secret path is exercisable on a CI runner that has no
//! credential store, and it **records every call it was handed** — which is what
//! lets a scenario assert how many keyring accesses an operation made and in
//! what order a write and a deletion happened. Both are requirements
//! (`ASV-application-secret-vault.md` ASV-FR-24, ASV-FR-25, ASV-FR-30) rather
//! than incidental behaviour, and neither is observable from the return value
//! alone.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::{decode, Decoded, Value, VaultBackend};

/// One recorded keyring access, in the order it was made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Call {
    Read,
    Write,
    Delete,
    ReadLegacy(String, String),
    DeleteLegacy(String, String),
}

#[derive(Default)]
pub(crate) struct FakeKeyringState {
    /// The consolidated entry, or `None` where it does not exist.
    pub(crate) entry: Option<String>,
    /// Legacy entries, keyed by `(service, account)`.
    pub(crate) legacy: HashMap<(String, String), String>,
    pub(crate) calls: Vec<Call>,
    /// Refusal switches, each standing for a keyring that will not answer.
    pub(crate) refuse_read: bool,
    pub(crate) refuse_write: bool,
    pub(crate) refuse_legacy_read: Vec<(String, String)>,
    pub(crate) refuse_legacy_delete: Vec<(String, String)>,
    /// How many of the next writes silently store something other than what
    /// they were handed, so the read-back cannot match (ASV-FR-11). A count
    /// rather than a flag: a rollback is itself a write, and a scenario that
    /// mangled that one too would be testing a keyring that never works rather
    /// than one whose write did not take.
    pub(crate) mangle_next_writes: usize,
    /// Writes from this index onward are refused. Set together with
    /// `mangle_next_writes` it makes the *rollback* write fail, which is the
    /// one branch of ASV-FR-12 that leaves the entry holding a value nothing
    /// asked for.
    pub(crate) refuse_write_from: Option<usize>,
    /// How many writes have been attempted, which `refuse_write_from` counts
    /// against.
    pub(crate) writes: usize,
    /// The consolidated entry disappears the moment it is written, so the
    /// read-back finds nothing (ASV-FR-11).
    pub(crate) vanish_after_write: bool,
    /// The keyring refuses to delete the consolidated entry, which is what a
    /// rollback of a write onto an entry that did not exist has to do.
    pub(crate) refuse_delete: bool,
    /// Slept inside `read`, so a read-modify-write cycle is wide enough that a
    /// second thread would reliably interleave with it were the vault not
    /// serialising them (ASV-FR-09, ASV-FR-14).
    pub(crate) read_delay: std::time::Duration,
    /// While set, a `read` blocks after it is recorded, until
    /// `FakeKeyring::release_reads` runs. Lets a scenario hold one request
    /// inside the keyring while others arrive, with no sleep.
    pub(crate) hold_reads: bool,
}

#[derive(Clone, Default)]
pub(crate) struct FakeKeyring(
    Arc<Mutex<FakeKeyringState>>,
    Arc<std::sync::Condvar>,
);

impl FakeKeyring {
    pub(crate) fn state(&self) -> std::sync::MutexGuard<'_, FakeKeyringState> {
        self.0.lock().unwrap()
    }

    /// Make every later `read` block until `release_reads`.
    pub(crate) fn hold_reads(&self) {
        self.state().hold_reads = true;
    }

    pub(crate) fn release_reads(&self) {
        self.state().hold_reads = false;
        self.1.notify_all();
    }

    pub(crate) fn calls(&self) -> Vec<Call> {
        self.state().calls.clone()
    }

    pub(crate) fn entry(&self) -> Option<String> {
        self.state().entry.clone()
    }

    pub(crate) fn set_entry(&self, value: &str) {
        self.state().entry = Some(value.to_string());
    }

    pub(crate) fn set_legacy(&self, service: &str, account: &str, secret: &str) {
        self.state().legacy.insert(
            (service.to_string(), account.to_string()),
            secret.to_string(),
        );
    }

    pub(crate) fn legacy(&self, service: &str, account: &str) -> Option<String> {
        self.state()
            .legacy
            .get(&(service.to_string(), account.to_string()))
            .cloned()
    }

    pub(crate) fn legacy_count(&self) -> usize {
        self.state().legacy.len()
    }

    /// The decoded consolidated object, which every assertion about content
    /// goes through.
    pub(crate) fn object(&self) -> serde_json::Map<String, Value> {
        let raw = self.entry().expect("the consolidated entry exists");
        match decode(&raw) {
            Decoded::Object(object) => object,
            _ => panic!("the consolidated entry does not decode"),
        }
    }

    pub(crate) fn count(&self, wanted: &Call) -> usize {
        self.calls().iter().filter(|c| *c == wanted).count()
    }

    /// The `(service, account)` pairs of every legacy entry this keyring was
    /// asked about, in any way. ASV-FR-32 turns on this set being exactly the
    /// set of candidates supplied and nothing else.
    pub(crate) fn legacy_entries_touched(&self) -> Vec<(String, String)> {
        let mut touched: Vec<(String, String)> = self
            .calls()
            .iter()
            .filter_map(|c| match c {
                Call::ReadLegacy(service, account) | Call::DeleteLegacy(service, account) => {
                    Some((service.clone(), account.clone()))
                }
                _ => None,
            })
            .collect();
        touched.sort();
        touched.dedup();
        touched
    }
}

impl VaultBackend for FakeKeyring {
    fn read(&self) -> Result<Option<String>, String> {
        let (delay, answer) = {
            let mut state = self.state();
            state.calls.push(Call::Read);
            while state.hold_reads {
                state = self.1.wait(state).unwrap();
            }
            if state.refuse_read {
                return Err("locked".into());
            }
            (state.read_delay, state.entry.clone())
        };
        // Outside the lock: the point of the delay is to widen the window
        // between this module's read and its write, not to hold the fake's own
        // state while it elapses.
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
        Ok(answer)
    }

    fn write(&self, value: &str) -> Result<(), String> {
        let mut state = self.state();
        state.calls.push(Call::Write);
        let index = state.writes;
        state.writes += 1;
        if state.refuse_write || state.refuse_write_from.is_some_and(|from| index >= from) {
            return Err("locked".into());
        }
        if state.mangle_next_writes > 0 {
            state.mangle_next_writes -= 1;
            // Decodable, so the read-back reaches the *comparison* rather than
            // failing to parse — and carrying a field no real object ever has,
            // so it can never coincide with what the caller meant to write.
            state.entry = Some(r#"{"version":1,"__mangled_by_the_fake":"x"}"#.to_string());
            return Ok(());
        }
        if state.vanish_after_write {
            state.entry = None;
            return Ok(());
        }
        state.entry = Some(value.to_string());
        Ok(())
    }

    fn delete(&self) -> Result<(), String> {
        let mut state = self.state();
        state.calls.push(Call::Delete);
        if state.refuse_delete {
            return Err("locked".into());
        }
        state.entry = None;
        Ok(())
    }

    fn read_legacy(&self, service: &str, account: &str) -> Result<Option<String>, String> {
        let mut state = self.state();
        state
            .calls
            .push(Call::ReadLegacy(service.to_string(), account.to_string()));
        let key = (service.to_string(), account.to_string());
        if state.refuse_legacy_read.contains(&key) {
            return Err("locked".into());
        }
        Ok(state.legacy.get(&key).cloned())
    }

    fn delete_legacy(&self, service: &str, account: &str) -> Result<(), String> {
        let mut state = self.state();
        state
            .calls
            .push(Call::DeleteLegacy(service.to_string(), account.to_string()));
        let key = (service.to_string(), account.to_string());
        if state.refuse_legacy_delete.contains(&key) {
            return Err("locked".into());
        }
        state.legacy.remove(&key);
        Ok(())
    }
}
