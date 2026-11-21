//! The build of a project's dev container (DVC-FR-OXCZ, DVC-FR-RTBA,
//! DVC-FR-EKAO, DVC-FR-LFQQ).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;

use super::process::{self, ProcessEnd};
use crate::project_settings::agent_environment::ProbeFinding;
use crate::tools::agent_exec::runtime::CancellationToken;

/// DVC-FR-RTBA: `"devcontainer build progress"`, emitted for each line of a
/// running build.
pub const DEVCONTAINER_BUILD_PROGRESS: &str = "devcontainer-build-progress";
/// DVC-FR-RTBA: `"devcontainer build finished"`, emitted once when a build
/// ends.
pub const DEVCONTAINER_BUILD_FINISHED: &str = "devcontainer-build-finished";
/// DVC-FR-OXCZ: a second build of the same project while one runs.
pub const ERR_BUILD_IN_FLIGHT: &str = "build_in_flight";
/// The longest a build may run. A build that pulls a large base image and many
/// Features can take many minutes, but none takes hours.
pub const BUILD_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);
/// DVC-FR-EKAO: the longest one probe of a built image may run.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(60);
/// DVC-FR-LFQQ: how many lines a build in flight keeps for a surface that opens
/// during it.
const KEPT_LINES: usize = 500;
/// DVC-FR-RTBA: the most characters of the last error line a result carries.
const ERROR_LINE_LIMIT: usize = 300;

/// DVC-FR-RTBA: one line of a running build.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevcontainerBuildProgress {
    pub operation_id: String,
    pub message: String,
}

/// DVC-FR-RTBA: how a build ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildOutcome {
    Succeeded,
    Failed,
    Cancelled,
}

/// DVC-FR-RTBA: the end of a build.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevcontainerBuildFinished {
    pub operation_id: String,
    pub outcome: BuildOutcome,
    pub image: Option<String>,
    pub error: Option<String>,
}

/// DVC-FR-LFQQ: the build that runs for a project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevcontainerBuildInFlight {
    pub operation_id: String,
    pub lines: Vec<String>,
}

/// Where a build reports.
pub trait BuildEvents: Send + Sync {
    fn progress(&self, payload: DevcontainerBuildProgress);
    fn finished(&self, payload: DevcontainerBuildFinished);
}

struct Running {
    operation_id: String,
    lines: Vec<String>,
    cancel: CancellationToken,
}

/// DVC-FR-OXCZ / DVC-FR-LFQQ: the builds that run, one at most for each
/// project root.
#[derive(Default)]
pub struct DevcontainerBuildRegistry {
    running: Mutex<HashMap<PathBuf, Running>>,
}

impl DevcontainerBuildRegistry {
    /// DVC-FR-OXCZ: claim the project for a build, or refuse.
    pub fn claim(&self, root: &Path, operation_id: &str) -> Result<CancellationToken, String> {
        let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        if running.contains_key(root) {
            return Err(ERR_BUILD_IN_FLIGHT.to_string());
        }
        let cancel = CancellationToken::new();
        running.insert(
            root.to_path_buf(),
            Running {
                operation_id: operation_id.to_string(),
                lines: Vec::new(),
                cancel: cancel.clone(),
            },
        );
        Ok(cancel)
    }

    fn push_line(&self, root: &Path, line: &str) {
        let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(build) = running.get_mut(root) {
            build.lines.push(line.to_string());
            if build.lines.len() > KEPT_LINES {
                build.lines.remove(0);
            }
        }
    }

    /// DVC-FR-RTBA: a build ends and frees its project.
    pub fn release(&self, root: &Path) {
        self.running.lock().unwrap_or_else(|e| e.into_inner()).remove(root);
    }

    /// DVC-FR-LFQQ: the build that runs for `root`, or `None`.
    pub fn in_flight(&self, root: &Path) -> Option<DevcontainerBuildInFlight> {
        let running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        running.get(root).map(|build| DevcontainerBuildInFlight {
            operation_id: build.operation_id.clone(),
            lines: build.lines.clone(),
        })
    }

    /// DVC-FR-RTBA: ask the named build to stop. An id that names no build
    /// does nothing.
    pub fn cancel(&self, operation_id: &str) {
        let running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        for build in running.values().filter(|b| b.operation_id == operation_id) {
            build.cancel.cancel();
        }
    }
}

/// DVC-FR-OXCZ: what one build runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildSpec {
    pub cli: String,
    pub docker: String,
    pub workspace: PathBuf,
    pub config: PathBuf,
    pub no_cache: bool,
    pub image_name: String,
}

