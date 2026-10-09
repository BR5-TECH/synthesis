//! The Docker runtime seam (EAC-FR-17).
//!
//! Every container operation goes through [`DockerRuntime`] rather than through
//! a process spawn at a call site. Production binds it to the host Docker CLI;
//! a test binds it to a double that asserts the exact argument vector and the
//! exact stdin bytes and replays a configured stdout, stderr, and exit status.
//!
//! That seam is what lets `../infra/ACM-agentic-cli-mock.md` stand in for the
//! runtime — its `docker` adapter validates a whole `docker run` vector, which
//! carries the vendor's own vector as its tail — **without any production path
//! changing** (AVI-FR-12). Integration resolution, descriptor selection, and
//! argument generation are identical under test and in a shipped build; the
//! only substitution is which program the seam launches.
//!
//! ## Why availability, image, and launch are three failures
//!
//! A machine with no Docker, a machine without the pinned image, and a machine
//! that refused this particular container each call for a different correction
//! by whoever is looking at the loop that failed. Collapsing them into one
//! "docker failed" would make the log record useless at exactly the moment it
//! is read.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncWriteExt;

use super::descriptor::{docker_search_paths, DOCKER_PROGRAM};
use crate::agentic::SecretString;

/// How often the cancellation flag is looked at while a container runs.
const CANCEL_POLL: Duration = Duration::from_millis(50);
/// How long the runtime is given to answer a liveness or image query.
const CONTROL_TIMEOUT: Duration = Duration::from_secs(30);
/// How long one candidate program is given to say that it is there.
///
/// Shorter than a control query on purpose: this asks the client alone whether
/// it exists, which needs no daemon, and several candidates may be tried in a
/// row before the seam gives up.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

mod cancel;
mod capture;

pub use cancel::CancellationToken;
use capture::read_bounded;
#[cfg(not(test))]
use capture::StreamCapture;

/// Which of the child's two streams a line arrived on (EAC-FR-32).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamChannel {
    Stdout,
    Stderr,
}

impl StreamChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            StreamChannel::Stdout => "stdout",
            StreamChannel::Stderr => "stderr",
        }
    }
}

/// Someone watching a run while it happens (EAC-FR-32).
///
/// The runtime hands over each complete line the moment it has one, rather than
/// at exit. Both pinned CLIs write a line per event as they work, so a turn that
/// takes a quarter of an hour is otherwise a quarter of an hour of silence
/// followed by one document — and a turn that never finishes is silence and
/// nothing else.
///
/// Deliberately a line rather than a chunk: an observer's whole job is reading
/// one event at a time, and a chunk boundary falls wherever the pipe filled.
///
/// The runtime calls this on the thread pumping the pipe, so an implementation
/// that blocks is an implementation that stops draining the child.
pub trait StreamObserver: Send + Sync {
    /// One line, without its terminator. Not necessarily valid UTF-8, and not
    /// necessarily whole: a line that reached [`LIMIT_OBSERVED_LINE`] is handed
    /// over at that bound with `whole` false, so a CLI that writes an
    /// unterminated megabyte cannot grow the buffer holding it.
    fn line(&self, channel: StreamChannel, bytes: &[u8], whole: bool);
}

/// The longest line an observer is handed in one piece.
///
/// A bound on the *reassembly* buffer rather than on the capture, which
/// `stdout_limit` and `stderr_limit` already bound. Well above any event either
/// pinned CLI emits — a Claude Code assistant event carrying a large tool result
/// is tens of kilobytes — and far below a size worth holding for a stream that
/// turns out to have no line ending at all.
pub const LIMIT_OBSERVED_LINE: usize = 256 * 1024;

/// One stream, captured up to its bound (EAC-FR-23).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapturedStream {
    pub bytes: Vec<u8>,
    /// Whether the bound was reached, so what is held is a prefix. A truncated
    /// stdout is never parsed for an envelope.
    pub truncated: bool,
}

