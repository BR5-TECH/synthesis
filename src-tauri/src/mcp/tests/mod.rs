//! The tests of the MCP module, and the fixtures they share.
//!
//! [`TestServer`] is a real MCP server that runs in the test process.
//! [`DuplexConnector`] connects the manager to it over an in-memory pipe, so a
//! test drives the real client and the real protocol with no process and no
//! network.

mod config;
mod http_transport;
mod manager;
mod oauth;
mod oauth_flow;
mod operations;
mod stdio;
mod tool;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::{RequestContext, RunningService};
use rmcp::{ErrorData, Peer, RoleServer, ServerHandler, ServiceExt};

use super::config::{HttpTransport, McpAuth, McpServer, McpTransport, StdioTransport};
use super::connect::{ConnectError, ConnectRequest, Connector, McpSession};
use super::manager::{McpManager, McpObserver, Timing};
use super::secrets::MemoryMcpSecrets;
use super::status::{McpFailure, McpServerStatus};
use crate::logging::{Fields, LogLevel};

/// A tool with an object schema of one string property.
pub fn tool(name: &str, description: &str) -> Tool {
    let schema = serde_json::json!({
        "type": "object",
        "properties": { "text": { "type": "string" } },
    });
    let serde_json::Value::Object(schema) = schema else { unreachable!() };
    Tool::new(name.to_string(), description.to_string(), Arc::new(schema))
}

/// The server side of the tests. Its tools can change while it runs.
#[derive(Clone)]
pub struct TestServer {
    pub tools: Arc<Mutex<Vec<Tool>>>,
    /// The peers of the sessions it serves, so a test can notify a client.
    pub peers: Arc<Mutex<Vec<Peer<RoleServer>>>>,
    /// Set when the client cancels a call of `hang` (`notifications/cancelled`).
    pub hang_cancelled: Arc<std::sync::atomic::AtomicBool>,
}

impl Default for TestServer {
    fn default() -> Self {
        Self {
            tools: Arc::new(Mutex::new(vec![
                tool("echo", "Return the text."),
                tool("fail", "Always reports an error."),
                tool("bad_args", "Refuses its arguments."),
                tool("hang", "Never answers."),
                tool("picture", "Returns an image."),
            ])),
            peers: Arc::default(),
            hang_cancelled: Arc::default(),
        }
    }
}

