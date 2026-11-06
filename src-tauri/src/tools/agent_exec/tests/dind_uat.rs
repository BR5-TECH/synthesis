//! EAC-FR-UDQN, EAC-FR-BWTF, EAC-FR-LJXE, EAC-FR-PZKC: the Docker-in-Docker
//! acceptance test.
//!
//! Each test is one launch shape of EAC-FR-SNMS, run through the production
//! launch with real Docker. The shape is set through the stored project and
//! global settings, and the application reads them as a graduation launch
//! does. The image is the project's own image, built from the dedicated
//! Dockerfile of the shape under `tests/dind-uat/` through the project image
//! build (PSS-FR-26). Its entrypoint replaces the vendor CLI: it runs the
//! checks, writes `.uat/result.json`, and answers through `agentic-cli-mock`.
//! Nothing here mocks a Docker call or an executor step.
//!
//! Ignored by default, and run by no pipeline lane: it needs a Docker daemon,
//! the public registry, and for shape 3 hardware virtualisation. Run it with:
//!
//! ```text
//! task uat:dind
//! ```

use super::*;
use crate::docker::{DockerBackendConfig, DockerBackendMode, DockerBackendSeams, DockerEndpoint};
use crate::isolation::{IsolationMode, IsolationRecord};
use crate::project_settings::images::{
    self, BuildTerminal, ImageBuildRegistry, ProjectVendorImage,
};
use crate::project_settings::{DockerInDockerSettings, UnderMicrosandbox};

/// One launch shape of EAC-FR-SNMS.
struct Shape {
    number: u8,
    /// The suffix of the Dockerfile of the shape, and of its image name.
    name: &'static str,
    isolation: IsolationMode,
    under_microsandbox: UnderMicrosandbox,
    /// Store the socket path of the current Docker context, in place of the
    /// empty path that means the platform default.
    context_socket: bool,
}

impl Shape {
    fn host_socket(&self) -> bool {
        self.isolation == IsolationMode::None
            || self.under_microsandbox == UnderMicrosandbox::HostSocket
    }
}

const SHAPE_1: Shape = Shape {
    number: 1,
    name: "none-host-socket",
    isolation: IsolationMode::None,
    under_microsandbox: UnderMicrosandbox::VmDaemon,
    context_socket: false,
};
const SHAPE_2: Shape = Shape {
    number: 2,
    name: "microsandbox-host-socket",
    isolation: IsolationMode::Microsandbox,
    under_microsandbox: UnderMicrosandbox::HostSocket,
    context_socket: false,
};
const SHAPE_3: Shape = Shape {
    number: 3,
    name: "microsandbox-vm-daemon",
    isolation: IsolationMode::Microsandbox,
    under_microsandbox: UnderMicrosandbox::VmDaemon,
    context_socket: false,
};

/// Shape 1 with the socket path of the current Docker context stored, which on
/// Docker Desktop is not `/var/run/docker.sock`.
const SHAPE_1_CONTEXT_SOCKET: Shape = Shape {
    number: 1,
    name: "none-host-socket",
    isolation: IsolationMode::None,
    under_microsandbox: UnderMicrosandbox::VmDaemon,
    context_socket: true,
};

/// The four checks of EAC-FR-BWTF, in the order the entrypoint runs them.
const CHECKS: [&str; 4] = ["docker_version", "docker_run", "testcontainers", "browser"];
/// The folder of the build context in the temporary project.
const CONTEXT_DIR: &str = "dind-uat";
/// A turn pulls Postgres and a small image, so it gets more than the minute
/// of an ordinary test turn.
const TURN_TIMEOUT_MS: u64 = 20 * 60 * 1_000;

/// The repository root, where `tests/dind-uat/` and the mock crate are.
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("the repository root").to_path_buf()
}

