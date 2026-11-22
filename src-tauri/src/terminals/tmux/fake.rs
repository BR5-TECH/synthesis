//! A fake tmux for tests: a runner that records each command list and answers
//! from a script, and clients that a test drives by hand.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, oneshot};

use super::{ClientSpawner, Tmux, TmuxError, TmuxRunner, TmuxSource};
use crate::terminals::process::{ProcessControl, ProcessHandle};

/// Records each call, and answers the calls of one command (the first
/// argument) from a queue; with no queued answer, the call succeeds with no
/// output.
#[derive(Default)]
pub struct FakeRunner {
    calls: Mutex<Vec<Vec<String>>>,
    replies: Mutex<HashMap<String, VecDeque<Result<String, TmuxError>>>>,
    /// A command that always answers so, after its queue is empty.
    always: Mutex<HashMap<String, Result<String, TmuxError>>>,
}

impl FakeRunner {
    pub fn reply(&self, command: &str, answer: Result<String, TmuxError>) {
        self.replies.lock().unwrap().entry(command.into()).or_default().push_back(answer);
    }

    pub fn always(&self, command: &str, answer: Result<String, TmuxError>) {
        self.always.lock().unwrap().insert(command.into(), answer);
    }

    pub fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }

    /// The calls whose first argument is `command`.
    pub fn calls_of(&self, command: &str) -> Vec<Vec<String>> {
        self.calls()
            .into_iter()
            .filter(|c| c.first().map(String::as_str) == Some(command))
            .collect()
    }
}

impl TmuxRunner for FakeRunner {
    fn run(&self, args: &[String]) -> Result<String, TmuxError> {
        self.calls.lock().unwrap().push(args.to_vec());
        let key = args.first().cloned().unwrap_or_default();
        if let Some(answer) = self.replies.lock().unwrap().get_mut(&key).and_then(|q| q.pop_front())
        {
            return answer;
        }
        self.always.lock().unwrap().get(&key).cloned().unwrap_or(Ok(String::new()))
    }
}

/// One fake client. Its output and its end are in the test's hands.
pub struct FakeClient {
    out: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    exit: Mutex<Option<oneshot::Sender<Option<i32>>>>,
    pub writes: Mutex<Vec<Vec<u8>>>,
    pub sizes: Mutex<Vec<(u16, u16)>>,
    pub terminated: AtomicUsize,
    pub session: String,
    pub start_size: (u16, u16),
}

impl FakeClient {
    pub async fn output(&self, data: &[u8]) {
        let sender = self.out.lock().unwrap().clone();
        if let Some(sender) = sender {
            let _ = sender.send(data.to_vec()).await;
        }
    }

    /// The client ends: its output closes and it reports its exit.
    pub fn end(&self) {
        self.out.lock().unwrap().take();
        if let Some(exit) = self.exit.lock().unwrap().take() {
            let _ = exit.send(Some(0));
        }
    }
}

impl ProcessControl for FakeClient {
    fn write(&self, data: &[u8]) -> io::Result<()> {
        self.writes.lock().unwrap().push(data.to_vec());
        Ok(())
    }
    fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        self.sizes.lock().unwrap().push((cols, rows));
        Ok(())
    }
    fn terminate(&self) {
        self.terminated.fetch_add(1, Ordering::SeqCst);
        self.end();
    }
}

#[derive(Default)]
pub struct FakeClients {
    pub spawned: Mutex<Vec<Arc<FakeClient>>>,
    pub fail: AtomicBool,
}

impl FakeClients {
    pub fn all(&self) -> Vec<Arc<FakeClient>> {
        self.spawned.lock().unwrap().clone()
    }

    pub fn count(&self) -> usize {
        self.spawned.lock().unwrap().len()
    }
}

impl ClientSpawner for FakeClients {
    fn spawn(&self, session: &str, cols: u16, rows: u16) -> io::Result<ProcessHandle> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(io::Error::other("no client"));
        }
        let (out_tx, out_rx) = mpsc::channel(16);
        let (exit_tx, exit_rx) = oneshot::channel();
        let client = Arc::new(FakeClient {
            out: Mutex::new(Some(out_tx)),
            exit: Mutex::new(Some(exit_tx)),
            writes: Mutex::default(),
            sizes: Mutex::default(),
            terminated: AtomicUsize::new(0),
            session: session.to_string(),
            start_size: (cols, rows),
        });
        self.spawned.lock().unwrap().push(client.clone());
        Ok(ProcessHandle { output: out_rx, exit: exit_rx, control: client, environment_name: None })
    }
}

/// A fake tmux, with its parts at hand.
pub struct Fake {
    pub runner: Arc<FakeRunner>,
    pub clients: Arc<FakeClients>,
    pub tmux: Arc<Tmux>,
}

pub fn fake() -> Fake {
    let runner = Arc::new(FakeRunner::default());
    let clients = Arc::new(FakeClients::default());
    let tmux = Arc::new(Tmux { runner: runner.clone(), clients: clients.clone() });
    Fake { runner, clients, tmux }
}

/// A source that gives the same tmux each time, or none.
pub struct FakeSource(pub Option<Arc<Tmux>>);

impl TmuxSource for FakeSource {
    fn probe(&self) -> Result<Arc<Tmux>, &'static str> {
        self.0.clone().ok_or("tmux_not_found")
    }
}
