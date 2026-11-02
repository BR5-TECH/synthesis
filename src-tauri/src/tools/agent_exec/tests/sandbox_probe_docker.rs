//! EAC-FR-RVQE / EAC-FR-QMRW / EAC-FR-ZPTA: the Docker daemon in a real VM.
//!
//! These probes check what the recording double cannot: that the guest kernel
//! of the sandbox runtime runs `dockerd` (overlay file system, cgroups,
//! netfilter), that the host user reaches it with no root, and that an image
//! with no `dockerd` fails with the typed cause.

use super::*;
use crate::project_settings::UnderMicrosandbox;
use crate::tools::agent_exec::sandbox::SandboxFailureCause;

/// The tag of the probe image with a Docker daemon. Test-only.
const DOCKER_PROBE_IMAGE: &str = "synthesis-sandbox-probe-docker:test";
const DOCKER_DOCKERFILE: &str = include_str!("sandbox_probe/Dockerfile.docker");

struct Images(&'static str);

impl VendorImageSource for Images {
    fn image_for(&self, _vendor: &str) -> Result<String, String> {
        Ok(self.0.to_string())
    }
}

static WITH_DOCKER: Images = Images(DOCKER_PROBE_IMAGE);
static WITHOUT_DOCKER: Images = Images(PROBE_IMAGE);

/// Build the probe image with a Docker daemon once per test process, on top of
/// the plain probe image.
fn build_docker_probe_image() {
    static BUILT: OnceLock<()> = OnceLock::new();
    build_probe_image();
    BUILT.get_or_init(|| {
        let context = tempfile::tempdir().expect("build context");
        std::fs::write(context.path().join("Dockerfile"), DOCKER_DOCKERFILE).unwrap();
        let built = std::process::Command::new("docker")
            .args(["build", "--quiet", "-t", DOCKER_PROBE_IMAGE])
            .arg(context.path())
            .status()
            .expect("docker is on PATH");
        assert!(built.success(), "the Docker probe image did not build");
    });
}

/// A VM with room for a daemon and its containers.
fn daemon_isolation() -> ResolvedIsolation {
    ResolvedIsolation::Microsandbox(SandboxSettings {
        cpus: 2,
        memory_gib: 2,
        disk_gib: 16,
        block_local_network: true,
        allowed: Vec::new(),
        unsupported_reason: None,
    })
}

fn daemon_harness() -> Harness {
    let mut harness = harness_for("claude_code");
    harness.docker_in_docker = DockerInDockerLaunch {
        enabled: true,
        socket_path: "/var/run/docker.sock".to_string(),
        host_gateway: false,
        under_microsandbox: UnderMicrosandbox::VmDaemon,
    };
    harness
}

async fn launch_with(
    executor: &AgentCliExecutor,
    harness: &Harness,
    images: &Images,
) -> Result<AgentExecution, AgentExecutionError> {
    let context = LaunchContext { images, ..harness.context() };
    executor
        .execute_agent_cli(
            &Silent,
            &context,
            AgentExecutionRequest {
                sandbox_scope: Some(probe_scope()),
                turn_kind: TurnKind::Work,
                execution_directory: harness.workspace(),
                task: task("probe"),
                cancellation: CancellationToken::new(),
                activity: None,
                durable_output: None,
                supplementary_mount: None,
            },
        )
        .await
}

// EAC-FR-RVQE, EAC-FR-QMRW, EAC-FR-KTWO: a VM booted with the daemon option
// runs `dockerd`; the turn, as the host user, reaches it through
// `DOCKER_HOST` and runs a container from a local image, also on the default
// bridge network.
#[test]
#[ignore = "needs hardware virtualisation and a Docker daemon"]
fn sandbox_real_daemon_runs_containers_in_the_vm() {
    build_docker_probe_image();
    let harness = daemon_harness();
    let root = tempfile::tempdir().unwrap();
    repository(root.path(), &harness.workspace(), "daemon", "one\n");
    let pool = real_pool();
    let rt = runtime();
    let _stop = StopOnDrop(&rt, pool.clone());
    let executor = executor(&harness, daemon_isolation(), &pool);
    let (uid, gid) = descriptor::host_ids();

    let execution = rt.block_on(launch_with(&executor, &harness, &WITH_DOCKER));
    let execution = execution.unwrap_or_else(|error| panic!("the daemon launch failed: {error:?}"));
    let text = std::fs::read_to_string(harness.workspace().join(".probe/result.json"))
        .unwrap_or_else(|error| {
            panic!(
                "no probe result ({error}); stderr {}",
                String::from_utf8_lossy(&execution.stderr.bytes)
            )
        });
    let result: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    eprintln!("docker probe result: {result}");

    assert_eq!(field(&result, "uid"), uid.to_string(), "{result}");
    assert_eq!(field(&result, "docker_host"), "unix:///var/run/docker.sock", "{result}");
    assert_eq!(field(&result, "docker_socket_group"), gid.to_string(), "{result}");
    assert_eq!(field(&result, "docker_run"), "container-ran", "{result}");
    assert_eq!(field(&result, "docker_bridge"), "bridge-ran", "{result}");
}

// EAC-FR-RVQE, EAC-FR-ZPTA, EAC-FR-NCSA: an image with no `dockerd` fails the
// launch with `docker_daemon_unavailable`, and the message says so.
#[test]
#[ignore = "needs hardware virtualisation and a Docker daemon"]
fn sandbox_real_an_image_without_dockerd_fails_with_its_cause() {
    build_probe_image();
    let harness = daemon_harness();
    let root = tempfile::tempdir().unwrap();
    repository(root.path(), &harness.workspace(), "nodaemon", "one\n");
    let pool = real_pool();
    let rt = runtime();
    let _stop = StopOnDrop(&rt, pool.clone());
    let executor = executor(&harness, daemon_isolation(), &pool);

    let error = rt.block_on(launch_with(&executor, &harness, &WITHOUT_DOCKER)).unwrap_err();
    match error {
        AgentExecutionError::SandboxLaunchFailed(failure) => {
            assert_eq!(failure.cause, SandboxFailureCause::DockerDaemonUnavailable);
            assert!(failure.message.contains("dockerd is not in the image"), "{}", failure.message);
        }
        other => panic!("expected a daemon failure, got {other:?}"),
    }
}
