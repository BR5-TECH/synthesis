//! The dev container acceptance test (DVC-FR-LKRY, DVC-FR-QGUT, DVC-FR-BUXC,
//! DVC-FR-DCQG, DVC-FR-JXEA). It drives the real dev container CLI and the
//! real Docker daemon through the production runtime, so it is ignored. Run it
//! from `src-tauri/` with
//! `TMPDIR=/tmp cargo nextest run --run-ignored only devcontainer_uat`.
//!
//! The CLI is the one `DEVCONTAINER_CLI` names, or the one detection finds.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::*;
use crate::devcontainer::config::{self, LaunchMount};
use crate::tools::agent_exec::runtime::CancellationToken;

fn tool(env: &str, detect: impl FnOnce() -> Option<String>) -> String {
    std::env::var(env)
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(detect)
        .unwrap_or_else(|| panic!("no {env}: install it, or name it in {env}"))
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/devcontainer-uat")
}

async fn docker(docker: &str, args: &[&str], env: &[(&str, &str)]) -> (i32, String) {
    let mut command = tokio::process::Command::new(docker);
    command.args(args);
    for (name, value) in env {
        command.env(name, value);
    }
    let output = command.output().await.expect("docker runs");
    (output.status.code().unwrap_or(-1), String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// DVC-FR-LKRY / DVC-FR-QGUT / DVC-FR-BUXC / DVC-FR-DCQG / DVC-FR-JXEA: one
// `devcontainer up` serves two turns; the lifecycle command ran once and its
// marker stays; the worktree has its host path; a read-only mount is read-only;
// the probed environment holds `remoteEnv`; a credential passes by name; a
// stop removes the container.
#[test]
#[ignore = "needs the dev container CLI, a Docker daemon and the public registry"]
fn devcontainer_uat_up_two_turns_and_stop() {
    let path_var = std::env::var("PATH").ok();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let cli = tool("DEVCONTAINER_CLI", || {
        crate::devcontainer::cli::detect(
            path_var.as_deref(),
            home.as_deref(),
            &crate::agentic::RealFileProbe,
        )
    });
    let docker_cli = tool("DOCKER_CLI", || {
        crate::docker::detect_docker_cli(
            path_var.as_deref(),
            home.as_deref(),
            &crate::agentic::RealFileProbe,
        )
    });

    // Docker Desktop shares /tmp, and a short path keeps the mounts simple.
    let work = tempfile::Builder::new().prefix("dvc-uat").tempdir_in("/tmp").unwrap();
    let worktree = std::fs::canonicalize(work.path()).unwrap().join("worktree");
    let state = std::fs::canonicalize(work.path()).unwrap().join("state");
    std::fs::create_dir_all(&worktree).unwrap();
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(state.join("read-only"), "x").unwrap();
    let config_path = fixture().join(".devcontainer/devcontainer.json");
    let text = std::fs::read_to_string(&config_path).unwrap();
    let parsed = config::parse_config(&text).unwrap();
    let worktree_text = worktree.to_string_lossy().to_string();
    let mounts = vec![LaunchMount {
        source: state.to_string_lossy().to_string(),
        target: "/synthesis/state".to_string(),
        read_only: true,
    }];
    let generated = config::generated_config(&parsed, &worktree_text, &mounts, &[]);
    let override_path = work.path().join("override/devcontainer.json");
    std::fs::create_dir_all(override_path.parent().unwrap()).unwrap();
    let override_text = serde_json::to_string_pretty(&generated).unwrap();
    std::fs::write(&override_path, &override_text).unwrap();

    let scope_key = format!("uat-{}", std::process::id());
    let start = DevcontainerStart {
        scope_key: scope_key.clone(),
        project_key: "uat".to_string(),
        cli,
        docker: docker_cli.clone(),
        worktree: worktree_text.clone(),
        config: config_path,
        config_hash: "uat".to_string(),
        override_path,
        override_text,
        compose: false,
    };
    let pool = DevcontainerPool::new(Arc::new(CliDevcontainerRuntime));

    tauri::async_runtime::block_on(async {
        let first = pool
            .with_container(&start, &CancellationToken::new(), |running| async move { running })
            .await
            .expect("devcontainer up");
        assert!(
            first.probed_env.iter().any(|(n, v)| n == "UAT_REMOTE_ENV" && v == "from-remote-env"),
            "{:?}",
            first.probed_env
        );
        let id = first.container_id.clone();
        let (code, out) = docker(
            &docker_cli,
            &[
                "exec", "-e", "UAT_SECRET", "-w", &worktree_text, &id, "sh", "-c",
                "cat /tmp/synthesis-uat-marker; echo $UAT_SECRET; pwd; touch turn-one; touch /synthesis/state/x 2>/dev/null && echo writable || echo read-only",
            ],
            &[("UAT_SECRET", "by-name")],
        )
        .await;
        assert_eq!(code, 0);
        assert_eq!(out, format!("created\nby-name\n{worktree_text}\nread-only"));
        assert!(worktree.join("turn-one").exists(), "the worktree is the host folder");

        let second = pool
            .with_container(&start, &CancellationToken::new(), |running| async move { running })
            .await
            .unwrap();
        assert_eq!(second.container_id, id, "the second turn uses the same container");

        pool.stop(&scope_key).await.expect("the stop removes the container");
        let filter = format!("label={SCOPE_LABEL}={scope_key}");
        let (_, left) = docker(&docker_cli, &["ps", "-aq", "--filter", &filter], &[]).await;
        assert_eq!(left, "", "the stop removed the container");
    });
}
