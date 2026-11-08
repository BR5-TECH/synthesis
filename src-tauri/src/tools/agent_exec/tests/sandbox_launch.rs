//! EAC-FR-SNMS, EAC-FR-QYBJ, EAC-FR-BJLI, EAC-FR-TMQS, EAC-FR-ZIYG,
//! EAC-FR-PZNB, EAC-FR-DARS, EAC-FR-NCSA — a graduation launch under
//! Microsandbox runs as one process in its scope's VM, with the mounts a Docker
//! launch has and the repository as a generated store, and every other launch
//! stays a Docker launch.

use super::sandbox::{
    BootError, BootImage, SandboxFailureCause, SandboxPool, SandboxProcess, SandboxRuntime,
    SandboxScope, SandboxSpec,
};
use super::*;
use crate::isolation::{AllowedHost, ResolvedIsolation, SandboxSettings};

/// One process the double was asked to run.
#[derive(Clone, Debug)]
struct RecordedProcess {
    vm: String,
    vendor_args: Vec<String>,
    workdir: String,
    user: String,
    env: BTreeMap<String, String>,
    stdin: Vec<u8>,
    /// EAC-FR-HZIE: the final-event rule the launch handed the process.
    final_event: Option<super::super::final_event::FinalEventRule>,
}

/// A sandbox runtime that records each boot and each process, and replies with
/// a scripted stdout.
struct RecordingSandbox {
    stdout: String,
    boot_error: Option<BootError>,
    boots: StdMutex<Vec<(String, SandboxSpec, BootImage)>>,
    processes: StdMutex<Vec<RecordedProcess>>,
    /// EAC-FR-WTFH: every hosts text written.
    pins: StdMutex<Vec<String>>,
    /// EAC-FR-QMRW: the group of every daemon start.
    daemons: StdMutex<Vec<u32>>,
    /// EAC-FR-ZPTA: the answer of every daemon start; `None` succeeds.
    daemon_error: Option<String>,
}

impl RecordingSandbox {
    fn replying(stdout: &str) -> Arc<RecordingSandbox> {
        Arc::new(RecordingSandbox {
            stdout: stdout.to_string(),
            boot_error: None,
            boots: StdMutex::new(Vec::new()),
            processes: StdMutex::new(Vec::new()),
            pins: StdMutex::new(Vec::new()),
            daemons: StdMutex::new(Vec::new()),
            daemon_error: None,
        })
    }

    fn failing(error: BootError) -> Arc<RecordingSandbox> {
        Arc::new(RecordingSandbox {
            stdout: String::new(),
            boot_error: Some(error),
            boots: StdMutex::new(Vec::new()),
            processes: StdMutex::new(Vec::new()),
            pins: StdMutex::new(Vec::new()),
            daemons: StdMutex::new(Vec::new()),
            daemon_error: None,
        })
    }

    fn only_boot(&self) -> SandboxSpec {
        let boots = self.boots.lock().unwrap();
        assert_eq!(boots.len(), 1, "expected exactly one boot");
        boots[0].1.clone()
    }

    fn only_process(&self) -> RecordedProcess {
        let processes = self.processes.lock().unwrap();
        assert_eq!(processes.len(), 1, "expected exactly one process");
        processes[0].clone()
    }
}

impl SandboxRuntime for RecordingSandbox {
    fn boot<'a>(
        &'a self,
        vm: &'a str,
        spec: &'a SandboxSpec,
        image: &'a BootImage,
    ) -> BoxFuture<'a, Result<(), BootError>> {
        Box::pin(async move {
            self.boots.lock().unwrap().push((vm.to_string(), spec.clone(), image.clone()));
            match &self.boot_error {
                Some(error) => Err(error.clone()),
                None => Ok(()),
            }
        })
    }

    fn load_export<'a>(
        &'a self,
        _archive: &'a Path,
        _reference: &'a str,
    ) -> BoxFuture<'a, Result<(), BootError>> {
        Box::pin(async move {
            Err(BootError::Failed(
                crate::tools::agent_exec::sandbox::SandboxFailureCause::ImageLoadFailed,
                "no export under test".to_string(),
            ))
        })
    }

    fn reopen_catalog(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move { Err("no catalog under test".to_string()) })
    }

    fn run<'a>(
        &'a self,
        vm: &'a str,
        process: SandboxProcess<'a>,
    ) -> BoxFuture<'a, Result<RuntimeOutcome, RuntimeError>> {
        Box::pin(async move {
            self.processes.lock().unwrap().push(RecordedProcess {
                vm: vm.to_string(),
                vendor_args: process.vendor_args.to_vec(),
                workdir: process.workdir.to_string(),
                user: process.user.clone(),
                env: process
                    .env
                    .iter()
                    .map(|(name, value)| (name.clone(), value.expose().to_string()))
                    .collect(),
                stdin: process.stdin.to_vec(),
                final_event: process.final_event,
            });
            Ok(RuntimeOutcome {
                end: RunEnd::Exited,
                exit_code: Some(0),
                stdout: CapturedStream {
                    bytes: substitute_session(&self.stdout, process.vendor_args).into_bytes(),
                    truncated: false,
                },
                stderr: CapturedStream::default(),
            })
        })
    }

    fn refresh<'a>(&'a self, _vm: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move { Ok(()) })
    }

    fn stop<'a>(&'a self, _vm: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {})
    }

    fn pin_hosts<'a>(&'a self, _vm: &'a str, text: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            self.pins.lock().unwrap().push(text.to_string());
            Ok(())
        })
    }

    fn start_docker_daemon<'a>(
        &'a self,
        _vm: &'a str,
        group: u32,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            self.daemons.lock().unwrap().push(group);
            match &self.daemon_error {
                Some(error) => Err(error.clone()),
                None => Ok(()),
            }
        })
    }
}

