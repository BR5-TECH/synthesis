use std::path::Path;
use std::sync::{Arc, Mutex};

use super::compose::{self, Composed};
use super::*;
use crate::agentic::LaunchVariable;
use crate::tools::agent_exec::descriptor::{BindMount, ContainerSpec};
use crate::tools::agent_exec::devcontainer_pool::RunningDevcontainer;
use crate::tools::agent_exec::runtime::{BoxFuture, RuntimeError, RuntimeOutcome};
use crate::tools::agent_exec::sandbox::{
    BootError, BootImage, InteractiveVmProcess, SandboxPool, SandboxProcess, SandboxRuntime,
    SandboxSpec, NO_VM_RUNNING,
};

fn composed(workspace: &str) -> Composed {
    Composed {
        workspace: workspace.to_string(),
        aligned: true,
        mounts: vec![BindMount {
            source: workspace.to_string(),
            target: workspace.to_string(),
            read_only: false,
        }],
        env_literals: compose::terminal_literals(),
        ..Default::default()
    }
}

fn spec_for(program: &InteractiveProgram, composed: &Composed) -> ContainerSpec {
    compose::container_spec("synthesis-t1".into(), "repo/image:1".into(), composed, program)
}

fn claude_credential(value: &str) -> AgentLaunchCredential {
    AgentLaunchCredential::ClaudeEnvironment(vec![LaunchVariable {
        name: "CLAUDE_CODE_OAUTH_TOKEN".into(),
        value: SecretString::new(value.to_string()),
        masked: true,
    }])
}

// EAC-FR-LQJC, EAC-FR-HTEB, EAC-FR-LIQF: a shell is `docker run -i -t` with
// the entrypoint replaced by a login shell, in the checkout at its own path.
#[test]
fn a_shell_runs_with_a_terminal_and_a_login_shell() {
    let composed = composed("/repo/wt");
    let args = interactive_run_args(&spec_for(&InteractiveProgram::Shell, &composed));
    let at = args.iter().position(|a| a == "--interactive").expect("interactive");
    assert_eq!(args[at + 1], "--tty");
    let entry = args.iter().position(|a| a == "--entrypoint").expect("entrypoint");
    assert_eq!(args[entry + 1], SHELL_PROGRAM);
    assert_eq!(&args[args.len() - 2..], &["-c".to_string(), SHELL_SCRIPT.to_string()]);
    assert!(SHELL_SCRIPT.contains("/bin/bash -l") && SHELL_SCRIPT.contains("/bin/sh -l"));
    let workdir = args.iter().position(|a| a == "--workdir").unwrap();
    assert_eq!(args[workdir + 1], "/repo/wt");
    assert!(args.iter().any(|a| a.contains("target=/repo/wt")));
    assert!(args.iter().any(|a| a == "TERM=xterm-256color"));
}

// EAC-FR-DQZP: a shell gets no credential and no session-state mount.
#[test]
fn a_shell_gets_no_credential_and_no_session_state() {
    let composed = composed("/repo/wt");
    let args = interactive_run_args(&spec_for(&InteractiveProgram::Shell, &composed));
    assert!(!args.iter().any(|a| a.contains("CLAUDE_CONFIG_DIR") || a.contains("CODEX_HOME")));
    assert!(!args.iter().any(|a| a == "CLAUDE_CODE_OAUTH_TOKEN"));
    assert_eq!(args.iter().filter(|a| *a == "--mount").count(), 1);
    assert_eq!(compose::takes_credential(&InteractiveProgram::Shell), None);
}

// EAC-FR-13, EAC-FR-LUZG: the headless vector of the same specification
// allocates no terminal.
#[test]
fn the_headless_vector_stays_without_a_terminal() {
    let composed = composed("/repo/wt");
    let spec = spec_for(&InteractiveProgram::Shell, &composed);
    let headless = spec.docker_run_args();
    assert!(!headless.iter().any(|a| a == "--tty" || a == "-t"));
    assert!(headless.iter().any(|a| a == "--interactive"));
    assert_eq!(interactive_run_args(&spec).len(), headless.len() + 1);
}

