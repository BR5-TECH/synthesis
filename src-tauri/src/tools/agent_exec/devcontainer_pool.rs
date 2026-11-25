//! The dev containers of the executor (DVC-FR-LKRY, DVC-FR-DELL, DVC-FR-OJEH,
//! DVC-FR-DTWX, DVC-FR-JXEA): one container for each scope, started with
//! `devcontainer up`, used by every turn of that scope, and removed where a VM
//! of a scope stops.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use super::runtime::{BoxFuture, CancellationToken};
use crate::devcontainer::process::{self, ProcessEnd};

/// DVC-FR-ATEO: the longest `devcontainer up` may run.
pub const UP_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// The longest one removal or one helper exec may run.
const HELPER_TIMEOUT: Duration = Duration::from_secs(60);
/// DVC-FR-ATEO: the most characters of the CLI's line a start failure carries.
pub const FAILURE_LINE_LIMIT: usize = 300;
/// DVC-FR-QGUT: the id label that names the scope of a container.
pub const SCOPE_LABEL: &str = "synthesis.scope";
/// DVC-FR-QGUT: the id label that names the project of a container.
pub const PROJECT_LABEL: &str = "synthesis.project";
/// Variables of the probed environment that describe the probe's own shell,
/// not the environment of the remote user.
const SHELL_OWN_VARIABLES: &[&str] = &["_", "PWD", "OLDPWD", "SHLVL", "HOSTNAME"];

/// DVC-FR-DELL: everything a scope's container is started with. Two launches
/// whose starts are equal share one container.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevcontainerStart {
    /// The scope key, also the value of the scope id label.
    pub scope_key: String,
    pub project_key: String,
    pub cli: String,
    pub docker: String,
    pub worktree: String,
    pub config: PathBuf,
    /// DVC-FR-DELL: the hash of the user's config and the files it names.
    pub config_hash: String,
    /// DVC-FR-QGUT: where the generated config is written, and its text.
    pub override_path: PathBuf,
    pub override_text: String,
    /// DVC-FR-JXEA: the config starts compose services.
    pub compose: bool,
}

impl DevcontainerStart {
    /// The config arguments every `devcontainer up` and `exec` of the scope
    /// takes (DVC-FR-QGUT, DVC-FR-BUXC, DVC-FR-GNOK).
    pub fn config_args(&self) -> Vec<String> {
        vec![
            "--workspace-folder".to_string(),
            self.worktree.clone(),
            "--config".to_string(),
            self.config.to_string_lossy().to_string(),
            "--override-config".to_string(),
            self.override_path.to_string_lossy().to_string(),
            "--id-label".to_string(),
            format!("{SCOPE_LABEL}={}", self.scope_key),
            "--id-label".to_string(),
            format!("{PROJECT_LABEL}={}", self.project_key),
            "--docker-path".to_string(),
            self.docker.clone(),
        ]
    }

    /// DVC-FR-QGUT: the arguments of `devcontainer up`.
    pub fn up_args(&self) -> Vec<String> {
        let mut args = vec!["up".to_string()];
        args.extend(self.config_args());
        args.extend([
            "--mount-workspace-git-root=false".to_string(),
            "--log-format".to_string(),
            "text".to_string(),
        ]);
        args
    }

    /// DVC-FR-BUXC: the arguments of the `devcontainer exec` that prints the
    /// environment of the remote user.
    pub fn env_probe_args(&self) -> Vec<String> {
        let mut args = vec!["exec".to_string()];
        args.extend(self.config_args());
        args.extend(["sh".to_string(), "-c".to_string(), "env -0 2>/dev/null || env".to_string()]);
        args
    }
}

/// DVC-FR-BUXC: what a start gives back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunningDevcontainer {
    pub container_id: String,
    pub remote_user: String,
    pub remote_workspace_folder: String,
    /// DVC-FR-BUXC: the environment of the remote user, without the variables
    /// of the probe's own shell.
    pub probed_env: Vec<(String, String)>,
}