impl CapturedStream {
    pub fn as_utf8(&self) -> Option<&str> {
        std::str::from_utf8(&self.bytes).ok()
    }
}

/// How a run ended, from the runtime's point of view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunEnd {
    /// The child exited on its own.
    Exited,
    /// The deadline elapsed and the child was killed.
    TimedOut,
    /// The caller cancelled and the child was killed.
    Cancelled,
    /// The child died without an exit code — a signal from outside.
    Terminated,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeOutcome {
    pub end: RunEnd,
    pub exit_code: Option<i32>,
    pub stdout: CapturedStream,
    pub stderr: CapturedStream,
}

/// EAC-FR-17: three distinct ways the runtime itself can fail a launch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeError {
    /// No Docker-compatible runtime, or one that will not answer.
    Unavailable,
    /// The pinned image is absent and could not be pulled.
    ImageUnavailable,
    /// The runtime refused to create or start this container.
    LaunchFailed(String),
}

/// One container launch, fully described.
pub struct RunRequest<'a> {
    pub argv: &'a [String],
    /// EAC-FR-39: the same launch, described structurally rather than as the
    /// Docker CLI's rendering of it, so a backend that does not take an
    /// argument vector creates the *same* container.
    ///
    /// `None` where there is no container at all: the seam is also what a test
    /// drives a plain child process through, to exercise the waiter, the
    /// reader, the timeout, and the cancellation without a container runtime on
    /// the machine. Production always carries one.
    pub container: Option<&'a super::descriptor::ContainerSpec>,
    /// Variables set on the *docker client's* environment, whose values
    /// `--env NAME` then forwards into the container. The secret never reaches
    /// `argv` (EAC-FR-15).
    pub env: &'a BTreeMap<String, SecretString>,
    pub stdin: &'a [u8],
    pub timeout: Duration,
    pub cancel: CancellationToken,
    pub stdout_limit: usize,
    pub stderr_limit: usize,
    /// EAC-FR-32: who watches the run as it happens. `None` runs exactly as
    /// before — the capture is unchanged either way, so what an observer sees is
    /// an addition to the outcome rather than a substitute for it.
    pub observer: Option<&'a dyn StreamObserver>,
}

pub trait DockerRuntime: Send + Sync {
    /// Is there a usable runtime on this machine at all?
    fn ensure_available(&self) -> BoxFuture<'_, Result<(), RuntimeError>>;

    /// EAC-FR-37: the program this seam settled on, once an operation has
    /// resolved one, for the record that says what was tried.
    ///
    /// A seam that runs no host program has none to name.
    fn resolved_program(&self) -> Option<String> {
        None
    }

    /// Is the pinned image present, and if not can it be pulled?
    fn ensure_image<'a>(&'a self, image: &'a str) -> BoxFuture<'a, Result<(), RuntimeError>>;

    fn run<'a>(&'a self, request: RunRequest<'a>)
        -> BoxFuture<'a, Result<RuntimeOutcome, RuntimeError>>;

    /// EAC-FR-26: confirm the container is gone, whatever path the run took
    /// out. `--rm` covers a clean exit; a killed client does not necessarily
    /// take the container with it, so removal is asked for explicitly and
    /// waited on.
    fn remove_container<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<(), RuntimeError>>;
}

// ---------------------------------------------------------------------------
// The host Docker CLI
// ---------------------------------------------------------------------------