// EAC-FR-HTEB, EAC-FR-DQZP, EAC-FR-15: an agent CLI runs the image's
// entrypoint with its executable name, and its credential goes by name alone.
#[test]
fn an_agent_cli_passes_its_credential_by_name() {
    let mut composed = composed("/repo/wt");
    let parts = compose::credential_parts(claude_credential("very-secret-value")).unwrap();
    composed.env_names = parts.env_names;
    composed.env = parts.env;
    let program = InteractiveProgram::AgentCli(CliVendor::ClaudeCode);
    let spec = spec_for(&program, &composed);
    assert_eq!(spec.entrypoint, None);
    let args = interactive_run_args(&spec);
    assert_eq!(args.last().map(String::as_str), Some("claude"));
    let env = args.iter().position(|a| a == "CLAUDE_CODE_OAUTH_TOKEN").expect("by name");
    assert_eq!(args[env - 1], "--env");
    assert!(!args.iter().any(|a| a.contains("very-secret-value")));
    assert_eq!(composed.env.get("CLAUDE_CODE_OAUTH_TOKEN").unwrap().expose(), "very-secret-value");
}

// EAC-FR-DQZP: Codex gets its login directory read/write and CODEX_HOME.
#[test]
fn codex_gets_its_login_directory() {
    let parts = compose::credential_parts(AgentLaunchCredential::CodexConfigMount {
        source: "/home/u/.codex".into(),
        target: "/codex-home".into(),
    })
    .unwrap();
    assert_eq!(parts.mounts.len(), 1);
    assert!(!parts.mounts[0].read_only);
    assert!(parts.env_literals.iter().any(|(n, v)| n == "CODEX_HOME" && v == "/codex-home"));
    assert!(parts.env_names.is_empty());
}

// EAC-FR-QGYS, EAC-FR-DQZP: OpenCode is a valid launch and gets no credential;
// Claude Code and Codex get theirs.
#[test]
fn only_claude_code_and_codex_take_a_credential() {
    use InteractiveProgram::AgentCli;
    assert_eq!(compose::takes_credential(&AgentCli(CliVendor::Opencode)), None);
    assert_eq!(compose::takes_credential(&AgentCli(CliVendor::Codex)), Some(CliVendor::Codex));
    assert_eq!(
        compose::takes_credential(&AgentCli(CliVendor::ClaudeCode)),
        Some(CliVendor::ClaudeCode)
    );
    assert_eq!(compose::program_argv(&AgentCli(CliVendor::Opencode)), vec!["opencode"]);
}

fn extras<'a>(sessions: &'a Path, id: &'a str) -> compose::ComposeExtras<'a> {
    compose::ComposeExtras {
        credential: None,
        session_root: Some(sessions),
        terminal_id: id,
        docker_in_docker: None,
    }
}

fn temp_fs(root: &Path) -> Arc<FsAccess> {
    Arc::new(FsAccess::builder().required_root(root).build().expect("fs"))
}

// EAC-FR-DQZP: an agent CLI with a dedicated session-state mount gets an empty
// directory of its own; a shell gets none.
#[test]
fn an_agent_cli_gets_a_fresh_session_state_directory() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let checkout = root.join("checkout");
    let fs = temp_fs(&root);
    fs.create_dir(&checkout).unwrap();
    let sessions = root.join("sessions");
    let executor = AgentCliExecutor::default().with_session_state_root(root.join("state"));
    let program = InteractiveProgram::AgentCli(CliVendor::ClaudeCode);
    let composed =
        executor.compose_interactive(&fs, &checkout, &program, extras(&sessions, "t-1")).unwrap();
    let dir = composed.session_dir.clone().expect("a session directory");
    assert!(dir.starts_with(&sessions));
    assert_eq!(fs.file_info(&dir).unwrap().kind, crate::fs::EntryKind::Dir);
    let (env, target) = compose::dedicated_session_state(CliVendor::ClaudeCode, false).unwrap();
    assert!(composed.mounts.iter().any(|m| m.target == target && !m.read_only));
    assert!(composed.env_literals.iter().any(|(n, v)| n == env && v == target));

    let shell = executor
        .compose_interactive(&fs, &checkout, &InteractiveProgram::Shell, extras(&sessions, "t-2"))
        .unwrap();
    assert!(shell.session_dir.is_none());
    assert_eq!(shell.mounts.len(), 1);
}

