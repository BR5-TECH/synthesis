//! `FTS-flow-tracker-sync.md`: the two-way sync of a Flow with GitHub.
//!
//! A Flow that carries a sync section is synced with the project's publication
//! repository (GHP-FR-YDAN). Each call compares, field by field, the Flow's
//! value, the tracker's value, and the base both had at the last sync. A
//! change on one side goes to the other side, and a field both sides changed
//! differently is a conflict that the author decides.
//!
//! The commands here only resolve the project, the repository, and the token,
//! read and write the Flow file, and log. The sync itself is in
//! [`engine`], pure over the tracker adapter (FTS-FR-ARMD), so the tests drive
//! it with an in-memory adapter and make no network request.

pub mod adapter;
pub mod engine;
pub mod github;
pub mod links;
pub mod logged;
mod merge;
mod pull;
pub mod records;
mod removed;
pub mod serialize;
pub mod values;
mod writes;

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Instant;

use tauri::Manager;

use crate::flow_validation::model::{parse_valid, FlowDocument, RefKind};
use crate::log_fields;
use crate::logging::{log_error, log_info, log_warn, Domain, BUFFER};
use crate::project::ProjectState;
use crate::project_settings::FlowTrackerSettings;

pub use records::*;

/// FTS-FR-TUEJ: the Flows a sync or a resolve runs for now, by project root
/// and Flow id.
static BUSY: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// FTS-FR-TUEJ: the claim of one Flow, released when it is dropped.
pub struct BusyClaim(String);

impl BusyClaim {
    /// The claim of `key`, or `None` while another call holds it.
    pub fn take(key: String) -> Option<Self> {
        let mut busy = BUSY.lock().unwrap_or_else(|e| e.into_inner());
        // The claim is made only after the insert: a claim that is dropped
        // releases its key, and it must not release the key of another call.
        if busy.get_or_insert_with(HashSet::new).insert(key.clone()) {
            Some(Self(key))
        } else {
            None
        }
    }
}

impl Drop for BusyClaim {
    fn drop(&mut self) {
        let mut busy = BUSY.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(set) = busy.as_mut() {
            set.remove(&self.0);
        }
    }
}

/// Run one command's body on a blocking thread: every sync can make HTTP
/// requests, and a command that is not `async` runs on the window's thread.
async fn off_thread<T, F>(work: F) -> Result<T, FlowSyncError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, FlowSyncError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| FlowSyncError::of(ERR_GITHUB_UNREACHABLE))?
}

/// Log a refusal and return it. The record carries the code alone.
fn refuse<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    operation: &str,
    error: FlowSyncError,
) -> FlowSyncError {
    log_warn(
        app,
        &BUFFER,
        &[Domain::Backend],
        "flow tracker operation refused",
        log_fields! { "operation" => operation, "code" => &error.code },
    );
    error
}

fn project_root<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<crate::fs::RootFs, FlowSyncError> {
    app.try_state::<ProjectState>()
        .and_then(|p| p.require_root().ok())
        .ok_or_else(|| FlowSyncError::of(ERR_NO_PROJECT))
}

/// FTS-FR-LTMW: the body of the Flow `flow_id`, read through the project's
/// root. An id that escapes the root, names no file, or names a file that is
/// not a Flow is `flow_not_found`.
fn read_flow<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &crate::fs::RootFs,
    flow_id: &str,
) -> Result<String, FlowSyncError> {
    let not_found = || FlowSyncError::of(ERR_FLOW_NOT_FOUND);
    crate::library::ensure_inside_root(app, root, "read_flow", flow_id).map_err(|_| not_found())?;
    if crate::artifacts::resolved_type(root, flow_id) != Some(crate::scanning::ArtifactType::Flow) {
        return Err(not_found());
    }
    let path = crate::fs::resolve_under(root, flow_id).map_err(|_| not_found())?;
    root.read_text(&path).map_err(|_| not_found())
}

fn read_valid_flow<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &crate::fs::RootFs,
    flow_id: &str,
) -> Result<FlowDocument, FlowSyncError> {
    let body = read_flow(app, root, flow_id)?;
    parse_valid(&body).map_err(FlowSyncError::invalid)
}

