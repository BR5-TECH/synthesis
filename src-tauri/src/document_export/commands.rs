//! The Tauri command of an export (DXP-FR-IEQA, DXP-FR-WLGE, DXP-FR-LSYA).
//!
//! The command reads the saved file through the IDE instance, converts it on a
//! worker thread, reports each stage to the window, and writes the result to
//! the destination the author chose in the save dialog.

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;
use tauri::{Emitter, Manager};

use super::convert::{self, Format, Input, Stage};
use super::source::check_doc_artifact;
use super::{
    ERR_CONVERSION_FAILED, ERR_INVALID_DESTINATION, ERR_NO_PROJECT_OPEN, ERR_WRITE_FAILED,
};
use crate::fs::{self, RootFs, UserChosenPath};
use crate::log_fields;
use crate::logging::{self, Domain, LogBuffer, LogSink, BUFFER};

/// DXP contract surface, "Events".
pub const DOCUMENT_EXPORT_PROGRESS: &str = "document-export-progress";

/// The largest image an export reads from the project, in bytes.
const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;

/// DXP-FR-LSYA: the payload of one progress event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    pub export_id: String,
    pub stage: &'static str,
    pub done: usize,
    pub total: usize,
}

/// DXP-FR-IOFM: one warning of an export.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportWarning {
    pub kind: &'static str,
    pub index: usize,
}

/// DXP contract surface: what `export_document` returns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub destination: String,
    pub warnings: Vec<ExportWarning>,
}

/// One export, as the command was given it.
#[derive(Clone, Debug)]
pub struct ExportJob {
    pub path: String,
    pub format: Format,
    pub include_frontmatter: bool,
    pub destination: String,
    pub export_id: String,
}

/// DXP-FR-IEQA: export the DOC artifact at `path` to `destination`.
#[tauri::command]
pub async fn export_document<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    path: String,
    format: String,
    include_frontmatter: bool,
    destination: String,
    export_id: String,
) -> Result<ExportResult, String> {
    let Some(format) = Format::parse(&format) else {
        return Err(ERR_CONVERSION_FAILED.to_string());
    };
    let job = ExportJob { path, format, include_frontmatter, destination, export_id };
    // DXP-FR-WLGE: the managed state is read on the worker thread too, because
    // a `State` can not cross onto it and the `AppHandle` can.
    let worker = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root =
            worker.try_state::<crate::project::ProjectState>().and_then(|p| p.require_root().ok());
        let emitter = worker.clone();
        let emit = move |progress: ExportProgress| {
            let _ = emitter.emit(DOCUMENT_EXPORT_PROGRESS, progress);
        };
        run_export(&worker, &BUFFER, root.as_ref(), &job, &convert::diagrams::render_png, &emit)
    })
    .await
    .map_err(|_| ERR_CONVERSION_FAILED.to_string())?
    .map_err(str::to_string)
}

/// The whole export with its logging (DXP-FR-NGUG). `root` is `None` with no
/// project open. `emit` receives every progress event (DXP-FR-LSYA).
pub fn run_export<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    root: Option<&RootFs>,
    job: &ExportJob,
    render_diagram: &dyn Fn(&str) -> Option<Vec<u8>>,
    emit: &dyn Fn(ExportProgress),
) -> Result<ExportResult, &'static str> {
    let started = Instant::now();
    let base = log_fields! {
        "format" => job.format.as_str(),
        "includeFrontmatter" => job.include_frontmatter,
    };
    logging::log_info(sink, buffer, &[Domain::Backend], "document export started", base.clone());
    let mut diagram_count = 0;
    let outcome = export(root, job, render_diagram, emit, &mut diagram_count);
    let mut fields = base;
    fields.insert("diagrams".into(), serde_json::json!(diagram_count));
    fields.insert("durationMs".into(), serde_json::json!(started.elapsed().as_millis() as u64));
    match &outcome {
        Ok(result) => {
            for warning in &result.warnings {
                logging::log_warn(
                    sink,
                    buffer,
                    &[Domain::Backend],
                    "a diagram stayed as code in a document export",
                    log_fields! { "index" => warning.index },
                );
            }
            fields.insert("warnings".into(), serde_json::json!(result.warnings.len()));
            logging::log_info(sink, buffer, &[Domain::Backend], "document export finished", fields);
        }
        Err(code) => {
            fields.insert("error".into(), serde_json::json!(code));
            logging::log_error(sink, buffer, &[Domain::Backend], "document export failed", fields);
        }
    }
    outcome
}

