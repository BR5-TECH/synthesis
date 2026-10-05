//! The machine's Docker backend
//! (`specifications/core/GSS-global-settings-storage.md` GSS-FR-35..GSS-FR-40).
//!
//! How this machine reaches a container runtime is a fact about the machine, so
//! the selected mode, the Docker Engine endpoint, and the Docker CLI executable
//! path are **user-global**: they live beside every other user-global setting in
//! `app_data_dir()/synthesis.toml` and are never written into a project's
//! `.synthesis/` (GSS-FR-35).
//!
//! Two modes, and each is verified on its own terms (GSS-FR-38). `bollard`
//! reaches the Docker Engine directly over the configured endpoint; `docker_cli`
//! runs the executable the author named. Verification succeeds only when the
//! **daemon answered** — a Docker CLI that exists and runs is not a Docker that
//! works — and the success it earns is bound to the three values it was
//! performed against (GSS-FR-39), so changing any of them asks for Verify again
//! while a daemon that is merely stopped afterwards does not.
//!
//! Everything that actually reaches Docker sits behind a trait
//! ([`DockerEngine`], and `crate::agentic::CliRunner` for the CLI mode), so the
//! decisions in this module are exercisable on a machine with no Docker at all.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::agentic::{CliOutput, CliRunner, FileProbe, RunError};

// ---------------------------------------------------------------------------
// Typed refusals (GSS-FR-38, GSS-FR-40)
// ---------------------------------------------------------------------------

/// Docker CLI mode: the path field is empty.
pub const ERR_CLI_PATH_EMPTY: &str = "cli_path_empty";
/// Docker CLI mode: nothing exists at the configured path.
pub const ERR_CLI_NOT_FOUND: &str = "cli_not_found";
/// Docker CLI mode: something is there, but this process cannot run it.
pub const ERR_CLI_NOT_EXECUTABLE: &str = "cli_not_executable";
/// Docker CLI mode: it runs, and it is not the Docker CLI.
pub const ERR_NOT_THE_DOCKER_CLI: &str = "not_the_docker_cli";
/// Either mode: the daemon could not answer. Told apart from every refusal
/// above because it asks the author to start Docker rather than to fix a path.
pub const ERR_DAEMON_UNREACHABLE: &str = "daemon_unreachable";
/// Either mode: the bounded verification deadline elapsed.
pub const ERR_TIMED_OUT: &str = "timed_out";
/// Docker Engine mode: the endpoint field is empty.
pub const ERR_ENDPOINT_EMPTY: &str = "endpoint_empty";
/// Docker Engine mode: the endpoint is not a well-formed socket path, pipe
/// name, or URL.
pub const ERR_ENDPOINT_INVALID: &str = "endpoint_invalid";
/// GSS-FR-40: nothing has verified, so no Docker operation may start.
pub const ERR_DOCKER_BACKEND_UNVERIFIED: &str = "docker_backend_unverified";

/// AIC-FR-04's bound, applied here for the same reason: a hung daemon or a
/// binary that never returns must not wedge the settings window.
pub const VERIFY_TIMEOUT: Duration = Duration::from_secs(20);

/// The request bound a connection is given while a caller times the same call
/// from **outside**.
///
/// The outer deadline has to be the one that ends a daemon that never answers,
/// because it is the only one that can tell a timeout from a daemon that
/// refused — and GSS-FR-38 keeps those two refusals apart because they ask the
/// author to correct different things. A connection whose own request bound
/// equals the outer deadline races it, and which of the two fires first decides
/// which refusal the author reads. Twice the deadline makes the order certain
/// and still bounds a connection that nothing else bounds.
pub(crate) fn connect_bound(deadline: Duration) -> Duration {
    deadline.saturating_mul(2)
}

// ---------------------------------------------------------------------------
// Wire shapes (GSS contract surface)
// ---------------------------------------------------------------------------

/// Which of the two ways to reach Docker is selected (GSS-FR-35).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockerBackendMode {
    /// Bollard / Docker Engine — the default, so a machine that has configured
    /// nothing reaches Docker the way the platform itself does.
    #[default]
    Bollard,
    DockerCli,
}

