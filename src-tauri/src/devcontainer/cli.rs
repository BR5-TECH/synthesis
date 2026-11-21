//! The dev container CLI of this machine (DVC-FR-WOAM, DVC-FR-BAID,
//! DVC-FR-XZCD, DVC-FR-GNOK).
//!
//! The path and its verification are user-global (GSS-FR-MVLQ): how this
//! machine reaches `@devcontainers/cli` is a fact of the machine, never of a
//! project.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::agentic::FileProbe;

/// DVC-FR-XZCD: the executable name of the dev container CLI.
pub const DEVCONTAINER_EXECUTABLE_NAMES: &[&str] = &["devcontainer"];
/// DVC-FR-BAID: how long `--version` may run.
pub const VERIFY_TIMEOUT: Duration = Duration::from_secs(15);

/// DVC-FR-HINW: the dev container CLI is not verified.
pub const ERR_DEVCONTAINER_CLI_MISSING: &str = "devcontainer_cli_missing";
/// DVC-FR-GNOK: no Docker CLI was found.
pub const ERR_DOCKER_CLI_REQUIRED: &str = "docker_cli_required";

/// DVC-FR-WOAM: what a verification recorded, against the path that earned it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DevcontainerCliVerification {
    pub path: String,
    pub version: String,
    pub verified_at: String,
}

/// DVC-FR-WOAM: the stored record in `synthesis.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DevcontainerCliRecord {
    /// Empty means the path `detect_devcontainer_cli` finds.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified: Option<DevcontainerCliVerification>,
}

/// DVC-FR-WOAM: the verification state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DevcontainerCliState {
    Unverified,
    Verified,
}

/// DVC-FR-WOAM: the shape the commands return.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevcontainerCli {
    pub path: String,
    pub state: DevcontainerCliState,
    pub version: Option<String>,
    pub verified_at: Option<String>,
}

impl DevcontainerCliRecord {
    /// DVC-FR-BAID: the verification counts only for the path that earned it.
    pub fn current_verification(&self) -> Option<&DevcontainerCliVerification> {
        self.verified.as_ref().filter(|v| self.path.is_empty() || v.path == self.path)
    }

    pub fn view(&self) -> DevcontainerCli {
        match self.current_verification() {
            Some(v) => DevcontainerCli {
                path: self.path.clone(),
                state: DevcontainerCliState::Verified,
                version: Some(v.version.clone()),
                verified_at: Some(v.verified_at.clone()),
            },
            None => DevcontainerCli {
                path: self.path.clone(),
                state: DevcontainerCliState::Unverified,
                version: None,
                verified_at: None,
            },
        }
    }

    /// DVC-FR-XZCD: a changed path is stored unverified.
    pub fn with_path(&self, path: &str) -> DevcontainerCliRecord {
        let path = path.trim().to_string();
        if path == self.path {
            return self.clone();
        }
        DevcontainerCliRecord { path, verified: None }
    }
}

/// DVC-FR-BAID: why a verification failed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevcontainerCliFailure {
    pub code: &'static str,
    pub message: String,
}

impl DevcontainerCliFailure {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        DevcontainerCliFailure { code, message: message.into() }
    }
}

/// DVC-FR-XZCD: the folders searched, in order: the login `PATH`, the usual
/// install folders, and the usual folders of npm and pnpm global packages.
pub fn detection_directories(path_var: Option<&str>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = crate::agentic::detection_directories(path_var, home);
    if let Some(home) = home {
        for suffix in ["Library/pnpm", ".local/share/pnpm", ".yarn/bin"] {
            dirs.push(home.join(suffix));
        }
    }
    dirs
}

/// DVC-FR-XZCD: the first `devcontainer` executable, or `None`. Stores nothing
/// and runs nothing.
pub fn detect(
    path_var: Option<&str>,
    home: Option<&Path>,
    probe: &dyn FileProbe,
) -> Option<String> {
    crate::agentic::detect_in(
        &detection_directories(path_var, home),
        DEVCONTAINER_EXECUTABLE_NAMES,
        probe,
    )
}

