//! A launch of a dev container project (EAC-FR-GOAR, DVC-FR-LKRY ..
//! DVC-FR-ODWK), against a recording dev container runtime and a Docker CLI
//! that is a shell script.

use super::*;
use crate::tools::agent_exec::devcontainer_pool::{
    DevcontainerPool, DevcontainerRuntime, DevcontainerStart, RunningDevcontainer,
};
use crate::tools::agent_exec::sandbox::SandboxScope;
use crate::tools::agent_exec::{DevcontainerFailure, DevcontainerLaunch};

#[derive(Default)]
struct Recording {
    calls: StdMutex<Vec<String>>,
    starts: StdMutex<Vec<DevcontainerStart>>,
    fail_up: StdMutex<Option<String>>,
    /// DVC-FR-TQHM: a process of the turn is left after the end of the turn.
    turn_left: std::sync::atomic::AtomicBool,
}

impl DevcontainerRuntime for Recording {
    fn up<'a>(
        &'a self,
        start: &'a DevcontainerStart,
        _cancel: &'a CancellationToken,
    ) -> runtime::BoxFuture<'a, Result<RunningDevcontainer, String>> {
        self.calls.lock().unwrap().push(format!("up {}", start.scope_key));
        self.starts.lock().unwrap().push(start.clone());
        let failure = self.fail_up.lock().unwrap().clone();
        Box::pin(async move {
            match failure {
                Some(message) => Err(message),
                None => Ok(RunningDevcontainer {
                    container_id: "cid".to_string(),
                    remote_user: "vscode".to_string(),
                    remote_workspace_folder: "/w".to_string(),
                    probed_env: vec![
                        ("PATH".to_string(), "/feature/bin:/usr/bin".to_string()),
                        ("CLAUDE_CODE_OAUTH_TOKEN".to_string(), "leaked".to_string()),
                    ],
                }),
            }
        })
    }
    fn end_turn<'a>(
        &'a self,
        _: &'a str,
        container: &'a str,
        _turn: &'a str,
    ) -> runtime::BoxFuture<'a, bool> {
        self.calls.lock().unwrap().push(format!("end_turn {container}"));
        let ended = !self.turn_left.load(std::sync::atomic::Ordering::SeqCst);
        Box::pin(async move { ended })
    }
    fn remove<'a>(
        &'a self,
        _: &'a str,
        scope_key: &'a str,
        _: bool,
    ) -> runtime::BoxFuture<'a, Result<(), String>> {
        self.calls.lock().unwrap().push(format!("remove {scope_key}"));
        Box::pin(async { Ok(()) })
    }
}

/// A Docker CLI that records its arguments, whether the credential reached it,
/// the mode of the env file and its text, then answers as the vendor CLI.
fn fake_docker(dir: &Path, stdout: &str, exit_code: i32) -> String {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(dir.join("stdout"), stdout).unwrap();
    let d = dir.display();
    let body = format!(
        r#"#!/bin/sh
printf '%s\n' "$@" > {d}/args
sid=""; prev=""
for a in "$@"; do
  if [ "$prev" = "--session-id" ]; then sid="$a"; fi
  if [ "$prev" = "--env-file" ]; then cp "$a" {d}/envfile; (stat -c %a "$a" 2>/dev/null || stat -f %Lp "$a") > {d}/mode; fi
  prev="$a"
done
if [ -n "$CLAUDE_CODE_OAUTH_TOKEN" ]; then echo set > {d}/token; fi
cat > {d}/stdin
if [ -f {d}/sleep ]; then exec sleep 30; fi
sed "s/{{{{SESSION_ID}}}}/$sid/g" {d}/stdout
exit {exit_code}
"#
    );
    let path = dir.join("docker");
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_string_lossy().to_string()
}

fn launch(docker: String) -> DevcontainerLaunch {
    DevcontainerLaunch {
        config: PathBuf::from("/project/.devcontainer/devcontainer.json"),
        config_text: "// team\n{ \"image\": \"debian\" }".to_string(),
        config_hash: "hash".to_string(),
        cli: "/bin/devcontainer".to_string(),
        docker,
    }
}

