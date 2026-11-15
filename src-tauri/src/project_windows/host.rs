//! The window side of the project window list (`PWN-project-windows.md`).
//!
//! [`ProjectWindows`] is the managed state that holds the list. The commands
//! below change the list and then change the OS windows to match it: they create
//! a window, focus it, set its title, and close it. Each change of the list
//! reaches every window as `"project-windows-changed"`.

use crate::project_scope::events::EmitToProject;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;
use tauri::{Emitter, Manager, Runtime};

use super::{
    project_window_title, OpenOutcome, ProjectWindowEntry, ProjectWindowList,
    ProjectWindowsSnapshot,
};
use crate::log_fields;
use crate::logging::{self, Domain, BUFFER};

/// The label of the first window. It is the Project picker until a project opens
/// in it, and a project window after that.
pub const MAIN_WINDOW_LABEL: &str = "main";

/// The label prefix of every window that `open_project_window` creates.
pub const PROJECT_WINDOW_PREFIX: &str = "project-";

/// PWN-FR-NEYF: the list changed.
pub const EVENT_WINDOWS_CHANGED: &str = "project-windows-changed";

/// PWN-FR-BYCL: the window of one project must run its close flow.
pub const EVENT_CLOSE_REQUESTED: &str = "project-window-close-requested";

/// PWN-FR-ILXF: the tabbing identifier of the native macOS tab group.
#[cfg(target_os = "macos")]
pub const TABBING_IDENTIFIER: &str = "tech.br5.synthesis.project";

/// The typed error of a project that another window holds.
pub const ERR_PROJECT_ALREADY_OPEN: &str = "project_already_open";

/// The typed error of a project that no window holds.
pub const ERR_PROJECT_NOT_OPEN: &str = "project_not_open";

const DOMAINS: [Domain; 1] = [Domain::Backend];

/// The list of project windows, held in memory only (PWN-FR-JFLE).
#[derive(Default)]
pub struct ProjectWindows {
    list: Mutex<ProjectWindowList>,
    counter: AtomicU32,
}

impl ProjectWindows {
    pub(crate) fn lock(&self) -> MutexGuard<'_, ProjectWindowList> {
        self.list.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The label of the next window that `open_project_window` creates.
    pub(crate) fn next_label(&self) -> String {
        format!("{PROJECT_WINDOW_PREFIX}{}", self.counter.fetch_add(1, Ordering::Relaxed) + 1)
    }

    /// The window label of one project.
    pub fn label_of(&self, project_id: &str) -> Option<String> {
        self.lock().find(project_id).map(|e| e.window_label.clone())
    }

    /// The project id that one window holds.
    pub fn project_of(&self, window_label: &str) -> Option<String> {
        self.lock().find_by_label(window_label).map(|e| e.project_id.clone())
    }

    /// The entries in the order of opening.
    pub fn entries(&self) -> Vec<ProjectWindowEntry> {
        self.lock().entries().to_vec()
    }
}

/// Whether a window label is the label of a project window or of the first one.
pub fn is_project_window_label(label: &str) -> bool {
    label == MAIN_WINDOW_LABEL || label.starts_with(PROJECT_WINDOW_PREFIX)
}

/// The label of the project window that has OS focus, or the first window.
pub fn focused_project_window_label<R: Runtime>(app: &tauri::AppHandle<R>) -> String {
    app.webview_windows()
        .into_iter()
        .filter(|(label, _)| is_project_window_label(label))
        .find(|(_, window)| window.is_focused().unwrap_or(false))
        .map(|(label, _)| label)
        .unwrap_or_else(|| MAIN_WINDOW_LABEL.to_string())
}

fn log_event<R: Runtime>(app: &tauri::AppHandle<R>, message: &str, name: &str, label: &str) {
    logging::log_info(
        app,
        &BUFFER,
        &DOMAINS,
        message,
        log_fields! { "project" => name, "window" => label },
    );
}

