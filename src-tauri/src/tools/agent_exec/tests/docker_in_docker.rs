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
    harness.docker_in_docker = DockerInDockerLaunch {
        enabled: true,
        socket_path: socket.clone(),
        host_gateway,
        ..Default::default()
    };
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

/// EAC-FR-DKRS, EAC-FR-13, EAC-FR-14, EAC-FR-GKSW: the enabled launch mounts the
/// socket at its fixed target, adds the group the socket has in a container, and
/// changes nothing about the user, the privileges or the security options.
#[cfg(unix)]
#[test]
fn an_enabled_launch_adds_the_socket_and_its_group_only() {
    let (harness, _listener, _dir, socket) = enabled_harness(false);
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    // EAC-FR-GKSW: Docker Desktop shows the socket as owned by group 0 in a
    // container. Group 0 is a group like any other here.
    runtime.answer_socket_group(Ok(0));
    run(&harness, runtime.clone(), task("go")).expect("runs");
    let recorded = runtime.only_run();

    let mounts = flag_values(&recorded.argv, "--mount");
    assert!(
        mounts.contains(&format!("type=bind,source={socket},target=/var/run/docker.sock")),
        "{mounts:?}"
    );
    // The workspace, the session directory and the socket.
    assert_eq!(mounts.len(), 3, "{mounts:?}");

    assert_eq!(flag_values(&recorded.argv, "--group-add"), vec!["0".to_string()]);
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
    harness.docker_in_docker = DockerInDockerLaunch {
        enabled: true,
        socket_path: path.clone(),
        host_gateway: false,
        ..Default::default()
    };
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
        ..Default::default()
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
        ..Default::default()
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

/// EAC-FR-GKSW, EAC-FR-DKRS: the launch adds the group that the socket has in a
/// container of the launch image, read with the image and the socket path of
/// the launch, and not the group of the host file.
#[cfg(unix)]
#[test]
fn the_launch_adds_the_group_a_container_sees() {
    let (harness, _listener, _dir, socket) = enabled_harness(false);
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    runtime.answer_socket_group(Ok(4242));
    run(&harness, runtime.clone(), task("go")).expect("runs");
    let recorded = runtime.only_run();

    assert_eq!(flag_values(&recorded.argv, "--group-add"), vec!["4242".to_string()]);
    assert_eq!(recorded.container.expect("the specification").groups, vec![4242]);
    let probes = runtime.socket_probes();
    assert_eq!(probes.len(), 1, "one helper read for one launch");
    let (uid, gid) = descriptor::host_ids();
    assert_eq!(probes[0].socket_source, socket);
    assert_eq!(probes[0].image, test_image_reference("claude_code"));
    assert_eq!((probes[0].host_uid, probes[0].host_gid), (uid, gid));
}

/// EAC-FR-QWNT: where the helper gives no group, the launch adds the group of
/// the host socket file and still runs.
#[cfg(unix)]
#[test]
fn a_failed_helper_falls_back_to_the_host_group() {
    use std::os::unix::fs::MetadataExt;

    let (harness, _listener, _dir, socket) = enabled_harness(false);
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    runtime.answer_socket_group(Err("stat: not found".to_string()));
    run(&harness, runtime.clone(), task("go")).expect("runs");

    let gid = std::fs::metadata(&socket).unwrap().gid();
    assert_eq!(flag_values(&runtime.only_run().argv, "--group-add"), vec![gid.to_string()]);
}

/// EAC-FR-GKSW: no helper runs where Docker-in-Docker is off or the turn is
/// masked.
#[test]
fn no_helper_runs_without_the_socket() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    runtime.answer_socket_group(Ok(1));
    run(&harness, runtime.clone(), task("go")).expect("runs");
    assert!(runtime.socket_probes().is_empty());

    let dir = tempfile::tempdir().unwrap();
    let mut masked = harness_for("claude_code");
    masked.docker_in_docker = DockerInDockerLaunch {
        enabled: true,
        socket_path: dir.path().join("gone.sock").to_str().unwrap().to_string(),
        ..Default::default()
    };
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    runtime.answer_socket_group(Ok(1));
    run_as(&masked, runtime.clone(), None, None, super::super::TurnKind::SemanticRebase)
        .expect("runs");
    assert!(runtime.socket_probes().is_empty());
}

