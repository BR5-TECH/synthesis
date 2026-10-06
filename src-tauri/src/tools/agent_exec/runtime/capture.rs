//! Bounded stream capture and line reassembly for the runtime seam (EAC-FR-23, EAC-FR-32).

use tokio::io::AsyncReadExt;

use super::{CapturedStream, StreamChannel, StreamObserver, LIMIT_OBSERVED_LINE};

/// Read a pipe to EOF, keeping at most `limit` bytes and handing every complete
/// line to `observer` as it arrives (EAC-FR-23, EAC-FR-32).
///
/// Past the bound the stream is still drained — into nothing — so the child is
/// never blocked on a full pipe by a caller that has stopped caring. That is
/// what makes the bound a *memory* bound rather than a way to hang the agent.
/// The observer keeps receiving lines past that bound for the same reason it
/// exists: what a run is doing while it overruns its capture is exactly what
/// somebody watching it wants to see.
pub(super) async fn read_bounded<R>(
    reader: &mut Option<R>,
    limit: usize,
    channel: StreamChannel,
    observer: Option<&dyn StreamObserver>,
) -> CapturedStream
where
    R: tokio::io::AsyncRead + Unpin,
{
    let Some(reader) = reader.as_mut() else {
        return CapturedStream::default();
    };

    let mut kept: Vec<u8> = Vec::new();
    let mut truncated = false;
    let mut chunk = [0_u8; 8192];
    let mut pending: Vec<u8> = Vec::new();

    loop {
        match reader.read(&mut chunk).await {
            Ok(0) => break,
            Err(_) => {
                // NOT the same as EOF. A stream that failed mid-read has given
                // us a prefix, and reporting it as complete is precisely how a
                // truncated document reaches the parser — the one thing
                // EAC-FR-23 exists to prevent. Marking it truncated routes it
                // to `invalid_structured_output` instead.
                truncated = true;
                break;
            }
            Ok(n) => {
                if let Some(observer) = observer {
                    split_lines(&mut pending, &chunk[..n], channel, observer);
                }
                if kept.len() < limit {
                    let room = limit - kept.len();
                    let take = room.min(n);
                    kept.extend_from_slice(&chunk[..take]);
                    if take < n {
                        truncated = true;
                    }
                } else {
                    truncated = true;
                }
            }
        }
    }

    // What the child wrote after its last line ending is still something it
    // said. A CLI that dies mid-line, and one that never terminates its final
    // line, both leave their last words here.
    //
    // Delivered even when nothing is pending, and that is deliberate: an
    // observer may be holding bytes of its own from a piece delivered at the
    // reassembly bound (EAC-FR-32), and a stream that ends exactly on that
    // bound leaves nothing here to prompt a final delivery. One whole-marked
    // call at end of stream is what lets it flush. An observer with nothing
    // held makes nothing of an empty one.
    if let Some(observer) = observer {
        observer.line(channel, &pending, true);
    }

    CapturedStream {
        bytes: kept,
        truncated,
    }
}

/// Feed `chunk` through the line reassembler, delivering each complete line.
///
/// Split on `\n` alone, with a `\r` before it dropped, so a CLI that writes
/// Windows endings does not hand every observer a trailing control character.
fn split_lines(
    pending: &mut Vec<u8>,
    chunk: &[u8],
    channel: StreamChannel,
    observer: &dyn StreamObserver,
) {
    for &byte in chunk {
        if byte == b'\n' {
            if pending.last() == Some(&b'\r') {
                pending.pop();
            }
            observer.line(channel, pending, true);
            pending.clear();
            continue;
        }
        pending.push(byte);
        // A line that will not end is delivered at the bound and the buffer
        // starts again. Growing it instead would let one unterminated stream
        // hold as much memory as the child cares to write, which is the failure
        // the capture bound above already refuses to have.
        if pending.len() >= LIMIT_OBSERVED_LINE {
            observer.line(channel, pending, false);
            pending.clear();
        }
    }
}

/// The same bounded capture [`read_bounded`] performs, for a backend that is
/// handed chunks rather than a reader (EAC-FR-23, EAC-FR-32).
///
/// Kept beside it rather than merged into it because the two take their input
/// differently and share nothing but the bookkeeping: a bound, a truncation
/// flag, and the line reassembly an observer is fed from.
#[cfg_attr(test, allow(dead_code))]
pub(super) struct StreamCapture {
    limit: usize,
    kept: Vec<u8>,
    truncated: bool,
    pending: Vec<u8>,
    channel: StreamChannel,
}

#[cfg_attr(test, allow(dead_code))]
impl StreamCapture {
    pub(super) fn new(limit: usize, channel: StreamChannel) -> Self {
        StreamCapture {
            limit,
            kept: Vec::new(),
            truncated: false,
            pending: Vec::new(),
            channel,
        }
    }

    pub(super) fn push(&mut self, bytes: &[u8], observer: Option<&dyn StreamObserver>) {
        if let Some(observer) = observer {
            split_lines(&mut self.pending, bytes, self.channel, observer);
        }
        if self.kept.len() < self.limit {
            let room = self.limit - self.kept.len();
            let take = room.min(bytes.len());
            self.kept.extend_from_slice(&bytes[..take]);
            if take < bytes.len() {
                self.truncated = true;
            }
        } else if !bytes.is_empty() {
            self.truncated = true;
        }
    }

    /// What the backend wrote after its last line ending is still something it
    /// said, and an observer holding a piece delivered at the reassembly bound
    /// needs one whole-marked call to flush it.
    pub(super) fn finish(mut self, observer: Option<&dyn StreamObserver>) -> CapturedStream {
        if let Some(observer) = observer {
            observer.line(self.channel, &self.pending, true);
        }
        self.pending.clear();
        CapturedStream {
            bytes: self.kept,
            truncated: self.truncated,
        }
    }
}
