//! One terminal session: its record, its history, its attached channel, and
//! the pump that moves its process's output (TRS-FR-HIIK .. TRS-FR-NULR).

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use tokio::sync::Notify;

use super::flow::{Batcher, FlowControl, MAX_FRAME_BYTES};
use super::history::History;
use super::model::*;
use super::process::{ProcessControl, ProcessHandle};
use super::tmux::supervisor::Persistence;

/// Where the output of an attached tab goes. Production binds it to a Tauri
/// channel; a test binds it to a recorder.
pub trait OutputSink: Send + Sync {
    /// Sends one frame. An error means the receiver is gone.
    fn send(&self, frame: Vec<u8>) -> Result<(), SinkClosed>;
}

/// The receiver of an attached channel is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SinkClosed;

struct Attached {
    sink: Arc<dyn OutputSink>,
    batcher: Batcher,
    flow: FlowControl,
}

/// What a session is, fixed at creation (TRS-FR-YYER).
#[derive(Debug, Clone)]
pub struct SessionSpec {
    pub id: String,
    pub label: String,
    pub target: TargetRef,
    pub target_name: String,
    pub branch: Option<String>,
    pub kind: TerminalKind,
    pub vendor: Option<CliVendor>,
    pub runs_in: RunsIn,
    pub created_at: String,
    /// The creation order inside the project.
    pub sequence: u64,
}

struct Inner {
    state: TerminalState,
    exit_code: Option<i32>,
    end_reason: Option<EndReason>,
    end_detail: Option<String>,
    pinned: bool,
    busy_run_id: Option<String>,
    environment_name: Option<String>,
    history: History,
    attached: Option<Attached>,
    control: Option<Arc<dyn ProcessControl>>,
    /// The session was closed or its target is gone: a process that starts
    /// later is ended at once.
    ended_by_module: bool,
    /// The last size a tab asked for (TRS-FR-UNIL).
    size: Option<(u16, u16)>,
    /// TRS-FR-FKVW: the tmux session of a persistent session.
    persistence: Option<Arc<Persistence>>,
    /// TRS-FR-TCBS: a restored session whose first client attaches at the
    /// first size a tab asks for.
    waits_for_size: bool,
}

pub struct Session {
    pub spec: SessionSpec,
    inner: Mutex<Inner>,
    /// Wakes a pump that waits for an acknowledgement (TRS-FR-VYIB).
    resume: Notify,
}

impl Session {
    pub fn new(spec: SessionSpec, state: TerminalState, busy_run_id: Option<String>) -> Self {
        Self::with_history(spec, state, busy_run_id, History::new())
    }