/// The `PATH` a run of the CLI gets: the folder of the CLI first, because the
/// CLI is a Node script and its interpreter is usually installed beside it,
/// then the folder of the Docker CLI, then the usual install folders, then the
/// `PATH` of the application.
pub fn run_path(cli: &str, docker: Option<&str>) -> OsString {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let inherited = std::env::var("PATH").ok();
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(parent) = Path::new(cli).parent() {
        dirs.push(parent.to_path_buf());
    }
    if let Some(parent) = docker.and_then(|d| Path::new(d).parent()) {
        dirs.push(parent.to_path_buf());
    }
    dirs.extend(detection_directories(None, home.as_deref()));
    if let Some(inherited) = inherited.as_deref() {
        dirs.extend(std::env::split_paths(inherited));
    }
    let mut unique: Vec<PathBuf> = Vec::new();
    for dir in dirs {
        if !dir.as_os_str().is_empty() && !unique.contains(&dir) {
            unique.push(dir);
        }
    }
    std::env::join_paths(unique).unwrap_or_default()
}

/// A command that runs the dev container CLI with the `PATH` of [`run_path`].
pub fn command(cli: &str, docker: Option<&str>) -> tokio::process::Command {
    use crate::embeddings::InheritedEnvironment;
    let mut command = tokio::process::Command::new(cli);
    command.with_inherited_hub_environment().env("PATH", run_path(cli, docker));
    command
}

/// DVC-FR-BAID: run `<path> --version` within [`VERIFY_TIMEOUT`], and return
/// the version it prints.
pub async fn verify(path: &str, timeout: Duration) -> Result<String, DevcontainerCliFailure> {
    let file = Path::new(path);
    if !crate::agentic::RealFileProbe.exists(file) {
        return Err(DevcontainerCliFailure::new("not_found", path));
    }
    if !crate::agentic::RealFileProbe.is_executable(file) {
        return Err(DevcontainerCliFailure::new("not_executable", path));
    }
    let mut command = command(path, None);
    command
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let child =
        command.spawn().map_err(|e| DevcontainerCliFailure::new("failed", e.to_string()))?;
    let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Err(_) => return Err(DevcontainerCliFailure::new("timed_out", "")),
        Ok(Err(e)) => return Err(DevcontainerCliFailure::new("failed", e.to_string())),
        Ok(Ok(output)) => output,
    };
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let line = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
        return Err(DevcontainerCliFailure::new(
            "failed",
            line.chars().take(300).collect::<String>(),
        ));
    }
    Ok(stdout.lines().next().unwrap_or("").to_string())
}

/// DVC-FR-HINW: the path of the verified dev container CLI, or
/// `devcontainer_cli_missing`. Runs nothing.
pub fn resolve_devcontainer_cli(record: &DevcontainerCliRecord) -> Result<String, &'static str> {
    record.current_verification().map(|v| v.path.clone()).ok_or(ERR_DEVCONTAINER_CLI_MISSING)
}