/// FTS-FR-ZLFX: the adapter of the publication repository, with the token of
/// the project. A refusal of either resolution keeps its code.
fn resolve_tracker<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<github::GithubAdapter, FlowSyncError> {
    let project = app.state::<ProjectState>();
    let token = match (
        app.try_state::<crate::global_settings::GlobalSettingsStore>(),
        app.try_state::<crate::github_tokens::GithubTokens>(),
    ) {
        (Some(store), Some(tokens)) => {
            crate::github_tokens::resolve_project_token(&store, &tokens, &project.slot_key())
        }
        _ => Err(crate::github_tokens::ERR_TOKEN_MISSING.to_string()),
    };
    let repository =
        crate::github_publication::resolve_publication_repository_with(app, token.as_ref().ok())
            .map_err(FlowSyncError::of)?;
    let token = token.map_err(FlowSyncError::of)?;
    // GTS-FR-OBAS: the secret goes to the host of its own token alone.
    if crate::github_tokens::host::stored_host(&token.host)
        != crate::github_tokens::host::stored_host(&repository.repository_host)
    {
        return Err(FlowSyncError::of(crate::github_tokens::ERR_HOST_MISMATCH));
    }
    Ok(github::GithubAdapter::new(
        &repository.repository_host,
        &repository.repository_owner,
        &repository.repository_name,
        token.secret,
    ))
}

/// FTS-FR-LTMW: `"sync flow with tracker"`.
#[tauri::command]
pub async fn sync_flow_with_tracker(
    app: tauri::AppHandle,
    flow_id: String,
) -> Result<SyncResult, FlowSyncError> {
    off_thread(move || run_sync(&app, "sync_flow_with_tracker", &flow_id, &[])).await
}

/// FTS-FR-ARHM: `"resolve flow sync conflicts"`.
#[tauri::command]
pub async fn resolve_flow_sync_conflicts(
    app: tauri::AppHandle,
    flow_id: String,
    resolutions: Vec<ConflictResolution>,
) -> Result<SyncResult, FlowSyncError> {
    off_thread(move || run_sync(&app, "resolve_flow_sync_conflicts", &flow_id, &resolutions)).await
}

/// FTS-FR-LTMW to FTS-FR-FDDV: one sync or one resolve of `flow_id`.
fn run_sync<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    operation: &str,
    flow_id: &str,
    resolutions: &[ConflictResolution],
) -> Result<SyncResult, FlowSyncError> {
    let started = Instant::now();
    log_info(
        app,
        &BUFFER,
        &[Domain::Backend, Domain::Remote],
        "flow sync started",
        log_fields! { "operation" => operation, "flow" => flow_id, "resolutions" => resolutions.len() },
    );
    let outcome = sync_steps(app, operation, flow_id, resolutions);
    match &outcome {
        Ok(result) => log_info(
            app,
            &BUFFER,
            &[Domain::Backend, Domain::Remote],
            "flow sync finished",
            log_fields! {
                "operation" => operation,
                "flow" => flow_id,
                "created" => result.created,
                "updated" => result.updated,
                "added" => result.added,
                "changed" => result.changed,
                "conflicts" => result.conflicts.len(),
                "skipped" => result.skipped.len(),
                "stale" => result.stale.len(),
                "ms" => started.elapsed().as_millis() as u64,
            },
        ),
        Err(error) => log_error(
            app,
            &BUFFER,
            &[Domain::Backend, Domain::Remote],
            "flow sync failed",
            log_fields! {
                "operation" => operation,
                "flow" => flow_id,
                "code" => &error.code,
                "created" => error.created_urls.len(),
            },
        ),
    }
    outcome
}

fn sync_steps<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    operation: &str,
    flow_id: &str,
    resolutions: &[ConflictResolution],
) -> Result<SyncResult, FlowSyncError> {
    let root = project_root(app)?;
    let key = format!("{}\u{0}{flow_id}", root.path().display());
    let _claim = BusyClaim::take(key).ok_or_else(|| FlowSyncError::of(ERR_SYNC_IN_PROGRESS))?;
    let doc = read_valid_flow(app, &root, flow_id)?;
    if doc.sync.is_none() {
        return Err(FlowSyncError::of(ERR_SYNC_NOT_ENABLED));
    }
    let settings = crate::project_settings::load_flow_tracker_settings_from(&root)
        .map_err(FlowSyncError::of)?;
    let tracker = resolve_tracker(app)?;
    // The Flow is written after the tracker writes, so a write the branch
    // check refuses is refused before the first tracker request.
    crate::git::write_guard::commands::require_active_branch_write(app, &root, operation)
        .map_err(FlowSyncError::of)?;
    let logged = logged::LoggedAdapter::new(app, &tracker);
    let request = engine::SyncRequest { flow_id, doc, settings: &settings, resolutions };
    let outcome = engine::run(&logged, request)?;
    if outcome.doc_changed {
        // FTS-FR-VFNM: one write, through the save path that validates it.
        let body = serialize::write_flow(&outcome.doc);
        if crate::artifacts::save_artifact_contents_blocking(app, flow_id, &body).is_err() {
            // FTS-FR-AIDI.
            return Err(FlowSyncError::of(ERR_WRITE_FAILED).with_created(outcome.created_urls));
        }
    }
    if let Some(code) = outcome.write_error {
        return Err(FlowSyncError::of(code).with_created(outcome.created_urls));
    }
    Ok(outcome.result)
}