/// Tell every window about the list, and bring the OS titles in line with it.
fn publish<R: Runtime>(app: &tauri::AppHandle<R>, snapshot: &ProjectWindowsSnapshot) {
    for entry in &snapshot.windows {
        if let Some(window) = app.get_webview_window(&entry.window_label) {
            let title = project_window_title(&entry.name, entry.needs_attention);
            if window.title().map(|current| current != title).unwrap_or(true) {
                let _ = window.set_title(&title);
            }
        }
    }
    let _ = app.emit_project(EVENT_WINDOWS_CHANGED, snapshot);
}

fn bring_to_front<R: Runtime>(app: &tauri::AppHandle<R>, label: &str) {
    if crate::settings_window::a_settings_window_is_open(app) {
        // PWN-FR-VIYR: the Settings window is application-modal.
        if let Some(settings) =
            app.get_webview_window(crate::settings_window::SETTINGS_WINDOW_LABEL)
        {
            let _ = settings.set_focus();
        }
        return;
    }
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn entry_for(project_id: &str, window_label: &str) -> ProjectWindowEntry {
    ProjectWindowEntry {
        project_id: project_id.to_string(),
        name: crate::project::basename(project_id),
        window_label: window_label.to_string(),
        needs_attention: false,
    }
}

/// PWN-FR-MVWL: the project id of a path.
pub fn project_id_of_path(path: &str) -> String {
    crate::worktree::project_anchor(Path::new(path)).to_string_lossy().to_string()
}

/// PWN-FR-TJEN.
#[tauri::command]
pub fn list_project_windows(windows: tauri::State<'_, ProjectWindows>) -> ProjectWindowsSnapshot {
    windows.lock().snapshot()
}

/// The entry of the window that makes the call, if it holds a project.
#[tauri::command]
pub fn current_project_window<R: Runtime>(
    window: tauri::Window<R>,
    windows: tauri::State<'_, ProjectWindows>,
) -> Option<ProjectWindowEntry> {
    windows.lock().find_by_label(window.label()).cloned()
}

/// PWN-FR-AYTY: the calling window holds this project now. Called after the
/// project is mounted, at the first open in a window and at "This tab".
#[tauri::command]
pub fn attach_project_to_window<R: Runtime>(
    project_id: String,
    window: tauri::Window<R>,
    app: tauri::AppHandle<R>,
    windows: tauri::State<'_, ProjectWindows>,
    scopes: tauri::State<'_, crate::project_scope::ProjectScopes>,
) -> Result<(), String> {
    let label = window.label().to_string();
    let name = crate::project::basename(&project_id);
    let mut replaced_project = None;
    let snapshot = {
        let mut list = windows.lock();
        if let Some(previous) = list.find_by_label(&label).map(|e| e.project_id.clone()) {
            if previous != project_id {
                replaced_project = Some(previous);
            }
        }
        if list.find_by_label(&label).is_some() {
            if !list.replace_project(&label, &project_id, &name) {
                return Err(ERR_PROJECT_ALREADY_OPEN.to_string());
            }
        } else if let OpenOutcome::AlreadyOpen { .. } = list.open(entry_for(&project_id, &label)) {
            return Err(ERR_PROJECT_ALREADY_OPEN.to_string());
        }
        list.activate(&project_id);
        list.snapshot()
    };
    // PWN-FR-AYTY: the project this window held leaves with its scope.
    if let Some(previous) = replaced_project {
        scopes.remove(&previous);
    }
    log_event(&app, "project attached to window", &name, &label);
    publish(&app, &snapshot);
    Ok(())
}

/// The calling window holds no project now: it shows the Project picker.
#[tauri::command]
pub fn detach_project_from_window<R: Runtime>(
    window: tauri::Window<R>,
    app: tauri::AppHandle<R>,
    windows: tauri::State<'_, ProjectWindows>,
) {
    let label = window.label().to_string();
    let snapshot = {
        let mut list = windows.lock();
        let Some(id) = list.find_by_label(&label).map(|e| e.project_id.clone()) else {
            return;
        };
        list.close(&id);
        list.snapshot()
    };
    log_event(&app, "project detached from window", "", &label);
    publish(&app, &snapshot);
}

/// The result of `open_project_window`.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpenProjectWindowResult {
    pub project_id: String,
    pub window_label: String,
    pub created: bool,
}