/// The Docker CLI of this machine, found as the application finds it
/// (GSS-FR-37).
fn docker_cli() -> String {
    let path_var = std::env::var("PATH").ok();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    crate::docker::detect_docker_cli(
        path_var.as_deref(),
        home.as_deref(),
        &crate::agentic::RealFileProbe,
    )
    .expect("a Docker CLI on this machine")
}

/// The build context of EAC-FR-UDQN: the fixtures of `tests/dind-uat/`, the
/// mock crate, and the scenario the mock replays. Nothing else of the
/// repository is copied.
fn write_build_context(project: &Path) {
    let source = repository_root().join("tests/dind-uat");
    let context = project.join(CONTEXT_DIR);
    std::fs::create_dir_all(&context).unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            std::fs::copy(&path, context.join(path.file_name().unwrap())).unwrap();
        }
    }
    let testapp = context.join("testapp");
    std::fs::create_dir_all(&testapp).unwrap();
    for file in ["Cargo.toml", "Cargo.lock"] {
        std::fs::copy(source.join("testapp").join(file), testapp.join(file)).unwrap();
    }
    copy_tree(&source.join("testapp/src"), &testapp.join("src"));
    copy_tree(&source.join("testapp/tests"), &testapp.join("tests"));

    let mock = repository_root().join("tools/agentic-cli-mock");
    let target = context.join("agentic-cli-mock");
    std::fs::create_dir_all(&target).unwrap();
    for file in ["Cargo.toml", "Cargo.lock"] {
        std::fs::copy(mock.join(file), target.join(file)).unwrap();
    }
    copy_tree(&mock.join("src"), &target.join("src"));

    let scenario = json!({
        "version": 1,
        "expected": { "required_args": [] },
        "response": {
            "stdout": { "encoding": "utf8", "value": valid_claude_stdout() },
            "stderr": "",
            "exit_code": 0
        }
    });
    std::fs::write(context.join("scenario.template.json"), scenario.to_string()).unwrap();
}

/// The settings of one shape, written through the production stores: the
/// Docker backend verified against the real daemon (GSS-FR-38), the isolation
/// (GSS-FR-YDKS), the project's own image (PSS-FR-22), and the
/// Docker-in-Docker settings (PSS-FR-DIND).
fn store_settings(
    shape: &Shape,
    harness: &Harness,
    root: &crate::fs::RootFs,
    docker: &str,
    socket_path: &str,
) {
    crate::docker::verify_docker_backend_with(
        &Silent,
        &crate::logging::BUFFER,
        &harness.store,
        &DockerBackendSeams::default(),
        DockerBackendConfig {
            mode: DockerBackendMode::DockerCli,
            endpoint: DockerEndpoint::Automatic,
            cli_path: Some(docker.to_string()),
        },
    )
    .expect("the Docker backend verifies");
    harness
        .store
        .save_isolation(&IsolationRecord {
            mode: shape.isolation,
            cpus: Some(2),
            memory_gib: Some(4),
            disk_gib: Some(32),
            ..IsolationRecord::default()
        })
        .expect("the isolation saves");
    crate::project_settings::save_project_vendor_image_to(
        root,
        "claude_code",
        &ProjectVendorImage {
            image_name: format!("synthesis-dind-uat-{}", shape.name),
            tag: Some("test".to_string()),
            dockerfile: Some(format!("{CONTEXT_DIR}/Dockerfile.{}", shape.name)),
            build_context: Some(CONTEXT_DIR.to_string()),
        },
    )
    .expect("the project image saves");
    crate::project_settings::save_docker_in_docker_settings_to(
        root,
        DockerInDockerSettings {
            enabled: true,
            socket_path: socket_path.to_string(),
            host_gateway: true,
            under_microsandbox: shape.under_microsandbox,
        },
    )
    .expect("the Docker-in-Docker settings save");
}

/// Keeps the last build output, so a failed build says why.
#[derive(Default)]
struct BuildLog(StdMutex<Vec<String>>);

