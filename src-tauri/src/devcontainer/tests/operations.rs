use std::path::Path;
use std::sync::{Arc, Mutex};

use super::*;
use crate::logging::BufferState;
use crate::project_settings::agent_environment::{
    load_devcontainer_probe_from, write_agent_environment_to, AgentEnvironment,
    AgentEnvironmentSource, ProbeFinding,
};

#[derive(Clone)]
struct Silent;
impl LogSink for Silent {
    fn publish(&self, _state: &BufferState) {}
}

struct Files(Vec<String>);
impl FileProbe for Files {
    fn exists(&self, path: &Path) -> bool {
        self.0.iter().any(|p| Path::new(p) == path)
    }
    fn is_executable(&self, path: &Path) -> bool {
        self.exists(path)
    }
}

const CONFIG: &str = ".devcontainer/devcontainer.json";

fn project_with_config() -> (tempfile::TempDir, crate::fs::RootFs) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".devcontainer")).unwrap();
    std::fs::write(dir.path().join(CONFIG), "{ \"image\": \"debian\" }").unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    write_agent_environment_to(
        &root,
        &AgentEnvironment {
            source: AgentEnvironmentSource::Devcontainer,
            devcontainer_config: Some(CONFIG.to_string()),
        },
    )
    .unwrap();
    (dir, root)
}

fn verify(store: &GlobalSettingsStore, path: &str) {
    store
        .save_devcontainer_cli(&cli::DevcontainerCliRecord {
            path: path.to_string(),
            verified: Some(DevcontainerCliVerification {
                path: path.to_string(),
                version: "1".to_string(),
                verified_at: "t".to_string(),
            }),
        })
        .unwrap();
}

fn with_docker(store: &GlobalSettingsStore, docker: &str) {
    let mut config = store.load_docker_backend().unwrap().config();
    config.mode = crate::docker::DockerBackendMode::DockerCli;
    config.cli_path = Some(docker.to_string());
    store.save_docker_backend(&config).unwrap();
}

#[cfg(unix)]
fn script(dir: &Path, name: &str, body: &str) -> String {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_string_lossy().to_string()
}

// DVC-FR-OXCZ / DVC-FR-GNOK: the refusals come in their order, before
// anything runs; an allowed build has the project root as its workspace.
#[test]
fn a_build_is_prepared_or_refused_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let empty = crate::fs::RootFs::for_root(dir.path());
    let store = GlobalSettingsStore::in_memory();
    let docker = Files(vec!["/opt/docker".to_string()]);
    assert_eq!(prepare_build(&empty, &store, &docker, false), Err("devcontainer_config_missing"));

    let (project, root) = project_with_config();
    assert_eq!(prepare_build(&root, &store, &docker, false), Err("devcontainer_cli_missing"));
    verify(&store, "/bin/devcontainer");
    with_docker(&store, "/nowhere/docker");
    assert_eq!(prepare_build(&root, &store, &Files(vec![]), false), Err("docker_cli_required"));
    with_docker(&store, "/opt/docker");
    let prepared = prepare_build(&root, &store, &docker, true).unwrap();
    assert_eq!(prepared.spec.workspace, project.path());
    assert_eq!(prepared.spec.config, project.path().join(CONFIG));
    assert_eq!(prepared.spec.docker, "/opt/docker");
    assert!(prepared.spec.no_cache);
    assert_eq!(prepared.relative, CONFIG);
    assert_eq!(
        prepared.config_hash,
        config::config_hash(&root, &project.path().join(CONFIG)).unwrap()
    );
}

/// Records the events, and whether the project was free when the result came.
struct Recorded {
    registry: Arc<DevcontainerBuildRegistry>,
    root: std::path::PathBuf,
    finished: Mutex<Vec<(build::DevcontainerBuildFinished, bool)>>,
}

impl BuildEvents for Recorded {
    fn progress(&self, _payload: build::DevcontainerBuildProgress) {}
    fn finished(&self, payload: build::DevcontainerBuildFinished) {
        let free = self.registry.in_flight(&self.root).is_none();
        self.finished.lock().unwrap().push((payload, free));
    }
}