/// Which form of endpoint is configured (GSS-FR-36).
///
/// Stored beside its value as a scalar pair rather than as a tagged value,
/// because the whole record is serialised into TOML and a table-valued field
/// would have to be declared after every scalar of its parent. The wire shape
/// [`DockerEndpoint`] is the tagged one the contract defines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockerEndpointKind {
    /// The platform's own default location, and what an unset endpoint reads as.
    #[default]
    Automatic,
    UnixSocket,
    WindowsPipe,
    Tcp,
}

/// The endpoint, as the contract carries it: the string `"automatic"`, or a
/// one-key object naming the form and its value (GSS-FR-36).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockerEndpoint {
    #[default]
    Automatic,
    UnixSocket(String),
    WindowsPipe(String),
    Tcp(String),
}

impl DockerEndpoint {
    pub fn kind(&self) -> DockerEndpointKind {
        match self {
            DockerEndpoint::Automatic => DockerEndpointKind::Automatic,
            DockerEndpoint::UnixSocket(_) => DockerEndpointKind::UnixSocket,
            DockerEndpoint::WindowsPipe(_) => DockerEndpointKind::WindowsPipe,
            DockerEndpoint::Tcp(_) => DockerEndpointKind::Tcp,
        }
    }

    /// The configured location, or `""` for the automatic default which names
    /// none.
    pub fn value(&self) -> &str {
        match self {
            DockerEndpoint::Automatic => "",
            DockerEndpoint::UnixSocket(v)
            | DockerEndpoint::WindowsPipe(v)
            | DockerEndpoint::Tcp(v) => v.as_str(),
        }
    }

    pub fn from_parts(kind: DockerEndpointKind, value: &str) -> Self {
        match kind {
            DockerEndpointKind::Automatic => DockerEndpoint::Automatic,
            DockerEndpointKind::UnixSocket => DockerEndpoint::UnixSocket(value.to_string()),
            DockerEndpointKind::WindowsPipe => DockerEndpoint::WindowsPipe(value.to_string()),
            DockerEndpointKind::Tcp => DockerEndpoint::Tcp(value.to_string()),
        }
    }
}

/// What the author selected: the mode, the endpoint that mode may use, and the
/// executable path the other mode may use (GSS-FR-36).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DockerBackendConfig {
    pub mode: DockerBackendMode,
    pub endpoint: DockerEndpoint,
    pub cli_path: Option<String>,
}

impl DockerBackendConfig {
    /// The path, trimmed, or `""` when none is configured.
    fn cli_path_str(&self) -> &str {
        self.cli_path.as_deref().unwrap_or("").trim()
    }
}

/// Whether the selection standing in the store has verified (GSS-FR-39).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockerBackendState {
    #[default]
    Unverified,
    Verified,
}

/// What `load_docker_backend`, `save_docker_backend`, and a successful
/// `verify_docker_backend` all return.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DockerBackend {
    pub mode: DockerBackendMode,
    pub endpoint: DockerEndpoint,
    pub cli_path: Option<String>,
    pub state: DockerBackendState,
    pub server_version: Option<String>,
    pub verified_at: Option<String>,
}

/// The candidate `detect_docker_cli_binary` found, if any (GSS-FR-37). The same
/// shape `detect_agentic_cli_binary` returns, for the same reason: a detection
/// reports a path and commits nothing.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct DetectedDockerCli {
    pub path: Option<String>,
}

// ---------------------------------------------------------------------------
// The persisted record (GSS-FR-35, GSS-FR-39)
// ---------------------------------------------------------------------------

/// The success a verification earned, together with **the three values it was
/// earned against** (GSS-FR-39).
///
/// Recording the bound selection rather than a bare boolean is what makes
/// "changing the mode, the endpoint, or the CLI path requires Verify again"
/// enforceable by comparison instead of by remembering to clear a flag on every
/// write path.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DockerVerification {
    pub mode: DockerBackendMode,
    pub endpoint_kind: DockerEndpointKind,
    pub endpoint_value: String,
    pub cli_path: String,
    pub server_version: String,
    pub verified_at: String,
}