pub struct HostDockerCli {
    program: String,
    /// EAC-FR-37: where the seam may look when `PATH` does not hold the
    /// program, in order.
    ///
    /// Production searches the well-known locations. A caller that names its
    /// own program searches nowhere, because a stand-in that could not be
    /// started must fail as itself rather than quietly become the real runtime.
    search: Vec<String>,
    /// The program the search settled on, resolved once and kept.
    resolved: tokio::sync::OnceCell<String>,
    /// Arguments placed before the generated vector.
    ///
    /// Empty in production. It exists because `../infra/ACM-agentic-cli-mock.md`
    /// takes its own options *before* the vector it is validating
    /// (`--tool docker --scenario <path> --`), so standing in for the runtime
    /// needs somewhere to put them. The generated vector itself is untouched,
    /// which is the whole point: the mock then asserts the bytes production
    /// actually emits rather than a rewrite of them.
    arg_prefix: Vec<String>,
}

impl Default for HostDockerCli {
    fn default() -> Self {
        HostDockerCli {
            program: DOCKER_PROGRAM.to_string(),
            search: docker_search_paths(),
            resolved: tokio::sync::OnceCell::new(),
            arg_prefix: Vec::new(),
        }
    }
}

impl HostDockerCli {
    /// The same driver pointed at a different program.
    ///
    /// Production always names `docker`. A test uses this to drive a plain
    /// child process through the identical waiter, reader, and killer, which is
    /// how the timeout, the cancellation, and the bounded capture are exercised
    /// for real without a container runtime on the machine.
    pub fn with_program(program: impl Into<String>) -> Self {
        HostDockerCli {
            program: program.into(),
            search: Vec::new(),
            resolved: tokio::sync::OnceCell::new(),
            arg_prefix: Vec::new(),
        }
    }

    /// The same driver looking in `search` when `program` is not on `PATH`.
    ///
    /// What production does, with the locations stated rather than compiled in,
    /// so a test can prove the search finds a runtime `PATH` does not hold
    /// without one being installed where the test is running.
    pub fn with_program_searching(
        program: impl Into<String>,
        search: Vec<String>,
    ) -> Self {
        HostDockerCli {
            program: program.into(),
            search,
            resolved: tokio::sync::OnceCell::new(),
            arg_prefix: Vec::new(),
        }
    }

    /// The same driver placing `prefix` before every generated vector.
    pub fn with_arg_prefix(mut self, prefix: Vec<String>) -> Self {
        self.arg_prefix = prefix;
        self
    }

    /// EAC-FR-37: the program every operation of this seam runs.
    ///
    /// `PATH` first, because a machine that puts the runtime there is answered
    /// without looking anywhere else. Then each well-known location in turn,
    /// asked whether it is there rather than looked up on disk — starting a
    /// candidate is what proves it can be started, and a path that exists but
    /// cannot be run is passed over for the same reason an absent one is.
    ///
    /// Resolved once. A machine with no runtime at all settles on the plain
    /// name, so it fails exactly as it did before the search existed.
    async fn program(&self) -> &str {
        self.resolved
            .get_or_init(|| async {
                if self.search.is_empty() || Self::starts(&self.program).await {
                    return self.program.clone();
                }
                for candidate in &self.search {
                    if Self::starts(candidate).await {
                        return candidate.clone();
                    }
                }
                self.program.clone()
            })
            .await
    }

    /// Whether a program is there to be run at all.
    ///
    /// Asks the client for its own version, which needs no daemon: a runtime
    /// that is installed and stopped still answers, and is then reported as
    /// unavailable by [`DockerRuntime::ensure_available`] rather than as
    /// missing — two different corrections for the author.
    async fn starts(program: &str) -> bool {
        let run = tokio::process::Command::new(program)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status();
        matches!(tokio::time::timeout(RESOLVE_TIMEOUT, run).await, Ok(Ok(_)))
    }

    /// A short control command — a version query, an image inspect, a removal —
    /// bounded so an unresponsive daemon cannot wedge a launch.
    async fn control(&self, args: &[&str]) -> Result<std::process::Output, RuntimeError> {
        let run = tokio::process::Command::new(self.program().await)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output();

        match tokio::time::timeout(CONTROL_TIMEOUT, run).await {
            Ok(Ok(output)) => Ok(output),
            // The program is not on PATH, or could not be executed at all.
            Ok(Err(_)) => Err(RuntimeError::Unavailable),
            Err(_) => Err(RuntimeError::Unavailable),
        }
    }
}

