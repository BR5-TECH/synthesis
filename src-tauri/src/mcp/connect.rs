//! One connection to one MCP server (MCS-FR-BYBA, MCS-FR-APCF to MCS-FR-LZBV).
//!
//! The [`Connector`] seam opens a session; the production one speaks to a
//! local process or to a Streamable HTTP endpoint, and the tests give an
//! in-process server instead. The manager owns what a session becomes after it
//! opens.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rmcp::model::{ClientCapabilities, ClientConfig, Implementation};
use rmcp::service::{NotificationContext, RunningService};
use rmcp::{ClientHandler, RoleClient};
use tokio::sync::mpsc;

use super::config::{HttpTransport, McpAuth, McpServer, McpTransport, StdioTransport};
use super::secrets::{env_slot, header_slot, McpSecrets, BEARER_SLOT};
use super::status::McpFailure;
use crate::openspec::cli::{search_directories, CliEnvironment, CliProbe, RealCliProbe};

/// MCS-FR-ROVQ: how many lines of standard error a session keeps.
pub const STDERR_TAIL_LINES: usize = 20;

/// Something a server told this client after the handshake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientEvent {
    /// MCS-FR-XDZK: `notifications/tools/list_changed`.
    ToolsChanged,
}

/// The client side of one session. It answers the server with the identity of
/// the application, and passes on the notifications the manager acts on.
#[derive(Clone)]
pub struct McpClient {
    events: mpsc::UnboundedSender<ClientEvent>,
    /// MCS-FR-BLBG: the process group of a `stdio` server. It is stopped when
    /// the last copy of the client goes, which is when its session ends.
    group: Option<Arc<ProcessGroup>>,
}

impl McpClient {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<ClientEvent>) {
        let (events, receiver) = mpsc::unbounded_channel();
        (Self { events, group: None }, receiver)
    }
}

/// MCS-FR-BLBG: the process group a `stdio` server leads. A server that exits
/// when its input closes leaves the processes it started; dropping this stops
/// them all.
pub struct ProcessGroup(#[allow(dead_code)] u32);

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Ok(group) = libc::pid_t::try_from(self.0) {
            // SAFETY: `killpg` takes plain integers. A group that already ended
            // gives `ESRCH`, which changes nothing.
            unsafe {
                libc::killpg(group, libc::SIGKILL);
            }
        }
    }
}

impl ClientHandler for McpClient {
    async fn on_tool_list_changed(&self, _context: NotificationContext<RoleClient>) {
        let _ = self.events.send(ClientEvent::ToolsChanged);
    }

    fn get_info(&self) -> ClientConfig {
        ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::new("Synthesis", env!("CARGO_PKG_VERSION")),
        )
    }
}

/// An open session.
pub type McpSession = RunningService<RoleClient, McpClient>;

/// MCS-FR-ROVQ: the last lines of standard error of a `stdio` server, in memory
/// alone.
#[derive(Clone)]
pub struct StderrTail {
    lines: Arc<Mutex<VecDeque<String>>>,
    closed: Arc<tokio::sync::watch::Sender<bool>>,
}

impl Default for StderrTail {
    fn default() -> Self {
        Self { lines: Arc::default(), closed: Arc::new(tokio::sync::watch::channel(false).0) }
    }
}

impl StderrTail {
    pub fn push(&self, line: String) {
        if let Ok(mut lines) = self.lines.lock() {
            if lines.len() == STDERR_TAIL_LINES {
                lines.pop_front();
            }
            lines.push_back(line);
        }
    }

    pub fn lines(&self) -> Vec<String> {
        self.lines.lock().map(|lines| lines.iter().cloned().collect()).unwrap_or_default()
    }

    /// The pipe reached its end.
    pub fn close(&self) {
        self.closed.send_replace(true);
    }

    /// Wait until the pipe reaches its end, or until `limit` passes.
    pub async fn wait_closed(&self, limit: std::time::Duration) {
        let mut closed = self.closed.subscribe();
        let _ = tokio::time::timeout(limit, closed.wait_for(|closed| *closed)).await;
    }
}

