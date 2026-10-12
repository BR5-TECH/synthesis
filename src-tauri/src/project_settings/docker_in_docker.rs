//! The Docker-in-Docker settings (PSS-FR-DIND, PSS-FR-QWTN, PSS-FR-VRHC,
//! PSS-FR-MTJE).
//!
//! Project-local: the Docker socket is a fact of one machine, and a setting that
//! gives a container access to the host Docker daemon is never committed. The
//! three values live in their own section of `.synthesis/local.toml`, so the
//! save carries every other section through unchanged.

use serde::{Deserialize, Serialize};
use tauri::State;

use super::{load_local, local_store_lock, write_local};
use crate::project::ProjectState;

/// The key the settings live under in `.synthesis/local.toml`.
const DOCKER_IN_DOCKER_KEY: &str = "docker_in_docker";

/// PSS-FR-VRHC: the path breaks the rule for a socket path.
pub const ERR_DOCKER_SOCKET_PATH_INVALID: &str = "docker_socket_path_invalid";
/// PSS-FR-VRHC: Docker-in-Docker cannot be enabled on this platform.
pub const ERR_DOCKER_IN_DOCKER_UNSUPPORTED: &str = "docker_in_docker_unsupported";

/// PSS-FR-QWTN: the socket path of Linux and macOS.
#[cfg(unix)]
const PLATFORM_DEFAULT_SOCKET: &str = "/var/run/docker.sock";
/// PSS-FR-QWTN: Windows has no Unix socket to default to.
#[cfg(not(unix))]
const PLATFORM_DEFAULT_SOCKET: &str = "";

/// PSS-FR-DIND: the three stored values.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DockerInDockerSettings {
    pub enabled: bool,
    /// Empty means the platform default.
    pub socket_path: String,
    pub host_gateway: bool,
}

/// PSS-FR-QWTN: how well the platform supports the feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockerInDockerSupport {
    Supported,
    BestEffort,
    Unsupported,
}

/// PSS-FR-QWTN: the stored values and the two values read from the platform.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerInDockerState {
    pub enabled: bool,
    pub socket_path: String,
    pub host_gateway: bool,
    pub default_socket_path: String,
    pub support: DockerInDockerSupport,
}

/// PSS-FR-MTJE: the stored values with the socket path the platform default
/// fills in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectiveDockerInDocker {
    pub enabled: bool,
    pub socket_path: String,
    pub host_gateway: bool,
    pub effective_socket_path: String,
}

/// PSS-FR-QWTN: the support level of the platform this build runs on.
pub fn platform_support() -> DockerInDockerSupport {
    if cfg!(target_os = "macos") {
        DockerInDockerSupport::BestEffort
    } else if cfg!(unix) {
        DockerInDockerSupport::Supported
    } else {
        DockerInDockerSupport::Unsupported
    }
}

/// PSS-FR-QWTN: each value is read on its own, so one damaged value costs only
/// itself and reads as its default.
pub fn load_docker_in_docker_settings_from(root: &crate::fs::RootFs) -> DockerInDockerSettings {
    let Some(section) = load_local(root).get(DOCKER_IN_DOCKER_KEY).cloned() else {
        return DockerInDockerSettings::default();
    };
    let field = |key: &str| section.get(key).cloned();
    DockerInDockerSettings {
        enabled: field("enabled").and_then(|v| v.try_into::<bool>().ok()).unwrap_or_default(),
        socket_path: field("socketPath")
            .and_then(|v| v.try_into::<String>().ok())
            .unwrap_or_default(),
        host_gateway: field("hostGateway")
            .and_then(|v| v.try_into::<bool>().ok())
            .unwrap_or_default(),
    }
}

/// PSS-FR-MTJE: the one function the application reads the settings through.
pub fn read_docker_in_docker_settings(root: &crate::fs::RootFs) -> EffectiveDockerInDocker {
    let stored = load_docker_in_docker_settings_from(root);
    let effective_socket_path = if stored.socket_path.is_empty() {
        PLATFORM_DEFAULT_SOCKET.to_string()
    } else {
        stored.socket_path.clone()
    };
    EffectiveDockerInDocker {
        enabled: stored.enabled,
        socket_path: stored.socket_path,
        host_gateway: stored.host_gateway,
        effective_socket_path,
    }
}

fn state_of(settings: DockerInDockerSettings) -> DockerInDockerState {
    DockerInDockerState {
        enabled: settings.enabled,
        socket_path: settings.socket_path,
        host_gateway: settings.host_gateway,
        default_socket_path: PLATFORM_DEFAULT_SOCKET.to_string(),
        support: platform_support(),
    }
}

/// PSS-FR-VRHC: the rule for a stored socket path. An empty path is the default.
fn validate_socket_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Ok(());
    }
    let absolute = path.starts_with('/') || std::path::Path::new(path).is_absolute();
    if !absolute || path.contains(',') || path.contains('\n') || path.contains('\r') {
        return Err(ERR_DOCKER_SOCKET_PATH_INVALID.to_string());
    }
    Ok(())
}

/// PSS-FR-QWTN / PSS-FR-VRHC: write the three values whole to project-local
/// scope, leaving every other section of `local.toml` as it was. The socket is
/// never checked here, because the author may start Docker after the save.
pub fn save_docker_in_docker_settings_to(
    root: &crate::fs::RootFs,
    settings: DockerInDockerSettings,
) -> Result<(), String> {
    let socket_path = settings.socket_path.trim().to_string();
    validate_socket_path(&socket_path)?;
    if settings.enabled && platform_support() == DockerInDockerSupport::Unsupported {
        return Err(ERR_DOCKER_IN_DOCKER_UNSUPPORTED.to_string());
    }
    let record = DockerInDockerSettings { socket_path, ..settings };
    let value = toml::Value::try_from(record)
        .map_err(|e| format!("failed to encode docker-in-docker settings: {e}"))?;
    let _guard = local_store_lock();
    let mut config = load_local(root);
    config.insert(DOCKER_IN_DOCKER_KEY.to_string(), value);
    write_local(root, &config)
}

#[tauri::command]
pub fn load_docker_in_docker_settings(
    project: State<'_, ProjectState>,
) -> Result<DockerInDockerState, String> {
    // No project open: the defaults, as a worktree with nothing stored.
    match project.require_root() {
        Ok(root) => Ok(state_of(load_docker_in_docker_settings_from(&root))),
        Err(_) => Ok(state_of(DockerInDockerSettings::default())),
    }
}

#[tauri::command]
pub fn save_docker_in_docker_settings(
    settings: DockerInDockerSettings,
    project: State<'_, ProjectState>,
) -> Result<(), String> {
    let root = project.require_root()?;
    save_docker_in_docker_settings_to(&root, settings)
}
