//! One persistent session in tmux, and the clients that show it
//! (TRS-FR-BHCB, TRS-FR-CKIC, TRS-FR-KBOS).
//!
//! [`supervise`] gives the session's pump one process handle for the whole
//! life of the tmux session. A client that ends while its tmux session lives
//! is replaced, and the handle ends only when the tmux session or its process
//! ends, or when the module ends the client.

use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, oneshot};

use super::meta::{parse_pane, Meta, PaneState, PINNED_KEY};
use super::{commands, Tmux, TmuxError};
use crate::terminals::process::{HostCommand, ProcessControl, ProcessHandle, OUTPUT_QUEUE_CHUNKS};

/// TRS-FR-CKIC: the clients that may end in [`REATTACH_WINDOW`] before the
/// module stops to attach.
pub const MAX_CLIENT_ENDS: usize = 3;
pub const REATTACH_WINDOW: Duration = Duration::from_secs(10);

/// One tmux session of a persistent session.
pub struct Persistence {
    pub tmux: Arc<Tmux>,
    pub name: String,
    /// The tmux session is known to be gone.
    gone: AtomicBool,
    /// Makes each pin write read and write the flag as one step.
    pin_writes: Mutex<()>,
}

impl Persistence {
    pub fn new(tmux: Arc<Tmux>, name: String) -> Self {
        Self { tmux, name, gone: AtomicBool::new(false), pin_writes: Mutex::new(()) }
    }

    /// Whether the module removed the tmux session or saw it gone.
    pub fn is_gone(&self) -> bool {
        self.gone.load(Ordering::SeqCst)
    }

    fn run(&self, args: Vec<String>) -> Result<String, TmuxError> {
        self.tmux.runner.run(&args)
    }

