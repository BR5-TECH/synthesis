use super::*;

fn running(user: &str) -> RunningDevcontainer {
    RunningDevcontainer {
        container_id: "cid".to_string(),
        remote_user: user.to_string(),
        remote_workspace_folder: "/w".to_string(),
        probed_env: vec![],
    }
}

// DVC-FR-DCQG: the exec names the user, the worktree, the env file, the turn
// marker and each credential by name, then the CLI and its arguments.
#[test]
fn exec_arguments_are_exact() {
    let args = exec_args(
        &running("vscode"),
        "/work",
        Path::new("/data/turn.env"),
        "t1",
        &["CLAUDE_CODE_OAUTH_TOKEN".to_string()],
        "claude",
        &["-p".to_string(), "--verbose".to_string()],
    );
    assert_eq!(
        args,
        vec![
            "exec",
            "-i",
            "-u",
            "vscode",
            "-w",
            "/work",
            "--env-file",
            "/data/turn.env",
            "-e",
            "SYNTHESIS_TURN=t1",
            "-e",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "cid",
            "claude",
            "-p",
            "--verbose",
        ]
    );
    let no_user = exec_args(&running(""), "/work", Path::new("/e"), "t", &[], "codex", &[]);
    assert!(!no_user.contains(&"-u".to_string()));
}

// DVC-FR-DCQG / DVC-FR-IFBK: literals win over the probed values, a credential
// name and a value with a line break never reach the file.
#[test]
fn the_env_file_merges_and_filters() {
    let probed = vec![
        ("PATH".to_string(), "/feature/bin".to_string()),
        ("TOKEN".to_string(), "probed".to_string()),
        ("MULTI".to_string(), "a\nb".to_string()),
        ("CLAUDE_CONFIG_DIR".to_string(), "/home/u/.claude".to_string()),
    ];
    let literals = vec![
        ("CLAUDE_CONFIG_DIR".to_string(), "/home/agent/.claude".to_string()),
        ("CODEX_HOME".to_string(), "/home/agent/.codex".to_string()),
    ];
    let text = env_file_text(&probed, &literals, &["TOKEN".to_string()]);
    assert_eq!(
        text,
        "CLAUDE_CONFIG_DIR=/home/agent/.claude\nCODEX_HOME=/home/agent/.codex\nPATH=/feature/bin\n"
    );
    assert_eq!(env_file_text(&[], &[], &[]), "");
}

// DVC-FR-ATEO / DVC-FR-ODWK: each failure names its cause.
#[test]
fn each_failure_names_its_cause() {
    assert_eq!(DevcontainerFailure::StartFailed("x".into()).cause(), "devcontainer_start_failed");
    assert_eq!(DevcontainerFailure::AgentCliMissing("claude".into()).cause(), "agent_cli_missing");
}

// DVC-FR-QGUT: a path with a comma or a line break cannot be mounted.
#[test]
fn a_comma_cannot_be_mounted() {
    assert!(unmountable("/a,b"));
    assert!(unmountable("/a\nb"));
    assert!(!unmountable("/a b"));
}

// DVC-FR-YZLH: a project of the vendor images has no dev container launch.
#[test]
fn vendor_images_have_no_dev_container_launch() {
    let dir = tempfile::tempdir().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    let store = GlobalSettingsStore::in_memory();
    assert_eq!(project_devcontainer_launch(&root, &store), None);
}

// DVC-FR-YZLH / DVC-FR-CHZS: a dev container that is not ready is an error,
// and a ready one reads its config from the project root.
#[test]
fn a_dev_container_launch_is_resolved_or_refused() {
    use crate::project_settings::agent_environment::{
        write_agent_environment_to, AgentEnvironment, AgentEnvironmentSource,
    };
    let dir = tempfile::tempdir().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    let store = GlobalSettingsStore::in_memory();
    let mut environment = AgentEnvironment {
        source: AgentEnvironmentSource::Devcontainer,
        devcontainer_config: None,
    };
    write_agent_environment_to(&root, &environment).unwrap();
    assert!(matches!(project_devcontainer_launch(&root, &store), Some(Err(_))));

    std::fs::create_dir_all(dir.path().join(".devcontainer")).unwrap();
    std::fs::write(dir.path().join(".devcontainer/devcontainer.json"), "{}").unwrap();
    environment.devcontainer_config = Some(".devcontainer/devcontainer.json".into());
    write_agent_environment_to(&root, &environment).unwrap();
    assert!(
        matches!(project_devcontainer_launch(&root, &store), Some(Err(_))),
        "the CLI is unverified"
    );

    let docker = dir.path().join("docker");
    std::fs::write(&docker, "").unwrap();
    store
        .save_devcontainer_cli(&crate::devcontainer::cli::DevcontainerCliRecord {
            path: "/bin/devcontainer".into(),
            verified: Some(crate::devcontainer::cli::DevcontainerCliVerification {
                path: "/bin/devcontainer".into(),
                version: "1".into(),
                verified_at: "t".into(),
            }),
        })
        .unwrap();
    let mut config = store.load_docker_backend().unwrap().config();
    config.mode = crate::docker::DockerBackendMode::DockerCli;
    config.cli_path = Some(docker.to_string_lossy().to_string());
    store.save_docker_backend(&config).unwrap();
    let launch = project_devcontainer_launch(&root, &store).unwrap().unwrap();
    assert_eq!(launch.config, dir.path().join(".devcontainer/devcontainer.json"));
    assert_eq!(launch.config_text, "{}");
    assert_eq!(launch.config_hash, config::config_hash(&root, &launch.config).unwrap());
    assert_eq!(launch.docker, docker.to_string_lossy());
}
