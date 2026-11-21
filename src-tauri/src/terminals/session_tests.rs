use super::*;
use crate::terminals::flow::HIGH_WATER_BYTES;
use crate::terminals::process::ProcessControl;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{mpsc, oneshot};

type FakeProcess =
    (ProcessHandle, mpsc::Sender<Vec<u8>>, oneshot::Sender<Option<i32>>, Arc<FakeControl>);

#[derive(Default)]
pub(crate) struct Recorder {
    pub frames: Mutex<Vec<Vec<u8>>>,
}

impl Recorder {
    pub fn bytes(&self) -> Vec<u8> {
        self.frames.lock().unwrap().concat()
    }
}

impl OutputSink for Recorder {
    fn send(&self, frame: Vec<u8>) -> Result<(), SinkClosed> {
        self.frames.lock().unwrap().push(frame);
        Ok(())
    }
}

struct Closed;
impl OutputSink for Closed {
    fn send(&self, _frame: Vec<u8>) -> Result<(), SinkClosed> {
        Err(SinkClosed)
    }
}

#[derive(Default)]
pub(crate) struct FakeControl {
    pub terminated: AtomicUsize,
    pub sizes: Mutex<Vec<(u16, u16)>>,
}

impl ProcessControl for FakeControl {
    fn write(&self, _data: &[u8]) -> std::io::Result<()> {
        Ok(())
    }
    fn resize(&self, cols: u16, rows: u16) -> std::io::Result<()> {
        self.sizes.lock().unwrap().push((cols, rows));
        Ok(())
    }
    fn terminate(&self) {
        self.terminated.fetch_add(1, Ordering::SeqCst);
    }
}

pub(crate) fn spec(id: &str) -> SessionSpec {
    SessionSpec {
        id: id.into(),
        label: "Host shell 1".into(),
        target: TargetRef::Worktree { path: "/repo".into() },
        target_name: "repo".into(),
        branch: Some("main".into()),
        kind: TerminalKind::HostShell,
        vendor: None,
        runs_in: RunsIn::Host,
        created_at: "2026-01-01T00:00:00Z".into(),
        sequence: 0,
    }
}

fn fake_process() -> FakeProcess {
    let (out_tx, out_rx) = mpsc::channel(4);
    let (exit_tx, exit_rx) = oneshot::channel();
    let control = Arc::new(FakeControl::default());
    let handle = ProcessHandle {
        output: out_rx,
        exit: exit_rx,
        control: control.clone(),
        environment_name: None,
    };
    (handle, out_tx, exit_tx, control)
}

// TRS-FR-HIIK: an attach sends the whole history first, then live output,
// with no byte lost or repeated.
#[test]
fn attach_sends_history_then_live_output() {
    let session = Session::new(spec("t1"), TerminalState::Running, None);
    session.push_output(b"before ");
    let sink = Arc::new(Recorder::default());
    let attachment = session.attach(sink.clone());
    assert_eq!(attachment.history_bytes, 7);
    session.push_output(b"after");
    session.flush_due(Instant::now(), true);
    assert_eq!(sink.bytes(), b"before after");
}

// TRS-FR-HIIK: a new attach ends the earlier channel.
#[test]
fn a_new_attach_replaces_the_earlier_channel() {
    let session = Session::new(spec("t1"), TerminalState::Running, None);
    let first = Arc::new(Recorder::default());
    let second = Arc::new(Recorder::default());
    session.attach(first.clone());
    session.push_output(b"one");
    session.attach(second.clone());
    session.push_output(b"two");
    session.flush_due(Instant::now(), true);
    assert_eq!(first.bytes(), b"");
    assert_eq!(second.bytes(), b"onetwo");
}

// TRS-FR-PACY: pending output goes out once its deadline is reached.
#[test]
fn pending_output_waits_for_its_deadline() {
    let session = Session::new(spec("t1"), TerminalState::Running, None);
    let sink = Arc::new(Recorder::default());
    session.attach(sink.clone());
    let start = Instant::now();
    session.push_output(b"x");
    let deadline = session.flush_due(start, false).expect("a frame waits");
    assert!(sink.bytes().is_empty());
    assert_eq!(session.flush_due(deadline, false), None);
    assert_eq!(sink.bytes(), b"x");
}