// EAC-FR-DQZP, EAC-FR-QHIU: a Codex terminal in the `access_token` mode gets
// the token by name and an empty session-state directory of its own at
// `/home/agent/.codex`; in the `login_folder` mode it gets its login directory
// alone.
#[test]
fn a_codex_terminal_gets_the_session_state_of_its_mode() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let fs = temp_fs(&root);
    let checkout = root.join("checkout");
    fs.create_dir(&checkout).unwrap();
    let sessions = root.join("sessions");
    let executor = AgentCliExecutor::default().with_session_state_root(root.join("state"));
    let program = InteractiveProgram::AgentCli(CliVendor::Codex);

    let token = AgentLaunchCredential::CodexEnvironment(vec![LaunchVariable {
        name: "CODEX_ACCESS_TOKEN".into(),
        value: SecretString::new("codex-access-FAKE".to_string()),
        masked: true,
    }]);
    let mut with_token = extras(&sessions, "t-token");
    with_token.credential = Some(token);
    let composed = executor.compose_interactive(&fs, &checkout, &program, with_token).unwrap();
    let dir = composed.session_dir.clone().expect("a session directory");
    assert!(dir.starts_with(&sessions));
    let state = composed.mounts.iter().find(|m| m.target == "/home/agent/.codex").unwrap();
    assert_eq!(state.source, dir.to_string_lossy());
    assert!(!state.read_only);
    assert!(composed
        .env_literals
        .iter()
        .any(|(n, v)| n == "CODEX_HOME" && v == "/home/agent/.codex"));
    assert_eq!(composed.env_names, ["CODEX_ACCESS_TOKEN"]);
    // EAC-FR-DQZP: no model and no effort reach an interactive vector.
    assert_eq!(compose::program_args(&program).1, ["codex"]);

    let login = root.join("login");
    fs.create_dir(&login).unwrap();
    let mut with_login = extras(&sessions, "t-login");
    with_login.credential = Some(AgentLaunchCredential::CodexConfigMount {
        source: login.clone(),
        target: "/home/agent/.codex".into(),
    });
    let composed = executor.compose_interactive(&fs, &checkout, &program, with_login).unwrap();
    assert!(composed.session_dir.is_none());
    let mounts: Vec<_> =
        composed.mounts.iter().filter(|m| m.target == "/home/agent/.codex").collect();
    assert_eq!(mounts.len(), 1);
    assert_eq!(mounts[0].source, login.to_string_lossy());
    assert!(composed.env_names.is_empty());
    // The login directory is the session state, and `CODEX_HOME` names it.
    assert!(composed
        .env_literals
        .iter()
        .any(|(n, v)| n == "CODEX_HOME" && v == "/home/agent/.codex"));

    assert_eq!(compose::dedicated_session_state(CliVendor::Codex, true), None);
    assert_eq!(
        compose::dedicated_session_state(CliVendor::Codex, false),
        Some(("CODEX_HOME", "/home/agent/.codex"))
    );
}

// DVC-FR-WRXL: `docker exec -i -t` as the remote user, in the checkout, with
// the env file, a credential by name, and the program last.
#[test]
fn the_dev_container_exec_vector() {
    let running = RunningDevcontainer {
        container_id: "cid".into(),
        remote_user: "vscode".into(),
        remote_workspace_folder: "/repo/wt".into(),
        probed_env: vec![],
    };
    let argv = devcontainer_place::devcontainer_argv(&InteractiveProgram::Shell);
    let args = interactive_exec_args(
        &running,
        "/repo/wt",
        Path::new("/data/terminal.env"),
        &["CLAUDE_CODE_OAUTH_TOKEN".to_string()],
        &argv,
    );
    let head: Vec<&str> = args.iter().take(11).map(String::as_str).collect();
    assert_eq!(
        head,
        vec![
            "exec",
            "-i",
            "-t",
            "-u",
            "vscode",
            "-w",
            "/repo/wt",
            "--env-file",
            "/data/terminal.env",
            "-e",
            "CLAUDE_CODE_OAUTH_TOKEN"
        ]
    );
    assert_eq!(args[11], "cid");
    assert_eq!(&args[12..14], &["sh".to_string(), "-c".to_string()]);
    assert!(args[14].contains("exec bash -l") && args[14].contains("exec sh -l"));
    let agent =
        devcontainer_place::devcontainer_argv(&InteractiveProgram::AgentCli(CliVendor::Codex));
    assert_eq!(agent, vec!["codex"]);
}