impl DockerRuntime for HostDockerCli {
    fn resolved_program(&self) -> Option<String> {
        self.resolved.get().cloned()
    }

    fn ensure_available(&self) -> BoxFuture<'_, Result<(), RuntimeError>> {
        Box::pin(async move {
            let output = self.control(&["version", "--format", "{{.Server.Version}}"]).await?;
            if output.status.success() {
                Ok(())
            } else {
                // The CLI exists but the daemon does not answer, which is the
                // same correction as having no runtime: start or install one.
                Err(RuntimeError::Unavailable)
            }
        })
    }

    fn ensure_image<'a>(&'a self, image: &'a str) -> BoxFuture<'a, Result<(), RuntimeError>> {
        Box::pin(async move {
            if self
                .control(&["image", "inspect", image])
                .await?
                .status
                .success()
            {
                return Ok(());
            }
            // Absent locally. One pull, and if that fails the image is not
            // reachable — which is a different problem from having no runtime.
            if self.control(&["pull", "--quiet", image]).await?.status.success() {
                Ok(())
            } else {
                Err(RuntimeError::ImageUnavailable)
            }
        })
    }

    fn run<'a>(
        &'a self,
        request: RunRequest<'a>,
    ) -> BoxFuture<'a, Result<RuntimeOutcome, RuntimeError>> {
        Box::pin(async move {
            let mut command = tokio::process::Command::new(self.program().await);
            command
                .args(&self.arg_prefix)
                .args(request.argv)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                // EAC-FR-26: if this future is dropped, the child does not
                // outlive it.
                .kill_on_drop(true);

            // EAC-FR-24 requires terminating the whole process tree, and
            // `Child::kill` reaches the direct child alone. Making the child a
            // process-group leader gives us one handle that covers whatever it
            // forked — without it, a vendor CLI that spawns a helper leaves the
            // helper running after a timeout, holding the worktree mount open.
            #[cfg(unix)]
            command.process_group(0);

            // The secret is set here, on the client process's environment, and
            // reaches the container because the argv named the *variable*.
            for (name, value) in request.env {
                command.env(name, value.expose());
            }

            let mut child = command
                .spawn()
                .map_err(|e| RuntimeError::LaunchFailed(e.kind().to_string()))?;

            let mut stdin = child.stdin.take();
            let mut stdout = child.stdout.take();
            let mut stderr = child.stderr.take();

            let input = request.stdin.to_vec();
            let stdout_limit = request.stdout_limit;
            let stderr_limit = request.stderr_limit;
            let observer = request.observer;

            // The three pipes are pumped concurrently with the wait. A child
            // that fills one pipe while nobody reads the other is the classic
            // way this deadlocks, and it is why capture cannot wait for exit.
            let pump = async move {
                let write = async {
                    if let Some(mut handle) = stdin.take() {
                        // A broken pipe here is not an error: the agent is
                        // entitled to stop reading once it has what it needs.
                        let _ = handle.write_all(&input).await;
                        let _ = handle.shutdown().await;
                    }
                };
                let read_out =
                    read_bounded(&mut stdout, stdout_limit, StreamChannel::Stdout, observer);
                let read_err =
                    read_bounded(&mut stderr, stderr_limit, StreamChannel::Stderr, observer);
                let (_, out, err) = tokio::join!(write, read_out, read_err);
                (out, err)
            };

            let cancel = request.cancel.clone();
            let timeout = request.timeout;
            let waiter = async {
                let deadline = tokio::time::sleep(timeout);
                tokio::pin!(deadline);
                // A caller that cancelled before the child was even spawned
                // should not have the turn run for a poll interval first.
                if cancel.is_cancelled() {
                    kill_tree(&mut child).await;
                    return (RunEnd::Cancelled, None);
                }
                loop {
                    tokio::select! {
                        finished = child.wait() => {
                            return match finished {
                                Ok(status) => match status.code() {
                                    Some(code) => (RunEnd::Exited, Some(code)),
                                    // No code means a signal ended it, and
                                    // nothing here asked for that.
                                    None => (RunEnd::Terminated, None),
                                },
                                Err(_) => (RunEnd::Terminated, None),
                            };
                        }
                        _ = &mut deadline => {
                            kill_tree(&mut child).await;
                            return (RunEnd::TimedOut, None);
                        }
                        _ = tokio::time::sleep(CANCEL_POLL) => {
                            if cancel.is_cancelled() {
                                kill_tree(&mut child).await;
                                return (RunEnd::Cancelled, None);
                            }
                        }
                    }
                }
            };

            let ((stdout, stderr), (end, exit_code)) = tokio::join!(pump, waiter);

            Ok(RuntimeOutcome {
                end,
                exit_code,
                stdout,
                stderr,
            })
        })
    }

    fn remove_container<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<(), RuntimeError>> {
        Box::pin(async move {
            // `--force` because the container may still be running, and
            // removing one that is already gone is a success rather than a
            // failure to report — `--rm` may well have got there first.
            let _ = self.control(&["rm", "--force", name]).await?;
            Ok(())
        })
    }
}

