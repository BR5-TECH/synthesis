//! The live connections of a project's MCP servers (MCS-FR-RXZG to MCS-FR-QTSW,
//! MCS-FR-ENII, MCS-FR-LTTM).
//!
//! The manager holds one entry for each record of the open project. A server
//! that may connect gets a supervisor task, which opens the session, keeps the
//! tool list, reconnects after a drop, and ends the session when it is told
//! to. Everything a turn needs is read from memory (MCS-FR-ENII).

use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use rmcp::model::{CallToolRequestParams, CallToolResponse, CallToolResult, Tool};
use rmcp::{Peer, RoleClient};
use tokio::sync::watch;

use super::config::{ListedRecord, McpConfigError, McpServer};
use super::connect::{
    ClientEvent, ConnectError, ConnectRequest, Connector, McpClient, McpSession, StderrTail,
};
use super::secrets::McpSecrets;
use super::status::{gate, MachineFacts, McpFailure, McpServerStatus, McpToolInfo};
use crate::log_fields;
use crate::logging::{Fields, LogLevel};

/// MCS-FR-BYBA: the limit of the handshake and the tool list together.
pub const CONNECT_LIMIT: Duration = Duration::from_secs(30);
/// MCS-FR-HXJF: the waits before each reconnect attempt.
pub const RECONNECT_WAITS: [Duration; 5] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
    Duration::from_secs(16),
];
/// MCS-FR-EMPU: how long a session must live before its end restarts the waits.
pub const STABLE_AFTER: Duration = Duration::from_secs(10);
/// MCS-FR-QTSW: how long a stop waits for a session to end.
pub const STOP_LIMIT: Duration = Duration::from_secs(5);

/// Where status changes and log records go. The production one emits the
/// Tauri event and writes to the session log.
pub trait McpObserver: Send + Sync {
    fn status_changed(&self, id: &str, status: &McpServerStatus);
    fn log(&self, level: LogLevel, message: &str, fields: Fields);
}

/// An observer that drops everything, used until the application installs one.
pub struct NoopObserver;

impl McpObserver for NoopObserver {
    fn status_changed(&self, _id: &str, _status: &McpServerStatus) {}
    fn log(&self, _level: LogLevel, _message: &str, _fields: Fields) {}
}

/// The open project, as the manager knows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectContext {
    pub key: String,
    /// The active worktree.
    pub root: PathBuf,
}

/// One record and what this machine knows about it.
#[derive(Clone, Debug)]
pub struct ServerInput {
    pub listed: ListedRecord,
    pub facts: MachineFacts,
}

/// MCS-FR-ENII: one tool of a `ready` server.
#[derive(Clone, Debug)]
pub struct McpToolSnapshot {
    pub server_id: String,
    pub server_name: String,
    pub tool: Tool,
}

/// MCS-FR-YWVC: a sign-in that cannot start, because one already runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignInRefused;

/// MCS-FR-LTTM: why a call gave no result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpCallError {
    NotConnected,
    TimedOut,
    /// A JSON-RPC error: its code and its message.
    Rpc {
        code: i32,
        message: String,
    },
    ProtocolError,
}

struct Run {
    generation: u64,
    stop: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

struct SignIn {
    cancel: watch::Sender<bool>,
    previous: McpServerStatus,
}

struct Server {
    id: String,
    record: Option<McpServer>,
    invalid: Option<McpConfigError>,
    status: McpServerStatus,
    tools: Vec<Tool>,
    peer: Option<Peer<RoleClient>>,
    run: Option<Run>,
    started_with: Option<String>,
    stderr: StderrTail,
    sign_in: Option<SignIn>,
}

impl Server {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            record: None,
            invalid: None,
            status: McpServerStatus::Disabled,
            tools: Vec::new(),
            peer: None,
            run: None,
            started_with: None,
            stderr: StderrTail::default(),
            sign_in: None,
        }
    }
}

#[derive(Default)]
struct State {
    project: Option<ProjectContext>,
    servers: Vec<Server>,
    next_generation: u64,
}

/// The timing of the manager. The tests give short waits.
#[derive(Clone, Debug)]
pub struct Timing {
    pub connect_limit: Duration,
    pub reconnect_waits: Vec<Duration>,
    /// MCS-FR-EMPU: a session that ends sooner counts as a failed attempt.
    pub stable_after: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            connect_limit: CONNECT_LIMIT,
            reconnect_waits: RECONNECT_WAITS.to_vec(),
            stable_after: STABLE_AFTER,
        }
    }
}

