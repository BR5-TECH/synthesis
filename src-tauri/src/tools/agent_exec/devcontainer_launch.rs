//! A launch in a dev container (EAC-FR-GOAR, DVC-FR-LKRY .. DVC-FR-ODWK).
//!
//! The request, the vendor arguments, the stdin, the credentials and the
//! output parsing are those of a Docker launch. What differs is where the turn
//! runs: one `docker exec` in the dev container of the launch's scope.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::descriptor::BindMount;
use super::devcontainer_pool::{DevcontainerPool, DevcontainerStart, RunningDevcontainer};
use super::runtime::{
    CancellationToken, DockerRuntime, HostDockerCli, RunEnd, RunRequest, RuntimeError,
    RuntimeOutcome,
};
use super::sandbox::SandboxScope;
use super::*;
use crate::agentic::SecretString;
use crate::devcontainer::config::{self, LaunchMount};

/// DVC-FR-YZLH: what a launch of a dev container project needs, read by the
/// application before the launch from the project and the user-global store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevcontainerLaunch {
    /// DVC-FR-CHZS: the config, in the project root checkout.
    pub config: PathBuf,
    /// The text of the config, read from the project root checkout.
    pub config_text: String,
    /// DVC-FR-DELL: the hash of the config and of the files it names.
    pub config_hash: String,
    /// The verified dev container CLI (DVC-FR-HINW).
    pub cli: String,
    /// The Docker CLI (DVC-FR-GNOK).
    pub docker: String,
}

/// DVC-FR-ATEO / DVC-FR-ODWK: why a dev container launch did not run its turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DevcontainerFailure {
    /// The dev container could not start. The CLI's line, on one line.
    StartFailed(String),
    /// The dev container does not hold the named CLI.
    AgentCliMissing(String),
}

impl DevcontainerFailure {
    pub fn cause(&self) -> &'static str {
        match self {
            DevcontainerFailure::StartFailed(_) => "devcontainer_start_failed",
            DevcontainerFailure::AgentCliMissing(_) => "agent_cli_missing",
        }
    }
}

/// DVC-FR-YZLH: read the dev container launch of the project at `root`, or
/// `None` where its agent environment is the vendor images. An `Err` is a
/// start failure: the launch does not fall back to the vendor images.
pub fn project_devcontainer_launch(
    root: &crate::fs::RootFs,
    store: &GlobalSettingsStore,
) -> Option<Result<DevcontainerLaunch, String>> {
    use crate::project_settings::agent_environment::{
        load_agent_environment_from, AgentEnvironmentSource,
    };
    let environment = load_agent_environment_from(root);
    if environment.source != AgentEnvironmentSource::Devcontainer {
        return None;
    }
    Some((|| {
        let relative = environment
            .devcontainer_config
            .ok_or_else(|| "the project selects no devcontainer.json".to_string())?;
        let config = config::validate_config_path(root, &relative)
            .map_err(|problem| format!("the devcontainer.json path is not usable: {problem:?}"))?;
        let config_text = root.read_text(&config).map_err(|e| e.to_string())?;
        let config_hash = config::config_hash(root, &config)?;
        let record = store.load_devcontainer_cli()?;
        let cli = crate::devcontainer::cli::resolve_devcontainer_cli(&record)
            .map_err(|_| "the dev container CLI is not verified".to_string())?;
        let backend = store.load_docker_backend()?;
        let docker =
            crate::devcontainer::cli::resolve_docker_cli(&backend, &crate::agentic::RealFileProbe)
                .map_err(|_| "no Docker CLI was found".to_string())?;
        Ok(DevcontainerLaunch { config, config_text, config_hash, cli, docker })
    })())
}

