//! The fork-join arithmetic of an agent group (AGC-FR-ZOCI, AGC-FR-ZLWN).
//!
//! Pure: the caller passes the instant, so a test needs no real time.

use std::time::{Duration, Instant};

/// The count of finished and settled members, and the timeout that the quorum
/// starts.
#[derive(Clone, Debug)]
pub struct Join {
    total: usize,
    finished: usize,
    settled: usize,
    timeout: Duration,
    quorum_at: Option<Instant>,
}

impl Join {
    pub fn new(total: usize, timeout: Duration) -> Self {
        Self { total, finished: 0, settled: 0, timeout, quorum_at: None }
    }

    /// AGC-FR-ZOCI: more than half of the dispatched agents.
    pub fn quorum_of(total: usize) -> usize {
        total / 2 + 1
    }

    pub fn quorum(&self) -> usize {
        Self::quorum_of(self.total)
    }

    /// AGC-FR-ZOCI: a member that delivered or returned its advice. The first
    /// finish that reaches the quorum starts the timeout (AGC-FR-ZLWN).
    pub fn note_finished(&mut self, now: Instant) {
        self.finished += 1;
        self.settled += 1;
        if self.quorum_at.is_none() && self.finished >= self.quorum() {
            self.quorum_at = Some(now);
        }
    }

    /// AGC-FR-ZOCI: a member that failed or was cancelled. It does not count
    /// toward the quorum.
    pub fn note_unfinished(&mut self) {
        self.settled += 1;
    }

    pub fn finished(&self) -> usize {
        self.finished
    }

    /// AGC-FR-ZLWN: the instant the timeout ends. `None` before the quorum.
    pub fn deadline(&self) -> Option<Instant> {
        self.quorum_at.map(|at| at + self.timeout)
    }

    pub fn quorum_reached(&self) -> bool {
        self.quorum_at.is_some()
    }

    /// AGC-FR-ZLWN: every member has settled, or the timeout has ended.
    pub fn is_over(&self, now: Instant) -> bool {
        self.settled >= self.total || self.timed_out(now)
    }

    /// AGC-FR-ZLWN: the timeout has ended.
    pub fn timed_out(&self, now: Instant) -> bool {
        self.deadline().is_some_and(|deadline| now >= deadline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, secs: u64) -> Instant {
        base + Duration::from_secs(secs)
    }

    // AGC-FR-ZOCI: the quorum is a majority of the dispatched agents.
    #[test]
    fn the_quorum_is_a_majority() {
        assert_eq!(Join::quorum_of(1), 1);
        assert_eq!(Join::quorum_of(2), 2);
        assert_eq!(Join::quorum_of(3), 2);
        assert_eq!(Join::quorum_of(4), 3);
        assert_eq!(Join::quorum_of(5), 3);
    }

    // AGC-FR-ZLWN: the timeout starts at the quorum and not before.
    #[test]
    fn the_timeout_starts_when_the_quorum_has_finished() {
        let base = Instant::now();
        let mut join = Join::new(3, Duration::from_secs(60));
        assert_eq!(join.deadline(), None);
        join.note_finished(at(base, 5));
        assert_eq!(join.deadline(), None);
        assert!(!join.is_over(at(base, 500)));
        join.note_finished(at(base, 10));
        assert!(join.quorum_reached());
        assert_eq!(join.deadline(), Some(at(base, 70)));
        // A later finish does not move the deadline.
        join.note_finished(at(base, 20));
        assert!(join.is_over(at(base, 20)));
    }

    // AGC-FR-ZLWN: the join ends at the timeout while one member is still out.
    #[test]
    fn the_join_ends_when_the_timeout_ends() {
        let base = Instant::now();
        let mut join = Join::new(3, Duration::from_secs(60));
        join.note_finished(at(base, 1));
        join.note_finished(at(base, 2));
        assert!(!join.is_over(at(base, 61)));
        assert!(join.is_over(at(base, 62)));
        assert!(join.timed_out(at(base, 62)));
    }

    // AGC-FR-ZOCI: a failed or cancelled member does not count toward the
    // quorum, but it does settle, so a join of all-failed members ends at once.
    #[test]
    fn failed_members_settle_without_counting() {
        let base = Instant::now();
        let mut join = Join::new(3, Duration::from_secs(60));
        join.note_unfinished();
        join.note_finished(at(base, 1));
        assert!(!join.quorum_reached());
        assert_eq!(join.deadline(), None);
        join.note_unfinished();
        assert!(join.is_over(at(base, 1)));
        assert_eq!(join.finished(), 1);

        let mut none = Join::new(2, Duration::from_secs(60));
        none.note_unfinished();
        none.note_unfinished();
        assert!(none.is_over(base));
        assert_eq!(none.finished(), 0);
    }

    // AGC-FR-ZLWN: a zero timeout ends the join at the quorum.
    #[test]
    fn a_zero_timeout_ends_at_the_quorum() {
        let base = Instant::now();
        let mut join = Join::new(3, Duration::ZERO);
        join.note_finished(base);
        assert!(!join.is_over(base));
        join.note_finished(base);
        assert!(join.is_over(base));
    }
}
