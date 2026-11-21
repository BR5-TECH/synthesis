use std::path::Path;

use super::*;
use crate::project_settings::agent_environment::{write_devcontainer_probe_to, DevcontainerProbe};

const CONFIG: &str = ".devcontainer/devcontainer.json";

fn project() -> (tempfile::TempDir, crate::fs::RootFs) {
    let dir = tempfile::tempdir().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    (dir, root)
}

fn write(dir: &tempfile::TempDir, relative: &str, text: &str) {
    let path = dir.path().join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn local_toml(dir: &tempfile::TempDir) -> Option<String> {
    std::fs::read_to_string(dir.path().join(".synthesis/local.toml")).ok()
}

fn row(state: &AgentEnvironmentState, vendor: &str) -> AgentCliRow {
    state.agent_cli.iter().find(|r| r.vendor == vendor).cloned().unwrap()
}

fn select(root: &crate::fs::RootFs) -> AgentEnvironmentState {
    save(
        root,
        AgentEnvironment {
            source: AgentEnvironmentSource::Devcontainer,
            devcontainer_config: Some(CONFIG.to_string()),
        },
    )
    .unwrap()
}

// DVC-FR-WZSY / DVC-FR-SEXK: an empty project has the vendor images and both
// rows unknown; Claude Code names its known Feature and Codex none.
#[test]
fn an_empty_project_has_unknown_rows() {
    let (_dir, root) = project();
    let state = state_of(&root);
    assert_eq!(state.environment.source, AgentEnvironmentSource::VendorImages);
    assert!(!state.config_exists);
    assert_eq!(row(&state, "claude_code").state, AgentCliState::Unknown);
    assert_eq!(
        row(&state, "claude_code").known_feature.as_deref(),
        Some(config::CLAUDE_CODE_FEATURE)
    );
    assert_eq!(row(&state, "codex").known_feature, None);
}

// DVC-FR-WZSY: the Feature in the config makes Claude Code found.
#[test]
fn the_feature_makes_the_cli_found() {
    let (dir, root) = project();
    write(
        &dir,
        CONFIG,
        &format!("{{ \"features\": {{ \"{}:1\": {{}} }} }}", config::CLAUDE_CODE_FEATURE),
    );
    let state = select(&root);
    assert!(state.config_exists);
    assert_eq!(row(&state, "claude_code").state, AgentCliState::Found);
    assert_eq!(row(&state, "codex").state, AgentCliState::Unknown);
}

// DVC-FR-WZSY / DVC-FR-PFKJ: a probe of the current config wins over the
// Feature list; a probe of an older config or of another file counts for
// nothing.
#[test]
fn a_probe_of_the_current_config_wins() {
    let (dir, root) = project();
    write(
        &dir,
        CONFIG,
        &format!("{{ \"features\": {{ \"{}\": {{}} }} }}", config::CLAUDE_CODE_FEATURE),
    );
    select(&root);
    let hash = config::config_hash(&root, &dir.path().join(CONFIG)).unwrap();
    let probe = |config: &str, hash: &str| DevcontainerProbe {
        config: config.to_string(),
        config_hash: hash.to_string(),
        claude_code: Some(ProbeFinding::Missing),
        codex: Some(ProbeFinding::Found),
    };
    write_devcontainer_probe_to(&root, &probe(CONFIG, &hash)).unwrap();
    let state = state_of(&root);
    assert_eq!(row(&state, "claude_code").state, AgentCliState::Missing);
    assert_eq!(row(&state, "codex").state, AgentCliState::Found);

    write_devcontainer_probe_to(&root, &probe(".devcontainer.json", &hash)).unwrap();
    assert_eq!(row(&state_of(&root), "claude_code").state, AgentCliState::Found);

    write_devcontainer_probe_to(&root, &probe(CONFIG, &hash)).unwrap();
    write(
        &dir,
        CONFIG,
        &format!(
            "{{ \"name\": \"x\", \"features\": {{ \"{}\": {{}} }} }}",
            config::CLAUDE_CODE_FEATURE
        ),
    );
    let state = state_of(&root);
    assert_eq!(row(&state, "claude_code").state, AgentCliState::Found);
    assert_eq!(row(&state, "codex").state, AgentCliState::Unknown);
}

// DVC-FR-WXYK: each refused path stores nothing, and the earlier value stays.
#[test]
fn a_refused_save_stores_nothing() {
    let (dir, root) = project();
    write(&dir, CONFIG, "{}");
    select(&root);
    let before = local_toml(&dir);
    for (path, reason) in [
        ("/abs.json", config::ConfigPathProblem::Absolute),
        ("../out.json", config::ConfigPathProblem::LeavesProject),
        ("missing.json", config::ConfigPathProblem::NotAFile),
    ] {
        let refused = save(
            &root,
            AgentEnvironment {
                source: AgentEnvironmentSource::Devcontainer,
                devcontainer_config: Some(path.to_string()),
            },
        );
        assert_eq!(refused, Err(AgentEnvironmentError::ConfigPathInvalid { reason }));
        assert_eq!(local_toml(&dir), before);
    }
}

// DVC-FR-WXYK: a dev container with no config, or a blank one, is refused.
#[test]
fn a_dev_container_needs_a_config() {
    let (dir, root) = project();
    for config in [None, Some("  ".to_string())] {
        let refused = save(
            &root,
            AgentEnvironment {
                source: AgentEnvironmentSource::Devcontainer,
                devcontainer_config: config,
            },
        );
        assert_eq!(refused, Err(AgentEnvironmentError::ConfigMissing));
    }
    assert_eq!(local_toml(&dir), None);
    let kept = save(&root, AgentEnvironment::default()).unwrap();
    assert_eq!(kept.environment.source, AgentEnvironmentSource::VendorImages);
}

// DVC-FR-EVZD: a valid save stores the trimmed path project-local only.
#[test]
fn a_valid_save_is_project_local() {
    let (dir, root) = project();
    write(&dir, CONFIG, "{}");
    let state = save(
        &root,
        AgentEnvironment {
            source: AgentEnvironmentSource::Devcontainer,
            devcontainer_config: Some(format!(" {CONFIG} ")),
        },
    )
    .unwrap();
    assert_eq!(state.environment.devcontainer_config.as_deref(), Some(CONFIG));
    assert!(state.config_exists);
    assert!(local_toml(&dir).unwrap().contains(CONFIG));
    assert!(!dir.path().join(".synthesis/project.toml").exists());
}

// DVC-FR-BCDZ: the starter config is written, named after the folder, and
// selected; Claude Code is then found.
#[test]
fn the_starter_config_is_created_and_selected() {
    let (dir, root) = project();
    let state = create_config(&root).unwrap();
    assert_eq!(state.environment.source, AgentEnvironmentSource::Devcontainer);
    assert_eq!(state.environment.devcontainer_config.as_deref(), Some(STARTER_CONFIG_PATH));
    assert_eq!(row(&state, "claude_code").state, AgentCliState::Found);
    let text = std::fs::read_to_string(dir.path().join(STARTER_CONFIG_PATH)).unwrap();
    let name = dir.path().file_name().unwrap().to_string_lossy().to_string();
    assert_eq!(config::parse_config(&text).unwrap()["name"], name.as_str());
}

// DVC-FR-BCDZ: an existing file is never overwritten.
#[test]
fn an_existing_starter_is_not_overwritten() {
    let (dir, root) = project();
    write(&dir, STARTER_CONFIG_PATH, "// mine\n{}");
    assert_eq!(create_config(&root), Err(AgentEnvironmentError::ConfigExists));
    assert_eq!(
        std::fs::read_to_string(dir.path().join(STARTER_CONFIG_PATH)).unwrap(),
        "// mine\n{}"
    );
    assert_eq!(local_toml(&dir), None);
}

// DVC-FR-ABQZ / DVC-FR-JVKA: the Feature is added with the comments kept; a
// refusal and a config that has it leave the file unchanged.
#[test]
fn the_feature_is_added_or_the_file_is_left() {
    let (dir, root) = project();
    let path = dir.path().join(CONFIG);
    write(&dir, CONFIG, "// team\n{ \"image\": \"debian\" }\n");
    assert_eq!(add_feature(&root, "claude_code"), Err(AgentEnvironmentError::ConfigMissing));
    select(&root);
    assert_eq!(add_feature(&root, "codex"), Err(AgentEnvironmentError::NoKnownFeature));
    let state = add_feature(&root, "claude_code").unwrap();
    assert_eq!(row(&state, "claude_code").state, AgentCliState::Found);
    let added = std::fs::read_to_string(&path).unwrap();
    assert!(added.starts_with("// team\n"), "{added}");
    add_feature(&root, "claude_code").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), added);

    write(&dir, CONFIG, "{ \"image\": ");
    assert!(matches!(
        add_feature(&root, "claude_code"),
        Err(AgentEnvironmentError::ConfigInvalid { .. })
    ));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ \"image\": ");

    write(&dir, CONFIG, "{ \"features\": [] }");
    assert!(matches!(
        add_feature(&root, "claude_code"),
        Err(AgentEnvironmentError::ConfigInvalid { .. })
    ));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ \"features\": [] }");
}

