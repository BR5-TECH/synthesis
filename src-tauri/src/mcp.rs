//! MCP servers for conversation agents.
//!
//! `specifications/core/MCS-mcp-server-connections.md` owns the records, the
//! secrets, the trust, the sign-in, and the connections;
//! `specifications/tools/MCT-mcp-tool.md` owns how a tool of a server reaches a
//! turn (`../ai/CVL-conversation-loop.md` CVL-FR-TJZI).

pub mod commands;
pub mod config;
pub mod connect;
pub mod manager;
pub mod oauth;
pub mod operations;
pub mod secrets;
pub mod status;
pub mod tool;

#[cfg(test)]
pub(crate) mod tests;

use std::collections::BTreeMap;
use std::sync::Arc;

use tauri::{Emitter, Manager};

use crate::logging::{self, Domain, Fields, LogLevel};
pub use manager::{McpManager, McpObserver};

use config::{trust_fingerprint, ListedRecord, McpAuth, McpServer};
use manager::{ProjectContext, ServerInput, Timing};
use secrets::{required_slots, McpSecrets, VaultMcpSecrets};
use status::{MachineFacts, StatusChanged, STATUS_EVENT};

impl Default for McpManager {
    /// The production manager: the vault of this process and real connections.
    fn default() -> Self {
        McpManager::new(
            Arc::new(VaultMcpSecrets::new(crate::secret_vault::global())),
            Arc::new(connect::RealConnector),
            Timing::default(),
        )
    }
}

/// The production observer: the Tauri event of MCS-FR-NOKX and the session log
/// (MCS-FR-KEEF).
struct AppObserver<R: tauri::Runtime>(tauri::AppHandle<R>);

impl<R: tauri::Runtime> McpObserver for AppObserver<R> {
    fn status_changed(&self, id: &str, status: &status::McpServerStatus) {
        let payload = StatusChanged { id: id.to_string(), status: status.clone() };
        let _ = self.0.emit(STATUS_EVENT, payload);
    }

    fn log(&self, level: LogLevel, message: &str, fields: Fields) {
        logging::log(&self.0, &logging::BUFFER, level, &[Domain::Backend], message, fields);
    }
}

/// Give the manager the application's event and log channels. Called once, at
/// start.
pub fn install<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(manager) = app.try_state::<McpManager>() {
        manager.install_observer(Arc::new(AppObserver(app.clone())));
    }
}

/// What this machine knows about one record (MCS-FR-HQJQ, MCS-FR-PXRJ,
/// MCS-FR-MGCJ).
pub fn facts_for(
    record: &McpServer,
    project_key: &str,
    trust: &BTreeMap<String, String>,
    secrets: &dyn McpSecrets,
) -> MachineFacts {
    let trusted = match trust_fingerprint(record) {
        Some(expected) => trust.get(&record.id).is_some_and(|fingerprint| *fingerprint == expected),
        None => true,
    };
    // MCS-FR-HQJQ: nothing of the vault is read for an untrusted record.
    if !trusted {
        return MachineFacts::default();
    }
    let required = required_slots(record);
    let presence = secrets.presence(project_key, &record.id, &required);
    let oauth = (record.auth() == McpAuth::Oauth2)
        .then(|| oauth::read_slot(secrets, project_key, &record.id));
    let vault_unavailable = presence.is_err() || matches!(oauth, Some(Err(_)));
    MachineFacts {
        trusted,
        secrets_present: presence
            .is_ok_and(|p| required.iter().all(|slot| p.get(slot) == Some(&true))),
        signed_in: matches!(oauth, Some(Ok(slot)) if slot.signed_in()),
        vault_unavailable,
    }
}

/// One input for each record of the file.
pub fn inputs_for(
    listed: Vec<ListedRecord>,
    project_key: &str,
    trust: &BTreeMap<String, String>,
    secrets: &dyn McpSecrets,
) -> Vec<ServerInput> {
    listed
        .into_iter()
        .map(|listed| {
            let facts = match &listed.record {
                Ok(record) => facts_for(record, project_key, trust, secrets),
                Err(_) => MachineFacts::default(),
            };
            ServerInput { listed, facts }
        })
        .collect()
}

/// MCS-FR-RYIK: reread the records of the open project and reconcile.
pub fn reconcile_now<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    restart: Option<&str>,
) -> Result<(), String> {
    let (Some(manager), Some(project), Some(store)) = (
        app.try_state::<McpManager>(),
        app.try_state::<crate::project::ProjectState>(),
        app.try_state::<crate::global_settings::GlobalSettingsStore>(),
    ) else {
        return Ok(());
    };
    let context = operations::Context::from_states(&project, &store, &manager)?;
    context.reconcile(restart)
}

/// MCS-FR-RYIK: a project opened, or its active worktree changed.
pub fn on_content_root_mounted<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Err(error) = reconcile_now(app, None) {
        logging::log_warn(
            app,
            &logging::BUFFER,
            &[Domain::Backend],
            "mcp servers not read",
            crate::log_fields! { "error" => operations::error_code(&error) },
        );
    }
}

/// MCS-FR-QTSW: the project closed.
pub fn on_project_closed<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(manager) = app.try_state::<McpManager>() {
        manager.close_project();
    }
}

/// MCS-FR-QTSW: the application quits. Waits up to five seconds.
pub fn on_exit<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(manager) = app.try_state::<McpManager>() {
        let manager = manager.inner().clone();
        tauri::async_runtime::block_on(manager.shutdown());
    }
}

/// CVL-FR-TJZI, MCT-FR-GPCO: the MCP tools of a turn of the open project.
pub fn conversation_tools<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Vec<rig::tool::DynamicTool> {
    let (Some(manager), Some(project)) =
        (app.try_state::<McpManager>(), app.try_state::<crate::project::ProjectState>())
    else {
        return Vec::new();
    };
    let sink = app.clone();
    tool::conversation_tools(&manager, &project.slot_key(), move |level, message, fields| {
        logging::log(&sink, &logging::BUFFER, level, &tool::TOOL_DOMAINS, message, fields);
    })
}

/// The project the manager reconciles against.
pub fn project_context(key: &str, root: std::path::PathBuf) -> ProjectContext {
    ProjectContext { key: key.to_string(), root }
}