/// EAC-FR-GKSW, EAC-FR-39: the Engine backend creates the same helper
/// container the CLI backend renders: `stat` as the program, no network, the
/// socket at its fixed target, and the security options of EAC-FR-13.
#[test]
fn both_backends_render_the_same_helper_container() {
    let probe = SocketGroupProbe {
        image: "example/agent:1".to_string(),
        socket_source: "/var/run/docker.sock".to_string(),
        host_uid: 501,
        host_gid: 20,
    };
    let spec = super::super::docker_in_docker::socket_group_spec(&probe);
    let argv = spec.docker_run_args();

    assert_eq!(flag_values(&argv, "--network"), vec!["none".to_string()]);
    assert_eq!(flag_values(&argv, "--entrypoint"), vec!["stat".to_string()]);
    assert_eq!(flag_values(&argv, "--user"), vec!["501:20".to_string()]);
    assert_eq!(
        flag_values(&argv, "--mount"),
        vec!["type=bind,source=/var/run/docker.sock,target=/var/run/docker.sock".to_string()]
    );
    assert_eq!(flag_values(&argv, "--cap-drop"), vec!["ALL".to_string()]);
    assert_eq!(flag_values(&argv, "--security-opt"), vec!["no-new-privileges".to_string()]);
    assert!(argv.iter().any(|a| a == "--rm"));
    assert!(flag_values(&argv, "--env").is_empty(), "no variable reaches the helper");
    assert!(flag_values(&argv, "--add-host").is_empty());
    assert!(flag_values(&argv, "--group-add").is_empty());
    assert!(!argv.iter().any(|a| a == "--privileged" || a == "--cap-add"));
    let image_at = argv.iter().position(|a| a == "example/agent:1").expect("the image");
    assert!(argv.iter().position(|a| a == "--entrypoint").unwrap() < image_at);
    assert_eq!(&argv[image_at + 1..], ["-c", "%g", "/var/run/docker.sock"]);

    let config = super::super::runtime::engine_config(&spec, vec![]);
    assert_eq!(config.entrypoint, Some(vec!["stat".to_string()]));
    assert_eq!(config.cmd, Some(vec!["-c".into(), "%g".into(), "/var/run/docker.sock".into()]));
    let host = config.host_config.expect("host config");
    assert_eq!(host.network_mode.as_deref(), Some("none"));
    assert_eq!(host.security_opt, Some(spec.security_options()));
    assert_eq!(host.auto_remove, Some(true));
    assert_eq!(host.extra_hosts, None);
    assert_eq!(host.cap_drop, Some(vec!["ALL".to_string()]));
    assert_eq!(host.group_add, None);
    assert_eq!(config.user.as_deref(), Some("501:20"));
}

/// EAC-FR-13, EAC-FR-GKSW: an agent launch keeps the bridge network and runs
/// the entrypoint of its image.
#[test]
fn an_agent_launch_keeps_its_network_and_its_entrypoint() {
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    run(&harness_for("claude_code"), runtime.clone(), task("go")).expect("runs");
    let recorded = runtime.only_run();
    assert_eq!(flag_values(&recorded.argv, "--network"), vec!["bridge".to_string()]);
    assert!(flag_values(&recorded.argv, "--entrypoint").is_empty());
    let config = super::super::runtime::engine_config(&recorded.container.unwrap(), vec![]);
    assert_eq!(config.entrypoint, None);
}

/// EAC-FR-QWNT: a failed helper emits a WARN that names its cause and the host
/// group, and a helper that answers emits the group it read.
#[cfg(unix)]
#[test]
fn the_helper_outcome_is_logged_with_its_cause() {
    use std::os::unix::fs::MetadataExt;

    let marker = format!("MARKER-QWNT-{}", descriptor::unique_container_name());
    let (harness, _listener, _dir, socket) = enabled_harness(false);
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    runtime.answer_socket_group(Err(marker.clone()));
    run(&harness, runtime.clone(), task("go")).expect("runs");

    let gid = std::fs::metadata(&socket).unwrap().gid();
    let page = super::super::log_buffer()
        .query(&crate::logging::LogFilter::default(), None, 50_000)
        .expect("query");
    let records = serde_json::to_value(&page).expect("serialise");
    let warned = records["records"]
        .as_array()
        .expect("records")
        .iter()
        .find(|record| record.to_string().contains(&marker))
        .expect("a record names the cause");
    assert_eq!(warned["level"], "WARN", "{warned}");
    assert!(warned.to_string().contains(&format!("\"host_group\":{gid}")), "{warned}");
}

/// EAC-FR-GKSW, EAC-FR-HWQN, EAC-FR-38: the helper runs only once the socket
/// check passed, the runtime answered, and the image is present.
#[cfg(unix)]
#[test]
fn no_helper_runs_before_the_socket_the_runtime_and_the_image() {
    let dir = tempfile::tempdir().unwrap();
    let mut missing = harness_for("claude_code");
    missing.docker_in_docker = DockerInDockerLaunch {
        enabled: true,
        socket_path: dir.path().join("gone.sock").to_str().unwrap().to_string(),
        ..Default::default()
    };
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    runtime.answer_socket_group(Ok(1));
    assert!(run(&missing, runtime.clone(), task("go")).is_err());
    assert!(runtime.socket_probes().is_empty(), "no helper before the socket check");

    let (harness, _listener, _dir, _socket) = enabled_harness(false);
    for runtime in
        [RecordingRuntime::broken(false, true, None), RecordingRuntime::broken(true, false, None)]
    {
        runtime.answer_socket_group(Ok(1));
        assert!(run(&harness, runtime.clone(), task("go")).is_err());
        assert!(runtime.socket_probes().is_empty(), "no helper before the image is present");
    }
}

/// EAC-FR-GKSW: the helper runs under the token of the launch.
#[cfg(unix)]
#[test]
fn the_helper_runs_under_the_launch_token() {
    let (harness, _listener, _dir, _socket) = enabled_harness(false);
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    runtime.answer_socket_group(Ok(0));
    let cancel = CancellationToken::new();
    cancel.cancel();
    let _ = run_with_cancel(&harness, runtime.clone(), task("go"), cancel);
    assert_eq!(runtime.socket_cancels(), vec![true]);
}

/// EAC-FR-DKRS, EAC-FR-GKSW: the group is read at each launch and never stored,
/// so each launch adds the group its own read gave.
#[cfg(unix)]
#[test]
fn each_launch_reads_its_own_group() {
    let (harness, _listener, _dir, _socket) = enabled_harness(false);
    for group in [1001, 1002] {
        let runtime = RecordingRuntime::replying(&valid_claude_stdout());
        runtime.answer_socket_group(Ok(group));
        run(&harness, runtime.clone(), task("go")).expect("runs");
        assert_eq!(runtime.only_run().container.unwrap().groups, vec![group]);
        assert_eq!(runtime.socket_probes().len(), 1);
    }
}