/// The Docker backend as it sits in `synthesis.toml` (GSS-FR-35).
///
/// Every scalar is declared before the one sub-table, because TOML requires a
/// table's scalar values to be emitted before its sub-tables and serde
/// serialises in declaration order — the same constraint `PersistedState`'s two
/// `active_*` fields answer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DockerBackendRecord {
    pub mode: DockerBackendMode,
    pub endpoint_kind: DockerEndpointKind,
    pub endpoint_value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_path: Option<String>,
    /// `None` until something has verified. Declared last: it is the record's
    /// only sub-table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified: Option<DockerVerification>,
}

impl DockerBackendRecord {
    /// The selection alone, without the verification behind it.
    pub fn config(&self) -> DockerBackendConfig {
        DockerBackendConfig {
            mode: self.mode,
            endpoint: DockerEndpoint::from_parts(self.endpoint_kind, &self.endpoint_value),
            cli_path: self.cli_path.clone(),
        }
    }

    /// Write a selection into the record, **keeping the value the other mode
    /// uses** (GSS-FR-36) and **dropping the stored verification** (GSS-FR-39).
    ///
    /// The success is dropped rather than merely stopped from matching, so a
    /// selection changed away and back reads unverified and asks for Verify
    /// again: what a success describes is a daemon that answered at the moment
    /// it was asked, and an author who has been round a different endpoint in
    /// between has not been told anything about this one since.
    ///
    /// Safe to drop unconditionally because `record_success` applies the
    /// selection first and then writes the verification it earned.
    pub fn apply(&mut self, config: &DockerBackendConfig) {
        self.verified = None;
        self.mode = config.mode;
        // GSS-FR-36: each mode's own value is written by a save made **in that
        // mode**, so the store keeps both and returning to a mode returns to
        // the value it last carried. A save made from the Docker Engine mode
        // says nothing about the Docker CLI path and leaves it exactly as it
        // is, and the other way round.
        match config.mode {
            DockerBackendMode::Bollard => {
                self.endpoint_kind = config.endpoint.kind();
                self.endpoint_value = config.endpoint.value().trim().to_string();
            }
            DockerBackendMode::DockerCli => {
                self.cli_path = config
                    .cli_path
                    .as_ref()
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty());
            }
        }
    }

    /// GSS-FR-39: is the stored success still the success of what is selected
    /// now?
    ///
    /// `apply` drops a verification whenever the selection is written, so this
    /// answers `Some` for every record this application produced. It stays a
    /// comparison rather than a bare `is_some` because `synthesis.toml` is a
    /// file an author can edit, and a success carried over onto a selection it
    /// was never earned against must not be read as one.
    fn verification_matches(&self) -> Option<&DockerVerification> {
        let stored = self.verified.as_ref()?;
        let matches = stored.mode == self.mode
            && stored.endpoint_kind == self.endpoint_kind
            && stored.endpoint_value == self.endpoint_value
            && stored.cli_path == self.cli_path.clone().unwrap_or_default();
        matches.then_some(stored)
    }

    /// The outbound record (GSS-FR-39): `verified` only while the stored
    /// selection still equals the bound one.
    pub fn outbound(&self) -> DockerBackend {
        let verified = self.verification_matches();
        DockerBackend {
            mode: self.mode,
            endpoint: DockerEndpoint::from_parts(self.endpoint_kind, &self.endpoint_value),
            cli_path: self.cli_path.clone(),
            state: if verified.is_some() {
                DockerBackendState::Verified
            } else {
                DockerBackendState::Unverified
            },
            server_version: verified.map(|v| v.server_version.clone()),
            verified_at: verified.map(|v| v.verified_at.clone()),
        }
    }

    /// Record a success against the selection it was earned by.
    pub fn record_success(&mut self, config: &DockerBackendConfig, version: &str, at: &str) {
        self.apply(config);
        self.verified = Some(DockerVerification {
            mode: self.mode,
            endpoint_kind: self.endpoint_kind,
            endpoint_value: self.endpoint_value.clone(),
            cli_path: self.cli_path.clone().unwrap_or_default(),
            server_version: version.to_string(),
            verified_at: at.to_string(),
        });
    }
}

// ---------------------------------------------------------------------------
// The resolved backend (GSS-FR-40)
// ---------------------------------------------------------------------------