/// DVC-FR-BUXC: read `containerId`, `remoteUser` and `remoteWorkspaceFolder`
/// from the JSON result of `devcontainer up`. An error carries why.
pub fn parse_up_result(stdout: &str) -> Result<(String, String, String), String> {
    let result = stdout
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        // No result: the caller names the CLI's last line instead.
        .ok_or_else(String::new)?;
    let text = |key: &str| result.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string();
    if text("outcome") != "success" {
        let message = text("message");
        let description = text("description");
        return Err(if message.is_empty() { description } else { message });
    }
    let container_id = text("containerId");
    if container_id.is_empty() {
        return Err("the dev container CLI returned no container id".to_string());
    }
    Ok((container_id, text("remoteUser"), text("remoteWorkspaceFolder")))
}

/// DVC-FR-BUXC: the probed environment, from `env -0` output, or from `env`
/// output where it holds no NUL.
pub fn parse_probed_env(stdout: &str) -> Vec<(String, String)> {
    let entries: Vec<&str> =
        if stdout.contains('\0') { stdout.split('\0').collect() } else { stdout.lines().collect() };
    entries
        .into_iter()
        .filter_map(|entry| {
            let (name, value) = entry.split_once('=')?;
            let name = name.trim_start_matches('\n');
            let valid = !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !SHELL_OWN_VARIABLES.contains(&name);
            valid.then(|| (name.to_string(), value.to_string()))
        })
        .collect()
}

/// DVC-FR-SFWW: the script that ends every process of one turn. Each process
/// that a turn starts inherits its marker variable.
///
/// DVC-FR-TQHM: it exits 0 only where a scan finds no process of the turn
/// left, and it scans once more after the last kill. A container without the
/// tools of the scan cannot confirm anything, so the script then exits 1.
pub fn end_turn_script() -> &'static str {
    "command -v tr >/dev/null 2>&1 && command -v grep >/dev/null 2>&1 || exit 1; \
     scan() { left=0; for p in /proc/[0-9]*; do \
     if tr '\\0' '\\n' 2>/dev/null < \"$p/environ\" | grep -qx \"SYNTHESIS_TURN=$1\"; then \
     left=1; kill -KILL \"${p#/proc/}\" 2>/dev/null; fi; done; }; \
     for i in 1 2 3 4 5; do scan \"$1\"; [ \"$left\" = 0 ] && exit 0; sleep 1; done; \
     scan \"$1\"; [ \"$left\" = 0 ] && exit 0; exit 1"
}

/// The seam a launch uses (DVC contract surface, "Internal").
pub trait DevcontainerRuntime: Send + Sync {
    /// DVC-FR-QGUT / DVC-FR-BUXC: start the container of `start`.
    fn up<'a>(
        &'a self,
        start: &'a DevcontainerStart,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<RunningDevcontainer, String>>;

    /// DVC-FR-SFWW: end every process of the turn `turn` in `container`.
    /// DVC-FR-TQHM: answers whether no process of the turn is left.
    fn end_turn<'a>(
        &'a self,
        docker: &'a str,
        container: &'a str,
        turn: &'a str,
    ) -> BoxFuture<'a, bool>;

    /// DVC-FR-JXEA: remove every container that has the scope's id label. An
    /// error names the step that failed and why, in words with no output of
    /// the step in them.
    fn remove<'a>(
        &'a self,
        docker: &'a str,
        scope_key: &'a str,
        compose: bool,
    ) -> BoxFuture<'a, Result<(), String>>;
}

/// DVC-FR-DTWX: one removal that failed: the scope key and why.
pub type RemovalFailure = (String, String);

/// Production: the dev container CLI and the Docker CLI of the host.
pub struct CliDevcontainerRuntime;

/// One Docker CLI call. An error says why in words: the CLI did not start,
/// ran over its limit, or exited with a code.
async fn docker_output(docker: &str, args: &[&str]) -> Result<String, String> {
    use crate::embeddings::InheritedEnvironment;
    let mut command = tokio::process::Command::new(docker);
    command.with_inherited_hub_environment().args(args);
    let step = args.first().copied().unwrap_or("docker");
    let result = process::run(command, None, HELPER_TIMEOUT, &CancellationToken::new(), |_| {})
        .await
        .map_err(|e| format!("docker {step} did not start: {}", e.kind()))?;
    match (result.end, result.exit_code) {
        (ProcessEnd::Exited, Some(0)) => Ok(result.stdout),
        (ProcessEnd::Exited, code) => {
            Err(format!("docker {step} exited with code {}", code.unwrap_or(-1)))
        }
        _ => Err(format!("docker {step} ran longer than its time limit")),
    }
}