/// Kill the child and, on Unix, everything it forked.
///
/// The child was made a process-group leader at spawn, so one `killpg` reaches
/// the group. The direct `kill` still runs: it is what reaps the child and what
/// covers the platforms with no process group to signal.
async fn kill_tree(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // SAFETY: `killpg` touches no memory the caller owns. A failure means
        // the group is already gone, which is the outcome being asked for.
        unsafe {
            libc_killpg(pid as i32, LIBC_SIGKILL);
        }
    }
    let _ = child.kill().await;
}

#[cfg(unix)]
const LIBC_SIGKILL: i32 = 9;

#[cfg(unix)]
extern "C" {
    #[link_name = "killpg"]
    fn libc_killpg(pgrp: i32, sig: i32) -> i32;
}

// ---------------------------------------------------------------------------
// The Docker Engine, over Bollard
// ---------------------------------------------------------------------------

/// EAC-FR-15: the Engine backend's environment for one container, in the
/// Engine API's `NAME=value` form. Each name the spec passes takes its value
/// from `env`, in the spec's order, and the executor's own literals follow.
pub(crate) fn engine_env(
    spec: &super::descriptor::ContainerSpec,
    env: &BTreeMap<String, SecretString>,
) -> Vec<String> {
    let mut entries: Vec<String> = Vec::new();
    for name in &spec.env_names {
        if let Some(value) = env.get(name) {
            entries.push(format!("{name}={}", value.expose()));
        }
    }
    for (name, value) in &spec.env_literals {
        entries.push(format!("{name}={value}"));
    }
    entries
}