/// What one connection attempt is given.
pub struct ConnectRequest {
    pub project_key: String,
    /// The active worktree (MCS-FR-KLQS).
    pub root: PathBuf,
    pub server: McpServer,
    pub secrets: Arc<dyn McpSecrets>,
    pub client: McpClient,
    pub stderr: StderrTail,
}

/// Why an attempt did not give a session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectError {
    Failure(McpFailure),
    /// MCS-FR-PXRJ: a declared slot lost its value since the gate.
    NeedsSecret,
    /// MCS-FR-MGCJ, MCS-FR-SQWD: the server wants a new sign-in.
    NeedsSignIn,
}

/// The seam through which the manager opens a session.
#[async_trait::async_trait]
pub trait Connector: Send + Sync {
    async fn connect(&self, request: ConnectRequest) -> Result<McpSession, ConnectError>;
}

/// The production connector.
pub struct RealConnector;

#[async_trait::async_trait]
impl Connector for RealConnector {
    async fn connect(&self, request: ConnectRequest) -> Result<McpSession, ConnectError> {
        match &request.server.transport {
            McpTransport::Stdio(stdio) => connect_stdio(&request, stdio).await,
            McpTransport::Http(http) => connect_http(&request, http).await,
        }
    }
}

fn secret(request: &ConnectRequest, slot: &str) -> Result<String, ConnectError> {
    match request.secrets.get(&request.project_key, &request.server.id, slot) {
        Ok(Some(value)) => Ok(value),
        Ok(None) => Err(ConnectError::NeedsSecret),
        Err(_) => Err(ConnectError::Failure(McpFailure::KeychainUnavailable)),
    }
}

// ---------------------------------------------------------------------------
// stdio
// ---------------------------------------------------------------------------

/// MCS-FR-KLQS: the working directory of a `stdio` server.
pub fn working_directory(root: &Path, stdio: &StdioTransport) -> PathBuf {
    match &stdio.cwd {
        Some(cwd) => root.join(cwd),
        None => root.to_path_buf(),
    }
}

/// MCS-FR-APCF: the executable of a `command`. A bare name is searched in the
/// directories of the OpenSpec CLI; a path resolves against the working
/// directory.
pub fn resolve_command(
    command: &str,
    workdir: &Path,
    env: &CliEnvironment,
    probe: &dyn CliProbe,
) -> Option<PathBuf> {
    let has_separator = command.contains('/') || (cfg!(windows) && command.contains('\\'));
    if has_separator {
        let path = workdir.join(command);
        return probe.is_executable(&path).then_some(path);
    }
    let names: Vec<String> = if cfg!(windows) {
        vec![format!("{command}.exe"), format!("{command}.cmd"), command.to_string()]
    } else {
        vec![command.to_string()]
    };
    search_directories(env, probe).into_iter().find_map(|dir| {
        names.iter().map(|name| dir.join(name)).find(|path| probe.is_executable(path))
    })
}

/// MCS-FR-KAYL: `PATH` for the child — the folder of the executable, then the
/// search directories, then nothing else; the search directories already start
/// with the inherited `PATH`.
pub fn child_path(
    executable: &Path,
    env: &CliEnvironment,
    probe: &dyn CliProbe,
) -> std::ffi::OsString {
    let mut dirs: Vec<PathBuf> = executable.parent().map(Path::to_path_buf).into_iter().collect();
    for dir in search_directories(env, probe) {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    std::env::join_paths(dirs).unwrap_or_default()
}

async fn connect_stdio(
    request: &ConnectRequest,
    stdio: &StdioTransport,
) -> Result<McpSession, ConnectError> {
    let workdir = working_directory(&request.root, stdio);
    let env = CliEnvironment::from_process();
    let probe = RealCliProbe;
    let executable = resolve_command(&stdio.command, &workdir, &env, &probe)
        .ok_or(ConnectError::Failure(McpFailure::CommandNotFound))?;
    let mut command = tokio::process::Command::new(&executable);
    command.args(&stdio.args).current_dir(&workdir);
    command.env("PATH", child_path(&executable, &env, &probe));
    for (name, value) in &stdio.env {
        command.env(name, value);
    }
    for name in &stdio.secret_env {
        command.env(name, secret(request, &env_slot(name))?);
    }
    let mut wrapped = process_wrap::tokio::CommandWrap::from(command);
    // MCS-FR-BLBG: the server leads its own group, so a stop reaches every
    // process it started.
    #[cfg(unix)]
    wrapped.wrap(process_wrap::tokio::ProcessGroup::leader());
    #[cfg(windows)]
    wrapped.wrap(process_wrap::tokio::JobObject);
    let (transport, stderr) = rmcp::transport::TokioChildProcess::builder(wrapped)
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| {
            ConnectError::Failure(if error.kind() == std::io::ErrorKind::NotFound {
                McpFailure::CommandNotFound
            } else {
                McpFailure::ProcessExited
            })
        })?;
    if let Some(stderr) = stderr {
        let tail = request.stderr.clone();
        tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let mut lines = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tail.push(line);
            }
            tail.close();
        });
    }
    let mut client = request.client.clone();
    client.group = transport.id().map(|pid| Arc::new(ProcessGroup(pid)));
    rmcp::serve_client(client, transport)
        .await
        .map_err(|error| classify(&error, true, McpAuth::None, None))
}