/// DVC-FR-QGUT / DVC-FR-BUXC: `devcontainer up` and the environment probe,
/// with each line that `up` writes handed to `on_line` (DVC-FR-KTBH).
pub async fn up_reporting(
    start: &DevcontainerStart,
    cancel: &CancellationToken,
    mut on_line: impl FnMut(&str) + Send,
) -> Result<RunningDevcontainer, String> {
    let mut command = crate::devcontainer::cli::command(&start.cli, Some(&start.docker));
    command.args(start.up_args());
    let result = process::run(command, None, UP_TIMEOUT, cancel, &mut on_line)
        .await
        .map_err(|e| e.to_string())?;
    match result.end {
        ProcessEnd::TimedOut => {
            return Err("devcontainer up ran longer than its time limit".to_string());
        }
        ProcessEnd::Cancelled => return Err("the start was cancelled".to_string()),
        ProcessEnd::Exited => {}
    }
    let (container_id, remote_user, remote_workspace_folder) = parse_up_result(&result.stdout)
        .map_err(|message| {
            let line = result.last_error_line(FAILURE_LINE_LIMIT);
            match (message.is_empty(), line.is_empty()) {
                (false, _) => message,
                (true, false) => line,
                (true, true) => "the dev container CLI returned no result".to_string(),
            }
        })?;
    let mut command = crate::devcontainer::cli::command(&start.cli, Some(&start.docker));
    command.args(start.env_probe_args());
    let probed = process::run(command, None, HELPER_TIMEOUT, cancel, |_| {})
        .await
        .map_err(|e| e.to_string())?;
    if !probed.succeeded() {
        return Err(probed.last_error_line(FAILURE_LINE_LIMIT));
    }
    Ok(RunningDevcontainer {
        container_id,
        remote_user,
        remote_workspace_folder,
        probed_env: parse_probed_env(&probed.stdout),
    })
}

impl DevcontainerRuntime for CliDevcontainerRuntime {
    fn up<'a>(
        &'a self,
        start: &'a DevcontainerStart,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<RunningDevcontainer, String>> {
        Box::pin(up_reporting(start, cancel, |_| {}))
    }

    fn end_turn<'a>(
        &'a self,
        docker: &'a str,
        container: &'a str,
        turn: &'a str,
    ) -> BoxFuture<'a, bool> {
        Box::pin(async move {
            let args = ["exec", "-u", "root", container, "sh", "-c", end_turn_script(), "sh", turn];
            docker_output(docker, &args).await.is_ok()
        })
    }

    fn remove<'a>(
        &'a self,
        docker: &'a str,
        scope_key: &'a str,
        compose: bool,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            let filter = format!("label={SCOPE_LABEL}={scope_key}");
            let listed = docker_output(docker, &["ps", "-aq", "--filter", &filter]).await?;
            let ids: Vec<&str> = listed.split_whitespace().collect();
            if ids.is_empty() {
                return Ok(());
            }
            let mut projects: Vec<String> = Vec::new();
            if compose {
                let mut args = vec!["inspect", "--format", COMPOSE_PROJECT_FORMAT];
                args.extend(ids.iter().copied());
                if let Ok(out) = docker_output(docker, &args).await {
                    projects = out
                        .lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .map(String::from)
                        .collect();
                    projects.sort();
                    projects.dedup();
                }
            }
            let mut args = vec!["rm", "-f"];
            args.extend(ids.iter().copied());
            docker_output(docker, &args).await?;
            for project in projects {
                docker_output(docker, &["compose", "-p", &project, "down"]).await?;
            }
            Ok(())
        })
    }
}

/// DVC-FR-JXEA: the compose project label of a container.
const COMPOSE_PROJECT_FORMAT: &str = "{{index .Config.Labels \"com.docker.compose.project\"}}";

/// Under `cfg(test)` no test reaches the host Docker: a start fails.
#[cfg(test)]
pub struct NoDevcontainers;

#[cfg(test)]
impl DevcontainerRuntime for NoDevcontainers {
    fn up<'a>(
        &'a self,
        _start: &'a DevcontainerStart,
        _cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<RunningDevcontainer, String>> {
        Box::pin(async { Err("no dev container runtime in tests".to_string()) })
    }
    fn end_turn<'a>(&'a self, _: &'a str, _: &'a str, _: &'a str) -> BoxFuture<'a, bool> {
        Box::pin(async { true })
    }
    fn remove<'a>(&'a self, _: &'a str, _: &'a str, _: bool) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
}