struct Run {
    result: Result<AgentExecution, AgentExecutionError>,
    calls: Vec<String>,
    starts: Vec<DevcontainerStart>,
    docker_runtime: Arc<RecordingRuntime>,
}

/// The parts of a launch a test varies.
#[derive(Default)]
struct Options {
    timeout_ms: Option<u64>,
    isolation: Option<crate::isolation::ResolvedIsolation>,
    no_vendor_image: bool,
    recording: Option<Arc<Recording>>,
    pool: Option<Arc<DevcontainerPool>>,
}

/// EAC-FR-38: a project that configures no vendor image.
struct NoImages;
impl VendorImageSource for NoImages {
    fn image_for(&self, _vendor: &str) -> Result<String, String> {
        Err("vendor_image_unconfigured".to_string())
    }
}

fn run_dev(
    harness: &Harness,
    devcontainer: Option<Result<DevcontainerLaunch, String>>,
    scope: Option<SandboxScope>,
    fail_up: Option<&str>,
    cancellation: CancellationToken,
) -> Run {
    run_dev_with(harness, devcontainer, scope, fail_up, cancellation, Options::default())
}

fn run_dev_with(
    harness: &Harness,
    devcontainer: Option<Result<DevcontainerLaunch, String>>,
    scope: Option<SandboxScope>,
    fail_up: Option<&str>,
    cancellation: CancellationToken,
    options: Options,
) -> Run {
    let recording = options.recording.clone().unwrap_or_default();
    *recording.fail_up.lock().unwrap() = fail_up.map(str::to_string);
    let docker_runtime = RecordingRuntime::broken(false, false, None);
    let pool =
        options.pool.clone().unwrap_or_else(|| Arc::new(DevcontainerPool::new(recording.clone())));
    let mut executor = AgentCliExecutor::new(docker_runtime.clone())
        .with_session_state_root(harness.sessions_root())
        .with_devcontainer_pool(pool);
    if let Some(isolation) = options.isolation {
        executor = executor.with_isolation(isolation);
    }
    let images: &dyn VendorImageSource =
        if options.no_vendor_image { &NoImages } else { &TEST_IMAGES };
    let context = LaunchContext { devcontainer, images, ..harness.context() };
    let mut task = task("go");
    if let Some(timeout_ms) = options.timeout_ms {
        task.execution.timeout_ms = timeout_ms;
    }
    let result = block_on(executor.execute_agent_cli(
        &Silent,
        &context,
        AgentExecutionRequest {
            sandbox_scope: scope,
            turn_kind: super::super::TurnKind::Work,
            execution_directory: harness.workspace(),
            task,
            cancellation,
            activity: None,
            durable_output: None,
            supplementary_mount: None,
        },
    ));
    let calls = recording.calls.lock().unwrap().clone();
    let starts = recording.starts.lock().unwrap().clone();
    Run { result, calls, starts, docker_runtime }
}

fn scope() -> Option<SandboxScope> {
    Some(SandboxScope { queue_key: "stream-1".to_string() })
}

fn lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path).unwrap().lines().map(str::to_string).collect()
}