/// One turn in a dev container, as the launch composed it.
pub(super) struct DevcontainerRun<'a> {
    pub launch: &'a DevcontainerLaunch,
    pub scope: Option<&'a SandboxScope>,
    pub vendor: &'a str,
    /// The host path of the worktree, also its path in the container.
    pub worktree: &'a str,
    /// EAC-FR-ZKMR: the worktree's host path is one a container can be given.
    /// Where it is not, the masks of the launch name the fallback path, which a
    /// dev container does not use, so the launch does not start.
    pub aligned: bool,
    /// The mounts of the Docker launch; the worktree mount is not one of them.
    pub mounts: &'a [BindMount],
    pub env: &'a BTreeMap<String, SecretString>,
    pub env_names: &'a [String],
    pub env_literals: &'a [(String, String)],
    pub vendor_args: &'a [String],
    pub stdin: &'a [u8],
    pub timeout: Duration,
    pub cancel: CancellationToken,
    pub observer: &'a dyn super::runtime::StreamObserver,
    pub final_event: Option<super::final_event::FinalEventRule>,
}

/// DVC-FR-DCQG: the `docker exec` arguments of one turn.
pub fn exec_args(
    running: &RunningDevcontainer,
    worktree: &str,
    env_file: &Path,
    turn: &str,
    env_names: &[String],
    cli: &str,
    vendor_args: &[String],
) -> Vec<String> {
    let mut args = vec!["exec".to_string(), "-i".to_string()];
    if !running.remote_user.is_empty() {
        args.extend(["-u".to_string(), running.remote_user.clone()]);
    }
    args.extend([
        "-w".to_string(),
        worktree.to_string(),
        "--env-file".to_string(),
        env_file.to_string_lossy().to_string(),
        "-e".to_string(),
        format!("SYNTHESIS_TURN={turn}"),
    ]);
    for name in env_names {
        args.extend(["-e".to_string(), name.clone()]);
    }
    args.push(running.container_id.clone());
    args.push(cli.to_string());
    args.extend(vendor_args.iter().cloned());
    args
}

/// DVC-FR-DCQG: the env file of one turn: the probed environment, then the
/// literal variables of the launch, which win. A credential name is never in
/// it, and a value that holds a line break cannot be one line of the file.
pub fn env_file_text(
    probed: &[(String, String)],
    literals: &[(String, String)],
    credential_names: &[String],
) -> String {
    let mut merged: BTreeMap<&str, &str> = BTreeMap::new();
    for (name, value) in probed.iter().chain(literals.iter()) {
        merged.insert(name, value);
    }
    let mut text = String::new();
    for (name, value) in merged {
        if credential_names.iter().any(|n| n == name) || value.contains('\n') {
            continue;
        }
        text.push_str(name);
        text.push('=');
        text.push_str(value);
        text.push('\n');
    }
    text
}

/// DVC-FR-QGUT: the folder under the application data folder that holds the
/// generated config and the env files of one scope. `data` is the application
/// data folder, or the folder a test names in its place.
fn scope_folder(data: &Path, scope_key: &str) -> PathBuf {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(scope_key.as_bytes());
    let short: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    data.join("devcontainers").join(short)
}

/// DVC-FR-QGUT: a path that the `--mount` form of a config cannot carry. The
/// form separates its fields with commas.
fn unmountable(path: &str) -> bool {
    path.contains(',') || path.contains('\n')
}

impl AgentCliExecutor {
    /// DVC-FR-IFBK: the data folder the scope folders are under. A test that
    /// names its own session-state root keeps them beside it.
    fn devcontainer_data(&self) -> Result<PathBuf, String> {
        match &self.session_state_root {
            Some(root) => Ok(root.join("devcontainer-data")),
            None => crate::fs::app_data_dir().map_err(|e| e.to_string()),
        }
    }

    fn devcontainer_pool(&self) -> &DevcontainerPool {
        match &self.devcontainer_pool {
            Some(pool) => pool,
            None => super::devcontainer_pool::production_pool(),
        }
    }

