//! TRS-FR-GSYR, TRS-FR-DUME: the input queue of a session.
//!
//! `write_terminal_input` runs on the main thread, so that keystrokes keep the
//! order in which they arrive (`CEX-command-execution.md` CEX-FR-IXRE). A
//! write to a pseudo-terminal blocks while the process does not read its
//! input, so the command only puts the bytes in this queue. One writer thread
//! of the session writes them to the process in order (CEX-FR-VWRP).

use std::io;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use super::process::ProcessControl;

/// TRS-FR-DUME: above this many queued bytes, a write is refused.
pub const MAX_QUEUED_INPUT_BYTES: usize = 1024 * 1024;

/// Why the queue took no bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueRefusal {
    /// TRS-FR-DUME: more than [`MAX_QUEUED_INPUT_BYTES`] wait in the queue.
    /// `first` is true for the first refusal after a push was taken, so one
    /// stall writes one log record and not one for each keystroke.
    BacklogFull { first: bool },
    /// The writer thread has stopped.
    Closed,
}

/// Why a session took no input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputRefusal {
    /// The session runs no process, or its writer thread has stopped.
    NotRunning,
    /// TRS-FR-DUME. `first` as in [`QueueRefusal::BacklogFull`].
    BacklogFull { first: bool },
}

/// The input queue of one running process. Dropping it ends the writer
/// thread after the bytes already queued.
pub struct InputQueue {
    sender: mpsc::Sender<Vec<u8>>,
    queued: Arc<AtomicUsize>,
    refusing: AtomicBool,
}

impl InputQueue {
    /// Starts the writer thread for `control`. `on_error` runs on that thread
    /// for each write that fails. It gets the error and nothing of the input.
    pub fn spawn(
        control: Arc<dyn ProcessControl>,
        on_error: impl Fn(&io::Error) + Send + 'static,
    ) -> io::Result<Self> {
        let (sender, receiver) = mpsc::channel::<Vec<u8>>();
        let queued = Arc::new(AtomicUsize::new(0));
        let counter = queued.clone();
        std::thread::Builder::new().name("terminal-input".into()).spawn(move || {
            // One failure is reported for each run of failed writes, so a
            // process that went away writes one log record and not one for
            // each chunk.
            let mut failing = false;
            for chunk in receiver {
                match control.write(&chunk) {
                    Ok(()) => failing = false,
                    Err(error) => {
                        if !failing {
                            on_error(&error);
                        }
                        failing = true;
                    }
                }
                counter.fetch_sub(chunk.len(), Ordering::SeqCst);
            }
        })?;
        Ok(Self { sender, queued, refusing: AtomicBool::new(false) })
    }

    /// TRS-FR-GSYR: puts `data` last in the queue and returns at once.
    /// TRS-FR-DUME: refuses it while more than the limit waits.
    pub fn push(&self, data: &[u8]) -> Result<(), QueueRefusal> {
        if self.queued.load(Ordering::SeqCst) > MAX_QUEUED_INPUT_BYTES {
            let first = !self.refusing.swap(true, Ordering::SeqCst);
            return Err(QueueRefusal::BacklogFull { first });
        }
        self.refusing.store(false, Ordering::SeqCst);
        self.queued.fetch_add(data.len(), Ordering::SeqCst);
        self.sender.send(data.to_vec()).map_err(|_| {
            self.queued.fetch_sub(data.len(), Ordering::SeqCst);
            QueueRefusal::Closed
        })
    }

    /// The bytes that wait to be written.
    pub fn queued_bytes(&self) -> usize {
        self.queued.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
