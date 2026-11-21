//! DVC-FR-RMJQ / DVC-FR-WRXL / DVC-FR-KTBH (EAC-FR-QWOA): an interactive
//! launch in a dev container of its own.
//!
//! The container is started for the one session with the scope key
//! `terminal:<id>`, so it shares nothing with a graduation queue, and it is
//! removed when the session ends.

use std::path::Path;
use std::sync::Arc;

use super::super::devcontainer_launch::{restrict_to_owner, scope_folder, unmountable};
use super::super::devcontainer_pool::{
    up_reporting, CliDevcontainerRuntime, DevcontainerRuntime, DevcontainerStart,
    FAILURE_LINE_LIMIT,
};
use super::super::*;
use super::compose;
use super::{on_end, CleanupOnce, InteractiveLaunchRequest, Parts};
use crate::devcontainer::config::{self, LaunchMount};
use crate::terminals::process::ProcessHandle;

/// DVC-FR-RMJQ: the scope key of a terminal's dev container.
pub fn terminal_scope_key(terminal_id: &str) -> String {
    format!("terminal:{terminal_id}")
}

/// DVC-FR-KTBH: one line of a failed start, at most 300 characters.
pub fn failure_line(message: &str) -> String {
    message
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("the dev container did not start")
        .chars()
        .take(FAILURE_LINE_LIMIT)
        .collect()
}

impl AgentCliExecutor {
    pub(super) async fn launch_devcontainer_interactive(
        &self,
        parts: &Parts<'_>,
        fs: &Arc<FsAccess>,
        directory: &Path,
        request: &InteractiveLaunchRequest,
    ) -> Result<ProcessHandle, String> {
        let launch = match project_devcontainer_launch(&parts.project_root, &parts.store) {
            Some(Ok(launch)) => launch,
            Some(Err(message)) => return Err(failure_line(&message)),
            None => return Err("the project's agent environment is not a dev container".into()),
        };
        let parsed = config::parse_config(&launch.config_text).map_err(|e| e.message)?;
        let credential = match compose::takes_credential(&request.program) {
            Some(vendor) => Some(agentic::resolve_agent_launch_credential(
                &parts.store,
                &parts.integrations,
                vendor.id(),
            )?),
            None => None,
        };
        let session_root = super::session_root()?;
        let composed = self.compose_interactive(
            fs,
            directory,
            &request.program,
            compose::ComposeExtras {
                credential,
                session_root: Some(&session_root),
                terminal_id: &request.terminal_id,
                docker_in_docker: None,
            },
        )?;
        let checkout = composed.workspace.clone();
        // DVC-FR-QGUT: the checkout stands at its own path in the container,
        // and no path may break the mount form.
        if !composed.aligned
            || unmountable(&checkout)
            || composed.mounts.iter().any(|m| unmountable(&m.source) || unmountable(&m.target))
        {
            return Err("a path of this checkout cannot be mounted in a dev container".into());
        }
        // DVC-FR-IFBK: the mounts of a Docker launch, the checkout excluded.
        let mounts: Vec<LaunchMount> = composed
            .mounts
            .iter()
            .skip(1)
            .map(|m| LaunchMount {
                source: m.source.clone(),
                target: m.target.clone(),
                read_only: m.read_only,
            })
            .collect();
        let scope_key = terminal_scope_key(&request.terminal_id);
        let data = crate::fs::app_data_dir().map_err(|e| e.to_string())?;
        let folder = scope_folder(&data, &scope_key);
        let generated = config::generated_config(&parsed, &checkout, &mounts, &[]);
        let override_text = serde_json::to_string_pretty(&generated).unwrap_or_default();
        let override_path = folder.join("devcontainer.json");
        fs.write_text_atomic(&override_path, &override_text).map_err(|e| e.to_string())?;
        restrict_to_owner(&folder, 0o700);
        let compose_file = config::is_compose(&parsed);
        let start = DevcontainerStart {
            scope_key: scope_key.clone(),
            project_key: parts.project_key.clone(),
            cli: launch.cli.clone(),
            docker: launch.docker.clone(),
            worktree: checkout.clone(),
            config: launch.config.clone(),
            config_hash: launch.config_hash.clone(),
            override_path,
            override_text,
            compose: compose_file,
        };

        // DVC-FR-JXEA / DVC-FR-RMJQ: the container, its folder, and the
        // session state go when the session ends, and on a failed start.
        let removal = {
            let docker = launch.docker.clone();
            let scope_key = scope_key.clone();
            let fs = fs.clone();
            let folder = folder.clone();
            let session_dir = composed.session_dir.clone();
            CleanupOnce::new(move || {
                let removal = super::removals::begin();
                tauri::async_runtime::spawn(async move {
                    let _removal = removal;
                    let _ = CliDevcontainerRuntime.remove(&docker, &scope_key, compose_file).await;
                    let _ = fs.delete_path(&folder, true);
                    if let Some(dir) = session_dir {
                        let _ = fs.delete_path(&dir, true);
                    }
                });
            })
        };

        let progress = request.progress.clone();
        let cancel = super::super::runtime::CancellationToken::new();
        let running = up_reporting(&start, &cancel, move |line| {
            if let Some(progress) = &progress {
                progress(format!("{line}\r\n").as_bytes());
            }
        })
        .await;
        let running = match running {
            Ok(running) => running,
            Err(message) => {
                removal.run();
                return Err(failure_line(&message));
            }
        };

        // DVC-FR-WRXL: the probed environment and the literal variables in an
        // owner-only file; a credential only by name.
        let env_file = folder.join("terminal.env");
        let text = devcontainer_launch_env(&running.probed_env, &composed);
        if let Err(e) = fs.write_text_atomic(&env_file, &text) {
            removal.run();
            return Err(e.to_string());
        }
        restrict_to_owner(&env_file, 0o600);
        let argv = devcontainer_argv(&request.program);
        let args = compose::interactive_exec_args(
            &running,
            &checkout,
            &env_file,
            &composed.env_names,
            &argv,
        );
        let handle = spawn_exec(
            &launch.docker,
            args,
            directory,
            &composed,
            request,
            &running.container_id,
            removal.clone(),
        );
        match handle {
            Ok(handle) => Ok(on_end(handle, move |_| removal.run())),
            Err(e) => {
                removal.run();
                Err(e)
            }
        }
    }
}