// TRS-FR-VYIB: the pump holds back above the high-water mark until acknowledged.
#[test]
fn holds_back_until_acknowledged() {
    let session = Session::new(spec("t1"), TerminalState::Running, None);
    let sink = Arc::new(Recorder::default());
    session.attach(sink);
    session.push_output(&vec![b'a'; HIGH_WATER_BYTES + MAX_FRAME_BYTES]);
    assert!(session.is_held_back());
    session.acknowledge(HIGH_WATER_BYTES + MAX_FRAME_BYTES);
    assert!(!session.is_held_back());
}

// TRS-FR-KENG: with no channel attached, the pump never holds back.
#[test]
fn never_holds_back_without_a_channel() {
    let session = Session::new(spec("t1"), TerminalState::Running, None);
    session.push_output(&vec![b'a'; HIGH_WATER_BYTES * 2]);
    assert!(!session.is_held_back());
}

// TRS-FR-HIIK: a channel whose receiver is gone is dropped.
#[test]
fn a_closed_channel_is_dropped() {
    let session = Session::new(spec("t1"), TerminalState::Running, None);
    session.attach(Arc::new(Closed));
    session.push_output(&vec![b'a'; MAX_FRAME_BYTES]);
    assert!(!session.is_held_back());
}

// TRS-FR-NULR: a detach of a running session keeps its history; a detach of an
// ended session drops it.
#[test]
fn detach_drops_history_only_after_the_end() {
    let session = Session::new(spec("t1"), TerminalState::Running, None);
    session.push_output(b"x");
    session.detach();
    assert!(session.has_history());
    session.process_ended(Some(0), EndReason::ProcessExited);
    session.detach();
    assert!(!session.has_history());
    assert!(!session.summary().has_history);
}

// TRS-FR-DVBF, TRS-FR-PACY: the pump moves output in and records the exit.
#[tokio::test]
async fn the_pump_moves_output_and_records_the_exit() {
    let session = Arc::new(Session::new(spec("t1"), TerminalState::Running, None));
    let sink = Arc::new(Recorder::default());
    session.attach(sink.clone());
    let (handle, out_tx, exit_tx, _control) = fake_process();
    let changed = Arc::new(AtomicUsize::new(0));
    let counter = changed.clone();
    let task = tokio::spawn(pump(
        session.clone(),
        handle,
        |_| EndReason::ProcessExited,
        move || {
            counter.fetch_add(1, Ordering::SeqCst);
        },
    ));
    out_tx.send(b"hello".to_vec()).await.unwrap();
    drop(out_tx);
    exit_tx.send(Some(7)).unwrap();
    task.await.unwrap();
    assert_eq!(sink.bytes(), b"hello");
    let summary = session.summary();
    assert_eq!(summary.state, TerminalState::Exited);
    assert_eq!(summary.exit_code, Some(7));
    assert_eq!(summary.end_reason, Some(EndReason::ProcessExited));
    assert!(summary.has_history);
    assert_eq!(changed.load(Ordering::SeqCst), 1);
}

// TRS-FR-NPDK: a gone target keeps the history and stays gone after the exit.
#[test]
fn a_gone_target_keeps_its_history_and_state() {
    let session = Session::new(spec("t1"), TerminalState::Running, None);
    let control = Arc::new(FakeControl::default());
    assert!(session.started(control.clone(), None));
    session.push_output(b"log");
    let taken = session.target_gone().expect("a process ran");
    taken.terminate();
    session.process_ended(Some(1), EndReason::ProcessExited);
    let summary = session.summary();
    assert_eq!(summary.state, TerminalState::TargetGone);
    assert_eq!(summary.end_reason, Some(EndReason::TargetGone));
    assert!(summary.has_history);
    assert_eq!(control.terminated.load(Ordering::SeqCst), 1);
}

// TRS-FR-CXSV: a close drops the history, and a process that starts after it
// is refused.
#[test]
fn a_close_drops_history_and_refuses_a_late_start() {
    let session = Session::new(spec("t1"), TerminalState::Starting, None);
    session.push_output(b"starting");
    assert!(session.close().is_none());
    assert!(!session.has_history());
    assert!(!session.started(Arc::new(FakeControl::default()), Some("c".into())));
}

