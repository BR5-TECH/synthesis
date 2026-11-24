use super::*;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::time::Duration;

/// Long enough that only a real defect reaches it.
const WAIT: Duration = Duration::from_secs(30);

/// A process control that reports each write and can hold writes until the
/// test lets them go.
struct Recording {
    written: Mutex<Sender<Vec<u8>>>,
    gate: Mutex<Option<Receiver<()>>>,
    fail: bool,
}

impl Recording {
    /// Returns the control, the receiver of the written chunks, and, when
    /// `gated`, the sender that lets one write go for each message.
    fn new(gated: bool, fail: bool) -> (Arc<Self>, Receiver<Vec<u8>>, Option<Sender<()>>) {
        let (written, seen) = channel();
        let (open, gate) = channel();
        let control = Arc::new(Self {
            written: Mutex::new(written),
            gate: Mutex::new(gated.then_some(gate)),
            fail,
        });
        (control, seen, gated.then_some(open))
    }
}

impl ProcessControl for Recording {
    fn write(&self, data: &[u8]) -> io::Result<()> {
        if let Some(gate) = self.gate.lock().unwrap().as_ref() {
            gate.recv_timeout(WAIT).expect("the test lets the write go");
        }
        self.written.lock().unwrap().send(data.to_vec()).unwrap();
        if self.fail {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        } else {
            Ok(())
        }
    }
    fn resize(&self, _cols: u16, _rows: u16) -> io::Result<()> {
        Ok(())
    }
    fn terminate(&self) {}
}

// TRS-FR-GSYR: the writer thread writes the queued chunks in the order of the
// pushes, each chunk unchanged.
#[test]
fn writes_the_chunks_in_order() {
    let (control, seen, _) = Recording::new(false, false);
    let queue = InputQueue::spawn(control, |_| {}).unwrap();
    for n in 0..200u32 {
        queue.push(n.to_string().as_bytes()).unwrap();
    }
    for n in 0..200u32 {
        assert_eq!(seen.recv_timeout(WAIT).unwrap(), n.to_string().into_bytes());
    }
}

// TRS-FR-GSYR, CEX-FR-VWRP: a push returns while the process does not read its
// input.
#[test]
fn a_push_returns_while_the_process_does_not_read() {
    let (control, seen, gate) = Recording::new(true, false);
    let queue = InputQueue::spawn(control, |_| {}).unwrap();
    queue.push(b"first").unwrap();
    queue.push(b"second").unwrap();
    assert_eq!(queue.queued_bytes(), "firstsecond".len());
    let gate = gate.unwrap();
    gate.send(()).unwrap();
    gate.send(()).unwrap();
    assert_eq!(seen.recv_timeout(WAIT).unwrap(), b"first");
    assert_eq!(seen.recv_timeout(WAIT).unwrap(), b"second");
}

// TRS-FR-DUME: while more than 1 MiB waits, a push is refused and queues
// nothing. Once the queue drains, a push is taken again.
#[test]
fn a_full_backlog_refuses_input_until_it_drains() {
    let (control, seen, gate) = Recording::new(true, false);
    let queue = InputQueue::spawn(control, |_| {}).unwrap();
    let chunk = vec![b'x'; 64 * 1024];
    let mut pushed = 0;
    while queue.queued_bytes() <= MAX_QUEUED_INPUT_BYTES {
        queue.push(&chunk).unwrap();
        pushed += 1;
    }
    let before = queue.queued_bytes();
    assert_eq!(queue.push(b"more"), Err(QueueRefusal::BacklogFull { first: true }));
    assert_eq!(queue.push(b"more"), Err(QueueRefusal::BacklogFull { first: false }));
    assert_eq!(queue.queued_bytes(), before);
    let gate = gate.unwrap();
    for _ in 0..pushed {
        gate.send(()).unwrap();
        seen.recv_timeout(WAIT).unwrap();
    }
    // The writer lowers the count just after it reports a write, so at most
    // the last chunk can still count here, which is under the limit.
    queue.push(b"after").unwrap();
    gate.send(()).unwrap();
    assert_eq!(seen.recv_timeout(WAIT).unwrap(), b"after");
}

// TRS-FR-GSYR: a failed write is reported without its bytes, once for a run
// of failures, and the chunks after it are still written.
#[test]
fn a_failed_write_is_reported_and_the_queue_goes_on() {
    let (control, seen, _) = Recording::new(false, true);
    let (errors_tx, errors) = channel();
    let queue = InputQueue::spawn(control, move |e| errors_tx.send(e.kind()).unwrap()).unwrap();
    queue.push(b"a").unwrap();
    queue.push(b"b").unwrap();
    assert_eq!(seen.recv_timeout(WAIT).unwrap(), b"a");
    assert_eq!(seen.recv_timeout(WAIT).unwrap(), b"b");
    assert_eq!(errors.recv_timeout(WAIT).unwrap(), io::ErrorKind::BrokenPipe);
    // The writer reported the run of two failures once. Dropping the queue
    // ends the writer, which drops the error sender.
    drop(queue);
    assert!(errors.recv_timeout(WAIT).is_err());
}