/// What an actual Docker operation is performed through — a build (PSS-FR-26) or
/// a container launch (EAC-FR-39).
///
/// It carries the selection and nothing about whether Docker is answering right
/// now, because [`resolve_docker_backend`] probes nothing: an operation that
/// finds the backend unavailable reports its own failure rather than a second
/// verification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedDockerBackend {
    Engine { endpoint: DockerEndpoint },
    Cli { path: String },
}

impl ResolvedDockerBackend {
    pub fn mode(&self) -> DockerBackendMode {
        match self {
            ResolvedDockerBackend::Engine { .. } => DockerBackendMode::Bollard,
            ResolvedDockerBackend::Cli { .. } => DockerBackendMode::DockerCli,
        }
    }
}

/// GSS-FR-40: the verified backend, or the refusal. Reads the record and asks
/// the daemon nothing.
pub fn resolve_from_record(record: &DockerBackendRecord) -> Result<ResolvedDockerBackend, String> {
    if record.verification_matches().is_none() {
        return Err(ERR_DOCKER_BACKEND_UNVERIFIED.to_string());
    }
    match record.mode {
        DockerBackendMode::Bollard => Ok(ResolvedDockerBackend::Engine {
            endpoint: DockerEndpoint::from_parts(record.endpoint_kind, &record.endpoint_value),
        }),
        DockerBackendMode::DockerCli => {
            let path = record.cli_path.clone().unwrap_or_default();
            if path.trim().is_empty() {
                // A record cannot verify in this mode without a path
                // (GSS-FR-38), so this is a store edited from outside rather
                // than a state the application can reach.
                return Err(ERR_DOCKER_BACKEND_UNVERIFIED.to_string());
            }
            Ok(ResolvedDockerBackend::Cli { path })
        }
    }
}

// ---------------------------------------------------------------------------
// Detection (GSS-FR-37)
// ---------------------------------------------------------------------------

/// What detection looks for. Docker Desktop, Rancher Desktop, and every Linux
/// package install the same executable name.
pub const DOCKER_EXECUTABLE_NAMES: &[&str] = &["docker"];

/// GSS-FR-37: the first Docker CLI on `PATH` or in a conventional location, or
/// `None`. Persists nothing and executes nothing — exactly as CLI-kind agentic
/// detection does not (AIC-FR-03), whose search order this reuses.
pub fn detect_docker_cli(
    path_var: Option<&str>,
    home: Option<&Path>,
    probe: &dyn FileProbe,
) -> Option<String> {
    let mut dirs = crate::agentic::detection_directories(path_var, home);
    if let Some(home) = home {
        // Docker Desktop's own per-user CLI directory, which recent versions
        // prefer, and Rancher Desktop's. Neither is on a GUI application's
        // inherited `PATH`.
        dirs.push(home.join(".docker/bin"));
        dirs.push(home.join(".rd/bin"));
    }
    crate::agentic::detect_in(&dirs, DOCKER_EXECUTABLE_NAMES, probe)
}

// ---------------------------------------------------------------------------
// The Docker Engine, behind a seam
// ---------------------------------------------------------------------------

/// Why the Docker Engine did not answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineError {
    /// The daemon could not be reached, or refused to answer.
    Unreachable(String),
    /// The bounded deadline elapsed.
    TimedOut,
}

/// The Docker Engine, narrowed to the questions this application asks of it.
///
/// Production binds it to Bollard's own connection ([`BollardEngine`]); a test
/// binds a stub, which is what lets every decision in this module and in the
/// project image build be exercised on a machine with no Docker daemon.
pub trait DockerEngine: Send + Sync {
    /// GSS-FR-38: connect at `endpoint` and ask the daemon its version. The
    /// value returned is the **daemon's**, so a client that starts and a daemon
    /// that never answers are told apart by which arm this returns.
    fn server_version(
        &self,
        endpoint: &DockerEndpoint,
        timeout: Duration,
    ) -> Result<String, EngineError>;
}

/// Production Docker Engine, over Bollard.
pub struct BollardEngine;

