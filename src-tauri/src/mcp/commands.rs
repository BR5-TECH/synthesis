//! The Tauri commands of `MCS-mcp-server-connections.md`.
//!
//! Each command builds a [`Context`] and calls one operation. A refusal is the
//! code of the contract surface, as a string.

use tauri::{AppHandle, State};

use super::config::{McpAuth, McpServer, McpTransport};
use super::manager::McpManager;
use super::oauth::{self, SignInRequest, SIGN_IN_LIMIT};
use super::operations::{self, run_test, Context, NOT_OAUTH2, SIGN_IN_IN_PROGRESS};
use super::status::{McpFailure, McpServerEntry, McpTestResult};
use crate::global_settings::GlobalSettingsStore;
use crate::logging::{self, Domain};
use crate::project::ProjectState;

fn context<'a>(
    project: &ProjectState,
    store: &'a GlobalSettingsStore,
    manager: &'a McpManager,
) -> Result<Context<'a>, String> {
    Context::from_states(project, store, manager)
}

/// Log a refusal with its code alone (MCS-FR-MABN).
fn refused<T>(app: &AppHandle, operation: &str, result: Result<T, String>) -> Result<T, String> {
    if let Err(error) = &result {
        logging::log_warn(
            app,
            &logging::BUFFER,
            &[Domain::Backend],
            "mcp operation refused",
            crate::log_fields! { "operation" => operation, "error" => operations::error_code(error) },
        );
    }
    result
}

/// MCS-FR-FIEE.
#[tauri::command]
pub async fn list_mcp_servers(
    app: AppHandle,
    project: State<'_, ProjectState>,
    store: State<'_, GlobalSettingsStore>,
    manager: State<'_, McpManager>,
) -> Result<Vec<McpServerEntry>, String> {
    let result = context(&project, &store, &manager).and_then(|c| c.list());
    refused(&app, "list", result)
}

/// MCS-FR-XCOS.
#[tauri::command]
pub async fn save_mcp_server(
    app: AppHandle,
    project: State<'_, ProjectState>,
    store: State<'_, GlobalSettingsStore>,
    manager: State<'_, McpManager>,
    server: McpServer,
) -> Result<McpServerEntry, String> {
    let result = context(&project, &store, &manager).and_then(|c| c.save(server));
    refused(&app, "save", result)
}

/// MCS-FR-UOIO.
#[tauri::command]
pub async fn remove_mcp_server(
    app: AppHandle,
    project: State<'_, ProjectState>,
    store: State<'_, GlobalSettingsStore>,
    manager: State<'_, McpManager>,
    id: String,
) -> Result<(), String> {
    let result = context(&project, &store, &manager).and_then(|c| c.remove(&id));
    refused(&app, "remove", result)
}

/// MCS-FR-IXHD.
#[tauri::command]
pub async fn set_mcp_server_secret(
    app: AppHandle,
    project: State<'_, ProjectState>,
    store: State<'_, GlobalSettingsStore>,
    manager: State<'_, McpManager>,
    id: String,
    slot: String,
    value: String,
) -> Result<McpServerEntry, String> {
    let result = context(&project, &store, &manager).and_then(|c| c.set_secret(&id, &slot, &value));
    refused(&app, "set_secret", result)
}

/// MCS-FR-WBDV.
#[tauri::command]
pub async fn clear_mcp_server_secret(
    app: AppHandle,
    project: State<'_, ProjectState>,
    store: State<'_, GlobalSettingsStore>,
    manager: State<'_, McpManager>,
    id: String,
    slot: String,
) -> Result<McpServerEntry, String> {
    let result = context(&project, &store, &manager).and_then(|c| c.clear_secret(&id, &slot));
    refused(&app, "clear_secret", result)
}

/// MCS-FR-EVRS.
#[tauri::command]
pub async fn trust_mcp_server(
    app: AppHandle,
    project: State<'_, ProjectState>,
    store: State<'_, GlobalSettingsStore>,
    manager: State<'_, McpManager>,
    id: String,
    shown: McpServer,
) -> Result<McpServerEntry, String> {
    let result = context(&project, &store, &manager).and_then(|c| c.trust(&id, &shown));
    refused(&app, "trust", result)
}

