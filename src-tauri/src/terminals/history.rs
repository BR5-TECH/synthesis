//! The output history of one terminal session (TRS-FR-IZWM, TRS-FR-BWVT).
//!
//! The history holds every byte the process wrote, in order, up to a cap. It
//! lives in memory only. When new output passes the cap, the oldest bytes go
//! first, up to the start of a line where the dropped range holds a line break.

use std::collections::VecDeque;

/// TRS-FR-IZWM: the cap of one session's history.
pub const HISTORY_CAP_BYTES: usize = 4 * 1024 * 1024;

/// How far past a cut the history looks for the end of the line it falls in.
const LINE_SEARCH_BYTES: usize = 64 * 1024;

/// The output history of one session.
#[derive(Debug)]
pub struct History {
    bytes: VecDeque<u8>,
    cap: usize,
}

impl History {
    pub fn new() -> Self {
        Self::with_cap(HISTORY_CAP_BYTES)
    }

    pub fn with_cap(cap: usize) -> Self {
        Self { bytes: VecDeque::new(), cap: cap.max(1) }
    }

    /// Adds output to the end, and drops the oldest bytes when the cap is passed.
    pub fn push(&mut self, data: &[u8]) {
        if data.len() >= self.cap {
            let start = data.len() - self.cap;
            let mut tail = &data[start..];
            // The tail starts on a line where the byte before it ends one.
            if start > 0 && data[start - 1] != b'\n' {
                let window = &tail[..tail.len().min(LINE_SEARCH_BYTES)];
                if let Some(at) = window.iter().position(|b| *b == b'\n') {
                    tail = &tail[at + 1..];
                }
            }
            self.bytes.clear();
            self.bytes.extend(tail);
            return;
        }
        self.bytes.extend(data);
        if self.bytes.len() > self.cap {
            let excess = self.bytes.len() - self.cap;
            self.trim_to_line_start(excess);
        }
    }

    /// Drops `at_least` bytes from the front, and then the rest of the line
    /// that the cut falls in, where that line ends in the next
    /// `LINE_SEARCH_BYTES`. The history then starts on a line boundary.
    fn trim_to_line_start(&mut self, at_least: usize) {
        let mut cut = at_least;
        let from = at_least.saturating_sub(1);
        let to = (at_least + LINE_SEARCH_BYTES).min(self.bytes.len());
        if from < to {
            if let Some(next) = self.bytes.range(from..to).position(|b| *b == b'\n') {
                cut = (from + next + 1).max(at_least);
            }
        }
        if cut > 0 {
            self.bytes.drain(..cut.min(self.bytes.len()));
        }
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// A copy of the whole history, for an attach (TRS-FR-HIIK).
    pub fn snapshot(&self) -> Vec<u8> {
        let (a, b) = self.bytes.as_slices();
        let mut out = Vec::with_capacity(a.len() + b.len());
        out.extend_from_slice(a);
        out.extend_from_slice(b);
        out
    }

    /// TRS-FR-NULR, TRS-FR-CXSV: the history is dropped.
    pub fn clear(&mut self) {
        self.bytes = VecDeque::new();
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TRS-FR-IZWM: output under the cap is kept whole and in order.
    #[test]
    fn keeps_every_byte_under_the_cap() {
        let mut history = History::with_cap(64);
        history.push(b"hello ");
        history.push(b"world\n");
        assert_eq!(history.snapshot(), b"hello world\n");
        assert_eq!(history.len(), 12);
    }

    // TRS-FR-IZWM: past the cap, the oldest bytes go first, up to a line start.
    #[test]
    fn drops_the_oldest_lines_first() {
        let mut history = History::with_cap(16);
        history.push(b"line-1\nline-2\n");
        history.push(b"line-3\n");
        let kept = history.snapshot();
        assert!(kept.len() <= 16);
        assert_eq!(kept, b"line-2\nline-3\n");
    }

    // TRS-FR-IZWM: with no line break near the cut, the cut is exact.
    #[test]
    fn cuts_exactly_where_no_line_break_is_near() {
        let mut history = History::with_cap(8);
        history.push(b"abcdefgh");
        history.push(b"ij");
        assert_eq!(history.snapshot(), b"cdefghij");
    }

    // TRS-FR-IZWM: one write larger than the cap keeps its tail.
    #[test]
    fn a_write_larger_than_the_cap_keeps_its_tail() {
        let mut history = History::with_cap(4);
        history.push(b"0123456789");
        assert_eq!(history.snapshot(), b"6789");
    }

    // TRS-FR-IZWM: a large write keeps the whole lines of its tail, and a
    // write of exactly the cap is kept whole.
    #[test]
    fn a_large_write_keeps_whole_lines() {
        let mut history = History::with_cap(5);
        history.push(b"aaaa\nbbbb\n");
        assert_eq!(history.snapshot(), b"bbbb\n");
        let mut exact = History::with_cap(5);
        exact.push(b"abcd\n");
        assert_eq!(exact.snapshot(), b"abcd\n");
        let mut cut = History::with_cap(6);
        cut.push(b"xxaaaa\nbb");
        assert_eq!(cut.snapshot(), b"bb");
    }

    // TRS-FR-IZWM: the default cap is 4 MiB.
    #[test]
    fn the_default_cap_is_four_mebibytes() {
        let mut history = History::new();
        let chunk = vec![b'x'; 1024 * 1024];
        for _ in 0..5 {
            history.push(&chunk);
        }
        assert_eq!(history.len(), HISTORY_CAP_BYTES);
    }

    // TRS-FR-NULR: a cleared history is empty.
    #[test]
    fn clear_drops_everything() {
        let mut history = History::with_cap(32);
        history.push(b"abc");
        history.clear();
        assert!(history.is_empty());
    }
}
