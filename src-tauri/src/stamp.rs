//! A time stamp that is unique in the process.
//!
//! The ids of runs, streams and attempts, and the names of the temporary files
//! of an atomic write, are made from the time of the clock and the process id.
//! The clock of some platforms counts microseconds alone, so two calls in the
//! same microsecond read the same time. This stamp is always at least one
//! nanosecond after the stamp before it, so no two calls in one process get the
//! same value, and an id keeps its format.

use std::sync::atomic::{AtomicU64, Ordering};

/// Nanoseconds since the Unix epoch, larger than every value returned before
/// in this process.
pub fn unique_nanos() -> u64 {
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or_default();
    let mut previous = LAST.load(Ordering::Relaxed);
    loop {
        let next = now.max(previous.saturating_add(1));
        match LAST.compare_exchange_weak(previous, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(seen) => previous = seen,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // GRD-FR-VLFO, WKS-FR-QMTV, GRB-FR-YPEX, FSA-FR-04: stamps taken one after
    // the other, from several threads at once, are all different and close to
    // the clock.
    #[test]
    fn stamps_taken_at_the_same_time_are_all_different() {
        let threads: Vec<_> = (0..8)
            .map(|_| std::thread::spawn(|| (0..1000).map(|_| unique_nanos()).collect::<Vec<_>>()))
            .collect();
        let mut all: Vec<u64> = threads.into_iter().flat_map(|t| t.join().unwrap()).collect();
        let count = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), count, "no stamp repeats");
        let clock =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
                as u64;
        let newest = *all.last().unwrap();
        assert!(newest.abs_diff(clock) < 60_000_000_000, "the stamp follows the clock");
    }
}