// EAC-FR-GOAR / DVC-FR-LKRY / DVC-FR-DCQG: the turn runs as one exec in the
// scope's container, with no vendor image and no Docker runtime of its own; the
// credential passes by name only and the env file is the owner's alone.
#[cfg(unix)]
#[test]
fn a_turn_runs_as_one_exec_in_the_scope_container() {
    let harness = harness_for("claude_code");
    let dir = tempfile::tempdir().unwrap();
    let docker = fake_docker(dir.path(), &valid_claude_stdout(), 0);
    let run = run_dev(&harness, Some(Ok(launch(docker))), scope(), None, CancellationToken::new());
    let execution = run.result.expect("the turn runs");
    assert_eq!(execution.process_outcome, ProcessOutcome::Completed, "{:?}", execution.stderr);
    assert_eq!(run.docker_runtime.launched(), 0, "no Docker launch");
    assert!(run.calls.iter().any(|c| c.starts_with("up ")), "{:?}", run.calls);
    assert!(!run.calls.iter().any(|c| c.starts_with("end_turn")), "{:?}", run.calls);
    assert!(!run.calls.iter().skip(1).any(|c| c.starts_with("remove")), "the container stays");

    let args = lines(&dir.path().join("args"));
    assert_eq!(&args[..4], &["exec", "-i", "-u", "vscode"]);
    let worktree = harness.workspace_target();
    assert_eq!(args[args.iter().position(|a| a == "-w").unwrap() + 1], worktree);
    assert!(args.iter().any(|a| a.starts_with("SYNTHESIS_TURN=")));
    assert!(args.contains(&"CLAUDE_CODE_OAUTH_TOKEN".to_string()), "{args:?}");
    assert!(args.contains(&"cid".to_string()) && args.contains(&"claude".to_string()));
    assert_eq!(std::fs::read_to_string(dir.path().join("token")).unwrap().trim(), "set");
    assert_eq!(std::fs::read_to_string(dir.path().join("mode")).unwrap().trim(), "600");
    let envfile = std::fs::read_to_string(dir.path().join("envfile")).unwrap();
    assert!(envfile.contains("PATH=/feature/bin:/usr/bin"), "{envfile}");
    assert!(!envfile.contains("leaked"), "a credential name is never in the env file");
    assert!(envfile.contains("CLAUDE_CONFIG_DIR="), "{envfile}");
    let env_path = &args[args.iter().position(|a| a == "--env-file").unwrap() + 1];
    assert!(!Path::new(env_path).exists(), "the env file goes after the turn");
}

// DVC-FR-QGUT / DVC-FR-IFBK: the generated config puts the worktree at its host
// path, carries the session state mount, and sits under the data folder.
#[cfg(unix)]
#[test]
fn the_generated_config_carries_the_worktree_and_mounts() {
    let harness = harness_for("claude_code");
    let dir = tempfile::tempdir().unwrap();
    let docker = fake_docker(dir.path(), &valid_claude_stdout(), 0);
    let run = run_dev(&harness, Some(Ok(launch(docker))), scope(), None, CancellationToken::new());
    run.result.unwrap();
    let start = &run.starts[0];
    assert!(start.override_path.starts_with(harness.sessions_root()), "{:?}", start.override_path);
    let generated: serde_json::Value = serde_json::from_str(&start.override_text).unwrap();
    let worktree = harness.workspace_target();
    assert_eq!(generated["workspaceFolder"], worktree.as_str());
    assert_eq!(
        generated["workspaceMount"],
        format!("type=bind,source={worktree},target={worktree}")
    );
    assert_eq!(generated["image"], "debian");
    let mounts = generated["mounts"].to_string();
    assert!(mounts.contains(&harness.session_state_dir("claude_code")), "{mounts}");
    assert!(!mounts.contains("docker.sock"), "{mounts}");
    assert_eq!(start.config_hash, "hash");
}

// EAC-FR-GOAR: the project's Docker-in-Docker settings do not apply, also with
// a socket that does not exist.
#[cfg(unix)]
#[test]
fn docker_in_docker_does_not_apply() {
    let mut harness = harness_for("claude_code");
    harness.docker_in_docker = DockerInDockerLaunch {
        enabled: true,
        socket_path: "/nowhere/docker.sock".to_string(),
        host_gateway: true,
        under_microsandbox: Default::default(),
    };
    let dir = tempfile::tempdir().unwrap();
    let docker = fake_docker(dir.path(), &valid_claude_stdout(), 0);
    let run = run_dev(&harness, Some(Ok(launch(docker))), scope(), None, CancellationToken::new());
    run.result.expect("no socket refusal");
    assert!(!run.starts[0].override_text.contains("docker.sock"));
}

