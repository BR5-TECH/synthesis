//! End-to-end execution against `agentic-cli-mock`
//! (`../infra/ACM-agentic-cli-mock.md`).
//!
//! Everything in `tests.rs` runs against an in-process double, which is fast
//! and precise but proves nothing about the process boundary: a real argument
//! vector delivered by the operating system, real pipes, a real exit status,
//! real elapsed time, and a real child a caller can terminate mid-flight.
//!
//! These tests close that gap the way EAC's Testing section requires — through
//! the `DockerRuntime` seam, with the mock standing in for `docker` itself
//! (ACM-FR-18's `docker` adapter). **No production path changes**: integration
//! resolution, descriptor selection, credential handoff, and argument
//! generation are the shipped ones, and the only substitution is which program
//! the seam launches.
//!
//! ## What asserts what
//!
//! The mock validates the invocation from the far side of the process
//! boundary. A scenario that does not match makes the mock exit `65` before
//! emitting a byte of its configured response, so every test here that observes
//! a normal outcome has *already* proved the vector and the stdin bytes were
//! what the scenario demanded.
//!
//! Exactness is split deliberately. The stdin bytes are asserted exactly, and
//! so is every stable token of the vector — all flags, the mount, the
//! environment variable's name, and the whole vendor tail. Two elements are
//! matched position-independently instead, because they legitimately vary: the
//! container name is unique per request (EAC-FR-13) and the `uid:gid` is the
//! host user's (EAC-FR-14). The *complete ordered* vector is asserted against
//! the in-process double in `tests.rs`, where those two are knowable.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::Duration;

use serde_json::json;

// The parent test module: its harness, its doubles, and its fixtures. Building
// a second set here would let the two drift, and the point of this file is that
// the *same* production path is driven a different way.
use super::*;

mod claude_turns;
mod codex_turns;
mod boundary;

// ---------------------------------------------------------------------------
// Locating the mock
// ---------------------------------------------------------------------------

/// The mock is its own crate with its own `Cargo.lock` and its own `target/`
/// (ACM-FR-01), so `CARGO_BIN_EXE_` cannot reach it. It is located at the path
/// ACM-FR-02 documents and built if it is not there yet.
///
/// Built rather than skipped: a test that quietly does nothing when its double
/// is missing reports success for a run that verified nothing, and the `mock`
/// CI lane would never catch it because that lane does not run this suite.
fn mock_binary() -> &'static Path {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("the repository root")
            .join("tools/agentic-cli-mock");
        let manifest = crate_dir.join("Cargo.toml");
        let binary = crate_dir
            .join("target/debug")
            .join(format!("agentic-cli-mock{}", std::env::consts::EXE_SUFFIX));

        if !binary.exists() {
            let built = Command::new(env!("CARGO"))
                .args(["build", "--manifest-path"])
                .arg(&manifest)
                .status()
                .expect("cargo is on PATH");
            assert!(built.success(), "failed to build agentic-cli-mock");
        }
        assert!(
            binary.exists(),
            "agentic-cli-mock is not at the path ACM-FR-02 documents: {}",
            binary.display()
        );
        binary
    })
    .as_path()
}

/// A scenario file on disk, kept alive for the length of a test.
struct Scenario {
    _directory: tempfile::TempDir,
    path: PathBuf,
    tool: &'static str,
    /// What was written, kept so an assertion about the scenario's contents
    /// needs no read-back — the bytes are already here, and a round trip would
    /// only test the filesystem.
    document: String,
}

impl Scenario {
    fn new(tool: &'static str, document: serde_json::Value) -> Self {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("scenario.json");
        let text = serde_json::to_string_pretty(&document).expect("serialise");

        // Through the guarded handle rather than `std::fs`, like every other
        // write in the backend (FSA-FR-19). A scratch directory is exactly what
        // a root is for, and reaching around the guard in a test is how the
        // habit of reaching around it starts.
        FsAccess::builder()
            .allow_root(directory.path())
            .build()
            .expect("fs")
            .write_text_atomic(&path, &text)
            .expect("write scenario");

        Scenario {
            _directory: directory,
            path,
            tool,
            document: text,
        }
    }
}

// ---------------------------------------------------------------------------
// The runtime that launches it
// ---------------------------------------------------------------------------

