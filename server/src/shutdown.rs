//! Graceful shutdown of the service.
//!
//! Specification: `specifications/server/BMS-backend-microservice.md`.
//! Requirement: BMS-FR-10.

use std::future::Future;
use std::io;
use std::time::Duration;

/// How long the process lets the requests already in flight complete.
///
/// BMS-FR-10: a request that is still in flight after this period is dropped,
/// and the process still exits 0. A stuck connection therefore cannot keep a
/// container alive until its runtime kills it.
pub const DRAIN_PERIOD: Duration = Duration::from_secs(10);

/// How the drain ended. The process exits 0 in every case; the value only
/// explains the exit in the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrainOutcome {
    /// Every request in flight completed inside the drain period.
    Drained,
    /// The drain period elapsed and the remaining requests were dropped.
    TimedOut,
    /// A second signal arrived and the process stopped waiting.
    Interrupted,
}

impl DrainOutcome {
    /// The word the shutdown record reports.
    pub fn as_str(self) -> &'static str {
        match self {
            DrainOutcome::Drained => "drained",
            DrainOutcome::TimedOut => "timed-out",
            DrainOutcome::Interrupted => "interrupted",
        }
    }
}

/// Waits for the drain to finish, for the drain period to elapse, or for a
/// second signal, whichever happens first (BMS-FR-10).
///
/// The function is generic over its two futures, so a test drives every outcome
/// without sending a real signal.
pub async fn await_drain<S, I>(server: S, second_signal: I, drain_period: Duration) -> DrainOutcome
where
    S: Future<Output = ()>,
    I: Future<Output = ()>,
{
    tokio::select! {
        _ = server => DrainOutcome::Drained,
        _ = second_signal => DrainOutcome::Interrupted,
        _ = tokio::time::sleep(drain_period) => DrainOutcome::TimedOut,
    }
}

/// The termination signals of the target platform.
///
/// BMS-FR-10: on Linux these are `SIGTERM` and `SIGINT`. On a platform with no
/// POSIX signals the console interrupt takes their place, so the same shutdown
/// path runs everywhere the crate compiles.
pub struct Signals {
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
}

impl Signals {
    /// Installs the handlers.
    ///
    /// This runs before the listener is bound, so a signal that arrives during
    /// startup is never lost.
    #[cfg(unix)]
    pub fn install() -> io::Result<Self> {
        use tokio::signal::unix::{signal, SignalKind};

        Ok(Self {
            terminate: signal(SignalKind::terminate())?,
            interrupt: signal(SignalKind::interrupt())?,
        })
    }

    /// Installs the handlers.
    #[cfg(not(unix))]
    pub fn install() -> io::Result<Self> {
        Ok(Self {})
    }

    /// Waits for the next termination signal and reports its name.
    #[cfg(unix)]
    pub async fn recv(&mut self) -> &'static str {
        tokio::select! {
            _ = self.terminate.recv() => "SIGTERM",
            _ = self.interrupt.recv() => "SIGINT",
        }
    }

    /// Waits for the next termination signal and reports its name.
    #[cfg(not(unix))]
    pub async fn recv(&mut self) -> &'static str {
        let _ = tokio::signal::ctrl_c().await;
        "CTRL_C"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::pending;
    use tokio::sync::oneshot;

    // BMS-FR-10: the request in flight completes, and the drain reports that it
    // finished on its own.
    #[tokio::test]
    async fn a_finished_drain_reports_drained() {
        let (sender, receiver) = oneshot::channel::<()>();
        sender.send(()).expect("the receiver is alive");

        let outcome = await_drain(
            async {
                let _ = receiver.await;
            },
            pending::<()>(),
            Duration::from_secs(30),
        )
        .await;

        assert_eq!(outcome, DrainOutcome::Drained);
    }

    // BMS-FR-10: a request that is still in flight after the drain period is
    // dropped, and the process still leaves through the same path.
    #[tokio::test(start_paused = true)]
    async fn a_drain_that_does_not_finish_reports_timed_out() {
        let outcome =
            await_drain(pending::<()>(), pending::<()>(), Duration::from_millis(10)).await;

        assert_eq!(outcome, DrainOutcome::TimedOut);
    }

    // BMS-FR-10: a second signal during the drain stops the wait at once.
    #[tokio::test(start_paused = true)]
    async fn a_second_signal_reports_interrupted() {
        let (sender, receiver) = oneshot::channel::<()>();
        sender.send(()).expect("the receiver is alive");

        let outcome = await_drain(
            pending::<()>(),
            async {
                let _ = receiver.await;
            },
            Duration::from_secs(3600),
        )
        .await;

        assert_eq!(outcome, DrainOutcome::Interrupted);
    }

    // BMS-FR-10: the drain period is the 10 seconds the specification names.
    #[test]
    fn the_drain_period_is_ten_seconds() {
        assert_eq!(DRAIN_PERIOD, Duration::from_secs(10));
    }

    #[test]
    fn every_outcome_has_a_word_for_the_log() {
        assert_eq!(DrainOutcome::Drained.as_str(), "drained");
        assert_eq!(DrainOutcome::TimedOut.as_str(), "timed-out");
        assert_eq!(DrainOutcome::Interrupted.as_str(), "interrupted");
    }
}