/// FTS-FR-TPWE: `"read tracker item"`.
#[tauri::command]
pub async fn read_tracker_item(
    app: tauri::AppHandle,
    flow_id: String,
    kind: RefKind,
    number: u64,
) -> Result<TrackerItemSnapshot, FlowSyncError> {
    off_thread(move || {
        let root = project_root(&app).map_err(|e| refuse(&app, "read_tracker_item", e))?;
        let doc = read_valid_flow(&app, &root, &flow_id)
            .map_err(|e| refuse(&app, "read_tracker_item", e))?;
        let tracker = resolve_tracker(&app).map_err(|e| refuse(&app, "read_tracker_item", e))?;
        let logged = logged::LoggedAdapter::new(&app, &tracker);
        let snapshot = links::snapshot(&logged, &doc, kind, number)
            .map_err(|e| refuse(&app, "read_tracker_item", e))?;
        log_info(
            &app,
            &BUFFER,
            &[Domain::Backend, Domain::Remote],
            "tracker item read",
            log_fields! { "flow" => &flow_id, "kind" => format!("{kind:?}"), "number" => number },
        );
        Ok(snapshot)
    })
    .await
}

/// FTS-FR-OBPX: `"open tracker link"`.
#[tauri::command]
pub async fn open_tracker_link(app: tauri::AppHandle, url: String) -> Result<(), FlowSyncError> {
    off_thread(move || {
        let operation = "open_tracker_link";
        let repository = crate::github_publication::resolve_publication_repository(&app)
            .map_err(|code| refuse(&app, operation, FlowSyncError::of(code)))?;
        if !links::is_tracker_url(
            &url,
            &repository.repository_host,
            &repository.repository_owner,
            &repository.repository_name,
        ) {
            // The URL is not logged: a refused one can carry anything.
            return Err(refuse(&app, operation, FlowSyncError::of(ERR_TRACKER_URL_REFUSED)));
        }
        use tauri_plugin_opener::OpenerExt;
        app.opener().open_url(&url, None::<&str>).map_err(|_| {
            refuse(&app, operation, FlowSyncError::of(ERR_GITHUB_UNREACHABLE))
        })?;
        log_info(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "tracker link opened",
            log_fields! { "kind" => if url.contains("/milestone/") { "milestone" } else { "issue" } },
        );
        Ok(())
    })
    .await
}

/// FTS-FR-LMBE: `"get flow tracker settings"`, defaults applied.
#[tauri::command]
pub fn get_flow_tracker_settings(
    app: tauri::AppHandle,
) -> Result<FlowTrackerSettings, FlowSyncError> {
    let operation = "get_flow_tracker_settings";
    let root = project_root(&app).map_err(|e| refuse(&app, operation, e))?;
    crate::project_settings::load_flow_tracker_settings_from(&root)
        .map_err(|code| refuse(&app, operation, FlowSyncError::of(code)))
}

/// FTS-FR-ALOJ: `"set flow tracker settings"`. Stores the five Types and
/// returns them, or refuses and writes nothing.
#[tauri::command]
pub fn set_flow_tracker_settings(
    app: tauri::AppHandle,
    deliverable_type: String,
    epic_type: String,
    feature_type: String,
    task_type: String,
    defect_type: String,
) -> Result<FlowTrackerSettings, FlowSyncError> {
    let operation = "set_flow_tracker_settings";
    let root = project_root(&app).map_err(|e| refuse(&app, operation, e))?;
    let settings = links::validated_settings(
        &deliverable_type,
        &epic_type,
        &feature_type,
        &task_type,
        &defect_type,
    )
    .map_err(|e| refuse(&app, operation, e))?;
    crate::project_settings::save_flow_tracker_settings_to(&root, &settings)
        .map_err(|code| refuse(&app, operation, FlowSyncError::of(code)))?;
    log_info(&app, &BUFFER, &[Domain::Backend], "flow tracker settings saved", log_fields! {});
    Ok(settings)
}

#[cfg(test)]
mod tests;