// DVC-FR-ODWK: exit 127 with no output is a missing CLI; with output it is the
// CLI's own exit.
#[cfg(unix)]
#[test]
fn a_missing_cli_is_a_launch_failure() {
    let harness = harness_for("claude_code");
    let dir = tempfile::tempdir().unwrap();
    let docker = fake_docker(dir.path(), "", 127);
    let run = run_dev(&harness, Some(Ok(launch(docker))), scope(), None, CancellationToken::new());
    assert!(matches!(
        run.result,
        Err(AgentExecutionError::DevcontainerLaunchFailed(DevcontainerFailure::AgentCliMissing(ref cli))) if cli == "claude"
    ));
    let other = tempfile::tempdir().unwrap();
    let docker = fake_docker(other.path(), "some output\n", 127);
    let run = run_dev(&harness, Some(Ok(launch(docker))), scope(), None, CancellationToken::new());
    assert!(!matches!(run.result, Err(AgentExecutionError::DevcontainerLaunchFailed(_))));
}

// DVC-FR-ATEO: a failed start is one line of at most 300 characters.
#[cfg(unix)]
#[test]
fn a_failed_start_is_one_short_line() {
    let harness = harness_for("claude_code");
    let long = format!("{}\nsecond line", "x".repeat(1000));
    let run = run_dev(
        &harness,
        Some(Ok(launch("/bin/false".into()))),
        scope(),
        Some(&long),
        CancellationToken::new(),
    );
    match run.result {
        Err(AgentExecutionError::DevcontainerLaunchFailed(DevcontainerFailure::StartFailed(
            line,
        ))) => {
            assert_eq!(line.chars().count(), 300);
            assert!(!line.contains('\n'));
        }
        other => panic!("{other:?}"),
    }
}

// DVC-FR-ATEO / DVC-FR-YZLH: a dev container that cannot be resolved fails
// before anything starts, with no fall back to the vendor images.
#[test]
fn an_unresolved_dev_container_starts_nothing() {
    let harness = harness_for("claude_code");
    let run = run_dev(
        &harness,
        Some(Err("the dev container CLI is not verified".into())),
        scope(),
        None,
        CancellationToken::new(),
    );
    assert!(matches!(
        run.result,
        Err(AgentExecutionError::DevcontainerLaunchFailed(DevcontainerFailure::StartFailed(_)))
    ));
    assert!(run.calls.is_empty());
    assert_eq!(run.docker_runtime.launched(), 0);
}

// DVC-FR-SFWW: a start the caller cancelled is a cancelled run, not a failure.
#[cfg(unix)]
#[test]
fn a_cancelled_start_is_a_cancelled_run() {
    let harness = harness_for("claude_code");
    let cancel = CancellationToken::new();
    cancel.cancel();
    let run =
        run_dev(&harness, Some(Ok(launch("/bin/false".into()))), scope(), Some("stopped"), cancel);
    let execution = run.result.expect("a cancelled run");
    assert_eq!(execution.process_outcome, ProcessOutcome::Cancelled);
}

// DVC-FR-OHMC: a launch with no scope runs in a container of its own that is
// removed after the turn, and its folder goes with it.
#[cfg(unix)]
#[test]
fn a_launch_with_no_scope_is_removed_after_its_turn() {
    let harness = harness_for("claude_code");
    let dir = tempfile::tempdir().unwrap();
    let docker = fake_docker(dir.path(), &valid_claude_stdout(), 0);
    let run = run_dev(&harness, Some(Ok(launch(docker))), None, None, CancellationToken::new());
    run.result.unwrap();
    let key = run.starts[0].scope_key.clone();
    assert!(key.contains("-once-"), "{key}");
    assert_eq!(run.calls.last(), Some(&format!("remove {key}")));
    assert!(!run.starts[0].override_path.exists());
}

// DVC-FR-SFWW: a turn has the stdin of a Docker launch, and a turn that ends by
// its time limit ends the CLI's processes in the container and keeps it.
#[test]
fn a_timed_out_turn_ends_its_processes_and_keeps_the_container() {
    let harness = harness_for("claude_code");
    let dir = tempfile::tempdir().unwrap();
    let docker = fake_docker(dir.path(), &valid_claude_stdout(), 0);
    std::fs::write(dir.path().join("sleep"), "").unwrap();
    let options =
        Options { timeout_ms: Some(super::protocol::MIN_TIMEOUT_MS), ..Default::default() };
    let run = run_dev_with(
        &harness,
        Some(Ok(launch(docker))),
        scope(),
        None,
        CancellationToken::new(),
        options,
    );
    let execution = run.result.expect("a timed out run");
    assert_eq!(execution.process_outcome, ProcessOutcome::Timeout);
    assert!(run.calls.contains(&"end_turn cid".to_string()), "{:?}", run.calls);
    // DVC-FR-TQHM: the end of the turn confirmed no process is left.
    assert!(execution.container_removed);
    assert_eq!(
        run.calls.iter().filter(|c| c.starts_with("remove")).count(),
        1,
        "only the first launch removes"
    );
    let stdin = std::fs::read_to_string(dir.path().join("stdin")).unwrap();
    assert!(stdin.contains("go"), "{stdin}");
}