/// DVC-FR-WRXL: a login shell where the container holds `bash`, else `sh`;
/// or the vendor's CLI with no argument.
pub fn devcontainer_argv(program: &super::InteractiveProgram) -> Vec<String> {
    match program {
        super::InteractiveProgram::Shell => vec![
            "sh".to_string(),
            "-c".to_string(),
            "if command -v bash >/dev/null 2>&1; then exec bash -l; else exec sh -l; fi"
                .to_string(),
        ],
        super::InteractiveProgram::AgentCli(_) => compose::program_argv(program),
    }
}

fn devcontainer_launch_env(probed: &[(String, String)], composed: &compose::Composed) -> String {
    super::super::devcontainer_launch::env_file_text(
        probed,
        &composed.env_literals,
        &composed.env_names,
    )
}

#[cfg(unix)]
fn spawn_exec(
    docker: &str,
    args: Vec<String>,
    directory: &Path,
    composed: &compose::Composed,
    request: &InteractiveLaunchRequest,
    container_id: &str,
    removal: Arc<CleanupOnce>,
) -> Result<ProcessHandle, String> {
    use crate::terminals::process::{spawn_host, terminal_env, HostCommand};
    let mut env = terminal_env();
    for (name, value) in &composed.env {
        env.push((name.clone(), value.expose().to_string()));
    }
    let command = HostCommand {
        program: docker.to_string(),
        args,
        cwd: directory.to_string_lossy().into_owned(),
        env,
        env_remove: Vec::new(),
    };
    spawn_host(
        &command,
        request.cols,
        request.rows,
        Some(container_id.to_string()),
        Some(Box::new(move || removal.run())),
    )
    .map_err(|e| format!("docker did not start: {}", e.kind()))
}

#[cfg(not(unix))]
fn spawn_exec(
    _docker: &str,
    _args: Vec<String>,
    _directory: &Path,
    _composed: &compose::Composed,
    _request: &InteractiveLaunchRequest,
    _container_id: &str,
    _removal: Arc<CleanupOnce>,
) -> Result<ProcessHandle, String> {
    Err("platform_unsupported".to_string())
}
