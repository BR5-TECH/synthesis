//! The Tauri commands of the terminal session module. Names match
//! `../ui/TRM-terminals.md` (TRS contract surface).

use std::sync::Arc;

use tauri::ipc::{Channel, InvokeResponseBody};

use super::model::*;
use super::service;
use super::session::{OutputSink, SinkClosed};
use crate::log_fields;
use crate::logging::{self, Domain};

/// Logs a refused or failed call, with its code alone, and passes it on.
fn logged<T>(
    app: &tauri::AppHandle,
    operation: &str,
    result: Result<T, TerminalError>,
) -> Result<T, TerminalError> {
    if let Err(error) = &result {
        logging::log_warn(
            app,
            &logging::BUFFER,
            &[Domain::Backend],
            "terminal call refused",
            log_fields! {
                "operation" => operation,
                "code" => error.code,
                "reason" => error.reason.map(|r| r.code()).unwrap_or(""),
            },
        );
    }
    result
}

/// The attached tab's channel.
struct ChannelSink(Channel<InvokeResponseBody>);

impl OutputSink for ChannelSink {
    fn send(&self, frame: Vec<u8>) -> Result<(), SinkClosed> {
        self.0.send(InvokeResponseBody::Raw(frame)).map_err(|_| SinkClosed)
    }
}

#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn list_terminal_targets(app: tauri::AppHandle) -> Result<Vec<TerminalTarget>, TerminalError> {
    logged(&app, "list_targets", service::list_targets(&app))
}

#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn list_terminals(app: tauri::AppHandle) -> Result<Vec<TerminalSummary>, TerminalError> {
    logged(&app, "list", service::list(&app))
}

/// Runs off the thread the window draws from: a create can call git and tmux
/// (TRS-FR-OMYR), and neither may stop the window.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub async fn create_terminal(
    app: tauri::AppHandle,
    target: TargetRef,
    kind: TerminalKind,
    vendor: Option<CliVendor>,
) -> Result<TerminalSummary, TerminalError> {
    let worker = app.clone();
    let result = crate::project_scope::context::spawn_blocking(move || {
        service::create(&worker, target, kind, vendor)
    })
    .await
    .unwrap_or_else(|_| Err(TerminalError::new("start_failed")));
    logged(&app, "create", result)
}

#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn attach_terminal(
    app: tauri::AppHandle,
    terminal_id: String,
    output: Channel<InvokeResponseBody>,
) -> Result<TerminalAttachment, TerminalError> {
    logged(&app, "attach", service::attach(&app, &terminal_id, Arc::new(ChannelSink(output))))
}

#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn detach_terminal(app: tauri::AppHandle, terminal_id: String) -> Result<(), TerminalError> {
    logged(&app, "detach", service::detach(&app, &terminal_id))
}

#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn write_terminal_input(
    app: tauri::AppHandle,
    terminal_id: String,
    data: String,
) -> Result<(), TerminalError> {
    logged(&app, "write_input", service::write_input(&app, &terminal_id, data.as_bytes()))
}

#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn resize_terminal(
    app: tauri::AppHandle,
    terminal_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), TerminalError> {
    logged(&app, "resize", service::resize(&app, &terminal_id, cols, rows))
}

#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn acknowledge_terminal_output(
    app: tauri::AppHandle,
    terminal_id: String,
    bytes: usize,
) -> Result<(), TerminalError> {
    logged(&app, "acknowledge", service::acknowledge(&app, &terminal_id, bytes))
}

#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn set_terminal_pinned(
    app: tauri::AppHandle,
    terminal_id: String,
    pinned: bool,
) -> Result<TerminalSummary, TerminalError> {
    logged(&app, "set_pinned", service::set_pinned(&app, &terminal_id, pinned))
}

#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn set_terminal_label(
    app: tauri::AppHandle,
    terminal_id: String,
    label: String,
) -> Result<TerminalSummary, TerminalError> {
    logged(&app, "set_label", service::set_label(&app, &terminal_id, &label))
}

#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn set_terminal_theme(
    app: tauri::AppHandle,
    terminal_id: String,
    theme: Option<String>,
) -> Result<TerminalSummary, TerminalError> {
    logged(&app, "set_theme", service::set_theme(&app, &terminal_id, theme.as_deref()))
}

#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn close_terminal(app: tauri::AppHandle, terminal_id: String) -> Result<(), TerminalError> {
    logged(&app, "close", service::close(&app, &terminal_id))
}
