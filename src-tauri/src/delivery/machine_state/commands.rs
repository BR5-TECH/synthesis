//! The two Tauri commands of `DMS-delivery-machine-state.md`:
//! `load_delivery_layout(scope)` (DMS-FR-DPXR) and
//! `save_delivery_layout(scope, layout)` (DMS-FR-XAZV). The overlay and the
//! publish journal have no command (DMS-FR-VEHN).

use crate::project::OpenProject as _;

use crate::logging::{log_debug, log_info, log_warn, Domain, LogBuffer, BUFFER};

use super::super::{machine_store, DeliveryError};
use super::layout::{load_layout, parse_scope, save_layout, DeliveryLayout};

/// The kind of `scope` for a log record, never the scope itself: a Flow scope
/// holds an artifact id (DMS-FR-ROMF).
fn scope_kind(scope: &str) -> &'static str {
    parse_scope(scope).map(|s| s.kind()).unwrap_or("invalid")
}

/// DMS-FR-IHML: log a refusal for a missing project or store.
fn log_refusal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    operation: &str,
    scope: &str,
    error: &DeliveryError,
) {
    log_warn(
        app,
        buffer,
        &[Domain::Backend],
        "delivery layout operation refused",
        crate::log_fields! {
            "operation" => operation.to_string(),
            "scope_kind" => scope_kind(scope),
            "code" => error.code.clone(),
        },
    );
}

/// DMS-FR-DPXR, DMS-FR-IHML: the layout of `scope` for the open project.
pub fn load_delivery_layout_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    scope: &str,
) -> Result<DeliveryLayout, DeliveryError> {
    let project = app.open_project_state_or_default();
    let store = machine_store(&project).inspect_err(|e| {
        log_refusal(app, buffer, "load_delivery_layout", scope, e);
    })?;
    let layout = load_layout(app, buffer, &store, scope).inspect_err(|e| {
        log_refusal(app, buffer, "load_delivery_layout", scope, e);
    })?;
    log_debug(
        app,
        buffer,
        &[Domain::Backend],
        "delivery layout loaded",
        crate::log_fields! { "scope_kind" => scope_kind(scope), "items" => layout.items.len() },
    );
    Ok(layout)
}

/// DMS-FR-XAZV, DMS-FR-TJSJ, DMS-FR-IHML: replace the layout of `scope` for
/// the open project.
pub fn save_delivery_layout_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    scope: &str,
    layout: &DeliveryLayout,
) -> Result<(), DeliveryError> {
    let project = app.open_project_state_or_default();
    let store = machine_store(&project).inspect_err(|e| {
        log_refusal(app, buffer, "save_delivery_layout", scope, e);
    })?;
    save_layout(&store, scope, layout).inspect_err(|e| {
        log_refusal(app, buffer, "save_delivery_layout", scope, e);
    })?;
    log_info(
        app,
        buffer,
        &[Domain::Backend],
        "delivery layout saved",
        crate::log_fields! {
            "scope_kind" => scope_kind(scope),
            "items" => layout.items.len(),
            "removed" => layout.items.is_empty(),
        },
    );
    Ok(())
}

/// DMS contract surface: `"load delivery layout (scope)"`.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn load_delivery_layout(
    app: tauri::AppHandle,
    scope: String,
) -> Result<DeliveryLayout, DeliveryError> {
    load_delivery_layout_with(&app, &BUFFER, &scope)
}

/// DMS contract surface: `"save delivery layout (scope, layout)"`.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn save_delivery_layout(
    app: tauri::AppHandle,
    scope: String,
    layout: DeliveryLayout,
) -> Result<(), DeliveryError> {
    save_delivery_layout_with(&app, &BUFFER, &scope, &layout)
}