// Contract surface: the refusals have the shape the frontend reads.
#[test]
fn refusals_have_their_wire_shape() {
    let json = |e: &AgentEnvironmentError| serde_json::to_value(e).unwrap();
    assert_eq!(
        json(&AgentEnvironmentError::ConfigPathInvalid {
            reason: config::ConfigPathProblem::LeavesProject
        }),
        serde_json::json!({ "code": "config_path_invalid", "reason": "leaves_project" })
    );
    assert_eq!(
        json(&AgentEnvironmentError::ConfigMissing),
        serde_json::json!({ "code": "config_missing" })
    );
    assert_eq!(
        json(&AgentEnvironmentError::ConfigInvalid { message: "m".into(), line: Some(3) }),
        serde_json::json!({ "code": "config_invalid", "message": "m", "line": 3 })
    );
    let (_dir, root) = project();
    let state = serde_json::to_value(state_of(&root)).unwrap();
    assert!(state.get("agentCli").is_some() && state.get("configExists").is_some(), "{state}");
    assert!(state["environment"].get("devcontainerConfig").is_some(), "{state}");
}

/// GSU-FR-HIPF: a probe that only answers what is on disk, and that panics
/// where anything would run.
struct Files(Vec<String>);

impl crate::agentic::FileProbe for Files {
    fn exists(&self, path: &Path) -> bool {
        self.0.iter().any(|p| Path::new(p) == path)
    }
    fn is_executable(&self, path: &Path) -> bool {
        self.exists(path)
    }
}

