//! The four Tauri commands of `DMS-delivery-machine-state.md`:
//! `load_delivery_layout(scope)` (DMS-FR-DPXR),
//! `save_delivery_layout(scope, layout)` (DMS-FR-XAZV),
//! `load_delivery_view(scope)` (DMS-FR-JBKN), and
//! `save_delivery_view(scope, view)` (DMS-FR-TICX). The overlay and the
//! publish journal have no command (DMS-FR-VEHN).

use crate::project::OpenProject as _;

use crate::logging::{log_debug, log_info, log_warn, Domain, LogBuffer, BUFFER};

use super::super::{machine_store, DeliveryError};
use super::layout::{load_layout, parse_scope, save_layout, DeliveryLayout};
use super::view::{load_view, parse_view, save_view, DeliveryView};

/// The kind of `scope` for a log record, never the scope itself: a Flow scope
/// holds an artifact id (DMS-FR-ROMF).
fn scope_kind(scope: &str) -> &'static str {
    parse_scope(scope).map(|s| s.kind()).unwrap_or("invalid")
}

/// DMS-FR-IHML: log a refusal of a layout operation.
fn log_refusal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    operation: &str,
    scope: &str,
    error: &DeliveryError,
) {
    log_refusal_as(app, buffer, "delivery layout operation refused", operation, scope, error);
}

/// DMS-FR-IHML, DMS-FR-TICX: log a refusal of a view operation.
fn log_view_refusal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    operation: &str,
    scope: &str,
    error: &DeliveryError,
) {
    log_refusal_as(app, buffer, "delivery view operation refused", operation, scope, error);
}

/// Log one refusal with the scope kind and the error code alone
/// (DMS-FR-ROMF).
fn log_refusal_as<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    message: &str,
    operation: &str,
    scope: &str,
    error: &DeliveryError,
) {
    log_warn(
        app,
        buffer,
        &[Domain::Backend],
        message,
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

/// DMS-FR-JBKN, DMS-FR-IHML: the view of `scope` for the open project.
pub fn load_delivery_view_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    scope: &str,
) -> Result<DeliveryView, DeliveryError> {
    let project = app.open_project_state_or_default();
    let store = machine_store(&project).inspect_err(|e| {
        log_view_refusal(app, buffer, "load_delivery_view", scope, e);
    })?;
    let view = load_view(app, buffer, &store, scope).inspect_err(|e| {
        log_view_refusal(app, buffer, "load_delivery_view", scope, e);
    })?;
    log_debug(
        app,
        buffer,
        &[Domain::Backend],
        "delivery view loaded",
        crate::log_fields! {
            "scope_kind" => scope_kind(scope),
            "hidden" => view.hidden.len(),
            "blocked_only" => view.blocked_only,
        },
    );
    Ok(view)
}

/// DMS-FR-TICX, DMS-FR-XUYV, DMS-FR-IHML: replace the view of `scope` for
/// the open project. `view` is the argument as the frontend sends it, so a
/// status that is not a board status is `invalid_view` and not a failure of
/// the call.
pub fn save_delivery_view_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    scope: &str,
    view: &serde_json::Value,
) -> Result<(), DeliveryError> {
    let project = app.open_project_state_or_default();
    let store = machine_store(&project).inspect_err(|e| {
        log_view_refusal(app, buffer, "save_delivery_view", scope, e);
    })?;
    let view = parse_view(view).inspect_err(|e| {
        log_view_refusal(app, buffer, "save_delivery_view", scope, e);
    })?;
    save_view(&store, scope, &view).inspect_err(|e| {
        log_view_refusal(app, buffer, "save_delivery_view", scope, e);
    })?;
    log_info(
        app,
        buffer,
        &[Domain::Backend],
        "delivery view saved",
        crate::log_fields! {
            "scope_kind" => scope_kind(scope),
            "hidden" => view.hidden.len(),
            "blocked_only" => view.blocked_only,
            "removed" => view.is_empty(),
        },
    );
    Ok(())
}

/// DMS contract surface: `"load delivery view (scope)"`.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn load_delivery_view(
    app: tauri::AppHandle,
    scope: String,
) -> Result<DeliveryView, DeliveryError> {
    load_delivery_view_with(&app, &BUFFER, &scope)
}

/// DMS contract surface: `"save delivery view (scope, view)"`.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn save_delivery_view(
    app: tauri::AppHandle,
    scope: String,
    view: serde_json::Value,
) -> Result<(), DeliveryError> {
    save_delivery_view_with(&app, &BUFFER, &scope, &view)
}