    /// TRS-FR-CXSV: removes the tmux session. A session that is gone already
    /// is no failure.
    pub fn kill(&self) -> Result<(), TmuxError> {
        match self.run(commands::kill(&self.name)) {
            Ok(_) | Err(TmuxError::Status) => {
                self.gone.store(true, Ordering::SeqCst);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// TRS-FR-QMZM: writes the flag that `current` reads at the time of the
    /// write. Writes go one at a time, so the last write holds the last flag.
    pub fn write_pinned(&self, current: impl FnOnce() -> bool) -> Result<(), TmuxError> {
        let _one_at_a_time = self.pin_writes.lock().unwrap_or_else(|p| p.into_inner());
        self.set_pinned(current())
    }

    /// TRS-FR-QMZM: keeps the pinned flag in the tmux session.
    pub fn set_pinned(&self, pinned: bool) -> Result<(), TmuxError> {
        let value = if pinned { "1" } else { "0" };
        self.run(commands::set_option(&self.name, PINNED_KEY, value)).map(|_| ())
    }

    /// TRS-FR-KBOS: the pane state, or `None` where the tmux session is gone.
    pub fn pane(&self) -> Result<Option<PaneState>, TmuxError> {
        match self.run(commands::pane_state(&self.name)) {
            Ok(text) => parse_pane(&text).map(Some).ok_or(TmuxError::Output),
            Err(TmuxError::Status) => {
                self.gone.store(true, Ordering::SeqCst);
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// TRS-FR-TCBS, TRS-FR-HJZR: the pane's lines. With `above_screen`, only
    /// the lines above the visible screen, which are none while the pane has
    /// no history: tmux would give the first visible line in their place.
    pub fn capture(&self, above_screen: bool) -> Result<String, TmuxError> {
        if above_screen {
            let size = self.run(commands::history_size(&self.name))?;
            let size: u64 = size.trim().parse().map_err(|_| TmuxError::Output)?;
            if size == 0 {
                return Ok(String::new());
            }
        }
        self.run(commands::capture(&self.name, above_screen))
    }
}

/// TRS-FR-BHCB, TRS-FR-FIIJ: creates the tmux session. Where a call fails, a
/// tmux session that the call made is removed again.
pub fn create(
    tmux: Arc<Tmux>,
    name: String,
    command: &HostCommand,
    meta: &Meta,
    warn: &dyn Fn(&'static str),
) -> Result<Arc<Persistence>, TmuxError> {
    let persistence = Arc::new(Persistence::new(tmux, name));
    if let Err(e) = persistence.run(commands::create(&persistence.name, command, meta)) {
        let _ = persistence.kill();
        return Err(e);
    }
    // TRS-FR-ANJK: best effort, as a table with no key is refused.
    // A table that is empty already refuses with a status; another failure
    // can leave a key of tmux active, so it is reported.
    for unbind in commands::unbind_all() {
        if let Err(e @ (TmuxError::Spawn | TmuxError::Timeout | TmuxError::Output)) =
            persistence.run(unbind)
        {
            warn(e.cause());
        }
    }
    // TRS-FR-KBOS: best effort, as tmux 3.2 has no such option.
    let _ = persistence.run(commands::quiet_dead_pane());
    Ok(persistence)
}

/// The control of a persistent session: input and size go to the client of
/// the moment.
struct PersistentControl {
    current: Mutex<Option<Arc<dyn ProcessControl>>>,
    size: Mutex<(u16, u16)>,
    /// The module ended the client: no new client attaches.
    stopping: AtomicBool,
}

impl PersistentControl {
    fn current(&self) -> Option<Arc<dyn ProcessControl>> {
        self.current.lock().ok().and_then(|c| c.clone())
    }

    fn set_current(&self, control: Option<Arc<dyn ProcessControl>>) {
        if let Ok(mut current) = self.current.lock() {
            *current = control;
        }
    }

    fn size(&self) -> (u16, u16) {
        self.size.lock().map(|s| *s).unwrap_or((80, 24))
    }

    fn stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }
}

impl ProcessControl for PersistentControl {
    fn write(&self, data: &[u8]) -> io::Result<()> {
        match self.current() {
            Some(client) => client.write(data),
            None => Err(io::Error::from(io::ErrorKind::NotConnected)),
        }
    }

    fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        if let Ok(mut size) = self.size.lock() {
            *size = (cols, rows);
        }
        match self.current() {
            Some(client) => client.resize(cols, rows),
            None => Ok(()),
        }
    }

    /// TRS-FR-OTLH: ends the client alone. The tmux session stays.
    fn terminate(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        if let Some(client) = self.current() {
            client.terminate();
        }
    }
}

/// TRS-FR-CKIC: records one client end at `now`, and returns whether
/// [`MAX_CLIENT_ENDS`] ends fall inside [`REATTACH_WINDOW`].
pub fn record_end(ends: &mut VecDeque<Instant>, now: Instant) -> bool {
    ends.push_back(now);
    while ends.front().is_some_and(|t| now.duration_since(*t) > REATTACH_WINDOW) {
        ends.pop_front();
    }
    ends.len() >= MAX_CLIENT_ENDS
}

/// Reports one cause of a `WARN` record.
pub type Warn = Arc<dyn Fn(&'static str) + Send + Sync>;

/// TRS-FR-BHCB, TRS-FR-CKIC, TRS-FR-KBOS: attaches a client and gives one
/// handle for the life of the tmux session. A first client that does not
/// start is an error; the caller then starts no session.
pub fn supervise(
    persistence: Arc<Persistence>,
    cols: u16,
    rows: u16,
    warn: Warn,
) -> io::Result<ProcessHandle> {
    let first = persistence.tmux.clients.spawn(&persistence.name, cols, rows)?;
    let control = Arc::new(PersistentControl {
        current: Mutex::new(Some(first.control.clone())),
        size: Mutex::new((cols, rows)),
        stopping: AtomicBool::new(false),
    });
    let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>(OUTPUT_QUEUE_CHUNKS);
    let (exit_tx, exit_rx) = oneshot::channel();
    let watched = control.clone();
    crate::project_scope::context::spawn(async move {
        let code = watch(persistence, watched, first, out_tx, warn).await;
        let _ = exit_tx.send(code);
    });
    Ok(ProcessHandle { output: out_rx, exit: exit_rx, control, environment_name: None })
}

async fn watch(
    persistence: Arc<Persistence>,
    control: Arc<PersistentControl>,
    mut client: ProcessHandle,
    out_tx: mpsc::Sender<Vec<u8>>,
    warn: Warn,
) -> Option<i32> {
    let mut ends: VecDeque<Instant> = VecDeque::new();
    loop {
        while let Some(chunk) = client.output.recv().await {
            // A pump that is gone still lets the client drain to its end.
            let _ = out_tx.send(chunk).await;
        }
        let _ = (&mut client.exit).await;
        control.set_current(None);
        if control.stopping() {
            return None;
        }
        let probe = persistence.clone();
        let pane = tauri::async_runtime::spawn_blocking(move || probe.pane())
            .await
            .unwrap_or(Err(TmuxError::Spawn));
        match pane {
            Ok(None) => return None,
            Ok(Some(PaneState::Dead(code))) => {
                if persistence.kill().is_err() {
                    warn("tmux_kill_failed");
                }
                return code;
            }
            Ok(Some(PaneState::Alive)) => {
                if record_end(&mut ends, Instant::now()) {
                    warn("tmux_client_ends_again");
                    return None;
                }
                let (cols, rows) = control.size();
                match persistence.tmux.clients.spawn(&persistence.name, cols, rows) {
                    Ok(next) => {
                        control.set_current(Some(next.control.clone()));
                        if control.stopping() {
                            next.control.terminate();
                        }
                        client = next;
                    }
                    Err(_) => {
                        warn("tmux_attach_failed");
                        return None;
                    }
                }
            }
            Err(e) => {
                warn(e.cause());
                return None;
            }
        }
    }
}

#[cfg(test)]
#[path = "supervisor_tests.rs"]
mod tests;