/// PWN-FR-NAQR, PWN-FR-RORD: open a project in a new window, or activate the
/// window that holds it. The new window mounts the project itself at startup.
#[tauri::command]
pub async fn open_project_window<R: Runtime>(
    path: String,
    window: tauri::Window<R>,
    app: tauri::AppHandle<R>,
    windows: tauri::State<'_, ProjectWindows>,
) -> Result<OpenProjectWindowResult, String> {
    if path.trim().is_empty() {
        return Err("path is empty".to_string());
    }
    let project_id = project_id_of_path(&path);
    let label = windows.next_label();
    let outcome = windows.lock().open(entry_for(&project_id, &label));
    if let OpenOutcome::AlreadyOpen { window_label } = outcome {
        let snapshot = windows.lock().snapshot();
        bring_to_front(&app, &window_label);
        publish(&app, &snapshot);
        return Ok(OpenProjectWindowResult { project_id, window_label, created: false });
    }
    let name = crate::project::basename(&project_id);
    let created = create_project_window(&app, &window, &label, &name);
    if let Err(reason) = created {
        let snapshot = {
            let mut list = windows.lock();
            list.close(&project_id);
            list.snapshot()
        };
        publish(&app, &snapshot);
        logging::log_error(
            &app,
            &BUFFER,
            &DOMAINS,
            "project window could not be created",
            log_fields! { "window" => label.as_str(), "reason" => reason.as_str() },
        );
        return Err(reason);
    }
    let snapshot = windows.lock().snapshot();
    log_event(&app, "project window created", &name, &label);
    publish(&app, &snapshot);
    Ok(OpenProjectWindowResult { project_id, window_label: label, created: true })
}

fn create_project_window<R: Runtime>(
    app: &tauri::AppHandle<R>,
    from: &tauri::Window<R>,
    label: &str,
    title: &str,
) -> Result<(), String> {
    let mut builder =
        tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::App("index.html".into()))
            .title(title)
            .disable_drag_drop_handler();
    if let Ok(size) = from.inner_size() {
        let scale = from.scale_factor().unwrap_or(1.0);
        builder = builder.inner_size(size.width as f64 / scale, size.height as f64 / scale);
    }
    if from.is_maximized().unwrap_or(false) {
        builder = builder.maximized(true);
    }
    #[cfg(target_os = "macos")]
    {
        builder = builder.tabbing_identifier(TABBING_IDENTIFIER);
    }
    builder.build().map(|_| ()).map_err(|e| e.to_string())
}

/// PWN-FR-XUJP.
#[tauri::command]
pub fn activate_project_window<R: Runtime>(
    project_id: String,
    app: tauri::AppHandle<R>,
    windows: tauri::State<'_, ProjectWindows>,
) -> Result<(), String> {
    let (label, snapshot) = {
        let mut list = windows.lock();
        let label = list.activate(&project_id).ok_or_else(|| ERR_PROJECT_NOT_OPEN.to_string())?;
        (label, list.snapshot())
    };
    bring_to_front(&app, &label);
    publish(&app, &snapshot);
    Ok(())
}

/// PWN-FR-BYCL.
#[tauri::command]
pub fn request_project_window_close<R: Runtime>(
    project_id: String,
    app: tauri::AppHandle<R>,
    windows: tauri::State<'_, ProjectWindows>,
) -> Result<(), String> {
    let label = windows.label_of(&project_id).ok_or_else(|| ERR_PROJECT_NOT_OPEN.to_string())?;
    app.emit_to(label.as_str(), EVENT_CLOSE_REQUESTED, ProjectIdPayload { project_id })
        .map_err(|e| e.to_string())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectIdPayload {
    project_id: String,
}

/// The result of `close_project_window`.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CloseProjectWindowResult {
    /// The window that made the call holds no project and shows the picker.
    pub last_closed: bool,
}