fn export(
    root: Option<&RootFs>,
    job: &ExportJob,
    render_diagram: &dyn Fn(&str) -> Option<Vec<u8>>,
    emit: &dyn Fn(ExportProgress),
    diagram_count: &mut usize,
) -> Result<ExportResult, &'static str> {
    let progress = |stage: Stage, done: usize, total: usize| {
        emit(ExportProgress {
            export_id: job.export_id.clone(),
            stage: stage.as_str(),
            done,
            total,
        })
    };
    let root = root.ok_or(ERR_NO_PROJECT_OPEN)?;
    // DXP-FR-XRAI: a destination that is not absolute did not come from the
    // save dialog, so there is no choice of the author to honour.
    let chosen = UserChosenPath::from_frontend_response(PathBuf::from(&job.destination))
        .ok_or(ERR_INVALID_DESTINATION)?;
    progress(Stage::Reading, 0, 0);
    let rel = check_doc_artifact(root, &job.path)?;
    // DXP-FR-IEQA: the file on disk, never an editor buffer.
    let text = root.read_text(&rel).map_err(|_| super::ERR_NOT_FOUND)?;
    let pictures = pictures_dir(root, &job.export_id)?;
    let file_dir = Path::new(&rel).parent().map(Path::to_path_buf).unwrap_or_default();
    let load_image = |url: &str| load_project_image(root, &file_dir, url);
    let access = root.access();
    let write_file = |name: &str, bytes: &[u8]| {
        let target = pictures.join(name);
        access.write_bytes_atomic(&target, bytes).ok().map(|_| target)
    };
    let input = Input {
        text: &text,
        format: job.format,
        include_frontmatter: job.include_frontmatter,
        load_image: &load_image,
        render_diagram,
        pictures_dir: &pictures,
        write_file: &write_file,
    };
    let mut report = |stage: Stage, done: usize, total: usize| progress(stage, done, total);
    // DXP-FR-LHEV / DXP-FR-NGUG: a panic in a writer is a failed conversion
    // that is logged and that still removes the pictures folder.
    let converted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        convert::convert(&input, &mut report)
    }))
    .unwrap_or_else(|_| Err("the conversion panicked".to_string()));
    let _ = access.delete_path(&pictures, true);
    let converted = converted.map_err(|_| ERR_CONVERSION_FAILED)?;
    *diagram_count = converted.diagram_count;
    progress(Stage::Writing, 0, 0);
    access.write_bytes_at_user_choice(&chosen, &converted.bytes).map_err(|_| ERR_WRITE_FAILED)?;
    let warnings = converted
        .failed_diagrams
        .iter()
        .map(|&index| ExportWarning { kind: "diagram_failed", index })
        .collect();
    Ok(ExportResult { destination: job.destination.clone(), warnings })
}

/// The folder the PDF pictures of one export go to: in the session temp
/// directory of the IDE instance, or in the store of the project when the
/// instance has none.
fn pictures_dir(root: &RootFs, export_id: &str) -> Result<PathBuf, &'static str> {
    let safe: String = export_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(64)
        .collect();
    let base = match root.access().session_temp_dir() {
        Some(dir) => dir.to_path_buf(),
        None => root.path().join(".synthesis").join("tmp"),
    };
    let dir = base.join(format!("export-{safe}-{}", crate::notes::new_note_id()));
    root.access().create_dir(&dir).map_err(|_| ERR_CONVERSION_FAILED)?;
    Ok(dir)
}

/// DXP-FR-JYHX: the bytes of a local image of the project, relative to the
/// file. A remote image, a data URL, an absolute path, and a path that leaves
/// the project give `None`.
fn load_project_image(root: &RootFs, file_dir: &Path, url: &str) -> Option<Vec<u8>> {
    let url = url.trim();
    if url.is_empty() || url.contains("://") || url.starts_with("data:") || url.starts_with('/') {
        return None;
    }
    let decoded = url.replace("%20", " ");
    let rel = file_dir.join(decoded);
    let abs = fs::resolve_under(root.path(), &rel).ok()?;
    root.access().read_regular_bytes(&abs, MAX_IMAGE_BYTES).ok()
}
