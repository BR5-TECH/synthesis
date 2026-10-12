//! EAC-FR-DKRS, EAC-FR-HWQN, EAC-FR-13, EAC-FR-14, EAC-FR-39, EAC-FR-OWPP —
//! Docker-in-Docker adds the socket mount, the socket group and the host
//! gateway, and nothing else.

use super::*;

/// A harness whose project enables Docker-in-Docker with a real Unix socket.
/// The listener is returned so the socket outlives the launch.
#[cfg(unix)]
fn enabled_harness(
    host_gateway: bool,
) -> (Harness, std::os::unix::net::UnixListener, tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("docker.sock");
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let socket = path.to_str().unwrap().to_string();
    let mut harness = harness_for("claude_code");
    harness.docker_in_docker =
        DockerInDockerLaunch { enabled: true, socket_path: socket.clone(), host_gateway };
    (harness, listener, dir, socket)
}

fn flag_values(argv: &[String], flag: &str) -> Vec<String> {
    argv.windows(2).filter(|pair| pair[0] == flag).map(|pair| pair[1].clone()).collect()
}

/// EAC-FR-DKRS, EAC-FR-13: a project that leaves Docker-in-Docker off launches
/// the container it always launched — no socket, no group, no host mapping.
#[test]
fn a_launch_without_docker_in_docker_adds_nothing() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    run(&harness, runtime.clone(), task("go")).expect("runs");
    let recorded = runtime.only_run();

    assert!(flag_values(&recorded.argv, "--group-add").is_empty());
    assert!(flag_values(&recorded.argv, "--add-host").is_empty());
    assert!(!recorded.argv.iter().any(|a| a.contains("docker.sock")));
    assert!(!recorded.argv.iter().any(|a| a.contains("TESTCONTAINERS_HOST_OVERRIDE")));
    let container = recorded.container.expect("the specification");
    assert!(container.groups.is_empty());
    assert!(container.extra_hosts.is_empty());
}

/// EAC-FR-DKRS, EAC-FR-13, EAC-FR-14: the enabled launch mounts the socket at
/// its fixed target, adds the socket's group, and changes nothing about the user,
/// the privileges or the security options.
#[cfg(unix)]
#[test]
fn an_enabled_launch_adds_the_socket_and_its_group_only() {
    use std::os::unix::fs::MetadataExt;

    let (harness, _listener, _dir, socket) = enabled_harness(false);
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    run(&harness, runtime.clone(), task("go")).expect("runs");
    let recorded = runtime.only_run();

    let mounts = flag_values(&recorded.argv, "--mount");
    assert!(
        mounts.contains(&format!("type=bind,source={socket},target=/var/run/docker.sock")),
        "{mounts:?}"
    );
    // The workspace, the session directory and the socket.
    assert_eq!(mounts.len(), 3, "{mounts:?}");

    let gid = std::fs::metadata(&socket).unwrap().gid();
    assert_eq!(flag_values(&recorded.argv, "--group-add"), vec![gid.to_string()]);
    assert!(flag_values(&recorded.argv, "--add-host").is_empty());
    assert!(!recorded.argv.iter().any(|a| a.contains("TESTCONTAINERS_HOST_OVERRIDE")));

    let (uid, host_gid) = descriptor::host_ids();
    assert_eq!(flag_values(&recorded.argv, "--user"), vec![format!("{uid}:{host_gid}")]);
    assert!(!recorded.argv.iter().any(|a| a == "--privileged"));
    assert!(!recorded.argv.iter().any(|a| a == "--cap-add"));
    assert_eq!(flag_values(&recorded.argv, "--cap-drop"), vec!["ALL".to_string()]);
    assert_eq!(
        flag_values(&recorded.argv, "--security-opt"),
        vec!["no-new-privileges".to_string()]
    );
}

/// EAC-FR-DKRS: the host gateway toggle adds the mapping and the Testcontainers
/// variable, and only then.
#[cfg(unix)]
#[test]
fn the_host_gateway_adds_the_mapping_and_the_testcontainers_variable() {
    let (harness, _listener, _dir, _socket) = enabled_harness(true);
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    run(&harness, runtime.clone(), task("go")).expect("runs");
    let recorded = runtime.only_run();

    assert_eq!(
        flag_values(&recorded.argv, "--add-host"),
        vec!["host.docker.internal:host-gateway".to_string()]
    );
    assert!(flag_values(&recorded.argv, "--env")
        .contains(&"TESTCONTAINERS_HOST_OVERRIDE=host.docker.internal".to_string()));
}