fn microsandbox() -> ResolvedIsolation {
    ResolvedIsolation::Microsandbox(SandboxSettings {
        cpus: 3,
        memory_gib: 6,
        disk_gib: 48,
        block_local_network: true,
        allowed: Vec::new(),
        unsupported_reason: None,
    })
}

fn scope() -> Option<SandboxScope> {
    Some(SandboxScope { queue_key: "stream-1".to_string() })
}

/// One launch through an executor bound to both doubles.
fn launch(
    harness: &Harness,
    docker: Arc<RecordingRuntime>,
    sandbox: Arc<RecordingSandbox>,
    isolation: ResolvedIsolation,
    sandbox_scope: Option<SandboxScope>,
    turn_kind: TurnKind,
) -> Result<AgentExecution, AgentExecutionError> {
    let executor = AgentCliExecutor::new(docker)
        .with_session_state_root(harness.sessions_root())
        .with_isolation(isolation)
        .with_sandbox_pool(Arc::new(SandboxPool::new(sandbox)));
    block_on(executor.execute_agent_cli(
        &Silent,
        &harness.context(),
        AgentExecutionRequest {
            sandbox_scope,
            turn_kind,
            execution_directory: harness.workspace(),
            task: task("go"),
            cancellation: CancellationToken::new(),
            activity: None,
            durable_output: None,
            supplementary_mount: None,
        },
    ))
}

