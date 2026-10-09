//! EAC-FR-15 — the value of each variable passed by name reaches the process
//! that runs, on both backends.

use super::*;

fn secret_env(pairs: &[(&str, &str)]) -> BTreeMap<String, SecretString> {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), SecretString::new(value.to_string())))
        .collect()
}

/// EAC-FR-15, EAC-FR-39: the Docker CLI backend sets each value on the client
/// process it spawns, so a program started by it reads the value under its name.
#[test]
fn the_cli_backend_gives_the_spawned_process_each_value() {
    let sh = HostDockerCli::with_program("sh");
    let env = secret_env(&[
        ("ANTHROPIC_AUTH_TOKEN", "gw-FAKE-TEST-TOKEN-NOT-A-CREDENTIAL-k2Qz"),
        ("HTTPS_PROXY", "http://proxy:3128"),
    ]);
    let outcome = block_on(async {
        sh.run(RunRequest {
            argv: &[
                "-c".to_string(),
                r#"printf '%s|%s' "$ANTHROPIC_AUTH_TOKEN" "$HTTPS_PROXY""#.to_string(),
            ],
            container: None,
            env: &env,
            stdin: b"",
            timeout: Duration::from_secs(20),
            cancel: CancellationToken::new(),
            stdout_limit: 4096,
            stderr_limit: 4096,
            observer: None,
        })
        .await
    })
    .expect("ran");

    assert_eq!(outcome.end, RunEnd::Exited);
    assert_eq!(
        String::from_utf8_lossy(&outcome.stdout.bytes),
        "gw-FAKE-TEST-TOKEN-NOT-A-CREDENTIAL-k2Qz|http://proxy:3128"
    );
}

/// EAC-FR-15, EAC-FR-39: the Docker Engine backend sends each passed name with
/// its value, in the spec's order, and then the executor's own literals.
#[test]
fn the_engine_backend_sends_each_name_with_its_value() {
    let spec = descriptor::ContainerSpec {
        name: "synthesis-agent-1".to_string(),
        image: "registry.example/image:test".to_string(),
        host_uid: 501,
        host_gid: 20,
        workdir: "/host/worktree".to_string(),
        mounts: vec![],
        env_names: vec!["CLAUDE_CODE_USE_BEDROCK".into(), "ANTHROPIC_AUTH_TOKEN".into()],
        env_literals: vec![("CLAUDE_CONFIG_DIR".into(), "/session".into())],
        vendor_args: vec!["-p".to_string()],
    };
    let env = secret_env(&[
        ("ANTHROPIC_AUTH_TOKEN", "gw-FAKE-TEST-TOKEN-NOT-A-CREDENTIAL-k2Qz"),
        ("CLAUDE_CODE_USE_BEDROCK", "1"),
    ]);

    let entries = super::super::runtime::engine_env(&spec, &env);
    assert_eq!(
        entries,
        vec![
            "CLAUDE_CODE_USE_BEDROCK=1".to_string(),
            "ANTHROPIC_AUTH_TOKEN=gw-FAKE-TEST-TOKEN-NOT-A-CREDENTIAL-k2Qz".to_string(),
            "CLAUDE_CONFIG_DIR=/session".to_string(),
        ]
    );
    let config = super::super::runtime::engine_config(&spec, entries.clone());
    assert_eq!(config.env, Some(entries));
}

/// EAC-FR-15, EAC-FR-OWPP: the Engine backend sends a name only with a value,
/// and sends no value that the spec does not name.
#[test]
fn the_engine_backend_sends_only_named_variables_that_have_a_value() {
    let spec = descriptor::ContainerSpec {
        name: "synthesis-agent-1".to_string(),
        image: "registry.example/image:test".to_string(),
        host_uid: 501,
        host_gid: 20,
        workdir: "/host/worktree".to_string(),
        mounts: vec![],
        env_names: vec!["NAMED_WITHOUT_VALUE".into(), "NAMED".into()],
        env_literals: vec![],
        vendor_args: vec![],
    };
    let env = secret_env(&[("NAMED", "yes"), ("NOT_NAMED", "no")]);
    assert_eq!(
        super::super::runtime::engine_env(&spec, &env),
        vec!["NAMED=yes".to_string()]
    );
}