// EAC-FR-GOAR / EAC-FR-SNMS / EAC-FR-38 / PSS-FR-VNPU: a dev container launch
// with a scope runs in no VM under Microsandbox, and reads no vendor image.
#[test]
fn microsandbox_and_vendor_images_do_not_apply() {
    let harness = harness_for("claude_code");
    let dir = tempfile::tempdir().unwrap();
    let docker = fake_docker(dir.path(), &valid_claude_stdout(), 0);
    let isolation =
        crate::isolation::ResolvedIsolation::Microsandbox(crate::isolation::SandboxSettings {
            cpus: 2,
            memory_gib: 4,
            disk_gib: 32,
            block_local_network: true,
            allowed: Vec::new(),
            unsupported_reason: None,
        });
    let options =
        Options { isolation: Some(isolation), no_vendor_image: true, ..Default::default() };
    let run = run_dev_with(
        &harness,
        Some(Ok(launch(docker))),
        scope(),
        None,
        CancellationToken::new(),
        options,
    );
    let execution = run.result.expect("the turn runs in the dev container");
    assert_eq!(execution.process_outcome, ProcessOutcome::Completed);
    assert!(run.calls.iter().any(|c| c.starts_with("up ")));
    assert_eq!(run.docker_runtime.launched(), 0);
}

// DVC-FR-LKRY: two launches of one scope start one container.
#[test]
fn two_turns_of_a_scope_start_one_container() {
    let harness = harness_for("claude_code");
    let dir = tempfile::tempdir().unwrap();
    let docker = fake_docker(dir.path(), &valid_claude_stdout(), 0);
    let recording = Arc::new(Recording::default());
    let pool = Arc::new(DevcontainerPool::new(recording.clone()));
    for _ in 0..2 {
        let options = Options {
            recording: Some(recording.clone()),
            pool: Some(pool.clone()),
            ..Default::default()
        };
        run_dev_with(
            &harness,
            Some(Ok(launch(docker.clone()))),
            scope(),
            None,
            CancellationToken::new(),
            options,
        )
        .result
        .expect("the turn runs");
    }
    let ups = recording.calls.lock().unwrap().iter().filter(|c| c.starts_with("up ")).count();
    assert_eq!(ups, 1);
}

// DVC-FR-TQHM, EAC-FR-26: a stopped turn whose processes the end of the turn
// cannot confirm ended reports its container as not removed.
#[test]
fn a_turn_whose_processes_are_left_reports_its_container_not_removed() {
    let harness = harness_for("claude_code");
    let dir = tempfile::tempdir().unwrap();
    let docker = fake_docker(dir.path(), &valid_claude_stdout(), 0);
    std::fs::write(dir.path().join("sleep"), "").unwrap();
    let recording = Arc::new(Recording::default());
    recording.turn_left.store(true, std::sync::atomic::Ordering::SeqCst);
    let options = Options {
        timeout_ms: Some(super::protocol::MIN_TIMEOUT_MS),
        recording: Some(recording),
        ..Default::default()
    };
    let run = run_dev_with(
        &harness,
        Some(Ok(launch(docker))),
        scope(),
        None,
        CancellationToken::new(),
        options,
    );
    let execution = run.result.expect("a timed out run");
    assert_eq!(execution.process_outcome, ProcessOutcome::Timeout);
    assert!(run.calls.contains(&"end_turn cid".to_string()), "{:?}", run.calls);
    assert!(!execution.container_removed, "the turn's processes may still run");
}
