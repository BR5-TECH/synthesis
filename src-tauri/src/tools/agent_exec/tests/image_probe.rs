//! EAC-FR-WYYZ: `probe_agent_image` through the recording runtime.

use super::*;
use crate::isolation::{ResolvedIsolation, SandboxSettings};
use crate::project_settings::UnderMicrosandbox;
use crate::tools::agent_exec::image_probe::{
    ImageProbe, ImageProbeOutcome, ImageProbeRequest, ProbeCommand, PROBE_OUTPUT_LIMIT,
    PROBE_TIMEOUT,
};

fn microsandbox() -> ResolvedIsolation {
    ResolvedIsolation::Microsandbox(SandboxSettings {
        cpus: 2,
        memory_gib: 4,
        disk_gib: 32,
        block_local_network: true,
        allowed: Vec::new(),
        unsupported_reason: None,
    })
}

/// One probe through an executor bound to `runtime`, under `isolation`.
fn probe_under(
    runtime: Arc<RecordingRuntime>,
    harness: &Harness,
    isolation: ResolvedIsolation,
) -> Result<ImageProbeOutcome, AgentExecutionError> {
    let request = ImageProbeRequest {
        command: ProbeCommand::OpenSpecVersion,
        cancellation: CancellationToken::new(),
    };
    let executor = AgentCliExecutor::new(runtime).with_isolation(isolation);
    tauri::async_runtime::block_on(executor.probe_agent_image(&Silent, &harness.context(), request))
}

/// One probe under the isolation `none` that ran.
fn probe(
    runtime: Arc<RecordingRuntime>,
    harness: &Harness,
) -> Result<ImageProbe, AgentExecutionError> {
    match probe_under(runtime, harness, ResolvedIsolation::None)? {
        ImageProbeOutcome::Ran(probe) => Ok(probe),
        ImageProbeOutcome::Skipped => panic!("a probe under `none` is not skipped"),
    }
}

// EAC-FR-WYYZ, EAC-FR-38, EAC-FR-26: the probe runs `openspec --version` in
// one container of the project's image with no mount, no variable, no
// credential, and no network, bounded by 30 seconds, and removes the
// container. It answers the exit status and the output.
#[test]
fn the_probe_runs_one_bare_container_of_the_image_and_removes_it() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying("1.17.0\n");
    let answer = probe(runtime.clone(), &harness).expect("a probe");
    assert_eq!(answer.exit_status, Some(0));
    assert_eq!(answer.stdout, b"1.17.0\n");

    let run = runtime.only_run();
    let spec = run.container.expect("a container");
    assert!(spec.mounts.is_empty(), "no mount");
    assert!(spec.env_names.is_empty() && spec.env_literals.is_empty(), "no variable");
    assert!(run.env.is_empty(), "no credential reaches the client");
    assert!(run.stdin.is_empty());
    assert_eq!(spec.entrypoint.as_deref(), Some("openspec"));
    assert_eq!(spec.vendor_args, ["--version"]);
    assert_eq!(spec.network, descriptor::HELPER_NETWORK);
    assert_eq!(run.timeout, PROBE_TIMEOUT);
    assert_eq!(run.stdout_limit, PROBE_OUTPUT_LIMIT);
    assert!(run.final_event.is_none());
    assert_eq!(*runtime.removed.lock().unwrap(), vec![spec.name.clone()]);
}

// EAC-FR-WYYZ: the probe keeps the last 4 KiB of standard error.
#[test]
fn the_probe_keeps_the_last_four_kib_of_standard_error() {
    let harness = harness_for("claude_code");
    let long = format!("{}END", "x".repeat(6000));
    let runtime = RecordingRuntime::refused("", &long, 127);
    let answer = probe(runtime, &harness).expect("a probe");
    assert_eq!(answer.exit_status, Some(127));
    assert_eq!(answer.stderr.len(), PROBE_OUTPUT_LIMIT);
    assert!(answer.stderr.ends_with(b"END"));
}

// EAC-FR-WYYZ, EAC-FR-17: a runtime that does not answer starts no container.
#[test]
fn an_unavailable_runtime_starts_no_probe() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::broken(false, true, None);
    assert_eq!(probe(runtime.clone(), &harness), Err(AgentExecutionError::RuntimeUnavailable));
    assert_eq!(runtime.launched(), 0);
}

// OFP-FR-XBNJ, EAC-FR-17: a container that does not start is `LaunchFailed`,
// and the container is removed.
#[test]
fn a_container_that_does_not_start_is_launch_failed() {
    let harness = harness_for("claude_code");
    let runtime =
        RecordingRuntime::broken(true, true, Some(RuntimeError::LaunchFailed("no start".into())));
    assert_eq!(
        probe(runtime.clone(), &harness),
        Err(AgentExecutionError::LaunchFailed("no start".into()))
    );
}

// OFP-FR-RKMB: where the effective isolation is Microsandbox, the probe is
// skipped. It reaches no Docker runtime in place of the VM: a runtime that
// does not answer changes nothing, and no container is created.
#[test]
fn under_microsandbox_the_probe_is_skipped_and_reaches_no_docker() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::broken(false, false, None);
    assert_eq!(
        probe_under(runtime.clone(), &harness, microsandbox()),
        Ok(ImageProbeOutcome::Skipped)
    );
    assert_eq!(runtime.launched(), 0);
    assert!(runtime.images_asked.lock().unwrap().is_empty(), "no image is asked for");
}

// OFP-FR-RKMB, EAC-FR-SNMS: Docker-in-Docker with the host socket gives up the
// VM, so a turn is a Docker launch, and the probe runs in Docker.
#[test]
fn the_host_socket_choice_makes_the_probe_run_in_docker() {
    let mut harness = harness_for("claude_code");
    harness.docker_in_docker = DockerInDockerLaunch {
        enabled: true,
        under_microsandbox: UnderMicrosandbox::HostSocket,
        ..Default::default()
    };
    let runtime = RecordingRuntime::replying("1.17.0\n");
    let answer = probe_under(runtime.clone(), &harness, microsandbox()).expect("a probe");
    assert!(matches!(answer, ImageProbeOutcome::Ran(_)), "{answer:?}");
    assert_eq!(runtime.launched(), 1);
}

// OFP-FR-RKMB, EAC-FR-QMRW: Docker-in-Docker with a daemon in the VM keeps
// the VM, so the probe is skipped.
#[test]
fn the_vm_daemon_choice_keeps_the_skip() {
    let mut harness = harness_for("claude_code");
    harness.docker_in_docker = DockerInDockerLaunch {
        enabled: true,
        under_microsandbox: UnderMicrosandbox::VmDaemon,
        ..Default::default()
    };
    let runtime = RecordingRuntime::replying("1.17.0\n");
    assert_eq!(
        probe_under(runtime.clone(), &harness, microsandbox()),
        Ok(ImageProbeOutcome::Skipped)
    );
    assert_eq!(runtime.launched(), 0);
}