/// A `DockerRuntime` whose `run` really launches the mock in place of `docker`.
///
/// The three control operations answer trivially: this exists to exercise the
/// launch path — the vector, the stdin, the two streams, the exit status, the
/// deadline, and cancellation — and a liveness probe or an image pull is not
/// part of that. Their own failures are covered against the in-process double.
struct MockDockerRuntime {
    scenario: PathBuf,
    /// The composite adapter for the stack being replaced — `docker+claude` or
    /// `docker+codex`. Not plain `docker`: the entrypoint means one process
    /// stands in for both layers, so the adapter that validates it should
    /// require both halves.
    tool: &'static str,
    removed: StdMutex<Vec<String>>,
    /// Every vector forwarded across the process boundary, so a test can assert
    /// the exact tail rather than only the tokens the scenario demanded.
    ///
    /// The mock's own `required_args` check is presence-only: it catches a token
    /// production stopped emitting, but not one it started emitting, and not a
    /// reordering. Recording the vector here is what lets the two end-to-end
    /// tests close that gap without changing the mock's schema.
    argvs: StdMutex<Vec<Vec<String>>>,
}

impl MockDockerRuntime {
    fn new(scenario: &Scenario) -> Arc<Self> {
        Arc::new(MockDockerRuntime {
            scenario: scenario.path.clone(),
            tool: scenario.tool,
            removed: StdMutex::new(Vec::new()),
            argvs: StdMutex::new(Vec::new()),
        })
    }

    /// The vendor tail of the only recorded launch — everything after the image.
    fn only_vendor_tail(&self, image: &str) -> Vec<String> {
        let argvs = self.argvs.lock().unwrap();
        assert_eq!(argvs.len(), 1, "expected exactly one launch");
        let at = argvs[0]
            .iter()
            .position(|a| a == image)
            .expect("the image is in the vector");
        argvs[0][at + 1..].to_vec()
    }

    /// The scenario to drive this launch with, resolving the session placeholder
    /// against the vector production generated.
    fn resolve_scenario(&self, argv: &[String]) -> PathBuf {
        // Through the guarded handle, on the same grounds `Scenario::new` states:
        // reaching around it in a test is how the habit of reaching around it
        // starts.
        let directory = self.scenario.parent().expect("the scenario has a directory");
        let fs = FsAccess::builder()
            .allow_root(directory)
            .build()
            .expect("fs");

        let raw = fs
            .read_text(&self.scenario)
            .expect("the scenario is readable");
        if !raw.contains(SESSION_PLACEHOLDER) {
            return self.scenario.clone();
        }
        let assigned = assigned_session_id(argv)
            .expect("a scenario using the session placeholder needs an assigned session");
        let resolved = self.scenario.with_extension("resolved.json");
        fs.write_text_atomic(&resolved, &raw.replace(SESSION_PLACEHOLDER, &assigned))
            .expect("the resolved scenario is writable");
        resolved
    }
}

impl DockerRuntime for MockDockerRuntime {
    fn ensure_available(&self) -> BoxFuture<'_, Result<(), RuntimeError>> {
        Box::pin(async { Ok(()) })
    }

    fn ensure_image<'a>(&'a self, _image: &'a str) -> BoxFuture<'a, Result<(), RuntimeError>> {
        Box::pin(async { Ok(()) })
    }

    fn run<'a>(
        &'a self,
        request: RunRequest<'a>,
    ) -> BoxFuture<'a, Result<RuntimeOutcome, RuntimeError>> {
        Box::pin(async move {
            // The mock's own options go before the generated vector; the vector
            // itself crosses the process boundary untouched, which is what
            // makes the mock's verdict a statement about production output.
            self.argvs.lock().unwrap().push(request.argv.to_vec());

            // Stand in for the CLI reporting back the session it was told to
            // run in. The scenario is written before the launch, while the
            // identity production assigns is generated per launch, so a fixed
            // value could never match — resolving it here is what keeps the
            // assertion of CCP-FR-16 live across a real process boundary
            // instead of quietly skipped.
            let scenario = self.resolve_scenario(request.argv);

            let driver = HostDockerCli::with_program(mock_binary().to_string_lossy().into_owned())
                .with_arg_prefix(vec![
                    "--tool".into(),
                    self.tool.into(),
                    "--scenario".into(),
                    scenario.to_string_lossy().into_owned(),
                    "--".into(),
                ]);
            driver.run(request).await
        })
    }

    fn remove_container<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<(), RuntimeError>> {
        Box::pin(async move {
            self.removed.lock().unwrap().push(name.to_string());
            Ok(())
        })
    }
}

// ---------------------------------------------------------------------------
// Scenario construction
// ---------------------------------------------------------------------------