// ---------------------------------------------------------------------------
// Streamable HTTP
// ---------------------------------------------------------------------------

/// MCS-FR-MDBI: the plain and the secret headers of a request.
fn headers_of(
    request: &ConnectRequest,
    http: &HttpTransport,
) -> Result<HashMap<http::HeaderName, http::HeaderValue>, ConnectError> {
    let mut headers = HashMap::new();
    let protocol = ConnectError::Failure(McpFailure::ProtocolError);
    for (name, value) in &http.headers {
        let name = http::HeaderName::from_bytes(name.as_bytes()).map_err(|_| protocol)?;
        let value = http::HeaderValue::from_str(value).map_err(|_| protocol)?;
        headers.insert(name, value);
    }
    for name in &http.secret_headers {
        let value = secret(request, &header_slot(name))?;
        let name = http::HeaderName::from_bytes(name.as_bytes()).map_err(|_| protocol)?;
        let mut value = http::HeaderValue::from_str(&value).map_err(|_| protocol)?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    Ok(headers)
}

/// Whether a URL names this machine. Discovery refuses private addresses unless
/// it is told otherwise, and a local server is a valid record (MCS-FR-XDGN).
pub fn is_loopback(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|url| {
        matches!(
            url.host(),
            Some(url::Host::Domain("localhost"))
                | Some(url::Host::Ipv4(std::net::Ipv4Addr::LOCALHOST))
                | Some(url::Host::Ipv6(std::net::Ipv6Addr::LOCALHOST))
        )
    })
}

async fn connect_http(
    request: &ConnectRequest,
    http: &HttpTransport,
) -> Result<McpSession, ConnectError> {
    use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
    use rmcp::transport::StreamableHttpClientTransport;

    let (client, tls) = crate::tls::rig_http_client()
        .map_err(|_| ConnectError::Failure(McpFailure::ProtocolError))?;
    let mut config = StreamableHttpClientTransportConfig::with_uri(http.url.clone())
        .custom_headers(headers_of(request, http)?);
    let outcome = match http.auth {
        McpAuth::None | McpAuth::Bearer => {
            if http.auth == McpAuth::Bearer {
                config = config.auth_header(secret(request, BEARER_SLOT)?);
            }
            let transport = StreamableHttpClientTransport::with_client(client, config);
            rmcp::serve_client(request.client.clone(), transport).await
        }
        McpAuth::Oauth2 => {
            let manager = super::oauth::authorized_manager(request, http, client.clone()).await?;
            let transport = StreamableHttpClientTransport::with_client(
                rmcp::transport::AuthClient::new(client, manager),
                config,
            );
            rmcp::serve_client(request.client.clone(), transport).await
        }
    };
    outcome.map_err(|error| classify(&error, false, http.auth, Some(&tls)))
}

// ---------------------------------------------------------------------------
// Failure classes (MCS-FR-JNXV)
// ---------------------------------------------------------------------------

type HttpError = rmcp::transport::streamable_http_client::StreamableHttpError<reqwest013::Error>;

