//! The container removals of interactive launches that are still in flight
//! (TRS-FR-OTLH, EAC-FR-LQJC).
//!
//! A terminate returns at once and removes its container on a thread or a task
//! of its own. An application exit waits here, with a bound, so that no
//! container of a terminal outlives the process.

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

static PENDING: Mutex<usize> = Mutex::new(0);
static DONE: Condvar = Condvar::new();

/// One removal in flight. It counts as done when it is dropped.
pub struct Removal(());

/// Starts to count one removal.
pub fn begin() -> Removal {
    *PENDING.lock().unwrap_or_else(|p| p.into_inner()) += 1;
    Removal(())
}

impl Drop for Removal {
    fn drop(&mut self) {
        let mut pending = PENDING.lock().unwrap_or_else(|p| p.into_inner());
        *pending = pending.saturating_sub(1);
        DONE.notify_all();
    }
}

/// Waits until no removal is in flight, or until `timeout`. Returns whether
/// every removal ended.
pub fn wait_all(timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    let mut pending = PENDING.lock().unwrap_or_else(|p| p.into_inner());
    while *pending > 0 {
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        pending = DONE.wait_timeout(pending, deadline - now).unwrap_or_else(|p| p.into_inner()).0;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    // TRS-FR-OTLH: the exit waits for a removal in flight and returns when it ends.
    #[test]
    fn waits_for_a_removal_in_flight() {
        let removal = begin();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            drop(removal);
        });
        assert!(wait_all(Duration::from_secs(30)));
        handle.join().unwrap();
    }
}
