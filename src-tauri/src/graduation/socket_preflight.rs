//! The Docker socket check of the start preflight (GSU-FR-RQDS, GSU-FR-LMPW).
//!
//! Split out of `commands.rs` for size alone.

use super::errors::ERR_DOCKER_SOCKET_UNAVAILABLE;
use crate::isolation::ResolvedIsolation;
use crate::log_fields;
use crate::logging::{self, Domain, BUFFER};

/// GSU-FR-RQDS: where the project enables Docker-in-Docker and the effective
/// isolation is None (EAC-FR-SNMS), the socket path must name a Unix socket.
/// One metadata read; no connection is opened and no daemon is asked. Where
/// the feature is off, or the effective isolation is Microsandbox, no path is
/// read. The image is never inspected for a Docker daemon.
pub(super) fn docker_socket_preflight<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &crate::fs::RootFs,
    store: &crate::global_settings::GlobalSettingsStore,
) -> Result<(), String> {
    let settings = crate::project_settings::read_docker_in_docker_settings(root);
    if !settings.enabled {
        return Ok(());
    }
    // A record that cannot be read is not Microsandbox here: the launch
    // reports that error, and the socket is still checked.
    let resolved = crate::isolation::resolve_isolation(store).unwrap_or(ResolvedIsolation::None);
    let effective = crate::isolation::effective_isolation(
        resolved,
        settings.enabled,
        settings.under_microsandbox,
    );
    if matches!(effective, ResolvedIsolation::Microsandbox(_)) {
        return Ok(());
    }
    let path = settings.effective_socket_path;
    // The mount syntax separates its fields with commas, so a path that holds
    // one cannot be mounted. The launch refuses it, and so does the start.
    let probed = if path.contains(',') {
        Err(crate::fs::SocketProbeError::Missing)
    } else {
        crate::fs::probe_unix_socket(std::path::Path::new(&path))
    };
    match probed {
        Ok(_) => Ok(()),
        Err(problem) => {
            // The path is the author's own setting, never a credential.
            logging::log_warn(
                app,
                &BUFFER,
                &[Domain::Backend],
                "graduation preflight found no usable Docker socket",
                log_fields! { "path" => path.clone(), "problem" => format!("{problem:?}") },
            );
            Err(format!("{ERR_DOCKER_SOCKET_UNAVAILABLE}: {path}"))
        }
    }
}
