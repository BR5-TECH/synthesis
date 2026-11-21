//! Output batching and flow control of one attached channel (TRS-FR-PACY,
//! TRS-FR-VYIB).
//!
//! Both are plain state machines with the time passed in, so the rules are
//! tested without a clock.

use std::time::{Duration, Instant};

/// TRS-FR-PACY: the largest frame the module sends.
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
/// TRS-FR-PACY: the longest time a byte waits in a pending frame.
pub const FLUSH_DELAY: Duration = Duration::from_millis(16);
/// TRS-FR-VYIB: above this much unacknowledged output, reading stops.
pub const HIGH_WATER_BYTES: usize = 1024 * 1024;
/// TRS-FR-VYIB: at or below this much unacknowledged output, reading starts again.
pub const LOW_WATER_BYTES: usize = 256 * 1024;

/// The bytes that wait to go out as one frame.
#[derive(Debug, Default)]
pub struct Batcher {
    pending: Vec<u8>,
    first_byte_at: Option<Instant>,
}

impl Batcher {
    /// Adds output. Returns every frame that is full now.
    pub fn push(&mut self, data: &[u8], now: Instant) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        let mut rest = data;
        while !rest.is_empty() {
            if self.pending.is_empty() {
                self.first_byte_at = Some(now);
            }
            let room = MAX_FRAME_BYTES - self.pending.len();
            let take = room.min(rest.len());
            self.pending.extend_from_slice(&rest[..take]);
            rest = &rest[take..];
            if self.pending.len() == MAX_FRAME_BYTES {
                frames.push(self.take());
            }
        }
        frames
    }

    /// The time at which the pending frame must go out, where one waits.
    pub fn deadline(&self) -> Option<Instant> {
        self.first_byte_at.map(|at| at + FLUSH_DELAY)
    }

    /// The pending frame, where its deadline is reached at `now`.
    pub fn due(&mut self, now: Instant) -> Option<Vec<u8>> {
        match self.deadline() {
            Some(deadline) if now >= deadline => Some(self.take()),
            _ => None,
        }
    }

    /// The pending frame, whatever its deadline.
    pub fn take(&mut self) -> Vec<u8> {
        self.first_byte_at = None;
        std::mem::take(&mut self.pending)
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

/// The unacknowledged output of one attached channel.
#[derive(Debug, Default)]
pub struct FlowControl {
    unacknowledged: usize,
    paused: bool,
}

impl FlowControl {
    pub fn sent(&mut self, bytes: usize) {
        self.unacknowledged = self.unacknowledged.saturating_add(bytes);
        if self.unacknowledged > HIGH_WATER_BYTES {
            self.paused = true;
        }
    }

    pub fn acknowledged(&mut self, bytes: usize) {
        self.unacknowledged = self.unacknowledged.saturating_sub(bytes);
        if self.unacknowledged <= LOW_WATER_BYTES {
            self.paused = false;
        }
    }

    /// Whether the module must stop reading the session's output now.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn unacknowledged(&self) -> usize {
        self.unacknowledged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TRS-FR-PACY: a frame is never larger than 64 KiB.
    #[test]
    fn splits_output_into_frames_of_at_most_64_kib() {
        let mut batcher = Batcher::default();
        let now = Instant::now();
        let frames = batcher.push(&vec![1u8; MAX_FRAME_BYTES * 2 + 10], now);
        assert_eq!(frames.len(), 2);
        assert!(frames.iter().all(|f| f.len() == MAX_FRAME_BYTES));
        assert_eq!(batcher.take().len(), 10);
    }

    // TRS-FR-PACY: a pending frame is due 16 ms after its first byte.
    #[test]
    fn a_pending_frame_is_due_16_ms_after_its_first_byte() {
        let mut batcher = Batcher::default();
        let start = Instant::now();
        assert!(batcher.push(b"abc", start).is_empty());
        assert!(batcher.push(b"def", start + Duration::from_millis(10)).is_empty());
        assert_eq!(batcher.deadline(), Some(start + FLUSH_DELAY));
        assert!(batcher.due(start + Duration::from_millis(15)).is_none());
        assert_eq!(batcher.due(start + FLUSH_DELAY).as_deref(), Some(&b"abcdef"[..]));
        assert!(batcher.is_empty());
        assert!(batcher.deadline().is_none());
    }

    // TRS-FR-VYIB: reading stops above 1 MiB and starts again at 256 KiB.
    #[test]
    fn pauses_above_the_high_water_mark_and_resumes_at_the_low_one() {
        let mut flow = FlowControl::default();
        flow.sent(HIGH_WATER_BYTES);
        assert!(!flow.is_paused());
        flow.sent(1);
        assert!(flow.is_paused());
        flow.acknowledged(HIGH_WATER_BYTES - LOW_WATER_BYTES);
        assert!(flow.is_paused(), "one byte over the low mark still holds");
        flow.acknowledged(1);
        assert!(!flow.is_paused());
        assert_eq!(flow.unacknowledged(), LOW_WATER_BYTES);
    }

    // TRS-FR-VYIB: an acknowledgement larger than what was sent does not wrap.
    #[test]
    fn an_over_acknowledgement_does_not_wrap() {
        let mut flow = FlowControl::default();
        flow.sent(10);
        flow.acknowledged(100);
        assert_eq!(flow.unacknowledged(), 0);
    }
}