// TRS-FR-VGGS: a failed start writes one line of at most 300 characters.
#[test]
fn a_failed_start_records_one_line() {
    let session = Session::new(spec("t1"), TerminalState::Starting, None);
    let long = format!("first {}\nsecond", "x".repeat(400));
    session.start_failed(&long);
    let summary = session.summary();
    assert_eq!(summary.state, TerminalState::Exited);
    assert_eq!(summary.end_reason, Some(EndReason::StartFailed));
    let detail = summary.end_detail.unwrap();
    assert_eq!(detail.chars().count(), 300);
    assert!(detail.starts_with("first "));
    assert!(summary.has_history);
}

// TRS-FR-HWNA: a VM stop ends a Microsandbox shell as vm_stopped.
#[test]
fn a_vm_stop_is_recorded() {
    let session = Session::new(spec("t1"), TerminalState::Running, None);
    session.process_ended(None, EndReason::VmStopped);
    assert_eq!(session.summary().end_reason, Some(EndReason::VmStopped));
}

// TRS-FR-KOYL, TRS-FR-LUUX: the pinned flag and the busy run are held.
#[test]
fn pinned_and_busy_are_held() {
    let session = Session::new(spec("t1"), TerminalState::Running, None);
    session.set_pinned(true);
    assert!(session.summary().pinned);
    assert!(session.set_busy_run_id(Some("run-1".into())));
    assert!(!session.set_busy_run_id(Some("run-1".into())));
    assert_eq!(session.summary().busy_run_id.as_deref(), Some("run-1"));
}

// TRS-FR-CXSV, TRS-FR-VYIB: a close while the pump waits for an
// acknowledgement wakes the pump, so it drains the process and ends.
#[tokio::test]
async fn a_close_while_held_back_lets_the_pump_end() {
    let session = Arc::new(Session::new(spec("t1"), TerminalState::Running, None));
    session.attach(Arc::new(Recorder::default()));
    let (handle, out_tx, exit_tx, _control) = fake_process();
    let task = tokio::spawn(pump(session.clone(), handle, |_| EndReason::ProcessExited, || {}));
    out_tx.send(vec![b'a'; HIGH_WATER_BYTES + MAX_FRAME_BYTES]).await.unwrap();
    // The pump has read the flood and now waits for an acknowledgement.
    while !session.is_held_back() {
        tokio::task::yield_now().await;
    }
    session.close();
    out_tx.send(b"tail".to_vec()).await.unwrap();
    drop(out_tx);
    exit_tx.send(Some(0)).unwrap();
    task.await.unwrap();
}

// TRS-FR-NPDK, TRS-FR-VYIB: a gone target holds nothing back.
#[test]
fn a_gone_target_holds_nothing_back() {
    let session = Session::new(spec("t1"), TerminalState::Running, None);
    session.attach(Arc::new(Recorder::default()));
    session.push_output(&vec![b'a'; HIGH_WATER_BYTES + MAX_FRAME_BYTES]);
    assert!(session.is_held_back());
    session.target_gone();
    assert!(!session.is_held_back());
}

// TRS-FR-UNIL: a size asked for while the session starts is applied when it runs.
#[test]
fn a_size_asked_for_while_starting_is_applied_at_the_start() {
    let session = Session::new(spec("t1"), TerminalState::Starting, None);
    session.set_size(120, 40);
    let control = Arc::new(FakeControl::default());
    assert!(session.started(control.clone(), Some("c".into())));
    assert_eq!(*control.sizes.lock().unwrap(), vec![(120, 40)]);
}

// TRS-FR-NPDK: a target gone while starting stays gone after a late start or
// a failed start.
#[test]
fn a_target_gone_while_starting_stays_gone() {
    let session = Session::new(spec("t1"), TerminalState::Starting, None);
    session.target_gone();
    assert!(!session.started(Arc::new(FakeControl::default()), None));
    session.start_failed("boom");
    assert_eq!(session.summary().state, TerminalState::TargetGone);
}