fn verified_cli() -> cli::DevcontainerCliRecord {
    cli::DevcontainerCliRecord {
        path: "/bin/devcontainer".to_string(),
        verified: Some(cli::DevcontainerCliVerification {
            path: "/bin/devcontainer".to_string(),
            version: "0.91.0".to_string(),
            verified_at: "t".to_string(),
        }),
    }
}

fn backend_with_docker() -> crate::docker::DockerBackendRecord {
    crate::docker::DockerBackendRecord {
        cli_path: Some("/opt/docker".to_string()),
        ..Default::default()
    }
}

// DVC-FR-HINW / DVC-FR-ETQN: the checks refuse in their order.
#[test]
fn the_preflight_refuses_in_order() {
    let (dir, root) = project();
    let docker = Files(vec!["/opt/docker".to_string()]);
    let unverified = cli::DevcontainerCliRecord::default();
    let check =
        |config: Option<&str>, vendor: &str, record: &cli::DevcontainerCliRecord, probe: &Files| {
            preflight::check_with(&root, config, vendor, record, &backend_with_docker(), probe)
        };
    assert_eq!(
        check(None, "opencode", &unverified, &docker),
        Err("vendor_execution_unsupported".into())
    );
    assert_eq!(
        check(None, "claude_code", &verified_cli(), &docker),
        Err("devcontainer_config_missing".into())
    );
    assert_eq!(
        check(Some(CONFIG), "claude_code", &verified_cli(), &docker),
        Err("devcontainer_config_missing".into())
    );
    write(&dir, CONFIG, "{ broken");
    assert_eq!(
        check(Some(CONFIG), "claude_code", &unverified, &docker),
        Err("devcontainer_config_invalid".into())
    );
    write(&dir, CONFIG, "{}");
    assert_eq!(
        check(Some(CONFIG), "claude_code", &unverified, &docker),
        Err("devcontainer_cli_missing".into())
    );
    assert_eq!(check(Some(CONFIG), "claude_code", &verified_cli(), &docker), Ok(()));
    let hash = config::config_hash(&root, &dir.path().join(CONFIG)).unwrap();
    write_devcontainer_probe_to(
        &root,
        &DevcontainerProbe {
            config: CONFIG.to_string(),
            config_hash: hash,
            claude_code: Some(ProbeFinding::Missing),
            codex: Some(ProbeFinding::Found),
        },
    )
    .unwrap();
    assert_eq!(
        check(Some(CONFIG), "claude_code", &verified_cli(), &docker),
        Err("devcontainer_agent_cli_missing".into())
    );
    assert_eq!(check(Some(CONFIG), "codex", &verified_cli(), &docker), Ok(()));
}

// DVC-FR-GNOK / DVC-FR-ETQN: with no Docker CLI the start is refused. The
// backend names a path that does not exist and the probe finds nothing on
// any search path.
#[test]
fn the_preflight_needs_a_docker_cli() {
    let (dir, root) = project();
    write(&dir, CONFIG, "{}");
    let refused = preflight::check_with(
        &root,
        Some(CONFIG),
        "claude_code",
        &verified_cli(),
        &crate::docker::DockerBackendRecord {
            cli_path: Some("/nowhere/docker".into()),
            ..Default::default()
        },
        &Files(vec![]),
    );
    assert_eq!(refused, Err("docker_cli_required".into()));
}

// GSU-FR-GLVQ: a project of the vendor images gets no dev container checks.
#[test]
fn vendor_images_skip_the_dev_container_preflight() {
    let (_dir, root) = project();
    let store = crate::global_settings::GlobalSettingsStore::in_memory();
    assert_eq!(preflight::check(&root, &store, "claude_code"), None);
}

// DVC-FR-WOAM / GSS-FR-MVLQ: the record round-trips through the user-global
// store.
#[test]
fn the_cli_record_round_trips_through_the_global_store() {
    let store = crate::global_settings::GlobalSettingsStore::in_memory();
    assert_eq!(store.load_devcontainer_cli().unwrap(), cli::DevcontainerCliRecord::default());
    store.save_devcontainer_cli(&verified_cli()).unwrap();
    assert_eq!(store.load_devcontainer_cli().unwrap(), verified_cli());
}