    /// EAC-FR-GOAR: run one turn in the dev container of the launch.
    pub(super) async fn run_in_devcontainer<S>(
        &self,
        sink: &S,
        context: &LaunchContext<'_>,
        run: DevcontainerRun<'_>,
    ) -> Result<RuntimeOutcome, AgentExecutionError>
    where
        S: LogSink + Clone + Send + Sync + 'static,
    {
        let failed = |failure| AgentExecutionError::DevcontainerLaunchFailed(failure);
        let start_failed = |message: String| failed(DevcontainerFailure::StartFailed(message));
        // DVC-FR-ATEO: every start failure before the pool is logged with a
        // fixed reason, never with the config or a line of the CLI.
        let start_failed_because = |reason: &'static str, message: String| {
            logging::log_error(
                sink,
                &BUFFER,
                &[Domain::Ai, Domain::Backend],
                "dev container could not start",
                log_fields! { "cause" => "devcontainer_start_failed", "reason" => reason },
            );
            start_failed(message)
        };
        let cli = config::cli_name(run.vendor)
            .ok_or_else(|| AgentExecutionError::UnsupportedIntegration(run.vendor.to_string()))?;
        let parsed = config::parse_config(&run.launch.config_text)
            .map_err(|e| start_failed_because("config_invalid", e.message))?;

        let scope_key = match run.scope {
            Some(scope) => super::sandbox::vm_name(context.project_key, scope),
            None => format!("{}-once-{}", context.project_key, descriptor::new_session_id()),
        };
        // DVC-FR-QGUT: the worktree must have its host path in the container,
        // and no path may break the mount form.
        if !run.aligned
            || unmountable(run.worktree)
            || run.mounts.iter().any(|m| unmountable(&m.source) || unmountable(&m.target))
        {
            return Err(start_failed_because(
                "path_unmountable",
                "a path of this launch cannot be mounted at its own path in a dev container"
                    .to_string(),
            ));
        }
        let data = self.devcontainer_data().map_err(|e| start_failed_because("data_folder", e))?;
        let folder = scope_folder(&data, &scope_key);
        let mounts: Vec<LaunchMount> = run
            .mounts
            .iter()
            .map(|m| LaunchMount {
                source: m.source.clone(),
                target: m.target.clone(),
                read_only: m.read_only,
            })
            .collect();
        let generated = config::generated_config(&parsed, run.worktree, &mounts, &[]);
        let override_text = serde_json::to_string_pretty(&generated).unwrap_or_default();
        let override_path = folder.join("devcontainer.json");
        context
            .fs
            .write_text_atomic(&override_path, &override_text)
            .map_err(|e| start_failed_because("override_not_written", e.to_string()))?;
        // DVC-FR-DCQG: the folder is the owner's alone, so an env file in it
        // is never readable by another user, also while it is written.
        restrict_to_owner(&folder, 0o700);
        let start = DevcontainerStart {
            scope_key: scope_key.clone(),
            project_key: context.project_key.to_string(),
            cli: run.launch.cli.clone(),
            docker: run.launch.docker.clone(),
            worktree: run.worktree.to_string(),
            config: run.launch.config.clone(),
            config_hash: run.launch.config_hash.clone(),
            override_path,
            override_text,
            compose: config::is_compose(&parsed),
        };