struct Inner {
    state: Mutex<State>,
    /// MCS-FR-RYIK: one reconcile at a time, so two cannot start one server
    /// twice.
    reconciling: Mutex<()>,
    secrets: Arc<dyn McpSecrets>,
    connector: Arc<dyn Connector>,
    observer: RwLock<Arc<dyn McpObserver>>,
    timing: Timing,
}

/// The Tauri state of the MCP connections.
#[derive(Clone)]
pub struct McpManager {
    inner: Arc<Inner>,
}

/// MCS-FR-RYIK: what decides whether a session restarts. The name and the tool
/// filter are left out, because they change nothing the server sees.
fn start_key(record: &McpServer) -> String {
    let mut key = record.clone();
    key.name.clear();
    key.disabled_tools.clear();
    serde_json::to_string(&key).unwrap_or_default()
}

/// MCS-FR-HBYA: the tools a record offers, of the tools its server listed.
fn offered(tools: &[Tool], record: &McpServer) -> usize {
    tools.iter().filter(|t| !record.disabled_tools.iter().any(|d| d == t.name.as_ref())).count()
}

fn stop_run(server: &mut Server) -> Option<tokio::task::JoinHandle<()>> {
    server.peer = None;
    server.tools.clear();
    server.started_with = None;
    server.run.take().map(|run| {
        let _ = run.stop.send(true);
        run.task
    })
}