/// DVC-FR-GNOK: the Docker CLI every run of the dev container CLI names: the
/// path of the Docker backend where it is set, otherwise the one detection
/// finds. Runs nothing.
pub fn resolve_docker_cli(
    backend: &crate::docker::DockerBackendRecord,
    probe: &dyn FileProbe,
) -> Result<String, &'static str> {
    if let Some(path) = backend.cli_path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        if probe.exists(Path::new(path)) {
            return Ok(path.to_string());
        }
    }
    let path_var = std::env::var("PATH").ok();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    crate::docker::detect_docker_cli(path_var.as_deref(), home.as_deref(), probe)
        .ok_or(ERR_DOCKER_CLI_REQUIRED)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Probe(Vec<&'static str>);

    impl FileProbe for Probe {
        fn exists(&self, path: &Path) -> bool {
            self.0.iter().any(|p| Path::new(p) == path)
        }
        fn is_executable(&self, path: &Path) -> bool {
            self.exists(path)
        }
    }

    // DVC-FR-XZCD: the login PATH comes first, then the npm global folders.
    #[test]
    fn detection_finds_the_first_executable_in_order() {
        let probe = Probe(vec!["/home/a/.npm-global/bin/devcontainer", "/custom/devcontainer"]);
        let found = detect(Some("/custom"), Some(Path::new("/home/a")), &probe);
        assert_eq!(found.as_deref(), Some("/custom/devcontainer"));
        let found = detect(Some("/elsewhere"), Some(Path::new("/home/a")), &probe);
        assert_eq!(found.as_deref(), Some("/home/a/.npm-global/bin/devcontainer"));
        assert_eq!(detect(None, None, &Probe(vec![])), None);
    }

    // DVC-FR-XZCD / DVC-FR-BAID: a changed path is unverified, and the
    // verification counts only for the path that earned it.
    #[test]
    fn a_changed_path_is_unverified() {
        let record = DevcontainerCliRecord {
            path: "/a/devcontainer".to_string(),
            verified: Some(DevcontainerCliVerification {
                path: "/a/devcontainer".to_string(),
                version: "0.91.0".to_string(),
                verified_at: "t".to_string(),
            }),
        };
        assert_eq!(record.view().state, DevcontainerCliState::Verified);
        assert_eq!(record.view().version.as_deref(), Some("0.91.0"));
        assert_eq!(record.with_path("/a/devcontainer"), record);
        let changed = record.with_path("/b/devcontainer");
        assert_eq!(changed.view().state, DevcontainerCliState::Unverified);
        assert_eq!(resolve_devcontainer_cli(&changed), Err(ERR_DEVCONTAINER_CLI_MISSING));
        assert_eq!(resolve_devcontainer_cli(&record).as_deref(), Ok("/a/devcontainer"));
    }

    // DVC-FR-HINW: a machine that never verified has no CLI.
    #[test]
    fn nothing_verified_resolves_to_missing() {
        let record = DevcontainerCliRecord::default();
        assert_eq!(record.view().state, DevcontainerCliState::Unverified);
        assert_eq!(resolve_devcontainer_cli(&record), Err(ERR_DEVCONTAINER_CLI_MISSING));
    }

    // DVC-FR-GNOK: the Docker CLI of the backend wins, and detection is the
    // fallback.
    #[test]
    fn the_docker_cli_comes_from_the_backend_first() {
        let probe = Probe(vec!["/opt/docker", "/usr/local/bin/docker"]);
        let backend = crate::docker::DockerBackendRecord {
            cli_path: Some("/opt/docker".to_string()),
            ..Default::default()
        };
        assert_eq!(resolve_docker_cli(&backend, &probe).as_deref(), Ok("/opt/docker"));
        let backend = crate::docker::DockerBackendRecord::default();
        assert_eq!(resolve_docker_cli(&backend, &probe).as_deref(), Ok("/usr/local/bin/docker"));
        assert_eq!(resolve_docker_cli(&backend, &Probe(vec![])), Err(ERR_DOCKER_CLI_REQUIRED));
    }

    // DVC-FR-BAID: each failure is its own code.
    #[tokio::test]
    async fn verify_reports_each_failure() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("devcontainer");
        let failure = verify(missing.to_str().unwrap(), VERIFY_TIMEOUT).await.unwrap_err();
        assert_eq!(failure.code, "not_found");

        let plain = dir.path().join("plain");
        std::fs::write(&plain, "x").unwrap();
        let failure = verify(plain.to_str().unwrap(), VERIFY_TIMEOUT).await.unwrap_err();
        assert_eq!(failure.code, "not_executable");
    }

    // DVC-FR-BAID: a version is read from the output, a failing run is
    // `failed`, and a run over the limit is `timed_out`.
    #[cfg(unix)]
    #[tokio::test]
    async fn verify_reads_the_version_and_bounds_the_run() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = |name: &str, body: &str| {
            let path = dir.path().join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path.to_string_lossy().to_string()
        };
        let good = script("good", "echo 0.91.0");
        assert_eq!(verify(&good, VERIFY_TIMEOUT).await.unwrap(), "0.91.0");
        let bad = script("bad", "echo broken >&2; exit 3");
        let failure = verify(&bad, VERIFY_TIMEOUT).await.unwrap_err();
        assert_eq!((failure.code, failure.message.as_str()), ("failed", "broken"));
        let slow = script("slow", "exec sleep 30");
        let failure = verify(&slow, Duration::from_millis(200)).await.unwrap_err();
        assert_eq!(failure.code, "timed_out");
    }

    // DVC-FR-GNOK: the folder of the CLI and of the Docker CLI lead the PATH.
    #[test]
    fn the_run_path_puts_the_cli_folders_first() {
        let path = run_path("/x/bin/devcontainer", Some("/y/bin/docker"));
        let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
        assert_eq!(dirs[0], PathBuf::from("/x/bin"));
        assert_eq!(dirs[1], PathBuf::from("/y/bin"));
    }
}