/// Connect at `endpoint`, and read the daemon's version.
///
/// The whole call is confined to a current-thread runtime this function owns and
/// joins, the same arrangement `ai_openrouter` uses to drive an async SDK from a
/// synchronous call path: nothing here leaks a runtime into a Tauri command.
#[cfg(not(test))]
fn bollard_server_version(
    endpoint: &DockerEndpoint,
    timeout: Duration,
) -> Result<String, EngineError> {
    let endpoint = endpoint.clone();
    let handle = std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(e) => return Err(EngineError::Unreachable(e.to_string())),
        };
        runtime.block_on(async move {
            // The wider bound is deliberate: the `tokio::time::timeout` below is
            // what tells `TimedOut` from `Unreachable` (GSS-FR-38), so the
            // connection's own bound must not fire first and answer for it.
            let docker =
                connect(&endpoint, connect_bound(timeout)).map_err(EngineError::Unreachable)?;
            match tokio::time::timeout(timeout, docker.version()).await {
                Err(_) => Err(EngineError::TimedOut),
                Ok(Err(e)) => Err(EngineError::Unreachable(e.to_string())),
                Ok(Ok(version)) => version
                    .version
                    .filter(|v| !v.is_empty())
                    // A daemon that answers without naming its version has not
                    // proved it is a Docker daemon, and GSS-FR-38 forbids
                    // reporting a success the daemon did not give.
                    .ok_or_else(|| {
                        EngineError::Unreachable("the daemon reported no version".to_string())
                    }),
            }
        })
    });
    handle
        .join()
        .unwrap_or_else(|_| Err(EngineError::Unreachable("the probe thread failed".to_string())))
}

/// Open a Bollard connection for the configured endpoint form (GSS-FR-36).
///
/// `timeout` bounds **every request** made through the connection that is
/// returned, and it is applied to each endpoint form alike. The platform
/// default is the reason it is applied after the match rather than only
/// through the connector arguments: `connect_with_defaults` takes no timeout
/// and silently keeps Bollard's own two minutes, so a caller that asked for a
/// different bound would get two minutes on that one endpoint form and its own
/// value on every other. An operation that outlives two minutes — an image
/// build is the ordinary case — then fails on the platform default alone,
/// which is a failure the endpoint selection has no business deciding.
#[cfg(not(test))]
pub(crate) fn connect(
    endpoint: &DockerEndpoint,
    timeout: Duration,
) -> Result<bollard::Docker, String> {
    let seconds = timeout.as_secs().max(1);
    let version = bollard::API_DEFAULT_VERSION;
    match endpoint {
        DockerEndpoint::Automatic => bollard::Docker::connect_with_defaults(),
        DockerEndpoint::UnixSocket(path) => {
            bollard::Docker::connect_with_socket(path, seconds, version)
        }
        // A named pipe exists only on Windows, and Bollard offers the
        // connector only there. A record that names one on another platform is
        // a selection that platform cannot dial, which verification reports as
        // the daemon not answering rather than as a malformed endpoint — the
        // endpoint is well formed, there is simply no pipe to reach.
        #[cfg(windows)]
        DockerEndpoint::WindowsPipe(pipe) => {
            bollard::Docker::connect_with_named_pipe(pipe, seconds, version)
        }
        #[cfg(not(windows))]
        DockerEndpoint::WindowsPipe(_) => {
            return Err("named pipes are reachable on Windows alone".to_string())
        }
        DockerEndpoint::Tcp(url) => bollard::Docker::connect_with_http(url, seconds, version),
    }
    .map(|docker| docker.with_timeout(Duration::from_secs(seconds)))
    .map_err(|e| e.to_string())
}

impl DockerEngine for BollardEngine {
    #[cfg(not(test))]
    fn server_version(
        &self,
        endpoint: &DockerEndpoint,
        timeout: Duration,
    ) -> Result<String, EngineError> {
        bollard_server_version(endpoint, timeout)
    }