impl images::BuildEvents for BuildLog {
    fn progress(&self, payload: images::ImageBuildProgress) {
        let mut lines = self.0.lock().unwrap();
        lines.push(payload.message);
        let excess = lines.len().saturating_sub(40);
        lines.drain(..excess);
    }

    fn finished(&self, _payload: images::ImageBuildFinished) {}
}

/// PSS-FR-26: the image of the shape, built through the project image build.
fn build_image(harness: &Harness, root: &crate::fs::RootFs, shape: &Shape) -> Result<(), String> {
    let registry = ImageBuildRegistry::default();
    let operation = format!("dind-uat-build-{}", shape.number);
    let prepared = crate::project_settings::build_seam::prepare_project_vendor_image_build(
        root,
        &harness.store,
        "claude_code",
        &operation,
        &registry,
    )?;
    let log = BuildLog::default();
    let builder = crate::project_settings::image_builder::RealImageBuilder;
    match images::run_prepared_build(prepared, &builder, &log, &registry) {
        BuildTerminal::Succeeded => Ok(()),
        BuildTerminal::Failed(diagnostic) => {
            Err(format!("{diagnostic}; last output: {}", log.0.lock().unwrap().join(" | ")))
        }
        BuildTerminal::Cancelled => Err("the build was cancelled".to_string()),
    }
}

/// What one shape found: the failed step and its cause, or nothing.
type Verdict = Result<(), (String, String)>;

fn failed(step: &str, cause: impl Into<String>) -> Verdict {
    Err((step.to_string(), cause.into()))
}

/// The value of `name` in the result, or empty.
fn found<'a>(result: &'a serde_json::Value, name: &str) -> &'a str {
    result[name].as_str().unwrap_or("")
}

/// EAC-FR-BWTF, EAC-FR-LJXE: the verdict of one result. The checks of every
/// shape first, then the facts of the shape.
fn judge(shape: &Shape, result: &serde_json::Value, host_daemon: &str) -> Verdict {
    for check in CHECKS {
        if found(result, check) != "pass" {
            return failed(check, found(result, &format!("{check}_said")));
        }
    }
    // EAC-FR-GKSW, EAC-FR-QMRW: the agent holds the group of the socket it
    // sees. On a Linux host the host group also works, so this is what tells
    // the group a container sees from a fallback that happens to work.
    let socket_gid = found(result, "socket_gid");
    if !found(result, "groups").split_whitespace().any(|group| group == socket_gid) {
        return failed(
            "socket_group",
            format!("groups {:?} miss the socket group {socket_gid:?}", found(result, "groups")),
        );
    }
    if shape.host_socket() {
        let socket = found(result, "socket");
        if !socket.starts_with('s') || !socket.ends_with("/var/run/docker.sock") {
            return failed("socket", format!("no socket at /var/run/docker.sock: {socket}"));
        }
        let inspect = found(result, "self_inspect");
        if !inspect.contains("privileged=false")
            || !inspect.contains("cap_add=null")
            || !inspect.contains("cap_drop=[\"ALL\"]")
        {
            return failed(
                "privileges",
                format!("the container is not as EAC-FR-13 sets it: {inspect}"),
            );
        }
        if found(result, "cap_eff") != "0000000000000000" {
            return failed(
                "privileges",
                format!("capabilities in effect: {}", found(result, "cap_eff")),
            );
        }
        let host = found(result, "testcontainers_host");
        if host != "host.docker.internal" {
            return failed("host_gateway", format!("Postgres was reached through {host:?}"));
        }
    } else {
        if found(result, "socket_mounts") != "0" {
            return failed("no_host_socket", "a Docker socket is mounted in the VM");
        }
        if found(result, "docker_host") != "unix:///var/run/docker.sock" {
            return failed(
                "vm_daemon",
                format!("DOCKER_HOST is {:?}", found(result, "docker_host")),
            );
        }
        let daemon = found(result, "daemon_id");
        if daemon.is_empty() || daemon == host_daemon {
            return failed("vm_daemon", "the VM reached the daemon of the host");
        }
    }
    Ok(())
}