/// EAC-FR-10 / EAC-FR-ZKMR: the Engine backend's rendering of one
/// [`ContainerSpec`], composed apart from the connection that sends it.
///
/// Pure, and separate for the reason the CLI backend's argv is: the two
/// backends must create the **same** container, and a rendering that can only
/// be read by starting one is a rendering nothing compares.
pub(crate) fn engine_config(
    spec: &super::descriptor::ContainerSpec,
    env: Vec<String>,
) -> bollard::models::ContainerCreateBody {
    let mounts: Vec<bollard::models::Mount> = spec
        .mounts
        .iter()
        .map(|mount| bollard::models::Mount {
            typ: Some(bollard::models::MountType::BIND),
            source: Some(mount.source.clone()),
            target: Some(mount.target.clone()),
            read_only: Some(mount.read_only),
            ..Default::default()
        })
        .collect();

    let host_config = bollard::models::HostConfig {
        mounts: Some(mounts),
        network_mode: Some(super::descriptor::NETWORK.to_string()),
        cap_drop: Some(spec.dropped_capabilities()),
        security_opt: Some(spec.security_options()),
        // EAC-FR-26: removed when the agent exits, exactly as `--rm`.
        auto_remove: Some(true),
        ..Default::default()
    };

    bollard::models::ContainerCreateBody {
        image: Some(spec.image.clone()),
        cmd: Some(spec.vendor_args.clone()),
        env: Some(env),
        user: Some(spec.user()),
        // EAC-FR-ZKMR: the same directory the CLI backend names, so which
        // backend a machine uses changes nothing about where the turn stands.
        working_dir: Some(spec.workdir.clone()),
        // EAC-FR-09: the task reaches the agent on stdin and nowhere else, so
        // stdin is opened and attached. Never a TTY: a terminal would let the
        // CLI print progress decoration into the very stream EAC-FR-19 parses.
        open_stdin: Some(true),
        stdin_once: Some(true),
        attach_stdin: Some(true),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        tty: Some(false),
        host_config: Some(host_config),
        ..Default::default()
    }
}

/// EAC-FR-39: the runtime seam bound to the **Docker Engine** at the endpoint
/// the author configured and verified (GSS-FR-36, GSS-FR-40).
///
/// It creates and starts the *same* [`ContainerSpec`](super::descriptor::ContainerSpec)
/// the Docker CLI backend renders into a `docker run` vector — the same mounts,
/// the same working directory, the same user mapping, the same capabilities and
/// security options, the same network, and the same stdin and stream handling —
/// so which backend a machine uses changes how Docker is reached and nothing
/// about what the agent runs in.
pub struct BollardDocker {
    /// Read by the connection this backend opens, which is compiled out under
    /// `cfg(test)` — the suite must run identically with and without a Docker
    /// daemon, so a test binds a recording double through the same seam.
    #[cfg_attr(test, allow(dead_code))]
    endpoint: crate::docker::DockerEndpoint,
}

impl BollardDocker {
    pub fn new(endpoint: crate::docker::DockerEndpoint) -> Self {
        BollardDocker { endpoint }
    }

    /// The connection every call on this backend is made through.
    ///
    /// It is given a bound wider than [`CONTROL_TIMEOUT`] on purpose. A Docker
    /// Engine request is answered with a header only when the daemon has
    /// something to say — a pull says nothing until the registry has answered,
    /// and a wait says nothing until the container has stopped — so a
    /// connection bound equal to the control deadline would end those calls
    /// itself. The deadline each call names is what bounds it, and this is the
    /// backstop under a connection that nothing else bounds.
    #[cfg(not(test))]
    fn connect(&self) -> Result<bollard::Docker, RuntimeError> {
        crate::docker::connect(&self.endpoint, crate::docker::connect_bound(CONTROL_TIMEOUT))
            .map_err(|_| RuntimeError::Unavailable)
    }
}

