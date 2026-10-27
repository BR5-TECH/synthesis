//! The Tauri commands of the embeddings index (EMB contract surface), used by
//! the Global and the Project Indexing sections.

use tauri::Manager;

use super::engine::ReindexScope;
use super::service::{EmbeddingsService, EmbeddingsStatus};
use super::settings::{EmbeddingsRecord, EmbeddingsSettings, ProjectEmbeddingsMode};
use crate::global_settings::GlobalSettingsStore;
use crate::logging::{log_info, Domain, BUFFER};

/// `"load embeddings settings"`.
#[tauri::command]
pub fn load_embeddings_settings(
    store: tauri::State<'_, GlobalSettingsStore>,
) -> Result<EmbeddingsSettings, String> {
    Ok(EmbeddingsSettings::from(&store.load_embeddings_config()?))
}

/// `"save embeddings settings"` (EMB-FR-JFOV, EMB-FR-TGCL).
#[tauri::command]
pub fn save_embeddings_settings<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    settings: EmbeddingsSettings,
) -> Result<EmbeddingsSettings, String> {
    save_settings(&app, settings)
}

pub(crate) fn save_settings<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    settings: EmbeddingsSettings,
) -> Result<EmbeddingsSettings, String> {
    if super::catalog::find(&settings.model).is_none() {
        return Err(super::ERR_UNKNOWN_MODEL.to_string());
    }
    let record = EmbeddingsRecord { enabled: settings.enabled, model: Some(settings.model) };
    let store = app.state::<GlobalSettingsStore>();
    store.save_embeddings_config(&record)?;
    log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "embeddings settings saved",
        crate::log_fields! { "enabled" => record.enabled, "model" => record.model_id() },
    );
    if let Some(service) = app.try_state::<EmbeddingsService>() {
        service.apply_settings(record.clone());
        service.ensure_worker(app);
        service.emit(app);
    }
    Ok(EmbeddingsSettings::from(&record))
}

/// `"list embedding models"` (EMB-FR-MIDG).
#[tauri::command]
pub fn list_embedding_models() -> Vec<super::catalog::EmbeddingModelEntry> {
    super::catalog::list()
}

/// `"retry embeddings model download"` (EMB-FR-KEGS).
#[tauri::command]
pub fn retry_embeddings_model_download<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> EmbeddingsStatus {
    let service = app.state::<EmbeddingsService>();
    service.retry_download();
    service.ensure_worker(&app);
    service.emit(&app);
    service.status()
}

/// `"load project embeddings mode"` (GSS-FR-DSUG).
#[tauri::command]
pub fn load_project_embeddings_mode(
    project: tauri::State<'_, crate::project::ProjectState>,
    store: tauri::State<'_, GlobalSettingsStore>,
) -> Result<ProjectEmbeddingsMode, String> {
    let key = project.anchor().ok_or_else(|| super::ERR_NO_PROJECT_OPEN.to_string())?;
    store.load_embeddings_mode(&key)
}

/// `"save project embeddings mode"` (EMB-FR-CTXC).
#[tauri::command]
pub fn save_project_embeddings_mode<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    mode: ProjectEmbeddingsMode,
) -> Result<ProjectEmbeddingsMode, String> {
    let project = app.state::<crate::project::ProjectState>();
    let key = project.anchor().ok_or_else(|| super::ERR_NO_PROJECT_OPEN.to_string())?;
    app.state::<GlobalSettingsStore>().save_embeddings_mode(&key, mode)?;
    log_info(
        &app,
        &BUFFER,
        &[Domain::Backend],
        "project embeddings mode saved",
        crate::log_fields! { "mode" => serde_json::to_value(mode).unwrap_or_default() },
    );
    if let Some(service) = app.try_state::<EmbeddingsService>() {
        service.apply_mode(mode);
        service.ensure_worker(&app);
        service.emit(&app);
    }
    Ok(mode)
}

/// `"get embeddings status"` (EMB-FR-ZFGT).
#[tauri::command]
pub fn get_embeddings_status(service: tauri::State<'_, EmbeddingsService>) -> EmbeddingsStatus {
    service.status()
}

/// `"start reindex (scope)"` (EMB-FR-HRYV, EMB-FR-QNWF).
#[tauri::command]
pub fn start_reindex<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    scope: ReindexScope,
) -> Result<EmbeddingsStatus, String> {
    let service = app.state::<EmbeddingsService>();
    service.request_reindex(scope)?;
    log_info(
        &app,
        &BUFFER,
        &[Domain::Backend],
        "reindex requested",
        crate::log_fields! { "scope" => serde_json::to_value(scope).unwrap_or_default() },
    );
    service.ensure_worker(&app);
    service.emit(&app);
    Ok(service.status())
}