// DVC-FR-RMJQ, DVC-FR-KTBH: the scope key of a terminal, and one failure line
// of at most 300 characters.
#[test]
fn the_dev_container_scope_and_failure_line() {
    assert_eq!(devcontainer_place::terminal_scope_key("t-7"), "terminal:t-7");
    let long = format!("\n  first {}\nsecond", "x".repeat(400));
    let line = devcontainer_place::failure_line(&long);
    assert!(line.starts_with("first "));
    assert_eq!(line.chars().count(), 300);
}

/// A sandbox double that records boots and interactive processes.
#[derive(Default)]
struct RecordingVm {
    boots: Mutex<usize>,
    attached: Mutex<Vec<(String, InteractiveVmProcess)>>,
}

impl SandboxRuntime for RecordingVm {
    fn boot<'a>(
        &'a self,
        _vm: &'a str,
        _spec: &'a SandboxSpec,
        _image: &'a BootImage,
    ) -> BoxFuture<'a, Result<(), BootError>> {
        *self.boots.lock().unwrap() += 1;
        Box::pin(async { Ok(()) })
    }
    fn load_export<'a>(&'a self, _: &'a Path, _: &'a str) -> BoxFuture<'a, Result<(), BootError>> {
        Box::pin(async { Ok(()) })
    }
    fn reopen_catalog(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
    fn run<'a>(
        &'a self,
        _vm: &'a str,
        _process: SandboxProcess<'a>,
    ) -> BoxFuture<'a, Result<RuntimeOutcome, RuntimeError>> {
        Box::pin(async { Err(RuntimeError::Unavailable) })
    }
    fn refresh<'a>(&'a self, _vm: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
    fn stop<'a>(&'a self, _vm: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }
    fn start_docker_daemon<'a>(&'a self, _: &'a str, _: u32) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
    fn attach_interactive<'a>(
        &'a self,
        vm: &'a str,
        process: InteractiveVmProcess,
    ) -> BoxFuture<'a, Result<ProcessHandle, String>> {
        self.attached.lock().unwrap().push((vm.to_string(), process));
        Box::pin(async { Ok(fake_handle()) })
    }
}

struct NoControl;
impl crate::terminals::process::ProcessControl for NoControl {
    fn write(&self, _: &[u8]) -> std::io::Result<()> {
        Ok(())
    }
    fn resize(&self, _: u16, _: u16) -> std::io::Result<()> {
        Ok(())
    }
    fn terminate(&self) {}
}

fn fake_handle() -> ProcessHandle {
    let (_tx, output) = tokio::sync::mpsc::channel(1);
    let (_exit_tx, exit) = tokio::sync::oneshot::channel();
    ProcessHandle {
        output,
        exit,
        control: Arc::new(NoControl),
        environment_name: Some("vm".into()),
    }
}

fn request(scope: &str) -> InteractiveLaunchRequest {
    InteractiveLaunchRequest {
        execution_directory: "/repo/wt".into(),
        program: InteractiveProgram::Shell,
        place: InteractivePlace::Microsandbox,
        sandbox_scope: Some(SandboxScope { queue_key: scope.to_string() }),
        terminal_id: "t-1".into(),
        cols: 120,
        rows: 40,
        progress: None,
    }
}