#[cfg(not(test))]
impl DockerRuntime for BollardDocker {
    /// EAC-FR-17: a backend that will not answer is `RuntimeUnavailable`. The
    /// endpoint is not re-verified here — this is the daemon failing to answer
    /// a launch, which is a different thing from a selection that never proved
    /// itself (GSS-FR-40).
    fn ensure_available(&self) -> BoxFuture<'_, Result<(), RuntimeError>> {
        Box::pin(async move {
            let docker = self.connect()?;
            match tokio::time::timeout(CONTROL_TIMEOUT, docker.version()).await {
                Ok(Ok(_)) => Ok(()),
                _ => Err(RuntimeError::Unavailable),
            }
        })
    }

    /// The Engine backend runs no host program, so it has none to name.
    fn resolved_program(&self) -> Option<String> {
        None
    }

    /// EAC-FR-38: inspect before pulling, so an image the author built locally
    /// runs without a registry ever being reached; a pull that cannot find or
    /// fetch the reference is `ImageUnavailable`.
    fn ensure_image<'a>(&'a self, image: &'a str) -> BoxFuture<'a, Result<(), RuntimeError>> {
        Box::pin(async move {
            use futures_util::StreamExt;

            let docker = self.connect()?;
            if docker.inspect_image(image).await.is_ok() {
                return Ok(());
            }
            let options = bollard::query_parameters::CreateImageOptionsBuilder::default()
                .from_image(image)
                .build();
            // No credentials: this application authenticates to no registry, so
            // a pull that needs one fails as an unavailable image rather than
            // by presenting something.
            let mut stream = docker.create_image(Some(options), None, None);
            while let Some(item) = stream.next().await {
                if item.is_err() {
                    return Err(RuntimeError::ImageUnavailable);
                }
            }
            // The pull reported no error; confirm the image is actually there
            // rather than trusting a stream that ended quietly.
            docker
                .inspect_image(image)
                .await
                .map(|_| ())
                .map_err(|_| RuntimeError::ImageUnavailable)
        })
    }

    fn run<'a>(
        &'a self,
        request: RunRequest<'a>,
    ) -> BoxFuture<'a, Result<RuntimeOutcome, RuntimeError>> {
        Box::pin(async move {
            use futures_util::StreamExt;
            use tokio::io::AsyncWriteExt;

            let Some(spec) = request.container else {
                // Production always carries one; a caller that does not has
                // asked this backend to start something it cannot describe.
                return Err(RuntimeError::LaunchFailed(
                    "the Docker Engine backend needs a container specification".to_string(),
                ));
            };
            let docker = self.connect()?;

            // The environment, in the Engine API's `NAME=value` form. The
            // variables the CLI forwards *by name* take their values from this
            // process's own environment here, so a secret still never reaches
            // an argument vector (EAC-FR-15) — there is no argument vector at
            // all on this path.
            let config = engine_config(spec, engine_env(spec, request.env));

            let create = bollard::query_parameters::CreateContainerOptionsBuilder::default()
                .name(&spec.name)
                .build();
            docker
                .create_container(Some(create), config)
                .await
                .map_err(|e| RuntimeError::LaunchFailed(e.to_string()))?;

            let attach = bollard::query_parameters::AttachContainerOptionsBuilder::default()
                .stream(true)
                .stdin(true)
                .stdout(true)
                .stderr(true)
                .build();
            let mut attached = docker
                .attach_container(&spec.name, Some(attach))
                .await
                .map_err(|e| RuntimeError::LaunchFailed(e.to_string()))?;

            docker
                .start_container(
                    &spec.name,
                    None::<bollard::query_parameters::StartContainerOptions>,
                )
                .await
                .map_err(|e| RuntimeError::LaunchFailed(e.to_string()))?;

            // The whole task, then end-of-input: both pinned CLIs read their
            // prompt to EOF.
            let _ = attached.input.write_all(request.stdin).await;
            let _ = attached.input.flush().await;
            let _ = attached.input.shutdown().await;

            let mut stdout = StreamCapture::new(request.stdout_limit, StreamChannel::Stdout);
            let mut stderr = StreamCapture::new(request.stderr_limit, StreamChannel::Stderr);
            let deadline = tokio::time::Instant::now() + request.timeout;
            let mut end = RunEnd::Exited;

            loop {
                if request.cancel.is_cancelled() {
                    end = RunEnd::Cancelled;
                    break;
                }
                let next = tokio::time::timeout_at(
                    deadline.min(tokio::time::Instant::now() + POLL_SLICE),
                    attached.output.next(),
                )
                .await;
                match next {
                    Err(_) if tokio::time::Instant::now() >= deadline => {
                        end = RunEnd::TimedOut;
                        break;
                    }
                    // The slice elapsed without a line; go round and ask the
                    // cancellation token again.
                    Err(_) => continue,
                    Ok(None) => break,
                    Ok(Some(Err(_))) => {
                        end = RunEnd::Terminated;
                        break;
                    }
                    Ok(Some(Ok(output))) => match output {
                        bollard::container::LogOutput::StdOut { message }
                        | bollard::container::LogOutput::Console { message } => {
                            stdout.push(&message, request.observer);
                        }
                        bollard::container::LogOutput::StdErr { message } => {
                            stderr.push(&message, request.observer);
                        }
                        bollard::container::LogOutput::StdIn { .. } => {}
                    },
                }
            }

            if matches!(end, RunEnd::TimedOut | RunEnd::Cancelled) {
                // EAC-FR-24 / EAC-FR-25: the container and its whole process
                // tree are terminated, and removal is confirmed by the caller's
                // own `remove_container` on the way out.
                let _ = docker
                    .kill_container(
                        &spec.name,
                        None::<bollard::query_parameters::KillContainerOptions>,
                    )
                    .await;
                return Ok(RuntimeOutcome {
                    end,
                    exit_code: None,
                    stdout: stdout.finish(request.observer),
                    stderr: stderr.finish(request.observer),
                });
            }

            let exit_code = {
                let mut wait = docker.wait_container(
                    &spec.name,
                    None::<bollard::query_parameters::WaitContainerOptions>,
                );
                match tokio::time::timeout_at(deadline, wait.next()).await {
                    Ok(Some(Ok(response))) => Some(response.status_code as i32),
                    // A container removed by `auto_remove` before the wait
                    // arrived has already exited; the streams say what it did.
                    Ok(Some(Err(_))) | Ok(None) => None,
                    Err(_) => {
                        end = RunEnd::TimedOut;
                        None
                    }
                }
            };
            if exit_code.is_none() && end == RunEnd::Exited {
                end = RunEnd::Terminated;
            }

            Ok(RuntimeOutcome {
                end,
                exit_code,
                stdout: stdout.finish(request.observer),
                stderr: stderr.finish(request.observer),
            })
        })
    }

    fn remove_container<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<(), RuntimeError>> {
        Box::pin(async move {
            let docker = self.connect()?;
            let options = bollard::query_parameters::RemoveContainerOptionsBuilder::default()
                .force(true)
                .build();
            match docker.remove_container(name, Some(options)).await {
                Ok(()) => Ok(()),
                // Already gone — `auto_remove` took it — which is the outcome
                // this asks for.
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 404, ..
                }) => Ok(()),
                Err(e) => Err(RuntimeError::LaunchFailed(e.to_string())),
            }
        })
    }
}

/// Under `cfg(test)` this backend reaches nothing: the suite must run
/// identically on a machine with a Docker daemon and on one without, so a test
/// binds a recording double through the same seam instead.
#[cfg(test)]
impl DockerRuntime for BollardDocker {
    fn ensure_available(&self) -> BoxFuture<'_, Result<(), RuntimeError>> {
        Box::pin(async move { Err(RuntimeError::Unavailable) })
    }

    fn ensure_image<'a>(&'a self, _image: &'a str) -> BoxFuture<'a, Result<(), RuntimeError>> {
        Box::pin(async move { Err(RuntimeError::Unavailable) })
    }

    fn run<'a>(
        &'a self,
        _request: RunRequest<'a>,
    ) -> BoxFuture<'a, Result<RuntimeOutcome, RuntimeError>> {
        Box::pin(async move { Err(RuntimeError::Unavailable) })
    }

    fn remove_container<'a>(&'a self, _name: &'a str) -> BoxFuture<'a, Result<(), RuntimeError>> {
        Box::pin(async move { Err(RuntimeError::Unavailable) })
    }
}

/// How long the Engine loop waits for a line before asking the cancellation
/// token again.
#[allow(dead_code)]
const POLL_SLICE: Duration = Duration::from_millis(100);