impl ServerHandler for TestServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder().enable_tools().enable_tool_list_changed().build(),
        )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        if let Ok(mut peers) = self.peers.lock() {
            peers.push(context.peer.clone());
        }
        Ok(ListToolsResult::with_all_items(self.tools.lock().unwrap().clone()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let text = request
            .arguments
            .as_ref()
            .and_then(|args| args.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let result = match request.name.as_ref() {
            "echo" => CallToolResult::success(vec![ContentBlock::text(format!("echo: {text}"))]),
            "fail" => CallToolResult::error(vec![ContentBlock::text("the tool failed")]),
            "bad_args" => return Err(ErrorData::invalid_params("text is required", None)),
            "hang" => {
                context.ct.cancelled().await;
                self.hang_cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
                return Err(ErrorData::internal_error("cancelled", None));
            }
            "picture" => {
                CallToolResult::success(vec![ContentBlock::image("aGVsbG8=", "image/png")])
            }
            _ => return Err(ErrorData::invalid_params("no such tool", None)),
        };
        Ok(CallToolResponse::Complete(result))
    }
}

/// What the next connection attempt of a [`DuplexConnector`] does.
#[derive(Clone, Copy, Debug)]
pub enum Plan {
    Serve,
    Fail(ConnectError),
    /// The attempt never ends.
    Hang,
}

/// A connector to [`TestServer`] over `tokio::io::duplex`.
#[derive(Clone, Default)]
pub struct DuplexConnector {
    pub server: TestServer,
    /// Plans for the next attempts, oldest first. An empty queue serves.
    pub plans: Arc<Mutex<VecDeque<Plan>>>,
    /// The server side of each open session, so a test can drop one.
    pub sessions: Arc<Mutex<Vec<RunningService<RoleServer, TestServer>>>>,
    pub attempts: Arc<Mutex<Vec<McpServer>>>,
}

impl DuplexConnector {
    pub fn plan(&self, plans: &[Plan]) {
        self.plans.lock().unwrap().extend(plans.iter().copied());
    }

    pub fn attempt_count(&self) -> usize {
        self.attempts.lock().unwrap().len()
    }

    /// End every open server session, as a server process that exits does.
    pub async fn drop_sessions(&self) {
        let sessions: Vec<_> = self.sessions.lock().unwrap().drain(..).collect();
        for session in sessions {
            let _ = session.cancel().await;
        }
    }

    /// Send `notifications/tools/list_changed` to every client.
    pub async fn notify_tools_changed(&self) {
        let peers: Vec<_> = self.server.peers.lock().unwrap().clone();
        for peer in peers {
            let _ = peer.notify_tool_list_changed().await;
        }
    }
}

#[async_trait::async_trait]
impl Connector for DuplexConnector {
    async fn connect(&self, request: ConnectRequest) -> Result<McpSession, ConnectError> {
        self.attempts.lock().unwrap().push(request.server.clone());
        let plan = self.plans.lock().unwrap().pop_front().unwrap_or(Plan::Serve);
        match plan {
            Plan::Fail(error) => return Err(error),
            Plan::Hang => std::future::pending::<()>().await,
            Plan::Serve => {}
        }
        let (client_io, server_io) = tokio::io::duplex(64 * 1024);
        let server = self.server.clone();
        let sessions = Arc::clone(&self.sessions);
        tokio::spawn(async move {
            if let Ok(session) = server.serve(server_io).await {
                sessions.lock().unwrap().push(session);
            }
        });
        rmcp::serve_client(request.client, client_io)
            .await
            .map_err(|_| ConnectError::Failure(McpFailure::ProtocolError))
    }
}

/// An observer that keeps every status change and every log record.
#[derive(Default)]
pub struct RecordingObserver {
    pub statuses: Mutex<Vec<(String, McpServerStatus)>>,
    pub logs: Mutex<Vec<(LogLevel, String, Fields)>>,
}

impl McpObserver for RecordingObserver {
    fn status_changed(&self, id: &str, status: &McpServerStatus) {
        self.statuses.lock().unwrap().push((id.to_string(), status.clone()));
    }

    fn log(&self, level: LogLevel, message: &str, fields: Fields) {
        self.logs.lock().unwrap().push((level, message.to_string(), fields));
    }
}

impl RecordingObserver {
    /// Every log record as one text, to search for a value that must not be
    /// there.
    pub fn log_text(&self) -> String {
        self.logs
            .lock()
            .unwrap()
            .iter()
            .map(|(_, message, fields)| {
                format!("{message} {}", serde_json::to_string(fields).unwrap())
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Short waits, so the reconnect tests do not wait for real seconds.
pub fn fast_timing() -> Timing {
    Timing {
        connect_limit: Duration::from_secs(10),
        reconnect_waits: vec![Duration::from_millis(5); 5],
        stable_after: Duration::ZERO,
    }
}

/// A manager over memory secrets and a duplex connector, with a recorder.
pub fn test_manager() -> (McpManager, DuplexConnector, Arc<MemoryMcpSecrets>, Arc<RecordingObserver>)
{
    let connector = DuplexConnector::default();
    let secrets = Arc::new(MemoryMcpSecrets::default());
    let manager = McpManager::new(secrets.clone(), Arc::new(connector.clone()), fast_timing());
    let observer = Arc::new(RecordingObserver::default());
    manager.install_observer(observer.clone());
    (manager, connector, secrets, observer)
}

/// Wait until `check` holds, for at most 20 seconds of real time. The limit
/// only stops a test that would otherwise hang; no assertion depends on it.
pub async fn wait_until(mut check: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while !check() {
        assert!(std::time::Instant::now() < deadline, "the expected state never came");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// The status of one server now.
pub fn status_of(manager: &McpManager, id: &str) -> Option<McpServerStatus> {
    manager.snapshot(id).map(|(status, _)| status)
}

pub fn stdio_record(id: &str) -> McpServer {
    McpServer {
        id: id.to_string(),
        name: format!("Server {id}"),
        enabled: true,
        disabled_tools: Vec::new(),
        transport: McpTransport::Stdio(StdioTransport {
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "server".to_string()],
            ..Default::default()
        }),
    }
}

pub fn http_record(id: &str, auth: McpAuth) -> McpServer {
    McpServer {
        id: id.to_string(),
        name: format!("Server {id}"),
        enabled: true,
        disabled_tools: Vec::new(),
        transport: McpTransport::Http(HttpTransport {
            url: "https://mcp.example.com/mcp".to_string(),
            headers: Default::default(),
            secret_headers: Vec::new(),
            auth,
            client_id: None,
            scopes: Vec::new(),
            callback_port: None,
        }),
    }
}