/// The host path of the Unix socket of the current Docker context, or `None`
/// where the context names no Unix socket.
fn context_socket_path(docker: &str) -> Option<String> {
    let output = std::process::Command::new(docker)
        .args(["context", "inspect", "--format", "{{.Endpoints.docker.Host}}"])
        .output()
        .ok()?;
    let host = String::from_utf8_lossy(&output.stdout).trim().to_string();
    host.strip_prefix("unix://").map(str::to_string)
}

/// The highest sequence in the log buffer now, so a later read sees only what
/// came after it.
fn last_log_sequence() -> u64 {
    let page =
        log_buffer().query(&crate::logging::LogFilter::default(), None, 50_000).expect("query");
    page.records.last().map_or(0, |record| record.sequence)
}

/// EAC-FR-QWNT: the cause of a helper fallback logged after `after`, if any.
fn helper_fallback_since(after: u64) -> Option<String> {
    let page =
        log_buffer().query(&crate::logging::LogFilter::default(), None, 50_000).expect("query");
    page.records
        .iter()
        .filter(|record| record.sequence > after)
        .find(|record| record.message.contains("the host group is used"))
        .map(|record| serde_json::to_string(&record.fields).unwrap_or_default())
}

/// The ID of the host daemon, so shape 3 can tell its own daemon from it.
fn host_daemon_id(docker: &str) -> String {
    let output = std::process::Command::new(docker)
        .args(["info", "--format", "{{.ID}}"])
        .output()
        .expect("docker info runs");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Runs one shape and prints its result line: PASS, SKIP, or FAIL with the
/// failed step and its cause.
fn run_shape(shape: &Shape) -> Verdict {
    if !shape.host_socket() {
        if let Some(reason) = crate::isolation::read_host().unsupported_reason {
            println!(
                "SKIP shape {}: Microsandbox cannot run on this host: {reason:?}",
                shape.number
            );
            return Ok(());
        }
    }
    let verdict = launch_shape(shape);
    match &verdict {
        Ok(()) => println!("PASS shape {} ({})", shape.number, shape.name),
        Err((step, cause)) => {
            println!("FAIL shape {} ({}): {step}: {cause}", shape.number, shape.name)
        }
    }
    verdict
}

fn launch_shape(shape: &Shape) -> Verdict {
    let docker = docker_cli();
    let socket_path = if shape.context_socket {
        match context_socket_path(&docker) {
            Some(path) => path,
            None => return failed("socket_path", "the Docker context names no Unix socket"),
        }
    } else {
        String::new()
    };
    if shape.context_socket {
        println!("shape {} stores the socket path {socket_path}", shape.number);
    }
    let harness = harness_for("claude_code");
    let project = tempfile::tempdir().unwrap();
    let root = crate::fs::RootFs::for_root(project.path());
    write_build_context(project.path());
    store_settings(shape, &harness, &root, &docker, &socket_path);
    if let Err(cause) = build_image(&harness, &root, shape) {
        return failed("image_build", cause);
    }

    let checkout = tempfile::tempdir().unwrap();
    repository(checkout.path(), &harness.workspace(), &format!("uat{}", shape.number), "uat\n");
    let pool = real_pool();
    let rt = runtime();
    let _stop = StopOnDrop(&rt, pool.clone());
    // EAC-FR-SNMS: the runtime from the verified backend and the isolation
    // from the store, as in production. Only the pool and the exporter are
    // bound, so the VMs of this test use a runtime home of their own.
    let executor = AgentCliExecutor::default()
        .with_session_state_root(harness.sessions_root())
        .with_sandbox_pool(pool.clone())
        .with_image_exporter(Arc::new(HostDockerCli::with_program(docker.clone())));
    // EAC-FR-DKRS: the settings and the image source, read as a graduation
    // launch reads them.
    let (docker_in_docker, images) = project_launch_inputs(root.clone());
    let context = LaunchContext { images: &images, docker_in_docker, ..harness.context() };
    let logged_before = last_log_sequence();
    let mut turn = task("uat");
    turn.execution.timeout_ms = TURN_TIMEOUT_MS;
    let launched = rt.block_on(executor.execute_agent_cli(
        &Silent,
        &context,
        AgentExecutionRequest {
            sandbox_scope: Some(probe_scope()),
            turn_kind: TurnKind::Work,
            execution_directory: harness.workspace(),
            task: turn,
            cancellation: CancellationToken::new(),
            activity: None,
            durable_output: None,
            supplementary_mount: None,
        },
    ));
    let execution = match launched {
        Ok(execution) => execution,
        Err(error) => return failed("launch", format!("{error:?}")),
    };
    let text = match std::fs::read_to_string(harness.workspace().join(".uat/result.json")) {
        Ok(text) => text,
        Err(error) => {
            return failed(
                "result",
                format!(
                    "no result ({error}); outcome {:?}; stderr {}",
                    execution.process_outcome,
                    String::from_utf8_lossy(&execution.stderr.bytes)
                ),
            )
        }
    };
    let result: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => return failed("result", format!("not JSON ({error}): {text}")),
    };
    eprintln!("shape {} result: {result}", shape.number);
    if execution.process_outcome != ProcessOutcome::Completed {
        return failed("turn", format!("the turn ended as {:?}", execution.process_outcome));
    }
    if let Some(cause) = helper_fallback_since(logged_before) {
        return failed("socket_group", format!("the helper gave no group: {cause}"));
    }
    judge(shape, &result, &host_daemon_id(&docker))
}

