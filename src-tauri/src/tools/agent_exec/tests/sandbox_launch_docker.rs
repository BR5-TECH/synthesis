//! Docker-in-Docker under Microsandbox (EAC-FR-SNMS, EAC-FR-DARS,
//! EAC-FR-QMRW, EAC-FR-ZPTA, EAC-FR-DKRS, EAC-FR-HWQN, EAC-FR-OWPP).

use super::*;
use crate::project_settings::UnderMicrosandbox;

fn dind(socket_path: &str, under_microsandbox: UnderMicrosandbox) -> DockerInDockerLaunch {
    DockerInDockerLaunch {
        enabled: true,
        socket_path: socket_path.to_string(),
        host_gateway: true,
        under_microsandbox,
    }
}

fn work(
    harness: &Harness,
    docker: Arc<RecordingRuntime>,
    sandbox: Arc<RecordingSandbox>,
) -> Result<AgentExecution, AgentExecutionError> {
    launch(harness, docker, sandbox, microsandbox(), scope(), TurnKind::Work)
}

/// EAC-FR-DARS, EAC-FR-QMRW, EAC-FR-BJLI, EAC-FR-UEFI: with the daemon choice,
/// the VM boots with the daemon option and starts the daemon with the host
/// user's GID; the process gets `DOCKER_HOST` and none of the host socket
/// additions, and a missing host socket refuses nothing.
#[test]
fn the_daemon_choice_keeps_the_turn_in_the_vm_with_its_own_daemon() {
    let mut harness = harness_for("claude_code");
    harness.docker_in_docker = dind("/does/not/exist.sock", UnderMicrosandbox::VmDaemon);
    let docker = RecordingRuntime::replying("");
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    work(&harness, docker.clone(), sandbox.clone()).expect("the host socket is not needed");

    assert_eq!(docker.launched(), 0);
    let spec = sandbox.only_boot();
    assert!(spec.docker_daemon);
    assert!(spec.mounts.iter().all(|m| !m.target.contains("docker.sock")));
    let process = sandbox.only_process();
    let gid = process.user.split_once(':').unwrap().1.parse::<u32>().unwrap();
    assert_eq!(*sandbox.daemons.lock().unwrap(), vec![gid]);
    assert_eq!(
        process.env.get(descriptor::DOCKER_HOST_ENV).map(String::as_str),
        Some(descriptor::VM_DOCKER_HOST_VALUE)
    );
    assert!(!process.env.contains_key(descriptor::TESTCONTAINERS_HOST_OVERRIDE_ENV));
}

/// EAC-FR-DARS, EAC-FR-QMRW: with Docker-in-Docker off, the VM gets no daemon
/// and the process no `DOCKER_HOST`.
#[test]
fn a_vm_without_docker_in_docker_has_no_docker_access() {
    let harness = harness_for("claude_code");
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    work(&harness, RecordingRuntime::replying(""), sandbox.clone()).expect("runs");
    assert!(!sandbox.only_boot().docker_daemon);
    assert!(sandbox.daemons.lock().unwrap().is_empty());
    assert!(!sandbox.only_process().env.contains_key(descriptor::DOCKER_HOST_ENV));
}

/// EAC-FR-ZPTA, EAC-FR-NCSA: a daemon that is not available fails the launch
/// as `docker_daemon_unavailable`, runs no process, and creates no container.
#[test]
fn a_missing_daemon_is_a_typed_failure_with_no_fallback() {
    let mut harness = harness_for("claude_code");
    harness.docker_in_docker = dind("", UnderMicrosandbox::VmDaemon);
    let docker = RecordingRuntime::replying(&valid_claude_stdout());
    let sandbox = Arc::new(RecordingSandbox {
        daemon_error: Some("dockerd is not in the image (exit 127)".to_string()),
        ..Arc::try_unwrap(RecordingSandbox::replying("")).ok().unwrap()
    });
    let error = work(&harness, docker.clone(), sandbox.clone()).unwrap_err();
    assert!(matches!(
        error,
        AgentExecutionError::SandboxLaunchFailed(ref failure)
            if failure.cause == SandboxFailureCause::DockerDaemonUnavailable
                && failure.message == "dockerd is not in the image (exit 127)"
    ));
    assert!(sandbox.processes.lock().unwrap().is_empty());
    assert_eq!(docker.launched(), 0);
}