// EAC-FR-QQMA, TRS-FR-KJYH: with no VM booted, the attach is refused with
// no_vm_running and nothing boots.
#[tokio::test]
async fn an_attach_with_no_vm_is_refused_and_boots_nothing() {
    let runtime = Arc::new(RecordingVm::default());
    let pool = SandboxPool::new(runtime.clone());
    let scope = SandboxScope { queue_key: "stream-1".into() };
    assert!(!vm_place::running(&pool, "proj", &scope));
    let err = vm_place::launch(&pool, "proj", Path::new("/repo/wt"), &request("stream-1"))
        .await
        .err()
        .unwrap();
    assert_eq!(err, NO_VM_RUNNING);
    assert_eq!(*runtime.boots.lock().unwrap(), 0);
    assert!(runtime.attached.lock().unwrap().is_empty());
}

// EAC-FR-QQMA, TRS-FR-KJYH: a booted VM is running, and the attach runs a
// login shell as the host user, in the checkout, with no credential.
#[tokio::test]
async fn an_attach_to_a_booted_vm_runs_a_login_shell() {
    let runtime = Arc::new(RecordingVm::default());
    let pool = SandboxPool::new(runtime.clone());
    let scope = SandboxScope { queue_key: "stream-1".into() };
    let vm = crate::tools::agent_exec::sandbox::vm_name("proj", &scope);
    pool.mark_booted_for_test(&vm);
    assert!(vm_place::running(&pool, "proj", &scope));
    vm_place::launch(&pool, "proj", Path::new("/repo/wt"), &request("stream-1")).await.unwrap();
    let attached = runtime.attached.lock().unwrap().clone();
    assert_eq!(attached.len(), 1);
    let (name, process) = &attached[0];
    assert_eq!(name, &vm);
    assert_eq!(process.argv, compose::program_argv(&InteractiveProgram::Shell));
    assert_eq!(process.workdir, "/repo/wt");
    let (uid, gid) = crate::tools::agent_exec::descriptor::host_ids();
    assert_eq!(process.user, format!("{uid}:{gid}"));
    assert_eq!(process.env, vec![("TERM".to_string(), "xterm-256color".to_string())]);
    assert_eq!((process.cols, process.rows), (120, 40));
    assert_eq!(*runtime.boots.lock().unwrap(), 0);
}

// EAC-FR-INWE: an attach does not wait for a launch that holds the VM.
#[tokio::test]
async fn an_attach_does_not_wait_for_a_launch() {
    let runtime = Arc::new(RecordingVm::default());
    let pool = SandboxPool::new(runtime.clone());
    let scope = SandboxScope { queue_key: "stream-2".into() };
    let vm = crate::tools::agent_exec::sandbox::vm_name("proj", &scope);
    pool.mark_booted_for_test(&vm);
    let attached = pool
        .while_slot_held(&vm, async {
            assert!(vm_place::running(&pool, "proj", &scope));
            vm_place::launch(&pool, "proj", Path::new("/repo/wt"), &request("stream-2")).await
        })
        .await;
    assert!(attached.is_ok());
}

// EAC-FR-QQMA: an agent CLI does not run in a VM.
#[tokio::test]
async fn only_a_shell_runs_in_a_vm() {
    let pool = SandboxPool::new(Arc::new(RecordingVm::default()));
    let mut req = request("stream-1");
    req.program = InteractiveProgram::AgentCli(CliVendor::Codex);
    assert!(vm_place::launch(&pool, "proj", Path::new("/repo/wt"), &req).await.is_err());
}

// EAC-FR-HCIK: the end hook runs once with the exit code, before the caller
// sees the end; a cleanup runs once whichever route comes first.
#[tokio::test]
async fn the_end_hook_and_the_cleanup_run_once() {
    let (_otx, output) = tokio::sync::mpsc::channel(1);
    let (exit_tx, exit) = tokio::sync::oneshot::channel();
    let handle =
        ProcessHandle { output, exit, control: Arc::new(NoControl), environment_name: None };
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let handle = on_end(handle, move |code| record.lock().unwrap().push(code));
    exit_tx.send(Some(5)).unwrap();
    assert_eq!(handle.exit.await.unwrap(), Some(5));
    assert_eq!(*seen.lock().unwrap(), vec![Some(5)]);

    let count = Arc::new(Mutex::new(0));
    let counter = count.clone();
    let cleanup = CleanupOnce::new(move || *counter.lock().unwrap() += 1);
    cleanup.run();
    cleanup.run();
    assert_eq!(*count.lock().unwrap(), 1);
}