fn assert_shape(shape: &Shape) {
    if let Err((step, cause)) = run_shape(shape) {
        panic!("shape {} failed at {step}: {cause}", shape.number);
    }
}

// EAC-FR-UDQN, EAC-FR-BWTF, EAC-FR-LJXE, EAC-FR-DKRS, EAC-FR-GKSW: isolation
// None with Docker-in-Docker on is a Docker launch with the host socket, and
// the agent reaches the host daemon through it.
#[test]
#[ignore = "needs a Docker daemon and the public registry"]
fn dind_uat_shape1_none_host_socket() {
    assert_shape(&SHAPE_1);
}

// EAC-FR-LJXE, EAC-FR-DKRS: with the socket path of the Docker context stored
// (on Docker Desktop `~/.docker/run/docker.sock`), the container still sees the
// socket at `/var/run/docker.sock` and reaches the daemon through it.
#[test]
#[ignore = "needs a Docker daemon and the public registry"]
fn dind_uat_shape1_context_socket_path() {
    assert_shape(&SHAPE_1_CONTEXT_SOCKET);
}

// EAC-FR-UDQN, EAC-FR-BWTF, EAC-FR-LJXE, EAC-FR-SNMS: Microsandbox with
// `host_socket` gives up the VM and is the same Docker launch.
#[test]
#[ignore = "needs a Docker daemon and the public registry"]
fn dind_uat_shape2_microsandbox_host_socket() {
    assert_shape(&SHAPE_2);
}

// EAC-FR-UDQN, EAC-FR-BWTF, EAC-FR-LJXE, EAC-FR-QMRW, EAC-FR-BJLI: Microsandbox
// with `vm_daemon` stays in the VM, which runs its own daemon and mounts no
// host socket. A host that cannot run Microsandbox reports a skip.
#[test]
#[ignore = "needs a Docker daemon, the public registry, and hardware virtualisation"]
fn dind_uat_shape3_microsandbox_vm_daemon() {
    assert_shape(&SHAPE_3);
}