/// MCS-FR-ETIS.
#[tauri::command]
pub async fn test_mcp_server(
    app: AppHandle,
    project: State<'_, ProjectState>,
    store: State<'_, GlobalSettingsStore>,
    manager: State<'_, McpManager>,
    id: String,
) -> Result<McpTestResult, String> {
    let plan =
        refused(&app, "test", context(&project, &store, &manager).and_then(|c| c.test_plan(&id)))?;
    Ok(run_test(&manager, plan).await)
}

/// MCS-FR-KCIH.
#[tauri::command]
pub async fn reconnect_mcp_server(
    app: AppHandle,
    project: State<'_, ProjectState>,
    store: State<'_, GlobalSettingsStore>,
    manager: State<'_, McpManager>,
    id: String,
) -> Result<(), String> {
    let result = context(&project, &store, &manager).and_then(|c| c.reconnect(&id));
    refused(&app, "reconnect", result)
}

/// MCS-FR-YWVC: start a sign-in and return. Its result arrives as a status
/// change (MCS-FR-NOKX).
#[tauri::command]
pub async fn start_mcp_sign_in(
    app: AppHandle,
    project: State<'_, ProjectState>,
    store: State<'_, GlobalSettingsStore>,
    manager: State<'_, McpManager>,
    id: String,
) -> Result<(), String> {
    let prepared = context(&project, &store, &manager)
        .and_then(|c| Ok((c.key.clone(), c.signing_in_record(&id)?, c.root.clone())));
    let (key, record, root) = refused(&app, "start_sign_in", prepared)?;
    let started_with = crate::mcp::config::trust_fingerprint(&record);
    let http = match &record.transport {
        McpTransport::Http(http) if http.auth == McpAuth::Oauth2 => http.clone(),
        _ => return refused(&app, "start_sign_in", Err(NOT_OAUTH2.to_string())),
    };
    let cancelled = manager.begin_sign_in(&id).map_err(|_| SIGN_IN_IN_PROGRESS.to_string());
    let mut cancelled = refused(&app, "start_sign_in", cancelled)?;
    let manager = manager.inner().clone();
    let server_id = id.clone();
    let still_current = Box::new(move || {
        crate::mcp::config::list_records(&root)
            .ok()
            .and_then(|records| records.into_iter().find(|listed| listed.id == server_id))
            .and_then(|listed| listed.record.ok())
            .as_ref()
            .and_then(crate::mcp::config::trust_fingerprint)
            == started_with
    });
    let request = SignInRequest {
        project_key: key,
        server_id: id.clone(),
        http,
        secrets: manager.secrets(),
        still_current,
    };
    tauri::async_runtime::spawn(async move {
        let opener = app.clone();
        let open = move |url: &str| {
            use tauri_plugin_opener::OpenerExt;
            opener.opener().open_url(url, None::<&str>).map_err(|_| ())
        };
        let outcome = tokio::select! {
            outcome = tokio::time::timeout(SIGN_IN_LIMIT, oauth::sign_in(request, open)) => {
                outcome.unwrap_or(Err(McpFailure::OauthFailed))
            }
            _ = cancelled.wait_for(|cancelled| *cancelled) => return,
        };
        let succeeded = outcome.is_ok();
        manager.finish_sign_in(&id, outcome);
        if succeeded {
            let _ = super::reconcile_now(&app, Some(&id));
        }
    });
    Ok(())
}

/// MCS-FR-CMWV.
#[tauri::command]
pub async fn cancel_mcp_sign_in(manager: State<'_, McpManager>, id: String) -> Result<(), String> {
    manager.cancel_sign_in(&id);
    Ok(())
}

/// MCS-FR-QRIB.
#[tauri::command]
pub async fn sign_out_mcp_server(
    app: AppHandle,
    project: State<'_, ProjectState>,
    store: State<'_, GlobalSettingsStore>,
    manager: State<'_, McpManager>,
    id: String,
) -> Result<McpServerEntry, String> {
    let result = context(&project, &store, &manager).and_then(|c| c.sign_out(&id));
    refused(&app, "sign_out", result)
}