/// EAC-FR-SNMS, EAC-FR-DKRS: with the host socket choice, a graduation turn
/// under Microsandbox is a Docker launch with the socket, its group and the
/// host gateway, and boots no VM.
#[cfg(unix)]
#[test]
fn the_host_socket_choice_is_a_docker_launch_with_the_socket() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("docker.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let mut harness = harness_for("claude_code");
    harness.docker_in_docker = dind(path.to_str().unwrap(), UnderMicrosandbox::HostSocket);
    let docker = RecordingRuntime::replying(&valid_claude_stdout());
    // EAC-FR-GKSW: the Docker launch reads the group a container sees.
    docker.answer_socket_group(Ok(4242));
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    work(&harness, docker.clone(), sandbox.clone()).expect("runs");

    assert!(sandbox.boots.lock().unwrap().is_empty());
    assert_eq!(docker.socket_probes().len(), 1);
    let recorded = docker.only_run();
    assert!(
        recorded.argv.iter().any(|a| a.contains("/var/run/docker.sock")),
        "{:?}",
        recorded.argv
    );
    let container = recorded.container.expect("the specification");
    assert_eq!(container.groups, vec![4242]);
    assert_eq!(container.extra_hosts, vec![descriptor::DOCKER_HOST_GATEWAY_MAPPING.to_string()]);
    assert!(!recorded.argv.iter().any(|a| a.contains(descriptor::DOCKER_HOST_ENV)));
}

/// EAC-FR-SNMS, EAC-FR-HWQN: with the host socket choice, the socket check runs
/// for the Docker launch, so a missing socket fails it before any container.
#[test]
fn the_host_socket_choice_checks_the_socket() {
    let dir = tempfile::tempdir().unwrap();
    let gone = dir.path().join("gone.sock");
    let mut harness = harness_for("claude_code");
    harness.docker_in_docker = dind(gone.to_str().unwrap(), UnderMicrosandbox::HostSocket);
    let docker = RecordingRuntime::replying(&valid_claude_stdout());
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    let error = work(&harness, docker.clone(), sandbox.clone()).unwrap_err();
    assert!(matches!(error, AgentExecutionError::DockerSocketUnavailable(_)), "{error:?}");
    assert_eq!(docker.launched(), 0);
    assert!(sandbox.boots.lock().unwrap().is_empty());
}

/// EAC-FR-SNMS: the host socket choice with the checkbox off keeps the turn in
/// the VM, with no Docker access.
#[test]
fn the_host_socket_choice_without_the_checkbox_stays_in_the_vm() {
    let mut harness = harness_for("claude_code");
    harness.docker_in_docker = DockerInDockerLaunch {
        enabled: false,
        ..dind("/does/not/exist.sock", UnderMicrosandbox::HostSocket)
    };
    let docker = RecordingRuntime::replying("");
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    work(&harness, docker.clone(), sandbox.clone()).expect("runs");
    assert_eq!(docker.launched(), 0);
    assert!(!sandbox.only_boot().docker_daemon);
    assert!(sandbox.only_boot().mounts.iter().all(|m| !m.target.contains("docker.sock")));
}

/// EAC-FR-SNMS, EAC-FR-DKRS, EAC-FR-41: a masked turn under the host socket
/// choice gets no socket, and a missing socket does not fail it.
#[test]
fn a_masked_turn_under_the_host_socket_choice_gets_no_socket() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = harness_for("claude_code");
    harness.docker_in_docker =
        dind(dir.path().join("gone.sock").to_str().unwrap(), UnderMicrosandbox::HostSocket);
    let docker = RecordingRuntime::replying(&valid_claude_stdout());
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    launch(&harness, docker.clone(), sandbox, microsandbox(), scope(), TurnKind::SemanticRebase)
        .expect("a masked turn does not need the socket");
    let recorded = docker.only_run();
    assert!(!recorded.argv.iter().any(|a| a.contains("docker.sock")));
    assert!(recorded.container.expect("the specification").groups.is_empty());
}