/// DVC-FR-OXCZ: the image name a project's build produces. It holds a hash of
/// the project root, so two projects never share a name.
pub fn image_name_for(root: &Path) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(root.to_string_lossy().as_bytes());
    let short: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
    format!("synthesis-devcontainer-{short}:latest")
}

/// DVC-FR-OXCZ / DVC-FR-GNOK: the arguments of `devcontainer build`.
pub fn build_args(spec: &BuildSpec) -> Vec<String> {
    let mut args = vec![
        "build".to_string(),
        "--workspace-folder".to_string(),
        spec.workspace.to_string_lossy().to_string(),
        "--config".to_string(),
        spec.config.to_string_lossy().to_string(),
        "--docker-path".to_string(),
        spec.docker.clone(),
        "--image-name".to_string(),
        spec.image_name.clone(),
        "--log-format".to_string(),
        "text".to_string(),
    ];
    if spec.no_cache {
        args.push("--no-cache".to_string());
    }
    args
}

/// DVC-FR-OXCZ: the image the JSON result of `devcontainer build` names, or
/// `None` where the result is not a success.
pub fn built_image(stdout: &str, fallback: &str) -> Option<String> {
    let result = stdout
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())?;
    if result.get("outcome").and_then(|v| v.as_str()) != Some("success") {
        return None;
    }
    let named = match result.get("imageName") {
        Some(serde_json::Value::String(name)) => Some(name.clone()),
        Some(serde_json::Value::Array(names)) => {
            names.first().and_then(|v| v.as_str()).map(str::to_string)
        }
        _ => None,
    };
    Some(named.unwrap_or_else(|| fallback.to_string()))
}

/// DVC-FR-OXCZ / DVC-FR-RTBA: run one build. Each line reaches `events` and
/// the registry, and the one result is emitted at the end.
pub async fn run_build(
    spec: &BuildSpec,
    root: &Path,
    operation_id: &str,
    cancel: &CancellationToken,
    events: &dyn BuildEvents,
    registry: &DevcontainerBuildRegistry,
) -> DevcontainerBuildFinished {
    let mut command = super::cli::command(&spec.cli, Some(&spec.docker));
    command.args(build_args(spec));
    let result = process::run(command, None, BUILD_TIMEOUT, cancel, |line| {
        registry.push_line(root, line);
        events.progress(DevcontainerBuildProgress {
            operation_id: operation_id.to_string(),
            message: line.to_string(),
        });
    })
    .await;
    let finished = |outcome, image, error| DevcontainerBuildFinished {
        operation_id: operation_id.to_string(),
        outcome,
        image,
        error,
    };
    match result {
        Err(error) => finished(BuildOutcome::Failed, None, Some(error.to_string())),
        Ok(result) if result.end == ProcessEnd::Cancelled => {
            finished(BuildOutcome::Cancelled, None, None)
        }
        Ok(result) => match built_image(&result.stdout, &spec.image_name) {
            Some(image) if result.succeeded() => {
                finished(BuildOutcome::Succeeded, Some(image), None)
            }
            _ => {
                let error = if result.end == ProcessEnd::TimedOut {
                    "the build ran longer than its time limit".to_string()
                } else {
                    result.last_error_line(ERROR_LINE_LIMIT)
                };
                finished(BuildOutcome::Failed, None, Some(error))
            }
        },
    }
}

/// DVC-FR-EKAO: the arguments of one probe of `image` for `cli`.
pub fn probe_args(image: &str, cli: &str) -> Vec<String> {
    [
        "run",
        "--rm",
        "--network",
        "none",
        "--entrypoint",
        "sh",
        image,
        "-lc",
        &format!("command -v {cli}"),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// DVC-FR-EKAO: probe `image` for `cli`. `None` where the probe could not run:
/// the Docker CLI failed to start the container (exit code 125 and above).
pub async fn probe_image(docker: &str, image: &str, cli: &str) -> Option<ProbeFinding> {
    use crate::embeddings::InheritedEnvironment;
    let mut command = tokio::process::Command::new(docker);
    command.with_inherited_hub_environment().args(probe_args(image, cli));
    let result =
        process::run(command, None, PROBE_TIMEOUT, &CancellationToken::new(), |_| {}).await.ok()?;
    if result.end != ProcessEnd::Exited {
        return None;
    }
    match result.exit_code {
        Some(0) if !result.stdout.trim().is_empty() => Some(ProbeFinding::Found),
        Some(code) if code < 125 => Some(ProbeFinding::Missing),
        _ => None,
    }
}

#[cfg(test)]
#[path = "tests/build.rs"]
mod tests;
