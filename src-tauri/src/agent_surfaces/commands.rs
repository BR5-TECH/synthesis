//! The Tauri commands of `AUI-agent-ui-surfaces.md`.

use tauri::State;

use super::*;
use crate::comments;
use crate::global_settings::GlobalSettingsStore;
use crate::logging::{LogSink, BUFFER};
use crate::project::ProjectState;

/// AUI contract surface: `"get agent surface (id)"` (AUI-FR-QBXZ).
#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn get_agent_surface(
    surface_id: String,
    project: crate::project_scope::Scoped<'_, ProjectState>,
) -> Result<AgentSurface, String> {
    get_with(project.require_store().ok().as_ref(), &surface_id)
}

/// AUI-FR-BPDT: with no project open there is no store, and every operation
/// fails with `no_project_open`.
pub(crate) fn open_store(store: Option<&crate::fs::RootFs>) -> Result<&crate::fs::RootFs, String> {
    store.ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())
}

/// `"get agent surface (id)"` on the store of the open project, if any.
pub(crate) fn get_with(
    store: Option<&crate::fs::RootFs>,
    surface_id: &str,
) -> Result<AgentSurface, String> {
    store::get_impl(open_store(store)?, surface_id)
}

/// AUI contract surface: `"submit surface action (id, action)"` (AUI-FR-KBVY).
#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn submit_surface_action<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    surface_id: String,
    action: SurfaceAction,
    settings: State<'_, GlobalSettingsStore>,
    project: crate::project_scope::Scoped<'_, ProjectState>,
) -> Result<SurfaceSubmitResult, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let store = project.require_store().ok();
    let store = open_store(store.as_ref())?;
    // AUI-FR-AIRH: the acting human, resolved here and never supplied.
    let by = comments::acting_participant(&settings, &project)?;
    store::submit_impl(&app, &BUFFER, store, &surface_id, action, &by)
}

/// AUI contract surface: `"read surface media (id, source)"` (AUI-FR-JUDL).
#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn read_surface_media<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    surface_id: String,
    source: String,
    project: crate::project_scope::Scoped<'_, ProjectState>,
) -> Result<media::SurfaceMedia, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let store = project.require_store().ok();
    let store = open_store(store.as_ref())?;
    let root = project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    let outcome = media::read_media_impl(store, &root, &surface_id, &source);
    if let Err(error) = &outcome {
        // AUI-FR-DNZK: the ids and the typed code, never the source.
        crate::logging::log_warn(
            &app,
            &BUFFER,
            &[crate::logging::Domain::Backend],
            "agent surface media refused",
            crate::log_fields! { "surfaceId" => &surface_id, "error" => error },
        );
    }
    outcome
}