/// EAC-FR-DKRS, PSS-FR-MTJE, EAC-FR-SNMS, EAC-FR-38: the choice stored in
/// `local.toml` reaches the launch with the other values, through the reader a
/// graduation launch uses, and the image source gives the committed image.
#[test]
fn the_stored_choice_reaches_the_launch() {
    let dir = tempfile::tempdir().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    crate::project_settings::save_docker_in_docker_settings_to(
        &root,
        crate::project_settings::DockerInDockerSettings {
            enabled: true,
            socket_path: "/x/d.sock".to_string(),
            host_gateway: true,
            under_microsandbox: UnderMicrosandbox::HostSocket,
        },
    )
    .unwrap();
    std::fs::write(dir.path().join("Dockerfile"), "FROM scratch\n").unwrap();
    crate::project_settings::save_project_vendor_image_to(
        &root,
        "claude_code",
        &crate::project_settings::images::ProjectVendorImage {
            image_name: "example/agent".to_string(),
            tag: Some("7".to_string()),
            dockerfile: Some("Dockerfile".to_string()),
            build_context: None,
        },
    )
    .unwrap();
    let (launch, images) = project_launch_inputs(root);
    assert_eq!(launch, dind("/x/d.sock", UnderMicrosandbox::HostSocket));
    assert_eq!(images.image_for("claude_code"), Ok("example/agent:7".to_string()));
}

/// EAC-FR-OWPP, EAC-FR-QMRW: the launch environment cannot name `DOCKER_HOST`,
/// because the integration refuses it as reserved, so the process in the VM
/// always gets the executor's own value.
#[test]
fn the_launch_environment_cannot_override_docker_host_in_the_vm() {
    let mut harness = harness_for("claude_code");
    let config = VerifyConfig {
        path: Some("/usr/bin/claude".into()),
        auth_mode: Some("custom_gateway".into()),
        gateway_base_url: Some("https://gateway.example".into()),
        gateway_token_var: Some("ANTHROPIC_API_KEY".into()),
        gateway_token: Some("gw-FAKE-TEST-TOKEN-NOT-A-CREDENTIAL".into()),
        env_vars: Some(vec!["DOCKER_HOST=tcp://attacker.example:2375".into()]),
        ..Default::default()
    };
    let refused = verify_integration_impl(&harness.store, &harness.ai, "claude_code", &config);
    assert_eq!(refused.unwrap_err(), "env_var_reserved:DOCKER_HOST");
    harness.docker_in_docker = dind("", UnderMicrosandbox::VmDaemon);
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    let docker = RecordingRuntime::replying("");
    work(&harness, docker.clone(), sandbox.clone()).expect("runs");
    // EAC-FR-GKSW, EAC-FR-DARS: a sandbox launch reads no socket group.
    assert!(docker.socket_probes().is_empty());
    assert_eq!(
        sandbox.only_process().env.get(descriptor::DOCKER_HOST_ENV).map(String::as_str),
        Some(descriptor::VM_DOCKER_HOST_VALUE)
    );
}

/// EAC-FR-SNMS, EAC-FR-QYBJ: a launch that carries no scope is a Docker launch
/// whatever the choice, so it gets the host socket additions and no daemon.
#[cfg(unix)]
#[test]
fn a_launch_without_a_scope_is_a_docker_launch_with_the_socket() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("docker.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let mut harness = harness_for("claude_code");
    harness.docker_in_docker = dind(path.to_str().unwrap(), UnderMicrosandbox::VmDaemon);
    let docker = RecordingRuntime::replying(&valid_claude_stdout());
    let sandbox = RecordingSandbox::replying(&valid_claude_stdout());
    launch(&harness, docker.clone(), sandbox.clone(), microsandbox(), None, TurnKind::Work)
        .expect("runs");
    assert!(sandbox.boots.lock().unwrap().is_empty());
    let recorded = docker.only_run();
    assert!(recorded.argv.iter().any(|a| a.contains("/var/run/docker.sock")));
    assert!(!recorded.argv.iter().any(|a| a.contains(descriptor::DOCKER_HOST_ENV)));
}
