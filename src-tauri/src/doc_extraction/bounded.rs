//! The worker-thread runner that bounds a parser by a deadline and survives
//! its panic (CMS-FR-DUGE, DCL-FR-VRTS, DCL-FR-DLPX).

use std::time::Duration;

/// How one bounded run ended.
#[derive(Debug)]
pub enum Bounded<T> {
    /// The work returned this value.
    Done(T),
    /// The work panicked. The panic stopped this run only.
    Panicked,
    /// The work did not end before the deadline. The worker is abandoned: it
    /// cannot be stopped, and its result is dropped.
    PastDeadline,
    /// The worker thread could not start.
    NotStarted,
    /// The worker ended with no result.
    Stopped,
}

/// Runs `work` on a named worker thread and waits for it at most `deadline`.
pub fn run_bounded<T, F>(name: &str, deadline: Duration, work: F) -> Bounded<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (send, receive) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new().name(name.to_string()).spawn(move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
        // The receiver is gone when the deadline has passed.
        let _ = send.send(outcome);
    });
    if spawned.is_err() {
        return Bounded::NotStarted;
    }
    match receive.recv_timeout(deadline) {
        Ok(Ok(value)) => Bounded::Done(value),
        Ok(Err(_)) => Bounded::Panicked,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Bounded::PastDeadline,
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Bounded::Stopped,
    }
}
