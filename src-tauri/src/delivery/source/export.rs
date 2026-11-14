//! `export_delivery_to_file(path)` (`DSR-delivery-source.md` DSR-FR-OIAS to
//! DSR-FR-JWYO): move the delivery plan of the GitHub Project to a new Flow
//! file.
//!
//! The export changes nothing on GitHub and keeps the overlay of the GitHub
//! plan. The new file is written through the save path of
//! `PST-project-storage.md`, which validates a Flow before it writes it.

use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Runtime};

use crate::delivery::github;
use crate::delivery::machine_state::{load_layout, save_layout};
use crate::delivery::settings::DeliverySourceChoice;
use crate::delivery::DeliveryError;
use crate::logging::LogLevel;

use super::{
    log, project_root, refuse, resolve_local, Resolved, ERR_FLOW_INVALID, ERR_NOT_A_FLOW,
    ERR_NOT_GITHUB_SOURCE, ERR_PATH_EXISTS,
};

/// The new file could not be written (PST-FR-10).
pub const ERR_WRITE_FAILED: &str = "write_failed";

/// DSR contract surface: what an export returns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryExportResult {
    pub flow_id: String,
    pub path: String,
}

/// The project-relative path of the new file, or `not_a_flow`. The path ends
/// in `.flow` and stays inside the project.
fn checked_path(root: &crate::fs::RootFs, path: &str) -> Result<String, DeliveryError> {
    let path = path.trim().trim_start_matches('/').to_string();
    let refused = || {
        DeliveryError::new(ERR_NOT_A_FLOW, "The path must be inside the project and end in .flow.")
    };
    if !path.ends_with(".flow") || path.len() <= ".flow".len() || path.contains('\\') {
        return Err(refused());
    }
    crate::fs::resolve_under(root.path(), &path).map_err(|_| refused())?;
    Ok(path)
}

/// DSR-FR-OIAS to DSR-FR-JWYO: the export, on the calling thread.
pub fn export_impl<R: Runtime>(
    app: &AppHandle<R>,
    path: &str,
) -> Result<DeliveryExportResult, DeliveryError> {
    let started = Instant::now();
    let op = "export_delivery_to_file";
    log(app, LogLevel::Info, true, "delivery export started", crate::log_fields! {});
    let fail = |e| refuse(app, op, started, e);
    let root = project_root(app).map_err(fail)?;
    let (mut settings, resolved) = resolve_local(&root).map_err(fail)?;
    let Resolved::Github { project_node_id } = resolved else {
        return Err(fail(DeliveryError::new(
            ERR_NOT_GITHUB_SOURCE,
            "The delivery source of the project is not a GitHub Project.",
        )));
    };
    let path = checked_path(&root, path).map_err(fail)?;
    let target_path = crate::fs::resolve_under(root.path(), &path).map_err(|_| {
        fail(DeliveryError::new(ERR_NOT_A_FLOW, "The path is not inside the project."))
    })?;
    if root.file_info(&target_path).is_ok() {
        return Err(fail(DeliveryError::new(ERR_PATH_EXISTS, "A file or a folder has this path.")));
    }
    let target = github::target_of_selected(app).map_err(fail)?;
    let plan = github::load_target(app, &target, true).map_err(fail)?;
    if let Some(error) = plan.error {
        return Err(fail(error));
    }
    // DSR-FR-RZUH: a version 3 document with the Project's title as its name,
    // and each item, edge, and reference of the plan, its milestone items in
    // milestone order (FLO-FR-MCFR). A Flow document has no field for a
    // tracker reference.
    let mut document = plan.document.clone();
    super::milestones::order_milestone_items(&mut document.items);
    let body = serde_json::to_string_pretty(&document).map_err(|_| {
        fail(DeliveryError::new(ERR_WRITE_FAILED, "The plan could not be written."))
    })?;
    let report = crate::flow_validation::validate(&body);
    if !report.valid {
        return Err(fail(
            DeliveryError::new(ERR_FLOW_INVALID, "The plan is not a valid Flow.")
                .with_report(report),
        ));
    }
    // DSR-FR-RZUH, DSR-FR-JWYO: the steps go in this order, so a failed
    // export leaves no file that refuses a retry with `path_exists`. The
    // layout goes first: it is a record of the machine store that a retry
    // writes again. The file goes next. The settings go last; when they cannot
    // be written, the new file is removed again, and the source stays GitHub.
    let store = github::machine_store(app).map_err(fail)?;
    let layout =
        load_layout(app, github::buffer_of(app), &store, &format!("github:{project_node_id}"))
            .map_err(fail)?;
    if !layout.items.is_empty() {
        save_layout(&store, &format!("flow:{path}"), &layout).map_err(fail)?;
    }
    crate::artifacts::save_artifact_contents_blocking(app, &path, &format!("{body}\n"))
        .map_err(|reason| fail(DeliveryError::new(ERR_WRITE_FAILED, reason)))?;
    // DSR-FR-JWYO: the source is the new file now. GitHub and the overlay of
    // the GitHub plan stay as they are.
    settings.source = DeliverySourceChoice::File { flow_id: path.clone() };
    if let Err(error) = super::store_settings(&root, &settings) {
        remove_exported_file(app, &root, &path);
        return Err(fail(error));
    }
    log(
        app,
        LogLevel::Info,
        true,
        "delivery exported",
        crate::log_fields! {
            "items" => plan.document.items.len(),
            "edges" => plan.document.edges.len(),
            "layout_entries" => layout.items.len(),
            "ms" => started.elapsed().as_millis() as u64,
        },
    );
    super::announce(app);
    Ok(DeliveryExportResult { flow_id: path.clone(), path })
}

/// DSR-FR-RZUH, DSR-FR-JWYO: remove the file that a failed export wrote, so
/// that a retry does not find it. The content tracker forgets the file too.
fn remove_exported_file<R: Runtime>(app: &AppHandle<R>, root: &crate::fs::RootFs, path: &str) {
    use tauri::Manager;
    let removed = root.delete_under(root.path(), path, false);
    if let Some(tracker) = app.try_state::<crate::artifacts::ContentTracker>() {
        tracker.forget(path);
    }
    let level = if removed.is_ok() { LogLevel::Warn } else { LogLevel::Error };
    log(
        app,
        level,
        true,
        "delivery export file removed after a failure",
        crate::log_fields! { "removed" => removed.is_ok() },
    );
}

/// DSR contract surface: `"export delivery to file (path)"`.
#[tauri::command]
pub async fn export_delivery_to_file(
    app: AppHandle,
    path: String,
) -> Result<DeliveryExportResult, DeliveryError> {
    super::off_thread(move || export_impl(&app, &path)).await
}