/// The container of one scope, while it runs.
struct ScopeState {
    start: DevcontainerStart,
    running: RunningDevcontainer,
}

type Slot = Arc<tokio::sync::Mutex<Option<ScopeState>>>;

/// DVC-FR-DTWX: what a removal of a scope needs, kept from the start of its
/// `up`, because from then on a container of the scope can exist.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Tracked {
    docker: String,
    compose: bool,
    project_key: String,
}

/// DVC-FR-LKRY: the dev containers of this process, one for each scope.
pub struct DevcontainerPool {
    runtime: Arc<dyn DevcontainerRuntime>,
    slots: Mutex<HashMap<String, Slot>>,
    /// DVC-FR-DTWX: every scope that can have a container, also one whose
    /// start failed half way and one whose turn runs.
    tracked: Mutex<HashMap<String, Tracked>>,
}

/// DVC-FR-ATEO: why a scope has no container.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartFailed(pub String);

impl DevcontainerPool {
    pub fn new(runtime: Arc<dyn DevcontainerRuntime>) -> DevcontainerPool {
        DevcontainerPool {
            runtime,
            slots: Mutex::new(HashMap::new()),
            tracked: Mutex::new(HashMap::new()),
        }
    }

    pub fn runtime(&self) -> &Arc<dyn DevcontainerRuntime> {
        &self.runtime
    }

    fn slot(&self, scope_key: &str) -> Slot {
        let mut slots = self.slots.lock().unwrap_or_else(|e| e.into_inner());
        slots.entry(scope_key.to_string()).or_default().clone()
    }

    fn track(&self, start: &DevcontainerStart) {
        self.tracked.lock().unwrap_or_else(|e| e.into_inner()).insert(
            start.scope_key.clone(),
            Tracked {
                docker: start.docker.clone(),
                compose: start.compose,
                project_key: start.project_key.clone(),
            },
        );
    }

    fn tracked(&self, scope_key: &str) -> Option<Tracked> {
        self.tracked.lock().unwrap_or_else(|e| e.into_inner()).get(scope_key).cloned()
    }

    /// DVC-FR-DTWX: a removal that fails keeps the scope tracked, so a later
    /// stop or the next start of the scope tries again.
    async fn remove_tracked(&self, scope_key: &str) -> Result<(), String> {
        let Some(tracked) = self.tracked(scope_key) else { return Ok(()) };
        self.runtime.remove(&tracked.docker, scope_key, tracked.compose).await?;
        self.tracked.lock().unwrap_or_else(|e| e.into_inner()).remove(scope_key);
        Ok(())
    }