/// What production must emit, written out as literals.
///
/// Deliberately **not** derived from `descriptor::CLAUDE_CODE` and friends. An
/// expectation computed from the same constant the code under test reads is a
/// tautology: change the descriptor and both sides move together, so the
/// assertion holds while the behaviour changes underneath it. Spelling the
/// tokens here makes the scenario an independent statement of the contract —
/// which is the only thing that makes a golden expectation worth having, and
/// what lets an unintended change to a flag fail a test rather than propagate
/// silently into the vector a real container would receive.
///
/// The three elements not listed are the ones that legitimately vary per launch
/// and per machine: the container name (EAC-FR-13), the host `uid:gid`
/// (EAC-FR-14), and the working directory, which is the execution directory's
/// own path wherever a container can be given it (EAC-FR-ZKMR). `tests.rs` asserts the complete ordered vector including those,
/// against the in-process double where both are knowable.
fn required_args(vendor: &str, extra_vendor_args: &[&str]) -> Vec<serde_json::Value> {
    // The container layer, identical for both vendors.
    let mut values: Vec<&str> = vec![
        "run",
        "--rm",
        "--interactive",
        "--network",
        "bridge",
        "--workdir",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges",
        "--mount",
    ];

    // The vendor layer, which the entrypoint receives verbatim. Written out as
    // literals rather than read from the descriptors, so a changed flag fails
    // the run instead of moving both sides together (EAC-FR-09, EAC-FR-10, EAC-FR-11, EAC-FR-12, EAC-FR-13).
    match vendor {
        "claude_code" => values.extend([
            "--env",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "--env",
            "CLAUDE_CONFIG_DIR=/home/agent/.claude",
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--json-schema",
            "--permission-mode",
            "bypassPermissions",
            "--session-id",
        ]),
        "codex" => values.extend([
            "exec",
            "--json",
            "--sandbox",
            "danger-full-access",
            "--skip-git-repo-check",
            "-",
        ]),
        other => panic!("no vendor {other}"),
    }
    values.extend(extra_vendor_args.iter().copied());

    values
        .into_iter()
        .map(|value| json!({ "value": value }))
        .collect()
}

/// The composite adapter that replaces the whole stack for `vendor`.
fn stack_tool(vendor: &str) -> &'static str {
    match vendor {
        "claude_code" => "docker+claude",
        "codex" => "docker+codex",
        other => panic!("no stack for {other}"),
    }
}

fn scenario_for(
    vendor: &str,
    stdin: &str,
    stdout: &str,
    stderr: &str,
    exit_code: i32,
    delay_ms: u64,
    extra_vendor_args: &[&str],
) -> Scenario {
    Scenario::new(
        stack_tool(vendor),
        json!({
        "version": 1,
        "expected": {
            "required_args": required_args(vendor, extra_vendor_args),
            // Exact, byte for byte: this is the whole task envelope, and the
            // mock compares it rather than merely receiving it (ACM-FR-12).
            "stdin": stdin,
        },
        "response": {
            "stdout": stdout,
            "stderr": stderr,
            "exit_code": exit_code,
            "delay_ms": delay_ms,
        }
        }),
    )
}

/// Claude Code's result document as the mock replays it.
///
/// The session it reports is the placeholder `MockDockerRuntime::resolve_scenario`
/// rewrites with whatever `--session-id` production actually generated, so the
/// assertion CCP-FR-16 makes between the assigned and the reported identity is
/// live here rather than skipped for want of a value the scenario could not
/// know in advance.
fn mock_claude_stdout(envelope: &str) -> String {
    json!({
        "type": "result",
        "subtype": "success",
        "is_error": false,
        "result": envelope,
        "session_id": SESSION_PLACEHOLDER
    })
    .to_string()
}

/// The exact bytes the executor will send for `task`, so the scenario can
/// demand them.
fn stdin_for(task: &AgentTaskRequest) -> String {
    String::from_utf8(task.to_stdin_bytes().expect("within limits")).expect("utf8")
}

fn drive(
    harness: &Harness,
    scenario: &Scenario,
    task: AgentTaskRequest,
    cancel: CancellationToken,
) -> Result<AgentExecution, AgentExecutionError> {
    drive_with(harness, MockDockerRuntime::new(scenario), task, cancel)
}

/// The same, against a runtime the caller already holds so it can read back what
/// crossed the process boundary.
fn drive_with(
    harness: &Harness,
    runtime: Arc<MockDockerRuntime>,
    task: AgentTaskRequest,
    cancel: CancellationToken,
) -> Result<AgentExecution, AgentExecutionError> {
    let executor = AgentCliExecutor::new(runtime)
        .with_session_state_root(harness.sessions_root());
    block_on(executor.execute_agent_cli(
        &Silent,
        &harness.context(),
        AgentExecutionRequest {
            turn_kind: super::TurnKind::Work,
            execution_directory: harness.workspace(),
            task,
            cancellation: cancel,
            activity: None,
            supplementary_mount: None,
        },
    ))
}
