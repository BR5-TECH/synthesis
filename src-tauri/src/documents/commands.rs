//! The Tauri commands of the Documents collection (`DCL-documents-collection.md`
//! contract surface).
//!
//! A command resolves a document by id alone and passes no selected path to the
//! frontend as something it may read (DCL-FR-BAQY). The OS-native picker is
//! opened here, on the backend, so the frontend never holds a selected path
//! before the user has chosen it (DPN-FR-FDVO). Each command refuses with the
//! typed error code of `DocumentsError` as a string.

use std::path::PathBuf;

use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::fs::FsAccessState;

use super::lifecycle;
use super::manager::DocumentsCollection;
use super::model::{
    DocumentPdf, DocumentText, DocumentsError, DocumentsSnapshot, PickDocumentSourcesResult,
    PickMode,
};

/// DCL-FR-SQEP: the in-memory snapshot. It does not wait for a refresh.
#[tauri::command]
pub fn list_documents(
    app: tauri::AppHandle,
    collection: State<'_, DocumentsCollection>,
) -> Result<DocumentsSnapshot, String> {
    match collection.snapshot() {
        Ok(snapshot) => Ok(snapshot),
        // DCL-FR-MSHW: a read of the stored sources that failed is tried again
        // here, and a refresh follows when it works.
        Err(DocumentsError::StoreUnavailable) if lifecycle::retry_store(&app) => {
            lifecycle::request_refresh(&app);
            collection.snapshot().map_err(String::from)
        }
        Err(error) => Err(String::from(error)),
    }
}

/// DCL-FR-VXXI: open the OS-native picker for `mode` and add what it returns.
///
/// The picker blocks, so it runs on the blocking pool and the command future
/// stays non-blocking for the frontend. A cancel adds nothing and is not an
/// error.
#[tauri::command]
pub async fn pick_document_sources(
    app: tauri::AppHandle,
    mode: PickMode,
) -> Result<PickDocumentSourcesResult, String> {
    // The refusal of DCL-FR-MSHW comes before the picker opens, so a user is
    // never asked to choose files that cannot be stored.
    if let Some(collection) = tauri::Manager::try_state::<DocumentsCollection>(&app) {
        if let Err(error) = collection.snapshot() {
            lifecycle::log_refused(&app, "pick document sources", error);
            return Err(String::from(error));
        }
    }
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let picked = pick(&handle, mode);
        lifecycle::add_picked_sources(&handle, mode, picked).map_err(String::from)
    })
    .await
    .map_err(|e| format!("picker task failed: {e}"))?
}

/// Open the picker and return the chosen paths, or `None` when the user
/// cancelled.
fn pick(app: &tauri::AppHandle, mode: PickMode) -> Option<Vec<PathBuf>> {
    match mode {
        PickMode::Files => app
            .dialog()
            .file()
            .add_filter("Documents", &["pdf", "txt", "md", "markdown"])
            .blocking_pick_files()
            .map(|files| {
                files
                    .into_iter()
                    .filter_map(|file| file.into_path().ok())
                    .collect()
            }),
        PickMode::Folder => app
            .dialog()
            .file()
            .blocking_pick_folder()
            .and_then(|file| file.into_path().ok())
            .map(|path| vec![path]),
    }
}

/// DCL-FR-ATNL: remove one reference. The file or folder itself is untouched.
#[tauri::command]
pub async fn remove_document_source(
    app: tauri::AppHandle,
    path: String,
) -> Result<DocumentsSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        lifecycle::remove_source(&app, &path).map_err(String::from)
    })
    .await
    .map_err(|e| format!("remove task failed: {e}"))?
}

/// DCL-FR-NCBQ: the current text of a Markdown or text document.
#[tauri::command]
pub async fn read_document(app: tauri::AppHandle, id: String) -> Result<DocumentText, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let collection = tauri::Manager::try_state::<DocumentsCollection>(&app)
            .ok_or_else(|| String::from(super::model::DocumentsError::NoProjectOpen))?;
        let fs = tauri::Manager::try_state::<FsAccessState>(&app)
            .ok_or_else(|| String::from(super::model::DocumentsError::NoProjectOpen))?;
        collection.read_document(&fs, &id).map_err(|error| {
            lifecycle::log_refused(&app, "read document", error);
            String::from(error)
        })
    })
    .await
    .map_err(|e| format!("read task failed: {e}"))?
}

/// DCL-FR-TEUK: the whole content of a PDF document, base64 encoded. No other
/// operation returns PDF bytes.
#[tauri::command]
pub async fn read_document_pdf(app: tauri::AppHandle, id: String) -> Result<DocumentPdf, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let collection = tauri::Manager::try_state::<DocumentsCollection>(&app)
            .ok_or_else(|| String::from(super::model::DocumentsError::NoProjectOpen))?;
        let fs = tauri::Manager::try_state::<FsAccessState>(&app)
            .ok_or_else(|| String::from(super::model::DocumentsError::NoProjectOpen))?;
        collection.read_document_pdf(&fs, &id).map_err(|error| {
            lifecycle::log_refused(&app, "read document pdf", error);
            String::from(error)
        })
    })
    .await
    .map_err(|e| format!("read task failed: {e}"))?
}