    /// DVC-FR-LKRY / DVC-FR-DELL / DVC-FR-OJEH: run `turn` in the container of
    /// `start`'s scope. The container is started where the scope has none, and
    /// replaced where its start differs. The scope is held while `turn` runs,
    /// so a stop waits for it (EAC-FR-LQRI).
    pub async fn with_container<T, F, Fut>(
        &self,
        start: &DevcontainerStart,
        cancel: &CancellationToken,
        turn: F,
    ) -> Result<T, StartFailed>
    where
        F: FnOnce(RunningDevcontainer) -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        let slot = self.slot(&start.scope_key);
        let mut held = slot.lock().await;
        let reuse = held.as_ref().is_some_and(|state| state.start == *start);
        if !reuse {
            // DVC-FR-OJEH / DVC-FR-DELL: whatever has the scope's labels goes
            // first: a container of an earlier process, of an earlier spec, or
            // of a start that failed half way. `up` would reuse it.
            let earlier = held.take().map(|state| state.start);
            let docker = earlier.as_ref().map_or(start.docker.as_str(), |e| e.docker.as_str());
            let compose = earlier.as_ref().is_some_and(|e| e.compose) || start.compose;
            // A failed removal does not block the start: `up` then reuses what
            // is left, and the next stop tries the removal again.
            let _ = self.runtime.remove(docker, &start.scope_key, compose).await;
            self.track(start);
            let running = self.runtime.up(start, cancel).await.map_err(StartFailed)?;
            *held = Some(ScopeState { start: start.clone(), running });
        }
        let running = held.as_ref().map(|state| state.running.clone()).expect("started above");
        Ok(turn(running).await)
    }

    /// DVC-FR-OHMC: run `turn` in a container of its own, and remove it after.
    pub async fn with_container_once<T, F, Fut>(
        &self,
        start: &DevcontainerStart,
        cancel: &CancellationToken,
        turn: F,
    ) -> Result<T, StartFailed>
    where
        F: FnOnce(RunningDevcontainer) -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        self.track(start);
        let outcome = match self.runtime.up(start, cancel).await {
            Ok(running) => Ok(turn(running).await),
            Err(message) => Err(StartFailed(message)),
        };
        let _ = self.remove_tracked(&start.scope_key).await;
        outcome
    }

    /// The scope has, or can have, a container.
    pub fn holds(&self, scope_key: &str) -> bool {
        self.tracked(scope_key).is_some()
    }

    /// DVC-FR-DTWX: remove the container of one scope. Waits for a turn that
    /// holds it.
    pub async fn stop(&self, scope_key: &str) -> Result<(), String> {
        self.stop_when(scope_key, || true).await.map(|_| ())
    }

    /// DVC-FR-DTWX / EAC-FR-LQRI: remove the container of a scope whose queue
    /// is still idle once the stop holds it.
    pub async fn stop_when(
        &self,
        scope_key: &str,
        still_idle: impl FnOnce() -> bool,
    ) -> Result<bool, String> {
        let slot = self.slots.lock().unwrap_or_else(|e| e.into_inner()).get(scope_key).cloned();
        let Some(slot) = slot else { return Ok(false) };
        let mut held = slot.lock().await;
        if self.tracked(scope_key).is_none() || !still_idle() {
            return Ok(false);
        }
        held.take();
        self.remove_tracked(scope_key).await?;
        Ok(true)
    }

    /// DVC-FR-DTWX: remove the containers of one project and of no other.
    pub async fn stop_project(&self, project_key: &str) -> Vec<RemovalFailure> {
        let keys: Vec<String> = {
            let tracked = self.tracked.lock().unwrap_or_else(|e| e.into_inner());
            tracked
                .iter()
                .filter(|(_, t)| t.project_key == project_key)
                .map(|(key, _)| key.clone())
                .collect()
        };
        let mut failures = Vec::new();
        for scope_key in keys {
            let slot = self.slot(&scope_key);
            let mut held = slot.lock().await;
            held.take();
            if let Err(reason) = self.remove_tracked(&scope_key).await {
                failures.push((scope_key, reason));
            }
        }
        failures
    }

    /// DVC-FR-DTWX: remove every container this pool started. A turn that runs
    /// is not waited for: the application is exiting.
    /// The removals run together, so one slow daemon call does not use the
    /// whole exit bound of the others.
    pub async fn stop_all(&self) -> Vec<RemovalFailure> {
        let slots: Vec<Slot> =
            self.slots.lock().unwrap_or_else(|e| e.into_inner()).values().cloned().collect();
        for slot in slots {
            if let Ok(mut held) = slot.try_lock() {
                held.take();
            }
        }
        let keys: Vec<String> =
            self.tracked.lock().unwrap_or_else(|e| e.into_inner()).keys().cloned().collect();
        let removals = keys.into_iter().map(|scope_key| async move {
            self.remove_tracked(&scope_key).await.err().map(|reason| (scope_key, reason))
        });
        futures_util::future::join_all(removals).await.into_iter().flatten().collect()
    }
}

/// The one pool of the application.
pub fn production_pool() -> &'static DevcontainerPool {
    static POOL: OnceLock<DevcontainerPool> = OnceLock::new();
    POOL.get_or_init(|| DevcontainerPool::new(production_runtime()))
}

#[cfg(not(test))]
fn production_runtime() -> Arc<dyn DevcontainerRuntime> {
    Arc::new(CliDevcontainerRuntime)
}

#[cfg(test)]
fn production_runtime() -> Arc<dyn DevcontainerRuntime> {
    Arc::new(NoDevcontainers)
}

#[cfg(test)]
#[path = "tests/devcontainer_pool_unit.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/devcontainer_uat.rs"]
mod uat;