/// EAC-FR-39, EAC-FR-DKRS: the Engine backend creates the same container the
/// CLI backend renders — the same socket mount, group and host mapping.
#[cfg(unix)]
#[test]
fn both_backends_produce_the_same_container() {
    let (harness, _listener, _dir, socket) = enabled_harness(true);
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    run(&harness, runtime.clone(), task("go")).expect("runs");
    let recorded = runtime.only_run();
    let spec = recorded.container.expect("the specification");

    let config = super::super::runtime::engine_config(
        &spec,
        super::super::runtime::engine_env(&spec, &BTreeMap::new()),
    );
    let host = config.host_config.expect("host config");
    assert_eq!(host.group_add, Some(spec.group_add()));
    assert_eq!(host.group_add.as_ref().map(Vec::len), Some(1));
    assert_eq!(host.extra_hosts, Some(vec!["host.docker.internal:host-gateway".to_string()]));
    let mounts = host.mounts.expect("mounts");
    assert!(mounts.iter().any(|m| m.source.as_deref() == Some(socket.as_str())
        && m.target.as_deref() == Some("/var/run/docker.sock")
        && m.read_only == Some(false)));
    assert_eq!(host.cap_drop, Some(vec!["ALL".to_string()]));
    assert_eq!(host.privileged, None);
    assert_eq!(host.cap_add, None);
    let env = config.env.expect("env");
    assert!(env.contains(&"TESTCONTAINERS_HOST_OVERRIDE=host.docker.internal".to_string()));
    assert_eq!(config.user, Some(spec.user()));

    // And a launch without Docker-in-Docker leaves both fields unset.
    let plain = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    run(&plain, runtime.clone(), task("go")).expect("runs");
    let spec = runtime.only_run().container.expect("the specification");
    let host = super::super::runtime::engine_config(&spec, vec![]).host_config.unwrap();
    assert_eq!(host.group_add, None);
    assert_eq!(host.extra_hosts, None);
}

/// EAC-FR-HWQN: a socket that is not there fails the launch naming the path,
/// and no container is created.
#[test]
fn a_missing_socket_fails_the_launch_naming_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gone.sock").to_str().unwrap().to_string();
    let mut harness = harness_for("claude_code");
    harness.docker_in_docker =
        DockerInDockerLaunch { enabled: true, socket_path: path.clone(), host_gateway: false };
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());

    match run(&harness, runtime.clone(), task("go")) {
        Err(AgentExecutionError::DockerSocketUnavailable(named)) => assert_eq!(named, path),
        other => panic!("expected DockerSocketUnavailable, got {other:?}"),
    }
    assert_eq!(runtime.launched(), 0, "no container is created");
}

/// EAC-FR-HWQN: the socket is checked at each launch, so a restored socket
/// lets the next launch run.
#[cfg(unix)]
#[test]
fn a_restored_socket_lets_the_next_launch_run() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("docker.sock");
    let mut harness = harness_for("claude_code");
    harness.docker_in_docker = DockerInDockerLaunch {
        enabled: true,
        socket_path: path.to_str().unwrap().to_string(),
        host_gateway: false,
    };
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    assert!(run(&harness, runtime.clone(), task("go")).is_err());

    let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    run(&harness, runtime.clone(), task("go")).expect("runs once the socket is back");
    assert_eq!(runtime.launched(), 1);
}

/// EAC-FR-OWPP, EAC-FR-DKRS: a launch-environment entry that names the
/// Testcontainers variable does not override the executor's own value.
#[cfg(unix)]
#[test]
fn the_launch_environment_cannot_override_the_testcontainers_variable() {
    let (mut harness, _listener, _dir, _socket) = enabled_harness(true);
    let config = VerifyConfig {
        path: Some("/usr/bin/claude".into()),
        auth_mode: Some("custom_gateway".into()),
        gateway_base_url: Some("https://gateway.example".into()),
        gateway_token_var: Some("ANTHROPIC_API_KEY".into()),
        gateway_token: Some("gw-FAKE-TEST-TOKEN-NOT-A-CREDENTIAL".into()),
        env_vars: Some(vec!["TESTCONTAINERS_HOST_OVERRIDE=attacker.example".into()]),
        ..Default::default()
    };
    verify_integration_impl(&harness.store, &harness.ai, "claude_code", &config)
        .expect("gateway verifies");
    harness.docker_in_docker.host_gateway = true;

    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    run(&harness, runtime.clone(), task("go")).expect("runs");
    let recorded = runtime.only_run();

    let overrides: Vec<&String> =
        recorded.argv.iter().filter(|a| a.contains("TESTCONTAINERS_HOST_OVERRIDE")).collect();
    assert_eq!(overrides, vec!["TESTCONTAINERS_HOST_OVERRIDE=host.docker.internal"]);
    assert!(!recorded.env.contains_key("TESTCONTAINERS_HOST_OVERRIDE"));
}

/// EAC-FR-DKRS, EAC-FR-40: the model chooses none of it — the task a model
/// writes has no field that names a mount, a group, a host or the socket.
#[test]
fn no_task_field_can_name_the_socket_a_group_or_a_host() {
    let source = include_str!("../protocol.rs");
    let task_struct = source
        .split("pub struct AgentTaskRequest {")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .expect("the task struct");
    for word in ["socket", "group", "host", "mount", "docker"] {
        assert!(!task_struct.to_lowercase().contains(word), "the task names {word}");
    }
}

/// EAC-FR-DKRS, EAC-FR-41: a masked turn gets none of the four additions, and a
/// missing socket does not fail it, because the check does not run for it.
#[test]
fn a_masked_turn_gets_none_of_the_additions() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = harness_for("claude_code");
    harness.docker_in_docker = DockerInDockerLaunch {
        enabled: true,
        socket_path: dir.path().join("gone.sock").to_str().unwrap().to_string(),
        host_gateway: true,
    };
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    run_as(&harness, runtime.clone(), None, None, super::super::TurnKind::SemanticRebase)
        .expect("a masked turn does not need the socket");
    let recorded = runtime.only_run();

    assert!(flag_values(&recorded.argv, "--group-add").is_empty());
    assert!(flag_values(&recorded.argv, "--add-host").is_empty());
    assert!(!recorded.argv.iter().any(|a| a.contains("docker.sock")));
    assert!(!recorded.argv.iter().any(|a| a.contains("TESTCONTAINERS_HOST_OVERRIDE")));
}