/// PWN-FR-YRXF, PWN-FR-QEGI: drop the scope of the project, remove its entry,
/// and close its window. The last window stays and shows the Project picker.
#[tauri::command]
pub fn close_project_window<R: Runtime>(
    project_id: String,
    app: tauri::AppHandle<R>,
    windows: tauri::State<'_, ProjectWindows>,
    scopes: tauri::State<'_, crate::project_scope::ProjectScopes>,
) -> Result<CloseProjectWindowResult, String> {
    let (label, name, outcome, snapshot) = {
        let mut list = windows.lock();
        let entry =
            list.find(&project_id).cloned().ok_or_else(|| ERR_PROJECT_NOT_OPEN.to_string())?;
        let outcome = list.close(&project_id);
        (entry.window_label, entry.name, outcome, list.snapshot())
    };
    scopes.remove(&project_id);
    let last_closed = snapshot.windows.is_empty();
    log_event(&app, "project window closed", &name, &label);
    if !last_closed {
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.destroy();
        }
        if let Some(next) = outcome
            .as_ref()
            .filter(|o| o.active_changed)
            .and_then(|o| o.active_window_label.clone())
        {
            bring_to_front(&app, &next);
        }
    }
    publish(&app, &snapshot);
    Ok(CloseProjectWindowResult { last_closed })
}

/// PWN-FR-CFSW.
#[tauri::command]
pub fn set_project_window_attention<R: Runtime>(
    project_id: String,
    needs_attention: bool,
    app: tauri::AppHandle<R>,
    windows: tauri::State<'_, ProjectWindows>,
) -> Result<(), String> {
    let snapshot = {
        let mut list = windows.lock();
        if list.find(&project_id).is_none() {
            return Err(ERR_PROJECT_NOT_OPEN.to_string());
        }
        if !list.set_attention(&project_id, needs_attention) {
            return Ok(());
        }
        list.snapshot()
    };
    publish(&app, &snapshot);
    Ok(())
}

/// PJT-FR-EKKA: activate the tab that follows or precedes the active one.
pub fn activate_neighbour<R: Runtime>(app: &tauri::AppHandle<R>, step: i32) {
    let Some(windows) = app.try_state::<ProjectWindows>() else {
        return;
    };
    let target = {
        let list = windows.lock();
        list.neighbour(step).map(str::to_string)
    };
    let Some(project_id) = target else {
        return;
    };
    let label_and_snapshot = {
        let mut list = windows.lock();
        list.activate(&project_id).map(|label| (label, list.snapshot()))
    };
    if let Some((label, snapshot)) = label_and_snapshot {
        bring_to_front(app, &label);
        publish(app, &snapshot);
    }
}

/// A window gained OS focus: its tab is the active one and needs no attention.
pub fn note_window_focused<R: Runtime>(app: &tauri::AppHandle<R>, label: &str) {
    let Some(windows) = app.try_state::<ProjectWindows>() else {
        return;
    };
    let snapshot = {
        let mut list = windows.lock();
        let Some(id) = list.find_by_label(label).map(|e| e.project_id.clone()) else {
            return;
        };
        let unchanged = list.active_project_id() == Some(id.as_str())
            && list.find(&id).is_some_and(|e| !e.needs_attention);
        if unchanged {
            return;
        }
        list.activate(&id);
        list.snapshot()
    };
    publish(app, &snapshot);
}

/// PWN-FR-NTFY: the window label of one project, if a window holds it.
pub fn window_label_of_project<R: Runtime>(
    app: &tauri::AppHandle<R>,
    project_id: &str,
) -> Option<String> {
    if project_id.is_empty() {
        return None;
    }
    app.try_state::<ProjectWindows>()?.label_of(project_id)
}

/// PWN-FR-NTFY: a click on a notification of a project activates the tab of that
/// project. Returns the label of the window to raise.
pub fn activate_project_for_notification<R: Runtime>(
    app: &tauri::AppHandle<R>,
    project_id: &str,
) -> Option<String> {
    if project_id.is_empty() {
        return None;
    }
    let windows = app.try_state::<ProjectWindows>()?;
    let (label, snapshot) = {
        let mut list = windows.lock();
        let label = list.activate(project_id)?;
        (label, list.snapshot())
    };
    publish(app, &snapshot);
    Some(label)
}