impl McpManager {
    pub fn new(
        secrets: Arc<dyn McpSecrets>,
        connector: Arc<dyn Connector>,
        timing: Timing,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(State::default()),
                reconciling: Mutex::new(()),
                secrets,
                connector,
                observer: RwLock::new(Arc::new(NoopObserver)),
                timing,
            }),
        }
    }

    pub fn install_observer(&self, observer: Arc<dyn McpObserver>) {
        if let Ok(mut slot) = self.inner.observer.write() {
            *slot = observer;
        }
    }

    pub fn secrets(&self) -> Arc<dyn McpSecrets> {
        Arc::clone(&self.inner.secrets)
    }

    pub fn connector(&self) -> Arc<dyn Connector> {
        Arc::clone(&self.inner.connector)
    }

    pub fn connect_limit(&self) -> Duration {
        self.inner.timing.connect_limit
    }

    fn observer(&self) -> Arc<dyn McpObserver> {
        self.inner
            .observer
            .read()
            .map(|o| Arc::clone(&o))
            .unwrap_or_else(|_| Arc::new(NoopObserver))
    }

    fn log(&self, level: LogLevel, message: &str, fields: Fields) {
        self.observer().log(level, message, fields);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.inner.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Set a status and emit it when it changed (MCS-FR-NOKX). Returns the
    /// event to emit after the lock is released.
    fn set_status(
        server: &mut Server,
        status: McpServerStatus,
    ) -> Option<(String, McpServerStatus)> {
        if server.status == status {
            return None;
        }
        server.status = status.clone();
        Some((server.id.clone(), status))
    }

    fn emit(&self, changes: Vec<(String, McpServerStatus)>) {
        let observer = self.observer();
        for (id, status) in changes {
            let mut fields = log_fields! { "server" => &id, "status" => status.code() };
            if let McpServerStatus::Failed { failure } = &status {
                fields.insert("failure".into(), serde_json::json!(failure.code()));
            }
            observer.log(LogLevel::Info, "mcp server status changed", fields);
            observer.status_changed(&id, &status);
        }
    }

    /// MCS-FR-RYIK: bring the connections in line with the records. A server
    /// whose record and facts did not change keeps its session, unless it is
    /// `restart`.
    pub fn reconcile(
        &self,
        project: ProjectContext,
        inputs: Vec<ServerInput>,
        restart: Option<&str>,
    ) {
        let _one_at_a_time = self.inner.reconciling.lock().unwrap_or_else(|e| e.into_inner());
        let mut changes = Vec::new();
        let mut stopped = Vec::new();
        {
            let mut state = self.lock();
            if state.project.as_ref() != Some(&project) {
                for server in state.servers.iter_mut() {
                    stopped.extend(stop_run(server));
                    if let Some(sign_in) = server.sign_in.take() {
                        let _ = sign_in.cancel.send(true);
                    }
                }
                state.servers.clear();
                state.project = Some(project.clone());
            }
            let mut previous: Vec<Server> = std::mem::take(&mut state.servers);
            let mut next = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for input in inputs {
                // MCS-FR-HBOD: a later record with the id of an earlier one is
                // listed as invalid, and has no server of its own here.
                if !seen.insert(input.listed.id.clone()) {
                    continue;
                }
                let index = previous
                    .iter()
                    .position(|s| s.id == input.listed.id && !input.listed.id.is_empty());
                let mut server = match index {
                    Some(index) => previous.remove(index),
                    None => Server::new(&input.listed.id),
                };
                match input.listed.record {
                    Err(code) => {
                        stopped.extend(stop_run(&mut server));
                        server.record = None;
                        server.invalid = Some(code);
                        changes.extend(Self::set_status(&mut server, McpServerStatus::Invalid));
                    }
                    Ok(record) => {
                        server.invalid = None;
                        let key = start_key(&record);
                        match gate(&record, &input.facts) {
                            Err(status) => {
                                stopped.extend(stop_run(&mut server));
                                let keep_sign_in = server.sign_in.is_some()
                                    && status == McpServerStatus::NeedsSignIn;
                                if !keep_sign_in {
                                    changes.extend(Self::set_status(&mut server, status));
                                }
                            }
                            Ok(()) => {
                                let unchanged = server.started_with.as_deref()
                                    == Some(key.as_str())
                                    && server.run.is_some()
                                    && restart != Some(server.id.as_str());
                                server.record = Some(record.clone());
                                if unchanged {
                                    // A new name or a new tool filter keeps the session.
                                    if let McpServerStatus::Ready { .. } = server.status {
                                        let count = offered(&server.tools, &record);
                                        changes.extend(Self::set_status(
                                            &mut server,
                                            McpServerStatus::Ready { tool_count: count },
                                        ));
                                    }
                                } else {
                                    stopped.extend(stop_run(&mut server));
                                    server.started_with = Some(key);
                                    let generation = state.next_generation;
                                    state.next_generation += 1;
                                    server.run = Some(self.spawn_run(
                                        generation,
                                        &project,
                                        &record,
                                        &server.stderr,
                                    ));
                                }
                            }
                        }
                        server.record = Some(record);
                    }
                }
                next.push(server);
            }
            for mut server in previous {
                stopped.extend(stop_run(&mut server));
                if let Some(sign_in) = server.sign_in.take() {
                    let _ = sign_in.cancel.send(true);
                }
            }
            state.servers = next;
        }
        drop(stopped);
        self.emit(changes);
    }

    /// MCS-FR-ETIS, MCS-FR-UOIO: forget the project and end every session.
    pub fn close_project(&self) {
        let handles = {
            let mut state = self.lock();
            let mut handles = Vec::new();
            for server in state.servers.iter_mut() {
                handles.extend(stop_run(server));
                if let Some(sign_in) = server.sign_in.take() {
                    let _ = sign_in.cancel.send(true);
                }
            }
            state.servers.clear();
            state.project = None;
            handles
        };
        if !handles.is_empty() {
            self.log(
                LogLevel::Info,
                "mcp sessions ended",
                log_fields! { "count" => handles.len() },
            );
        }
    }

    /// MCS-FR-QTSW: end every session and wait up to [`STOP_LIMIT`] for them.
    pub async fn shutdown(&self) {
        let handles = {
            let mut state = self.lock();
            let handles: Vec<_> = state.servers.iter_mut().filter_map(stop_run).collect();
            state.servers.clear();
            state.project = None;
            handles
        };
        let _ = tokio::time::timeout(STOP_LIMIT, futures_util::future::join_all(handles)).await;
    }

    /// Spawn the supervisor of one run. It is called with the state locked, so
    /// the run is registered before its task can read or change anything.
    fn spawn_run(
        &self,
        generation: u64,
        project: &ProjectContext,
        record: &McpServer,
        stderr: &StderrTail,
    ) -> Run {
        let (stop, stop_rx) = watch::channel(false);
        let manager = self.clone();
        let (project, record, stderr) = (project.clone(), record.clone(), stderr.clone());
        let task = tauri::async_runtime::spawn(async move {
            manager.supervise(generation, project, record, stderr, stop_rx).await;
        });
        let tauri::async_runtime::JoinHandle::Tokio(task) = task;
        Run { generation, stop, task }
    }

    /// Run `apply` on the server of this run, when the run is still current.
    fn with_run<T>(
        &self,
        id: &str,
        generation: u64,
        apply: impl FnOnce(&mut Server) -> T,
    ) -> Option<T> {
        let mut state = self.lock();
        let server = state.servers.iter_mut().find(|s| s.id == id)?;
        if server.run.as_ref().map(|r| r.generation) != Some(generation) {
            return None;
        }
        Some(apply(server))
    }

    fn run_status(&self, id: &str, generation: u64, status: McpServerStatus) {
        if let Some(Some(change)) = self.with_run(id, generation, |s| Self::set_status(s, status)) {
            self.emit(vec![change]);
        }
    }

    /// MCS-FR-BYBA: open a session and list its tools, under the limit.
    pub async fn open(
        connector: &dyn Connector,
        request: ConnectRequest,
        limit: Duration,
    ) -> Result<(McpSession, Vec<Tool>), ConnectError> {
        let work = async {
            let session = connector.connect(request).await?;
            let tools = session
                .peer()
                .list_all_tools()
                .await
                .map_err(|_| ConnectError::Failure(McpFailure::ProtocolError))?;
            Ok((session, tools))
        };
        tokio::time::timeout(limit, work)
            .await
            .unwrap_or(Err(ConnectError::Failure(McpFailure::TimedOut)))
    }

    async fn supervise(
        &self,
        generation: u64,
        project: ProjectContext,
        record: McpServer,
        stderr: StderrTail,
        mut stop: watch::Receiver<bool>,
    ) {
        let id = record.id.clone();
        let fields = || {
            log_fields! { "server" => &record.id, "transport" => record.transport_name(), "auth" => record.auth().as_str() }
        };
        let waits = self.inner.timing.reconnect_waits.clone();
        // `None` before the first session opened; afterwards the number of
        // reconnect attempts made since the last drop (MCS-FR-HXJF).
        let mut attempts: Option<usize> = None;
        loop {
            if let Some(made) = attempts {
                let Some(wait) = waits.get(made).copied() else { return };
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    _ = stop.changed() => return,
                }
            }
            self.run_status(&id, generation, McpServerStatus::Connecting);
            let (client, mut events) = McpClient::new();
            let request = ConnectRequest {
                project_key: project.key.clone(),
                root: project.root.clone(),
                server: record.clone(),
                secrets: Arc::clone(&self.inner.secrets),
                client,
                stderr: stderr.clone(),
            };
            let started = std::time::Instant::now();
            let opened = tokio::select! {
                opened = Self::open(self.inner.connector.as_ref(), request, self.inner.timing.connect_limit) => opened,
                _ = stop.changed() => return,
            };
            let (session, tools) = match opened {
                Ok(opened) => opened,
                Err(error) => {
                    let status = match error {
                        ConnectError::Failure(failure) => McpServerStatus::Failed { failure },
                        ConnectError::NeedsSecret => McpServerStatus::NeedsSecret,
                        ConnectError::NeedsSignIn => McpServerStatus::NeedsSignIn,
                    };
                    let mut warn = fields();
                    warn.insert("status".into(), serde_json::json!(status.code()));
                    if let McpServerStatus::Failed { failure } = &status {
                        warn.insert("failure".into(), serde_json::json!(failure.code()));
                    }
                    self.log(LogLevel::Warn, "mcp connection failed", warn);
                    let retry = matches!(error, ConnectError::Failure(_)) && attempts.is_some();
                    if retry {
                        let made = attempts.unwrap_or(0) + 1;
                        if made < waits.len() {
                            attempts = Some(made);
                            continue;
                        }
                    }
                    self.run_status(&id, generation, status);
                    return;
                }
            };
            let opened_at = std::time::Instant::now();
            let peer = session.peer().clone();
            let count = self.store_tools(&id, generation, &peer, tools);
            let mut opened_fields = fields();
            opened_fields.insert("tools".into(), serde_json::json!(count));
            opened_fields.insert(
                "durationMs".into(),
                serde_json::json!(started.elapsed().as_millis() as u64),
            );
            self.log(LogLevel::Info, "mcp connection opened", opened_fields);

            let cancel = session.cancellation_token();
            let mut waiting = tokio::spawn(session.waiting());
            let stopped = loop {
                tokio::select! {
                    _ = &mut waiting => break false,
                    _ = stop.changed() => break true,
                    event = events.recv() => {
                        if event != Some(ClientEvent::ToolsChanged) {
                            continue;
                        }
                        // MCS-FR-XDZK, MCS-FR-QTSW: a stop is seen during a relist too.
                        let limit = self.inner.timing.connect_limit;
                        tokio::select! {
                            listed = tokio::time::timeout(limit, peer.list_all_tools()) => {
                                if let Ok(Ok(tools)) = listed {
                                    self.store_tools(&id, generation, &peer, tools);
                                }
                            }
                            _ = stop.changed() => break true,
                        }
                    }
                }
            };
            if stopped {
                cancel.cancel();
                if tokio::time::timeout(STOP_LIMIT, &mut waiting).await.is_err() {
                    waiting.abort();
                }
                self.log(LogLevel::Info, "mcp connection closed", fields());
                return;
            }
            self.log(LogLevel::Warn, "mcp connection dropped", fields());
            // MCS-FR-EMPU: a session that lived a short time is a failed attempt,
            // so a server that opens and drops at once does not restart forever.
            if opened_at.elapsed() >= self.inner.timing.stable_after {
                attempts = Some(0);
            } else {
                let made = attempts.map_or(1, |made| made + 1);
                if made >= waits.len() {
                    let closed = match record.transport {
                        super::config::McpTransport::Stdio(_) => McpFailure::ProcessExited,
                        super::config::McpTransport::Http(_) => McpFailure::Unreachable,
                    };
                    self.with_run(&id, generation, |s| {
                        s.peer = None;
                        s.tools.clear();
                    });
                    self.run_status(&id, generation, McpServerStatus::Failed { failure: closed });
                    return;
                }
                attempts = Some(made);
            }
            // MCS-FR-HXJF: the server is `connecting` from the moment it drops.
            let change = self.with_run(&id, generation, |s| {
                s.peer = None;
                s.tools.clear();
                Self::set_status(s, McpServerStatus::Connecting)
            });
            if let Some(Some(change)) = change {
                self.emit(vec![change]);
            }
        }
    }

    fn store_tools(
        &self,
        id: &str,
        generation: u64,
        peer: &Peer<RoleClient>,
        tools: Vec<Tool>,
    ) -> usize {
        let result = self.with_run(id, generation, |server| {
            let offered = server.record.as_ref().map(|r| offered(&tools, r)).unwrap_or(tools.len());
            server.tools = tools;
            server.peer = Some(peer.clone());
            (offered, Self::set_status(server, McpServerStatus::Ready { tool_count: offered }))
        });
        match result {
            Some((count, change)) => {
                self.emit(change.into_iter().collect());
                count
            }
            None => 0,
        }
    }

    // -----------------------------------------------------------------------
    // Reads
    // -----------------------------------------------------------------------

    /// The status of each server, and its tools, for `list_mcp_servers`.
    pub fn snapshot(&self, id: &str) -> Option<(McpServerStatus, Vec<McpToolInfo>)> {
        let state = self.lock();
        let server = state.servers.iter().find(|s| s.id == id)?;
        let disabled = server.record.as_ref().map(|r| r.disabled_tools.clone()).unwrap_or_default();
        let tools = server
            .tools
            .iter()
            .map(|tool| McpToolInfo {
                name: tool.name.to_string(),
                description: tool.description.as_deref().unwrap_or_default().to_string(),
                disabled: disabled.iter().any(|d| d == tool.name.as_ref()),
            })
            .collect();
        Some((server.status.clone(), tools))
    }

    /// MCS-FR-ROVQ: the last lines of standard error of one server.
    pub fn stderr_tail(&self, id: &str) -> StderrTail {
        let state = self.lock();
        state.servers.iter().find(|s| s.id == id).map(|s| s.stderr.clone()).unwrap_or_default()
    }

    /// MCS-FR-ENII: the tools of every `ready` server, from memory alone.
    pub fn mcp_tools_now(&self, project_key: &str) -> Vec<McpToolSnapshot> {
        let state = self.lock();
        if state.project.as_ref().map(|p| p.key.as_str()) != Some(project_key) {
            return Vec::new();
        }
        let mut snapshots = Vec::new();
        for server in &state.servers {
            let Some(record) = &server.record else { continue };
            if !matches!(server.status, McpServerStatus::Ready { .. }) {
                continue;
            }
            for tool in &server.tools {
                if record.disabled_tools.iter().any(|d| d == tool.name.as_ref()) {
                    continue;
                }
                snapshots.push(McpToolSnapshot {
                    server_id: server.id.clone(),
                    server_name: record.name.clone(),
                    tool: tool.clone(),
                });
            }
        }
        snapshots
    }

    /// MCS-FR-LTTM: one `tools/call` on the server's current session. When the
    /// deadline passes first, the request is dropped, and `rmcp` sends
    /// `notifications/cancelled` for it.
    pub async fn call_mcp_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: serde_json::Map<String, serde_json::Value>,
        deadline: Duration,
    ) -> Result<CallToolResult, McpCallError> {
        let peer = {
            let state = self.lock();
            state
                .servers
                .iter()
                .find(|s| s.id == server_id && matches!(s.status, McpServerStatus::Ready { .. }))
                .and_then(|s| s.peer.clone())
        };
        let peer = peer.ok_or(McpCallError::NotConnected)?;
        let params = CallToolRequestParams::new(tool_name.to_string()).with_arguments(arguments);
        match tokio::time::timeout(deadline, peer.call_tool_once(params)).await {
            Err(_) => Err(McpCallError::TimedOut),
            Ok(Ok(CallToolResponse::Complete(result))) => Ok(result),
            Ok(Ok(_)) => Err(McpCallError::ProtocolError),
            Ok(Err(rmcp::ServiceError::McpError(error))) => {
                Err(McpCallError::Rpc { code: error.code.0, message: error.message.to_string() })
            }
            Ok(Err(rmcp::ServiceError::Timeout { .. })) => Err(McpCallError::TimedOut),
            Ok(Err(rmcp::ServiceError::TransportClosed)) => Err(McpCallError::NotConnected),
            Ok(Err(_)) => Err(McpCallError::ProtocolError),
        }
    }

    // -----------------------------------------------------------------------
    // Sign-in (MCS-FR-YWVC, MCS-FR-CMWV)
    // -----------------------------------------------------------------------

    /// MCS-FR-YWVC: mark the sign-in started. `Err` when one is already running,
    /// or when the manager holds no server with this id.
    pub fn begin_sign_in(&self, id: &str) -> Result<watch::Receiver<bool>, SignInRefused> {
        let (cancel, cancelled) = watch::channel(false);
        let change = {
            let mut state = self.lock();
            let server = state.servers.iter_mut().find(|s| s.id == id).ok_or(SignInRefused)?;
            if server.sign_in.is_some() {
                return Err(SignInRefused);
            }
            server.sign_in = Some(SignIn { cancel, previous: server.status.clone() });
            Self::set_status(server, McpServerStatus::SigningIn)
        };
        self.emit(change.into_iter().collect());
        self.log(LogLevel::Info, "mcp sign-in started", log_fields! { "server" => id });
        Ok(cancelled)
    }

    /// MCS-FR-CMWV: end a sign-in at once and restore the status it replaced.
    pub fn cancel_sign_in(&self, id: &str) {
        let change = {
            let mut state = self.lock();
            let Some(server) = state.servers.iter_mut().find(|s| s.id == id) else { return };
            let Some(sign_in) = server.sign_in.take() else { return };
            let _ = sign_in.cancel.send(true);
            Self::set_status(server, sign_in.previous)
        };
        self.emit(change.into_iter().collect());
        self.log(LogLevel::Info, "mcp sign-in cancelled", log_fields! { "server" => id });
    }

    /// The end of a sign-in that was not cancelled. On success the caller
    /// reconciles, so the server connects (MCS-FR-GTTD).
    pub fn finish_sign_in(&self, id: &str, outcome: Result<(), McpFailure>) {
        let change = {
            let mut state = self.lock();
            let Some(server) = state.servers.iter_mut().find(|s| s.id == id) else { return };
            if server.sign_in.take().is_none() {
                return;
            }
            match outcome {
                // The reconcile that follows connects the server (MCS-FR-GTTD).
                Ok(()) => None,
                Err(failure) => Self::set_status(server, McpServerStatus::Failed { failure }),
            }
        };
        self.emit(change.into_iter().collect());
        let mut fields = log_fields! { "server" => id, "succeeded" => outcome.is_ok() };
        if let Err(failure) = outcome {
            fields.insert("failure".into(), serde_json::json!(failure.code()));
            self.log(LogLevel::Warn, "mcp sign-in failed", fields);
        } else {
            self.log(LogLevel::Info, "mcp sign-in ended", fields);
        }
    }

    /// The project the manager serves now.
    pub fn project(&self) -> Option<ProjectContext> {
        self.lock().project.clone()
    }
}