fn classify_reqwest(error: &reqwest013::Error) -> McpFailure {
    if error.is_timeout() {
        McpFailure::TimedOut
    } else {
        McpFailure::Unreachable
    }
}

fn classify_auth(error: &rmcp::transport::AuthError) -> ConnectError {
    use rmcp::transport::AuthError;
    match error {
        AuthError::AuthorizationRequired | AuthError::TokenRefreshRejected(_) => {
            ConnectError::NeedsSignIn
        }
        AuthError::HttpError(error) => ConnectError::Failure(classify_reqwest(error)),
        _ => ConnectError::Failure(McpFailure::OauthFailed),
    }
}

/// MCS-FR-JNXV: the failure class of an error of a connection attempt. The
/// error itself is never logged or shown: it can hold a URL or a header.
pub fn classify(
    error: &(dyn std::error::Error + 'static),
    stdio: bool,
    auth: McpAuth,
    tls: Option<&crate::tls::TlsRecord>,
) -> ConnectError {
    if tls.is_some_and(|record| record.take().is_some()) {
        return ConnectError::Failure(McpFailure::TlsUntrusted);
    }
    let closed = if stdio { McpFailure::ProcessExited } else { McpFailure::Unreachable };
    let mut current: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(error) = current {
        if let Some(http) = error.downcast_ref::<HttpError>() {
            use rmcp::transport::streamable_http_client::StreamableHttpError as E;
            match http {
                E::AuthRequired(_) if auth == McpAuth::Oauth2 => return ConnectError::NeedsSignIn,
                E::AuthRequired(_) | E::InsufficientScope(_) => {
                    return ConnectError::Failure(McpFailure::Rejected);
                }
                E::HttpStatus(status) if matches!(status.status, 401 | 403) => {
                    return ConnectError::Failure(McpFailure::Rejected);
                }
                E::Client(error) => return ConnectError::Failure(classify_reqwest(error)),
                // A client without auth reports a refused request as text that
                // starts with its status. Only the status is read.
                E::UnexpectedServerResponse(text)
                    if text.starts_with("HTTP 401") || text.starts_with("HTTP 403") =>
                {
                    return if auth == McpAuth::Oauth2 {
                        ConnectError::NeedsSignIn
                    } else {
                        ConnectError::Failure(McpFailure::Rejected)
                    };
                }
                E::Auth(error) => return classify_auth(error),
                _ => {}
            }
        }
        if let Some(error) = error.downcast_ref::<rmcp::transport::AuthError>() {
            return classify_auth(error);
        }
        if let Some(error) = error.downcast_ref::<reqwest013::Error>() {
            return ConnectError::Failure(classify_reqwest(error));
        }
        if let Some(error) = error.downcast_ref::<rmcp::service::ClientInitializeError>() {
            use rmcp::service::ClientInitializeError as E;
            match error {
                E::ConnectionClosed(_) => return ConnectError::Failure(closed),
                // The transport error is a field, not a source, of this variant.
                E::TransportError { error, .. } => {
                    current = Some(error as &(dyn std::error::Error + 'static));
                    continue;
                }
                _ => {}
            }
        }
        if let Some(error) = error.downcast_ref::<rmcp::ServiceError>() {
            match error {
                rmcp::ServiceError::TransportClosed => return ConnectError::Failure(closed),
                rmcp::ServiceError::Timeout { .. } => {
                    return ConnectError::Failure(McpFailure::TimedOut);
                }
                rmcp::ServiceError::TransportSend(error) => {
                    current = Some(error as &(dyn std::error::Error + 'static));
                    continue;
                }
                _ => {}
            }
        }
        if let Some(error) = error.downcast_ref::<std::io::Error>() {
            use std::io::ErrorKind as K;
            return ConnectError::Failure(match error.kind() {
                K::NotFound if stdio => McpFailure::CommandNotFound,
                K::TimedOut => McpFailure::TimedOut,
                K::BrokenPipe | K::UnexpectedEof | K::ConnectionReset | K::ConnectionAborted => {
                    closed
                }
                K::ConnectionRefused if !stdio => McpFailure::Unreachable,
                _ => McpFailure::ProtocolError,
            });
        }
        current = error.source();
    }
    ConnectError::Failure(McpFailure::ProtocolError)
}