// EAC-FR-BWTF, EAC-FR-LJXE: the verdict names the first failed step with its
// own words, and holds each shape to its own facts. This test runs by default:
// it reads a result and starts nothing.
#[test]
fn dind_uat_verdict_names_the_failed_step() {
    let passing_socket = json!({
        "docker_version": "pass", "docker_run": "pass", "testcontainers": "pass", "browser": "pass",
        "groups": "20 0", "socket_gid": "0",
        "socket": "srw-rw---- 1 0 0 0 Nov 6 09:25 /var/run/docker.sock",
        "self_inspect": "privileged=false cap_add=null cap_drop=[\"ALL\"] mounts=/var/run/docker.sock,",
        "cap_eff": "0000000000000000",
        "testcontainers_host": "host.docker.internal",
    });
    assert_eq!(judge(&SHAPE_1, &passing_socket, "host"), Ok(()));
    assert_eq!(judge(&SHAPE_2, &passing_socket, "host"), Ok(()));

    let mut no_cli = passing_socket.clone();
    no_cli["docker_version"] = json!("fail");
    no_cli["docker_version_said"] = json!("docker: command not found");
    assert_eq!(
        judge(&SHAPE_1, &no_cli, "host"),
        failed("docker_version", "docker: command not found")
    );

    let mut privileged = passing_socket.clone();
    privileged["self_inspect"] = json!("privileged=true cap_add=null cap_drop=[\"ALL\"]");
    assert_eq!(judge(&SHAPE_1, &privileged, "host").unwrap_err().0, "privileges");

    let mut bridge = passing_socket.clone();
    bridge["testcontainers_host"] = json!("172.17.0.1");
    assert_eq!(judge(&SHAPE_1, &bridge, "host").unwrap_err().0, "host_gateway");

    let vm = json!({
        "docker_version": "pass", "docker_run": "pass", "testcontainers": "pass", "browser": "pass",
        "groups": "20", "socket_gid": "20",
        "socket_mounts": "0",
        "docker_host": "unix:///var/run/docker.sock",
        "daemon_id": "vm-daemon",
    });
    assert_eq!(judge(&SHAPE_3, &vm, "host-daemon"), Ok(()));
    assert_eq!(judge(&SHAPE_3, &vm, "vm-daemon").unwrap_err().0, "vm_daemon");
    let mut mounted = vm.clone();
    mounted["socket_mounts"] = json!("1");
    assert_eq!(judge(&SHAPE_3, &mounted, "host-daemon").unwrap_err().0, "no_host_socket");

    // The first failed check is the one named.
    let mut two = passing_socket.clone();
    two["docker_run"] = json!("fail");
    two["browser"] = json!("fail");
    assert_eq!(judge(&SHAPE_1, &two, "host").unwrap_err().0, "docker_run");

    // A fallback group that gives no access to the socket the agent sees.
    let mut host_group_only = passing_socket.clone();
    host_group_only["groups"] = json!("20");
    assert_eq!(judge(&SHAPE_1, &host_group_only, "host").unwrap_err().0, "socket_group");

    let mut no_socket = passing_socket.clone();
    no_socket["socket"] = json!("ls: cannot access '/var/run/docker.sock'");
    assert_eq!(judge(&SHAPE_1, &no_socket, "host").unwrap_err().0, "socket");

    let mut capable = passing_socket.clone();
    capable["cap_eff"] = json!("00000000a80425fb");
    assert_eq!(judge(&SHAPE_1, &capable, "host").unwrap_err().0, "privileges");

    let mut no_host = vm.clone();
    no_host["docker_host"] = json!("");
    assert_eq!(judge(&SHAPE_3, &no_host, "host-daemon").unwrap_err().0, "vm_daemon");

    let mut no_daemon = vm.clone();
    no_daemon["daemon_id"] = json!("");
    assert_eq!(judge(&SHAPE_3, &no_daemon, "host-daemon").unwrap_err().0, "vm_daemon");
}
