//! The recent audio of a VAD session, at the capture rate (VOP-FR-FAQA).
//!
//! One ring holds the pre-roll and the audio since `speech_start`. Positions
//! count samples from the start of the session, so a position stays valid while
//! the ring moves. Audio stays in memory (VOP-FR-YEFK).

use std::collections::VecDeque;

pub(crate) struct AudioRing {
    samples: VecDeque<i16>,
    capacity: usize,
    written: u64,
}

impl AudioRing {
    pub(crate) fn new(capacity: usize) -> Self {
        Self { samples: VecDeque::with_capacity(capacity), capacity, written: 0 }
    }

    pub(crate) fn push(&mut self, samples: &[i16]) {
        self.written += samples.len() as u64;
        let keep = samples.len().min(self.capacity);
        let room = self.capacity - keep;
        while self.samples.len() > room {
            self.samples.pop_front();
        }
        self.samples.extend(&samples[samples.len() - keep..]);
    }

    /// The position after the newest sample.
    pub(crate) fn written(&self) -> u64 {
        self.written
    }

    /// The audio from `position` to the newest sample. A position that left the
    /// ring gives what the ring still holds.
    pub(crate) fn since(&self, position: u64) -> Vec<i16> {
        let oldest = self.written - self.samples.len() as u64;
        let skip = position.saturating_sub(oldest) as usize;
        self.samples.iter().skip(skip).copied().collect()
    }
}