    /// Under `cfg(test)` the production engine reaches nothing: the suite must
    /// run identically on a machine with a Docker daemon and on one without, so
    /// a test that wants an engine binds a stub rather than the real one.
    #[cfg(test)]
    fn server_version(
        &self,
        _endpoint: &DockerEndpoint,
        _timeout: Duration,
    ) -> Result<String, EngineError> {
        Err(EngineError::Unreachable(
            "the production engine is not reachable under test".to_string(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Endpoint validation (GSS-FR-38)
// ---------------------------------------------------------------------------

/// GSS-FR-38: is the endpoint well formed *before* anything is dialled?
///
/// Structural only. An endpoint that passes here may still be unreachable, which
/// is the other refusal entirely — the two ask the author to correct different
/// things, so they are never merged.
pub fn validate_endpoint(endpoint: &DockerEndpoint) -> Result<(), String> {
    let value = endpoint.value().trim();
    if matches!(endpoint, DockerEndpoint::Automatic) {
        return Ok(());
    }
    if value.is_empty() {
        return Err(ERR_ENDPOINT_EMPTY.to_string());
    }
    match endpoint {
        // Answered above; the arm exists because the match is exhaustive.
        DockerEndpoint::Automatic => Ok(()),
        DockerEndpoint::UnixSocket(_) => {
            // A socket is addressed by an absolute path, with or without the
            // scheme Docker's own `DOCKER_HOST` spells it with.
            let bare = value.strip_prefix("unix://").unwrap_or(value);
            if bare.starts_with('/') {
                Ok(())
            } else {
                Err(ERR_ENDPOINT_INVALID.to_string())
            }
        }
        DockerEndpoint::WindowsPipe(_) => {
            let bare = value.strip_prefix("npipe://").unwrap_or(value);
            let looks_like_a_pipe = bare.starts_with("//./pipe/")
                || bare.starts_with(r"\\.\pipe\")
                || bare.starts_with("//?/pipe/");
            if looks_like_a_pipe {
                Ok(())
            } else {
                Err(ERR_ENDPOINT_INVALID.to_string())
            }
        }
        DockerEndpoint::Tcp(_) => {
            // Scheme, then a host that is not empty. `tcp://` is what
            // `DOCKER_HOST` uses; `http://` and `https://` are accepted because
            // the same daemon is addressed either way.
            let Some((scheme, rest)) = value.split_once("://") else {
                return Err(ERR_ENDPOINT_INVALID.to_string());
            };
            let known_scheme = matches!(scheme, "tcp" | "http" | "https");
            let host = rest.split('/').next().unwrap_or("");
            if known_scheme && !host.is_empty() && !host.starts_with(':') {
                Ok(())
            } else {
                Err(ERR_ENDPOINT_INVALID.to_string())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Verification (GSS-FR-38)
// ---------------------------------------------------------------------------

/// Does this program identify itself as the Docker CLI?
///
/// The banner is `Docker version 27.1.1, build …`, and every Docker-compatible
/// CLI that stands in for it says "docker" in the same place. Checking identity
/// rather than existence is what keeps pointing the field at `/bin/ls` from
/// "verifying" until the first real build (the reason AIC-FR-05 exists).
pub fn identifies_as_docker(output: &CliOutput) -> bool {
    output.combined().to_lowercase().contains("docker")
}

/// The daemon's version out of `docker version --format {{.Server.Version}}`.
///
/// The CLI prints exactly the server's version there and nothing else, so an
/// empty answer is a daemon that did not answer rather than one without a
/// version.
fn server_version_from_cli(output: &CliOutput) -> Option<String> {
    output
        .stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(64).collect())
}

/// The arguments the two CLI probes are run with. Named rather than spelled at
/// the call site so a test asserts the exact vector.
pub const CLI_IDENTITY_ARGS: &[&str] = &["--version"];
pub const CLI_DAEMON_ARGS: &[&str] = &["version", "--format", "{{.Server.Version}}"];

/// GSS-FR-38: verify the mode the configuration selects, and report **that
/// mode's** own states.
///
/// Returns the Docker server version on success. Nothing is persisted here; the
/// caller records the success against the selection that earned it
/// (GSS-FR-39).
pub fn verify_backend(
    config: &DockerBackendConfig,
    probe: &dyn FileProbe,
    runner: &dyn CliRunner,
    engine: &dyn DockerEngine,
    timeout: Duration,
) -> Result<String, String> {
    match config.mode {
        DockerBackendMode::DockerCli => verify_cli(config, probe, runner, timeout),
        DockerBackendMode::Bollard => {
            validate_endpoint(&config.endpoint)?;
            match engine.server_version(&config.endpoint, timeout) {
                Ok(version) => Ok(version),
                Err(EngineError::TimedOut) => Err(ERR_TIMED_OUT.to_string()),
                // GSS-FR-38: the endpoint was well formed, so whatever went
                // wrong beyond it is the daemon not answering. The underlying
                // text is deliberately not carried into the refusal — a
                // connection error can echo the address it dialled, and the
                // author is told which correction to make by the code alone.
                Err(EngineError::Unreachable(_)) => Err(ERR_DAEMON_UNREACHABLE.to_string()),
            }
        }
    }
}

fn verify_cli(
    config: &DockerBackendConfig,
    probe: &dyn FileProbe,
    runner: &dyn CliRunner,
    timeout: Duration,
) -> Result<String, String> {
    let path = config.cli_path_str();
    if path.is_empty() {
        return Err(ERR_CLI_PATH_EMPTY.to_string());
    }
    let path = Path::new(path);
    // Asked of the filesystem before anything is run, so a wrong path and a
    // permissions problem stay distinguishable (AIC-FR-05's distinction, for
    // the same reason: they call for different corrections).
    if !probe.exists(path) {
        return Err(ERR_CLI_NOT_FOUND.to_string());
    }
    if !probe.is_executable(path) {
        return Err(ERR_CLI_NOT_EXECUTABLE.to_string());
    }

    let identity = runner
        .run(path, CLI_IDENTITY_ARGS, timeout)
        .map_err(run_error_to_refusal)?;
    if !identifies_as_docker(&identity) {
        return Err(ERR_NOT_THE_DOCKER_CLI.to_string());
    }

    // The daemon, separately. A CLI that answers `--version` proves only that
    // the client is installed; GSS-FR-38 refuses to call that a success.
    let daemon = runner
        .run(path, CLI_DAEMON_ARGS, timeout)
        .map_err(run_error_to_refusal)?;
    if !daemon.success {
        return Err(ERR_DAEMON_UNREACHABLE.to_string());
    }
    server_version_from_cli(&daemon).ok_or_else(|| ERR_DAEMON_UNREACHABLE.to_string())
}

fn run_error_to_refusal(error: RunError) -> String {
    match error {
        RunError::NotFound => ERR_CLI_NOT_FOUND.to_string(),
        RunError::NotExecutable => ERR_CLI_NOT_EXECUTABLE.to_string(),
        RunError::TimedOut => ERR_TIMED_OUT.to_string(),
        // A child that could not be spawned or read is not a daemon that
        // refused; it is the executable failing to run at all.
        RunError::Failed(_) => ERR_CLI_NOT_EXECUTABLE.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Production seams, shared
// ---------------------------------------------------------------------------

/// The seams a command uses in production, held in Tauri state so a test can
/// swap any of them without touching a call site — the same arrangement
/// `crate::agentic::AgenticIntegrations` uses.
pub struct DockerBackendSeams {
    pub probe: Arc<dyn FileProbe>,
    pub runner: Arc<dyn CliRunner>,
    pub engine: Arc<dyn DockerEngine>,
}

impl Default for DockerBackendSeams {
    fn default() -> Self {
        Self {
            probe: Arc::new(crate::agentic::RealFileProbe),
            runner: Arc::new(crate::agentic::RealCliRunner),
            engine: Arc::new(BollardEngine),
        }
    }
}

// ---------------------------------------------------------------------------
// Tauri commands (GSS contract surface)
// ---------------------------------------------------------------------------

use crate::global_settings::GlobalSettingsStore;
use crate::logging::{self, Domain, LogBuffer, LogSink, BUFFER};
use crate::log_fields;
use tauri::State;

const DOMAINS: [Domain; 1] = [Domain::Backend];

/// GSS-FR-35: the stored record. Reads the store, runs no executable, and
/// reaches no daemon, so the Docker section renders offline and instantly.
#[tauri::command]
pub fn load_docker_backend(
    store: State<'_, GlobalSettingsStore>,
) -> Result<DockerBackend, String> {
    Ok(store.load_docker_backend()?.outbound())
}

/// GSS-FR-39: persist the selection, and return the record with the
/// verification state those values carry.
#[tauri::command]
pub fn save_docker_backend(
    config: DockerBackendConfig,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
) -> Result<DockerBackend, String> {
    save_docker_backend_with(&app, &BUFFER, &store, config)
}

pub fn save_docker_backend_with<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    store: &GlobalSettingsStore,
    config: DockerBackendConfig,
) -> Result<DockerBackend, String> {
    let record = store.save_docker_backend(&config)?;
    let outbound = record.outbound();
    // The endpoint and the path are configuration the author typed, not
    // credentials: this backend holds none, and no registry login is ever
    // performed (PSS-FR-26). The *kind* is logged rather than the value all the
    // same, because a TCP endpoint can carry a host somebody would rather not
    // read back out of an exported log.
    logging::log_info(
        sink,
        buffer,
        &DOMAINS,
        "docker backend selection saved",
        log_fields! {
            "mode" => format!("{:?}", outbound.mode),
            "endpoint_kind" => format!("{:?}", record.endpoint_kind),
            "cli_path_set" => record.cli_path.is_some(),
            "state" => format!("{:?}", outbound.state)
        },
    );
    Ok(outbound)
}

/// GSS-FR-37: the first Docker CLI on `PATH` or in a conventional location.
/// Persists nothing and executes nothing.
#[tauri::command]
pub fn detect_docker_cli_binary(
    seams: State<'_, DockerBackendSeams>,
) -> Result<DetectedDockerCli, String> {
    let path_var = std::env::var("PATH").ok();
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    Ok(DetectedDockerCli {
        path: detect_docker_cli(path_var.as_deref(), home.as_deref(), seams.probe.as_ref()),
    })
}

/// GSS-FR-38: verify the selected mode, and commit the success only where the
/// daemon answered.
#[tauri::command]
pub fn verify_docker_backend(
    config: DockerBackendConfig,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    seams: State<'_, DockerBackendSeams>,
) -> Result<DockerBackend, String> {
    verify_docker_backend_with(&app, &BUFFER, &store, &seams, config)
}

pub fn verify_docker_backend_with<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    store: &GlobalSettingsStore,
    seams: &DockerBackendSeams,
    config: DockerBackendConfig,
) -> Result<DockerBackend, String> {
    logging::log_info(
        sink,
        buffer,
        &DOMAINS,
        "docker backend verification started",
        log_fields! {
            "mode" => format!("{:?}", config.mode),
            "endpoint_kind" => format!("{:?}", config.endpoint.kind())
        },
    );
    match verify_backend(
        &config,
        seams.probe.as_ref(),
        seams.runner.as_ref(),
        seams.engine.as_ref(),
        VERIFY_TIMEOUT,
    ) {
        Ok(server_version) => {
            let at = crate::notes::now_rfc3339();
            let record = store.record_docker_verification(&config, &server_version, &at)?;
            logging::log_info(
                sink,
                buffer,
                &DOMAINS,
                "docker backend verified",
                log_fields! {
                    "mode" => format!("{:?}", config.mode),
                    "server_version" => server_version.as_str()
                },
            );
            Ok(record.outbound())
        }
        Err(refusal) => {
            // GSS-FR-38: a failed verification persists nothing, so the record
            // is exactly what it was — including a success earned earlier by a
            // selection that is still the one stored.
            logging::log_warn(
                sink,
                buffer,
                &DOMAINS,
                "docker backend verification refused",
                log_fields! {
                    "mode" => format!("{:?}", config.mode),
                    "reason" => refusal.as_str()
                },
            );
            Err(refusal)
        }
    }
}

/// GSS-FR-40: the verified backend an actual Docker operation is performed
/// through, or the refusal. Not a Tauri command, so no frontend call reaches
/// it, and it probes nothing.
pub fn resolve_docker_backend(
    store: &GlobalSettingsStore,
) -> Result<ResolvedDockerBackend, String> {
    resolve_from_record(&store.load_docker_backend()?)
}

#[cfg(test)]
mod tests;