/// EAC-FR-ZIYG, EAC-FR-HZIE: a sandbox launch of Claude Code hands its process
/// the final event of the descriptor and the grace of 30 seconds.
#[test]
fn a_sandbox_launch_hands_its_process_the_final_event_rule() {
    let harness = harness_for("claude_code");
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    launch(
        &harness,
        RecordingRuntime::replying(""),
        sandbox.clone(),
        microsandbox(),
        scope(),
        TurnKind::Work,
    )
    .expect("runs");
    let rule = sandbox.only_process().final_event.expect("the process has a final-event rule");
    assert_eq!(rule.grace, Duration::from_secs(30));
    assert!((rule.is_final)(br#"{"type":"result"}"#));
    assert!(!(rule.is_final)(br#"{"type":"assistant"}"#));
}

/// EAC-FR-SNMS, EAC-FR-ZIYG, EAC-FR-PZNB: under Microsandbox a graduation turn
/// runs as one process in its scope's VM, reaches no Docker runtime, and
/// completes on the same terms a container does.
#[test]
fn a_graduation_turn_runs_as_one_process_in_the_vm() {
    let harness = harness_for("claude_code");
    let docker = RecordingRuntime::replying("");
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    let execution =
        launch(&harness, docker.clone(), sandbox.clone(), microsandbox(), scope(), TurnKind::Work)
            .expect("runs");

    assert_eq!(execution.process_outcome, ProcessOutcome::Completed);
    assert!(execution.container_removed);
    assert_eq!(docker.launched(), 0, "no container is created");
    assert!(docker.images_asked.lock().unwrap().is_empty());

    let process = sandbox.only_process();
    assert_eq!(process.vm, sandbox::vm_name(PROJECT_KEY, &scope().unwrap()));
    let (uid, gid) = descriptor::host_ids();
    assert_eq!(process.user, format!("{uid}:{gid}"));
    let workspace = crate::changes::canonicalize_lenient(&harness.workspace());
    assert_eq!(process.workdir, workspace.to_string_lossy());
    // EAC-FR-09: the task reaches the agent on stdin and in no argument.
    let (sent, _) = split_task_document(&process.stdin);
    assert_eq!(sent.instruction, "go");
    assert!(!process.vendor_args.iter().any(|arg| arg.contains("\"instruction\"")));

    let spec = sandbox.only_boot();
    assert_eq!(spec.image, test_image_reference("claude_code"));
    // EAC-FR-KTWO: the host user owns what the mounts show.
    assert_eq!(spec.owner, format!("{uid}:{gid}"));
    assert_eq!(spec.cpus, 3);
    assert_eq!(spec.memory_gib, 6);
    // EAC-FR-QDVN: the disk size of the isolation reaches the VM.
    assert_eq!(spec.disk_gib, 48);
    assert!(spec.network.block_local_network);
}

/// EAC-FR-BJLI, EAC-FR-15: the VM mounts the execution directory read/write and
/// the session-state directory, and nothing else; the launch environment is
/// set on the process.
#[test]
fn the_vm_mounts_the_execution_directory_and_session_state_only() {
    // A directory that belongs to no repository gets no repository access, and
    // launches all the same (EAC-FR-FNFV).
    let harness = harness_for("claude_code");
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    launch(
        &harness,
        RecordingRuntime::replying(""),
        sandbox.clone(),
        microsandbox(),
        scope(),
        TurnKind::Work,
    )
    .expect("runs");
    let spec = sandbox.only_boot();
    assert_eq!(spec.mounts.len(), 2, "{:?}", spec.mounts);
    let workspace = crate::changes::canonicalize_lenient(&harness.workspace());
    let workspace = workspace.to_string_lossy();
    assert!(spec.mounts.iter().any(|m| m.target == workspace && !m.read_only));
    assert!(spec.mounts.iter().all(|m| !m.read_only));
    assert!(spec.mounts.iter().all(|m| !m.target.contains("docker.sock")));
    let process = sandbox.only_process();
    assert!(!process.env.is_empty(), "the launch environment reaches the process");
}

// EAC-FR-BJLI, EAC-FR-TMQS, EAC-FR-WBDE, EAC-FR-FNFV: a turn in the VM reaches
// the repository a turn in a container reaches. The store stands at its own
// path as a generated directory with an empty `config`, each presented entry is
// mounted read-only inside it, the `.git` pin is read-only, and the Git
// environment is the container's.
#[test]
fn the_vm_reaches_the_repository_through_a_generated_store() {
    let harness = harness_for("claude_code");
    linked_worktree_at(&harness.sessions_root(), &harness.workspace());
    let store = crate::changes::canonicalize_lenient(&harness.sessions_root().join("primary/.git"));
    let store = store.to_string_lossy().into_owned();
    let workspace = harness.workspace_target();

    let docker = RecordingRuntime::replying(&valid_claude_stdout());
    run(&harness, docker.clone(), task("go")).expect("runs");
    let container = docker.only_run().container.expect("the specification");
    assert!(container.mounts.iter().any(|m| m.target == store && m.read_only));

    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    launch(
        &harness,
        RecordingRuntime::replying(""),
        sandbox.clone(),
        microsandbox(),
        scope(),
        TurnKind::Review,
    )
    .expect("runs");
    let spec = sandbox.only_boot();
    let at = |target: &str| spec.mounts.iter().find(|m| m.target == target);

    let generated = at(&store).expect("the store stands at its own path");
    assert!(!generated.read_only, "the generated directory is the one writable part");
    assert_ne!(generated.source, store, "the store itself is never mounted whole");
    let config = Path::new(&generated.source).join("config");
    assert_eq!(std::fs::read(&config).expect("an empty config").len(), 0);

    // Exactly the presented entries the store holds, each read-only from the
    // store itself, and nothing else under the store's path.
    let existing: Vec<String> = super::super::sandbox_store::PRESENTED_STORE_ENTRIES
        .iter()
        .filter(|entry| Path::new(&store).join(entry).exists())
        .map(|entry| format!("{store}/{entry}"))
        .collect();
    assert!(existing.iter().any(|t| t.ends_with("/objects")), "{existing:?}");
    let mut under: Vec<String> = spec
        .mounts
        .iter()
        .filter(|m| m.target.starts_with(&format!("{store}/")))
        .map(|m| m.target.clone())
        .collect();
    under.sort();
    let mut expected = existing.clone();
    expected.sort();
    assert_eq!(under, expected);
    for target in &existing {
        let mount = at(target).unwrap();
        assert!(mount.read_only, "{target}");
        assert_eq!(&mount.source, target, "{target}");
    }
    assert!(at(&format!("{store}/config")).is_none(), "no mount over config");
    assert!(at(&format!("{store}/hooks")).is_none(), "no hooks");
    let pin = at(&format!("{workspace}/.git")).expect("the .git pin");
    assert!(pin.read_only);

    let env = sandbox.only_process().env;
    for (name, value) in container.env_literals.iter().filter(|(name, _)| name.starts_with("GIT_"))
    {
        assert_eq!(env.get(name), Some(value), "{name}");
    }
    assert_eq!(env.get("GIT_OPTIONAL_LOCKS").map(String::as_str), Some("0"));
}

// EAC-FR-WQNH, EAC-FR-PWLA: the process in the VM gets both Bash timeouts of a
// Claude Code launch, as a container does.
#[test]
fn a_sandbox_process_gets_the_bash_timeouts() {
    let harness = harness_for("claude_code");
    let docker = RecordingRuntime::replying("");
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    launch(&harness, docker, sandbox.clone(), microsandbox(), scope(), TurnKind::Work)
        .expect("runs");

    let env = sandbox.only_process().env;
    for name in descriptor::BASH_TIMEOUT_ENVS {
        assert_eq!(env.get(name).map(String::as_str), Some("900000"), "{name}");
    }
}

// EAC-FR-WQNH, EAC-FR-OWPP: in the VM too, a timeout that the author's entry
// names keeps the author's value, and the executor sets the other one.
#[test]
fn a_sandbox_process_keeps_a_timeout_the_author_names() {
    let harness = harness_for("claude_code");
    let (mut records, active) = harness.store.load_agentic_registry().expect("registry");
    let record = records.iter_mut().find(|r| r.vendor == "claude_code").unwrap();
    record.env_vars = vec!["BASH_MAX_TIMEOUT_MS=3600000".to_string()];
    harness.store.save_agentic_registry(records, active).expect("saved");
    let docker = RecordingRuntime::replying("");
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    launch(&harness, docker, sandbox.clone(), microsandbox(), scope(), TurnKind::Work)
        .expect("runs");

    let env = sandbox.only_process().env;
    assert_eq!(env.get("BASH_MAX_TIMEOUT_MS").map(String::as_str), Some("3600000"));
    assert_eq!(env.get("BASH_DEFAULT_TIMEOUT_MS").map(String::as_str), Some("900000"));
}

// EAC-FR-WBDE, EAC-FR-HXDN: a later launch removes what a turn added to the
// generated store and empties its `config` again. It keeps the directory and
// its mount points, so a reused VM still holds them.
#[test]
fn a_generated_store_drops_what_a_turn_added_and_keeps_its_mount_points() {
    let harness = harness_for("claude_code");
    let generated = harness.sessions_root().join("repository-store-test");
    let entries = [("objects", true), ("HEAD", false)];
    super::super::sandbox_store::prepare_generated_store(&harness.fs, &generated, &entries)
        .expect("prepared");
    let before = crate::fs::probe_directory_identity(&generated);
    let objects = crate::fs::probe_directory_identity(&generated.join("objects"));

    std::fs::write(generated.join("config"), b"[remote]").unwrap();
    std::fs::create_dir(generated.join("hooks")).unwrap();
    std::fs::write(generated.join("hooks/post-checkout"), b"#!/bin/sh").unwrap();
    std::fs::write(generated.join("index.lock"), b"").unwrap();
    super::super::sandbox_store::prepare_generated_store(&harness.fs, &generated, &entries)
        .expect("prepared again");

    assert_eq!(std::fs::read(generated.join("config")).unwrap().len(), 0);
    assert!(!generated.join("hooks").exists());
    assert!(!generated.join("index.lock").exists());
    assert!(generated.join("HEAD").is_file());
    assert_eq!(crate::fs::probe_directory_identity(&generated), before);
    assert_eq!(crate::fs::probe_directory_identity(&generated.join("objects")), objects);
}

/// EAC-FR-SNMS, EAC-FR-QYBJ: isolation `none`, a launch with no scope, and a
/// `semantic_rebase` turn all launch through Docker and boot no VM.
#[test]
fn other_launches_stay_docker_launches() {
    let cases = [
        (ResolvedIsolation::None, scope(), TurnKind::Work),
        (microsandbox(), None, TurnKind::Work),
        (microsandbox(), scope(), TurnKind::SemanticRebase),
    ];
    for (isolation, sandbox_scope, turn_kind) in cases {
        let harness = harness_for("claude_code");
        let docker = RecordingRuntime::replying(&valid_claude_stdout());
        let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
        launch(&harness, docker.clone(), sandbox.clone(), isolation, sandbox_scope, turn_kind)
            .expect("runs");
        assert_eq!(docker.launched(), 1, "{turn_kind:?}");
        assert!(sandbox.boots.lock().unwrap().is_empty());
    }
}

/// EAC-FR-NCSA: a VM that cannot be established returns `SandboxLaunchFailed`
/// with its cause, and no container is created in its place.
#[test]
fn a_vm_that_cannot_boot_is_a_typed_failure_with_no_fallback() {
    let harness = harness_for("claude_code");
    let docker = RecordingRuntime::replying(&valid_claude_stdout());
    let sandbox = RecordingSandbox::failing(BootError::Failed(
        SandboxFailureCause::InsufficientMemory,
        "cannot allocate memory".to_string(),
    ));
    let error = launch(&harness, docker.clone(), sandbox, microsandbox(), scope(), TurnKind::Work)
        .unwrap_err();
    assert!(matches!(
        error,
        AgentExecutionError::SandboxLaunchFailed(ref failure)
            if failure.cause == SandboxFailureCause::InsufficientMemory
    ));
    assert_eq!(error.kind(), "sandbox_launch_failed");
    assert_eq!(docker.launched(), 0);
}

/// EAC-FR-NCSA, GSS-FR-NCAQ: a machine that cannot run Microsandbox reports
/// `unsupported_host` rather than falling back.
#[test]
fn an_unsupported_machine_reports_its_cause() {
    let harness = harness_for("claude_code");
    let docker = RecordingRuntime::replying(&valid_claude_stdout());
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    let isolation = ResolvedIsolation::Microsandbox(SandboxSettings {
        cpus: 1,
        memory_gib: 1,
        disk_gib: 48,
        block_local_network: true,
        allowed: Vec::new(),
        unsupported_reason: Some(crate::isolation::UnsupportedReason::KvmMissing),
    });
    let error =
        launch(&harness, docker.clone(), sandbox.clone(), isolation, scope(), TurnKind::Work)
            .unwrap_err();
    assert!(matches!(
        error,
        AgentExecutionError::SandboxLaunchFailed(ref failure)
            if failure.cause == SandboxFailureCause::UnsupportedHost
    ));
    assert_eq!(docker.launched(), 0);
    assert!(sandbox.boots.lock().unwrap().is_empty());
}

/// EAC-FR-MIQG, EAC-FR-25: a graduation turn whose run is cancelled before its
/// VM is there ends `cancelled` with no response, boots nothing, and runs no
/// process. It is not a sandbox failure.
#[test]
fn a_turn_cancelled_before_its_vm_is_there_ends_cancelled() {
    let harness = harness_for("claude_code");
    let docker = RecordingRuntime::replying("");
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    let executor = AgentCliExecutor::new(docker.clone())
        .with_session_state_root(harness.sessions_root())
        .with_isolation(microsandbox())
        .with_sandbox_pool(Arc::new(SandboxPool::new(sandbox.clone())));
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let execution = block_on(executor.execute_agent_cli(
        &Silent,
        &harness.context(),
        AgentExecutionRequest {
            sandbox_scope: scope(),
            turn_kind: TurnKind::Work,
            execution_directory: harness.workspace(),
            task: task("go"),
            cancellation,
            activity: None,
            durable_output: None,
            supplementary_mount: None,
        },
    ))
    .expect("a cancelled turn is an execution, not an error");

    assert_eq!(execution.process_outcome, ProcessOutcome::Cancelled);
    assert!(execution.response.is_none());
    assert!(sandbox.boots.lock().unwrap().is_empty());
    assert!(sandbox.processes.lock().unwrap().is_empty());
    assert_eq!(docker.launched(), 0);
}

// EAC-FR-WBDE, EAC-FR-HXDN: a symbolic link a turn put in place of `config` is
// never followed: the file it names keeps its content, and the next launch
// stands on a generated store made again, with an empty `config` and another
// identity, so the scope's VM is rebooted onto it rather than reused.
#[test]
fn a_link_left_in_the_generated_store_is_never_followed() {
    let harness = harness_for("claude_code");
    linked_worktree_at(&harness.sessions_root(), &harness.workspace());
    let store = crate::changes::canonicalize_lenient(&harness.sessions_root().join("primary/.git"));
    let store = store.to_string_lossy().into_owned();
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    let executor = AgentCliExecutor::new(RecordingRuntime::replying(""))
        .with_session_state_root(harness.sessions_root())
        .with_isolation(microsandbox())
        .with_sandbox_pool(Arc::new(SandboxPool::new(sandbox.clone())));
    let work = || {
        block_on(executor.execute_agent_cli(
            &Silent,
            &harness.context(),
            AgentExecutionRequest {
                sandbox_scope: scope(),
                turn_kind: TurnKind::Work,
                execution_directory: harness.workspace(),
                task: task("go"),
                cancellation: CancellationToken::new(),
                activity: None,
                durable_output: None,
                supplementary_mount: None,
            },
        ))
        .expect("runs")
    };
    let generated = || {
        let boots = sandbox.boots.lock().unwrap();
        let spec = &boots.last().expect("a boot").1;
        PathBuf::from(&spec.mounts.iter().find(|m| m.target == store).unwrap().source)
    };

    work();
    let first = generated();
    let before = crate::fs::probe_directory_identity(&first);
    let outside = harness.sessions_root().join("outside");
    std::fs::write(&outside, b"keep").unwrap();
    std::fs::remove_file(first.join("config")).unwrap();
    std::os::unix::fs::symlink(&outside, first.join("config")).unwrap();

    work();
    assert_eq!(sandbox.boots.lock().unwrap().len(), 2, "the VM is rebooted, not reused");
    assert_eq!(generated(), first, "the store keeps its path");
    assert_eq!(std::fs::read(&outside).unwrap(), b"keep");
    let config = std::fs::symlink_metadata(first.join("config")).unwrap();
    assert!(config.is_file() && config.len() == 0);
    assert_ne!(crate::fs::probe_directory_identity(&first), before);
}

// ---------------------------------------------------------------------------
// EAC-FR-KVBT, EAC-FR-BZPC, EAC-FR-RJDQ, EAC-FR-WTFH: the Custom Gateway
// ---------------------------------------------------------------------------

use std::net::IpAddr;

use super::sandbox::gateway::GatewayResolver;

const GATEWAY_URL: &str = "https://gateway.corp.example:8443/v1";

/// A resolver that answers what it is told, and records what it was asked.
struct FixedResolver {
    answer: Result<Vec<IpAddr>, String>,
    asked: StdMutex<Vec<String>>,
    /// Whether the lookup never ends.
    hangs: bool,
    /// A run this lookup cancels as it starts.
    cancels: Option<CancellationToken>,
}

impl FixedResolver {
    fn answering(addresses: &[&str]) -> Arc<FixedResolver> {
        Arc::new(FixedResolver {
            answer: Ok(addresses.iter().map(|a| a.parse().unwrap()).collect()),
            asked: StdMutex::new(Vec::new()),
            hangs: false,
            cancels: None,
        })
    }

    fn failing(message: &str) -> Arc<FixedResolver> {
        Arc::new(FixedResolver {
            answer: Err(message.to_string()),
            asked: StdMutex::new(Vec::new()),
            hangs: false,
            cancels: None,
        })
    }

    fn hanging() -> Arc<FixedResolver> {
        Arc::new(FixedResolver {
            answer: Ok(Vec::new()),
            asked: StdMutex::new(Vec::new()),
            hangs: true,
            cancels: None,
        })
    }

    /// A lookup that cancels the run as it starts, and never ends.
    fn cancelling(token: CancellationToken) -> Arc<FixedResolver> {
        Arc::new(FixedResolver {
            answer: Ok(Vec::new()),
            asked: StdMutex::new(Vec::new()),
            hangs: true,
            cancels: Some(token),
        })
    }
}

impl GatewayResolver for FixedResolver {
    fn resolve<'a>(&'a self, host: &'a str) -> BoxFuture<'a, Result<Vec<IpAddr>, String>> {
        Box::pin(async move {
            self.asked.lock().unwrap().push(host.to_string());
            if let Some(token) = &self.cancels {
                token.cancel();
            }
            if self.hangs {
                std::future::pending::<()>().await;
            }
            self.answer.clone()
        })
    }
}

fn gateway_harness(entries: &[&str]) -> Harness {
    gateway_harness_at(GATEWAY_URL, entries)
}

fn gateway_harness_at(url: &str, entries: &[&str]) -> Harness {
    harness_verified_with(
        "claude_code",
        Some(VerifyConfig {
            auth_mode: Some("custom_gateway".into()),
            gateway_base_url: Some(url.into()),
            gateway_token: Some("gw-FAKE-TEST-TOKEN-NOT-A-CREDENTIAL".into()),
            env_vars: Some(entries.iter().map(|e| e.to_string()).collect()),
            ..Default::default()
        }),
    )
}

fn isolation(block: bool) -> ResolvedIsolation {
    match microsandbox() {
        ResolvedIsolation::Microsandbox(settings) => {
            ResolvedIsolation::Microsandbox(SandboxSettings {
                block_local_network: block,
                ..settings
            })
        }
        other => other,
    }
}

fn gateway_launch(
    harness: &Harness,
    sandbox: Arc<RecordingSandbox>,
    resolver: Arc<FixedResolver>,
    block: bool,
    cancellation: CancellationToken,
) -> Result<AgentExecution, AgentExecutionError> {
    let executor = AgentCliExecutor::new(RecordingRuntime::replying(""))
        .with_session_state_root(harness.sessions_root())
        .with_isolation(isolation(block))
        .with_sandbox_pool(Arc::new(SandboxPool::new(sandbox)))
        .with_gateway_resolver(resolver);
    block_on(executor.execute_agent_cli(
        &Silent,
        &harness.context(),
        AgentExecutionRequest {
            sandbox_scope: scope(),
            turn_kind: TurnKind::Work,
            execution_directory: harness.workspace(),
            task: task("go"),
            cancellation,
            activity: None,
            durable_output: None,
            supplementary_mount: None,
        },
    ))
}

fn ips(addresses: &[&str]) -> Vec<IpAddr> {
    addresses.iter().map(|a| a.parse().unwrap()).collect()
}

// EAC-FR-KVBT, EAC-FR-BZPC, EAC-FR-RJDQ, EAC-FR-WTFH: a gateway name is resolved
// on this machine; with the block switch on, each address that is not public
// is allowed; every address but loopback is pinned in the guest.
#[test]
fn a_private_gateway_is_resolved_here_allowed_and_pinned() {
    let harness = gateway_harness(&[]);
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    let resolver = FixedResolver::answering(&["52.1.1.1", "10.1.2.3", "127.0.0.1"]);
    let execution =
        gateway_launch(&harness, sandbox.clone(), resolver.clone(), true, CancellationToken::new())
            .expect("runs");
    assert_eq!(execution.process_outcome, ProcessOutcome::Completed);
    assert_eq!(*resolver.asked.lock().unwrap(), vec!["gateway.corp.example".to_string()]);
    assert_eq!(sandbox.only_boot().network.gateway_addresses, ips(&["10.1.2.3"]));
    assert_eq!(
        *sandbox.pins.lock().unwrap(),
        vec!["10.1.2.3 gateway.corp.example # synthesis-gateway\n\
             52.1.1.1 gateway.corp.example # synthesis-gateway\n"
            .to_string()]
    );
    sandbox.only_process();

    // EAC-FR-BZPC: with the switch off nothing is allowed, and the name is still
    // pinned.
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    gateway_launch(
        &harness,
        sandbox.clone(),
        FixedResolver::answering(&["10.1.2.3"]),
        false,
        CancellationToken::new(),
    )
    .expect("runs");
    assert!(sandbox.only_boot().network.gateway_addresses.is_empty());
    assert_eq!(sandbox.pins.lock().unwrap().len(), 1);
}

// EAC-FR-KVBT: the URL is read from the final environment, so an author entry
// that replaces it names the host that is resolved.
#[test]
fn the_final_environment_names_the_gateway() {
    let harness = gateway_harness(&["ANTHROPIC_BEDROCK_BASE_URL=https://other.corp.example"]);
    let resolver = FixedResolver::answering(&["10.9.9.9"]);
    gateway_launch(
        &harness,
        RecordingSandbox::replying(&valid_claude_stdout()),
        resolver.clone(),
        true,
        CancellationToken::new(),
    )
    .expect("runs");
    assert_eq!(*resolver.asked.lock().unwrap(), vec!["other.corp.example".to_string()]);
}

// EAC-FR-RJDQ: a lookup that fails, or that leaves only loopback, fails the
// launch with `gateway_unresolved` before the VM is touched.
#[test]
fn an_unresolved_gateway_fails_before_the_vm_is_touched() {
    let harness = gateway_harness(&[]);
    for resolver in
        [FixedResolver::failing("no such host"), FixedResolver::answering(&["127.0.0.1"])]
    {
        let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
        let error =
            gateway_launch(&harness, sandbox.clone(), resolver, true, CancellationToken::new())
                .expect_err("refused");
        match error {
            AgentExecutionError::SandboxLaunchFailed(failure) => {
                assert_eq!(failure.cause, SandboxFailureCause::GatewayUnresolved);
                assert!(
                    failure.message.starts_with("gateway.corp.example: "),
                    "{}",
                    failure.message
                );
            }
            other => panic!("expected a sandbox failure, got {other:?}"),
        }
        assert!(sandbox.boots.lock().unwrap().is_empty());
        assert!(sandbox.processes.lock().unwrap().is_empty());
    }
}

// EAC-FR-KVBT, EAC-FR-MIQG: a run cancelled during the lookup ends cancelled,
// and boots nothing.
#[test]
fn a_run_cancelled_during_the_lookup_ends_cancelled() {
    let harness = gateway_harness(&[]);
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let execution =
        gateway_launch(&harness, sandbox.clone(), FixedResolver::hanging(), true, cancellation)
            .expect("a cancelled turn is an execution");
    assert_eq!(execution.process_outcome, ProcessOutcome::Cancelled);
    assert!(sandbox.boots.lock().unwrap().is_empty());
}

// EAC-FR-KVBT: a launch with no Custom Gateway looks nothing up and pins
// nothing.
#[test]
fn a_subscription_launch_looks_nothing_up() {
    let harness = harness_for("claude_code");
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    let resolver = FixedResolver::answering(&["10.1.2.3"]);
    gateway_launch(&harness, sandbox.clone(), resolver.clone(), true, CancellationToken::new())
        .expect("runs");
    assert!(resolver.asked.lock().unwrap().is_empty());
    assert!(sandbox.pins.lock().unwrap().is_empty());
    assert!(sandbox.only_boot().network.gateway_addresses.is_empty());
}

// EAC-FR-KVBT, EAC-FR-MIQG: a run cancelled while the lookup runs ends
// cancelled, and boots nothing.
#[test]
fn a_run_cancelled_while_the_lookup_runs_ends_cancelled() {
    let harness = gateway_harness(&[]);
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    let cancellation = CancellationToken::new();
    let resolver = FixedResolver::cancelling(cancellation.clone());
    let execution = gateway_launch(&harness, sandbox.clone(), resolver.clone(), true, cancellation)
        .expect("a cancelled turn is an execution");
    assert_eq!(*resolver.asked.lock().unwrap(), vec!["gateway.corp.example".to_string()]);
    assert_eq!(execution.process_outcome, ProcessOutcome::Cancelled);
    assert!(sandbox.boots.lock().unwrap().is_empty());
}

// EAC-FR-BZPC: an IP gateway gets an address rule and no lookup; this
// machine's alias allows this machine; `localhost` and an unreadable URL add
// nothing; a name with public addresses only gets no rule and is pinned.
#[test]
fn each_gateway_form_adds_what_it_names() {
    let launch_with = |harness: Harness, block: bool, answers: &[&str]| {
        let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
        let resolver = FixedResolver::answering(answers);
        let execution = gateway_launch(
            &harness,
            sandbox.clone(),
            resolver.clone(),
            block,
            CancellationToken::new(),
        )
        .expect("runs");
        assert_eq!(execution.process_outcome, ProcessOutcome::Completed);
        let asked = resolver.asked.lock().unwrap().clone();
        let pins = sandbox.pins.lock().unwrap().clone();
        (sandbox.only_boot().network, asked, pins)
    };
    let launch_at = |url: &str, block: bool, answers: &[&str]| {
        launch_with(gateway_harness_at(url, &[]), block, answers)
    };

    let (network, asked, pins) = launch_at("http://10.1.2.3:4000", true, &[]);
    assert_eq!(network.gateway_addresses, ips(&["10.1.2.3"]));
    assert!(asked.is_empty() && pins.is_empty());

    let (network, asked, _) = launch_at("http://52.1.1.1:4000", true, &[]);
    assert!(network.gateway_addresses.is_empty() && asked.is_empty());

    let (network, asked, pins) = launch_at("http://host.microsandbox.internal:4000", true, &[]);
    assert!(network.allowed.contains(&AllowedHost::HostMachine));
    assert!(asked.is_empty() && pins.is_empty());
    let (network, _, _) = launch_at("http://host.microsandbox.internal:4000", false, &[]);
    assert!(!network.allowed.contains(&AllowedHost::HostMachine));

    // An unreadable URL can come only from an author entry, which verify does
    // not parse.
    let unreadable = || gateway_harness(&["ANTHROPIC_BEDROCK_BASE_URL=not-a-url"]);
    for (url, harness) in [
        ("localhost", gateway_harness_at("http://localhost:4000", &[])),
        ("loopback", gateway_harness_at("http://127.0.0.1:4000", &[])),
        ("unreadable", unreadable()),
    ] {
        let (network, asked, pins) = launch_with(harness, true, &["10.1.2.3"]);
        assert!(network.gateway_addresses.is_empty() && network.allowed.is_empty(), "{url}");
        assert!(asked.is_empty() && pins.is_empty(), "{url}");
    }

    let (network, asked, pins) = launch_at(GATEWAY_URL, true, &["52.1.1.1"]);
    assert!(network.gateway_addresses.is_empty());
    assert_eq!(asked, vec!["gateway.corp.example".to_string()]);
    assert_eq!(pins, vec!["52.1.1.1 gateway.corp.example # synthesis-gateway\n".to_string()]);
}

#[path = "sandbox_launch_docker.rs"]
mod docker;