    pub fn with_history(
        spec: SessionSpec,
        state: TerminalState,
        busy_run_id: Option<String>,
        history: History,
    ) -> Self {
        Self {
            spec,
            inner: Mutex::new(Inner {
                state,
                exit_code: None,
                end_reason: None,
                end_detail: None,
                pinned: false,
                busy_run_id,
                environment_name: None,
                history,
                attached: None,
                control: None,
                ended_by_module: false,
                size: None,
                persistence: None,
                waits_for_size: false,
            }),
            resume: Notify::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn summary(&self) -> TerminalSummary {
        let inner = self.lock();
        TerminalSummary {
            id: self.spec.id.clone(),
            label: self.spec.label.clone(),
            target: self.spec.target.clone(),
            target_name: self.spec.target_name.clone(),
            branch: self.spec.branch.clone(),
            kind: self.spec.kind,
            vendor: self.spec.vendor,
            runs_in: self.spec.runs_in,
            environment_name: inner.environment_name.clone(),
            state: inner.state,
            exit_code: inner.exit_code,
            end_reason: inner.end_reason,
            end_detail: inner.end_detail.clone(),
            pinned: inner.pinned,
            busy_run_id: inner.busy_run_id.clone(),
            has_history: !inner.history.is_empty(),
            persistent: inner.persistence.as_ref().is_some_and(|p| !p.is_gone()),
            created_at: self.spec.created_at.clone(),
        }
    }

    pub fn state(&self) -> TerminalState {
        self.lock().state
    }

    /// TRS-FR-FKVW: the session is persistent.
    pub fn set_persistence(&self, persistence: Arc<Persistence>) {
        self.lock().persistence = Some(persistence);
    }

    /// The session was closed or its target is gone.
    pub fn is_ended_by_module(&self) -> bool {
        self.lock().ended_by_module
    }

    /// The session is not persistent after all (TRS-FR-FIIJ).
    pub fn clear_persistence(&self) {
        self.lock().persistence = None;
    }

    pub fn persistence(&self) -> Option<Arc<Persistence>> {
        self.lock().persistence.clone()
    }

    /// TRS-FR-TCBS: the first client waits for the first size a tab asks for.
    pub fn wait_for_size(&self) {
        self.lock().waits_for_size = true;
    }

    /// TRS-FR-TCBS: true once, for the size that starts the first client of
    /// a restored session. A session that the module ended starts none.
    pub fn take_wait_for_size(&self) -> bool {
        let mut inner = self.lock();
        let waits = inner.waits_for_size && !inner.ended_by_module;
        inner.waits_for_size = false;
        waits
    }

    pub fn pinned(&self) -> bool {
        self.lock().pinned
    }

    /// TRS-FR-KOYL.
    pub fn set_pinned(&self, pinned: bool) {
        self.lock().pinned = pinned;
    }

    /// TRS-FR-LUUX: returns whether the value changed.
    pub fn set_busy_run_id(&self, busy: Option<String>) -> bool {
        let mut inner = self.lock();
        if inner.busy_run_id == busy {
            return false;
        }
        inner.busy_run_id = busy;
        true
    }

    /// Adds output to the history and to the attached channel, in one step, so
    /// that an attach sees each byte once (TRS-FR-HIIK).
    pub fn push_output(&self, data: &[u8]) {
        let mut inner = self.lock();
        inner.history.push(data);
        let now = Instant::now();
        let mut lost = false;
        if let Some(attached) = inner.attached.as_mut() {
            for frame in attached.batcher.push(data, now) {
                let len = frame.len();
                if attached.sink.send(frame).is_err() {
                    lost = true;
                    break;
                }
                attached.flow.sent(len);
            }
        }
        if lost {
            inner.attached = None;
        }
    }

    /// Sends the pending frame where its time has come. Returns the next
    /// deadline, where a frame still waits.
    pub fn flush_due(&self, now: Instant, force: bool) -> Option<Instant> {
        let mut inner = self.lock();
        let mut lost = false;
        let mut next = None;
        if let Some(attached) = inner.attached.as_mut() {
            let frame =
                if force { Some(attached.batcher.take()) } else { attached.batcher.due(now) };
            if let Some(frame) = frame.filter(|f| !f.is_empty()) {
                let len = frame.len();
                if attached.sink.send(frame).is_err() {
                    lost = true;
                } else {
                    attached.flow.sent(len);
                }
            }
            next = attached.batcher.deadline();
        }
        if lost {
            inner.attached = None;
            return None;
        }
        next
    }

    /// Whether the pump must wait before it reads more output (TRS-FR-VYIB).
    /// With no channel attached, it never waits (TRS-FR-KENG).
    pub fn is_held_back(&self) -> bool {
        // A session that the module ended holds nothing back: its pump drains
        // the process to its end, so the reader and the process can go.
        let inner = self.lock();
        !inner.ended_by_module && inner.attached.as_ref().is_some_and(|a| a.flow.is_paused())
    }

    /// TRS-FR-HIIK: sends the whole history, then lets live output follow.
    pub fn attach(&self, sink: Arc<dyn OutputSink>) -> TerminalAttachment {
        let mut inner = self.lock();
        let history = inner.history.snapshot();
        let mut flow = FlowControl::default();
        let mut ok = true;
        for frame in history.chunks(MAX_FRAME_BYTES) {
            if sink.send(frame.to_vec()).is_err() {
                ok = false;
                break;
            }
            flow.sent(frame.len());
        }
        inner.attached =
            if ok { Some(Attached { sink, batcher: Batcher::default(), flow }) } else { None };
        drop(inner);
        self.resume.notify_one();
        TerminalAttachment { state: self.state(), history_bytes: history.len() }
    }

    /// TRS-FR-NULR.
    pub fn detach(&self) {
        let mut inner = self.lock();
        inner.attached = None;
        if !inner.state.is_live() {
            inner.history.clear();
        }
        drop(inner);
        self.resume.notify_one();
    }

    /// TRS-FR-VYIB.
    pub fn acknowledge(&self, bytes: usize) {
        let mut inner = self.lock();
        if let Some(attached) = inner.attached.as_mut() {
            attached.flow.acknowledged(bytes);
        }
        drop(inner);
        self.resume.notify_one();
    }

    pub async fn wait_for_resume(&self) {
        self.resume.notified().await;
    }

    /// TRS-FR-UNIL: keeps the size a tab asked for.
    pub fn set_size(&self, cols: u16, rows: u16) {
        self.lock().size = Some((cols, rows));
    }

    pub fn control(&self) -> Option<Arc<dyn ProcessControl>> {
        self.lock().control.clone()
    }

    /// The process started. Returns false where the session was ended
    /// meanwhile; the caller then terminates the process.
    pub fn started(
        &self,
        control: Arc<dyn ProcessControl>,
        environment_name: Option<String>,
    ) -> bool {
        let mut inner = self.lock();
        inner.environment_name = environment_name;
        if inner.ended_by_module {
            return false;
        }
        // TRS-FR-UNIL: the size a tab asked for while the session started.
        if let Some((cols, rows)) = inner.size {
            let _ = control.resize(cols, rows);
        }
        inner.control = Some(control);
        inner.state = TerminalState::Running;
        true
    }

    /// TRS-FR-VGGS: the start failed.
    pub fn start_failed(&self, detail: &str) {
        let line = one_line(detail, 300);
        self.push_output(format!("\r\n{line}\r\n").as_bytes());
        let mut inner = self.lock();
        if inner.state == TerminalState::TargetGone {
            return;
        }
        inner.state = TerminalState::Exited;
        inner.end_reason = Some(EndReason::StartFailed);
        inner.end_detail = Some(line);
        inner.control = None;
    }

    /// TRS-FR-DVBF, TRS-FR-HWNA: the process ended on its own.
    pub fn process_ended(&self, exit_code: Option<i32>, reason: EndReason) {
        let mut inner = self.lock();
        inner.control = None;
        if inner.state == TerminalState::TargetGone || inner.ended_by_module {
            return;
        }
        inner.state = TerminalState::Exited;
        inner.exit_code = exit_code;
        inner.end_reason = Some(reason);
    }

    /// TRS-FR-NPDK: the target is gone. Returns the control to terminate, if a
    /// process runs.
    pub fn target_gone(&self) -> Option<Arc<dyn ProcessControl>> {
        let mut inner = self.lock();
        inner.ended_by_module = true;
        inner.state = TerminalState::TargetGone;
        inner.end_reason = Some(EndReason::TargetGone);
        let control = inner.control.take();
        drop(inner);
        // A pump that waits for an acknowledgement drains the ended process.
        self.resume.notify_one();
        control
    }

    /// TRS-FR-CXSV: the session is closed. Returns the control to terminate.
    pub fn close(&self) -> Option<Arc<dyn ProcessControl>> {
        let mut inner = self.lock();
        inner.ended_by_module = true;
        inner.attached = None;
        inner.history.clear();
        let control = inner.control.take();
        drop(inner);
        // A pump that waits for an acknowledgement drains the ended process.
        self.resume.notify_one();
        control
    }

    pub fn has_history(&self) -> bool {
        !self.lock().history.is_empty()
    }
}

/// Cuts text to one line of at most `max` characters.
pub fn one_line(text: &str, max: usize) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string();
    if line.chars().count() <= max {
        line
    } else {
        line.chars().take(max).collect()
    }
}

/// Moves a process's output into its session until the output ends, then
/// records the end. `on_change` runs after the end, so the module can emit
/// `"terminals changed"`.
pub async fn pump(
    session: Arc<Session>,
    mut handle: ProcessHandle,
    end_reason: impl Fn(Option<i32>) -> EndReason,
    on_change: impl Fn(),
) {
    loop {
        while session.is_held_back() {
            session.wait_for_resume().await;
        }
        let deadline = session.flush_due(Instant::now(), false);
        let chunk = match deadline {
            Some(deadline) => {
                tokio::select! {
                    chunk = handle.output.recv() => chunk,
                    _ = tokio::time::sleep_until(deadline.into()) => continue,
                }
            }
            None => handle.output.recv().await,
        };
        match chunk {
            Some(chunk) => session.push_output(&chunk),
            None => break,
        }
    }
    session.flush_due(Instant::now(), true);
    let code = handle.exit.await.ok().flatten();
    session.process_ended(code, end_reason(code));
    on_change();
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