// DVC-FR-RTBA / DVC-FR-EKAO / DVC-FR-PFKJ: a build that succeeds probes the
// image for each vendor, stores the findings with the hash of the build start,
// frees the project, and then emits exactly one result.
#[cfg(unix)]
#[tokio::test]
async fn a_claimed_build_probes_stores_and_finishes_once() {
    let (project, root) = project_with_config();
    let bin = tempfile::tempdir().unwrap();
    let cli = script(
        bin.path(),
        "devcontainer",
        "echo '{\"outcome\":\"success\",\"imageName\":[\"img:1\"]}'",
    );
    let docker = script(
        bin.path(),
        "docker",
        "case \"$*\" in *claude*) echo /usr/bin/claude;; *) exit 1;; esac",
    );
    let store = GlobalSettingsStore::in_memory();
    verify(&store, &cli);
    with_docker(&store, &docker);
    let probe = Files(vec![docker.clone()]);
    let prepared = prepare_build(&root, &store, &probe, false).unwrap();
    let start_hash = prepared.config_hash.clone();
    // An edit during the build: the result belongs to the config at the start.
    std::fs::write(project.path().join(CONFIG), "{ \"image\": \"ubuntu\" }").unwrap();

    let registry = Arc::new(DevcontainerBuildRegistry::default());
    let cancel = registry.claim(root.path(), "op").unwrap();
    let events = Recorded {
        registry: registry.clone(),
        root: root.path().to_path_buf(),
        finished: Mutex::new(vec![]),
    };
    run_claimed_build(&Silent, &root, prepared, "op", &cancel, &events, &registry).await;

    let finished = events.finished.lock().unwrap();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].0.outcome, BuildOutcome::Succeeded);
    assert!(finished[0].1, "the project is free when the result comes");
    let stored = load_devcontainer_probe_from(&root).unwrap();
    assert_eq!(stored.config_hash, start_hash);
    assert_eq!(stored.claude_code, Some(ProbeFinding::Found));
    assert_eq!(stored.codex, Some(ProbeFinding::Missing));
}

// DVC-FR-EKAO: a probe that cannot run stores nothing, and an earlier record
// stays.
#[cfg(unix)]
#[tokio::test]
async fn a_probe_that_cannot_run_stores_nothing() {
    let (_project, root) = project_with_config();
    let bin = tempfile::tempdir().unwrap();
    let docker = script(bin.path(), "docker", "exit 125");
    let earlier = DevcontainerProbe {
        config: CONFIG.to_string(),
        config_hash: "old".to_string(),
        claude_code: Some(ProbeFinding::Found),
        codex: None,
    };
    write_devcontainer_probe_to(&root, &earlier).unwrap();
    assert_eq!(probe_and_store(&Silent, &root, CONFIG, "new".into(), &docker, "img").await, None);
    assert_eq!(load_devcontainer_probe_from(&root), Some(earlier));
}

// DVC-FR-BAID: a verification stores `verified` with the version; a failure
// stores `unverified`; no path at all is `not_found`.
#[cfg(unix)]
#[tokio::test]
async fn a_verification_is_stored() {
    let bin = tempfile::tempdir().unwrap();
    let good = script(bin.path(), "good", "echo 0.91.0");
    let bad = script(bin.path(), "bad", "exit 1");
    let store = GlobalSettingsStore::in_memory();

    let verified =
        verify_and_store(&Silent, &store, Some(good.clone()), cli::VERIFY_TIMEOUT).await.unwrap();
    assert_eq!(verified.state, cli::DevcontainerCliState::Verified);
    assert_eq!(verified.version.as_deref(), Some("0.91.0"));
    assert_eq!(
        cli::resolve_devcontainer_cli(&store.load_devcontainer_cli().unwrap()).unwrap(),
        good
    );

    store.save_devcontainer_cli(&store.load_devcontainer_cli().unwrap().with_path(&bad)).unwrap();
    let failed = verify_and_store(&Silent, &store, None, cli::VERIFY_TIMEOUT).await.unwrap_err();
    assert_eq!(failed.code, "failed");
    assert_eq!(
        store.load_devcontainer_cli().unwrap().view().state,
        cli::DevcontainerCliState::Unverified
    );

    let empty = GlobalSettingsStore::in_memory();
    let missing = verify_and_store(&Silent, &empty, None, cli::VERIFY_TIMEOUT).await.unwrap_err();
    assert_eq!(missing.code, "not_found");
}