        let docker = run.launch.docker.clone();
        let turn_id = descriptor::new_session_id();
        let env_file = folder.join(format!("turn-{turn_id}.env"));
        let pool = self.devcontainer_pool();
        let runtime = pool.runtime().clone();
        let begun = std::time::Instant::now();
        let turn = |running: RunningDevcontainer| {
            let env_file = env_file.clone();
            let docker = docker.clone();
            let turn_id = turn_id.clone();
            let runtime = runtime.clone();
            let run = &run;
            async move {
                let text = env_file_text(&running.probed_env, run.env_literals, run.env_names);
                context
                    .fs
                    .write_text_atomic(&env_file, &text)
                    .map_err(|e| RuntimeError::LaunchFailed(e.to_string()))?;
                restrict_to_owner(&env_file, 0o600);
                let argv = exec_args(
                    &running,
                    run.worktree,
                    &env_file,
                    &turn_id,
                    run.env_names,
                    cli,
                    run.vendor_args,
                );
                let outcome = HostDockerCli::with_program(docker.clone())
                    .run(RunRequest {
                        argv: &argv,
                        container: None,
                        env: run.env,
                        stdin: run.stdin,
                        timeout: run.timeout,
                        cancel: run.cancel.clone(),
                        stdout_limit: protocol::LIMIT_STDOUT,
                        stderr_limit: protocol::LIMIT_STDERR,
                        observer: Some(run.observer),
                        final_event: run.final_event,
                    })
                    .await;
                let _ = context.fs.delete_path(&env_file, false);
                if let Ok(outcome) = &outcome {
                    if matches!(
                        outcome.end,
                        RunEnd::TimedOut | RunEnd::Cancelled | RunEnd::EndedAfterFinalEvent
                    ) {
                        runtime.end_turn(&docker, &running.container_id, &turn_id).await;
                    }
                }
                outcome
            }
        };
        let started = match run.scope {
            Some(_) => pool.with_container(&start, &run.cancel, turn).await,
            None => pool.with_container_once(&start, &run.cancel, turn).await,
        };
        // DVC-FR-OHMC: the folder of a launch with no scope goes with it.
        if run.scope.is_none() {
            let _ = context.fs.delete_path(&folder, true);
        }
        let outcome = match started {
            // DVC-FR-SFWW: a start that the caller cancelled is a cancelled run,
            // as the cancellation of a Docker launch is.
            Err(_) if run.cancel.is_cancelled() => {
                return Ok(RuntimeOutcome {
                    end: RunEnd::Cancelled,
                    exit_code: None,
                    stdout: Default::default(),
                    stderr: Default::default(),
                });
            }
            Err(failure) => {
                let line: String = failure
                    .0
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("")
                    .chars()
                    .take(super::devcontainer_pool::FAILURE_LINE_LIMIT)
                    .collect();
                logging::log_error(
                    sink,
                    &BUFFER,
                    &[Domain::Ai, Domain::Backend],
                    "dev container could not start",
                    log_fields! {
                        "scope" => scope_key.as_str(),
                        "cause" => DevcontainerFailure::StartFailed(String::new()).cause(),
                        "message" => line.as_str()
                    },
                );
                return Err(start_failed(line));
            }
            Ok(outcome) => outcome,
        };
        let outcome = outcome.map_err(|error| match error {
            RuntimeError::Unavailable => AgentExecutionError::RuntimeUnavailable,
            RuntimeError::ImageUnavailable => AgentExecutionError::RuntimeUnavailable,
            RuntimeError::LaunchFailed(reason) => AgentExecutionError::LaunchFailed(reason),
        })?;
        logging::log_info(
            sink,
            &BUFFER,
            &[Domain::Ai, Domain::Backend],
            "dev container turn finished",
            log_fields! {
                "scope" => scope_key.as_str(),
                "end" => format!("{:?}", outcome.end),
                "exit_code" => outcome.exit_code.unwrap_or(-1) as i64,
                "duration_ms" => begun.elapsed().as_millis() as u64
            },
        );
        // DVC-FR-ODWK: the CLI was not found before it wrote anything.
        if outcome.end == RunEnd::Exited
            && outcome.exit_code == Some(127)
            && outcome.stdout.bytes.is_empty()
        {
            let failure = DevcontainerFailure::AgentCliMissing(cli.to_string());
            logging::log_error(
                sink,
                &BUFFER,
                &[Domain::Ai, Domain::Backend],
                "dev container does not hold the agent CLI",
                log_fields! { "scope" => scope_key.as_str(), "cause" => failure.cause(), "cli" => cli },
            );
            return Err(failed(failure));
        }
        Ok(outcome)
    }
}

/// DVC-FR-DCQG: an owner-only mode for the scope folder and its env files.
fn restrict_to_owner(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // FSA-FR-19 exception, deliberate: the entry is one the executor itself
        // just wrote under the application data folder, and FsAccess has no
        // operation that sets a mode.
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
}

#[cfg(test)]
#[path = "tests/devcontainer_launch_unit.rs"]
mod tests;
