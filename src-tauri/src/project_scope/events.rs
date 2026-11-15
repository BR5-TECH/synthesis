//! Delivery of events to the window of their project (`PWN-project-windows.md`
//! PWN-FR-AINZ).
//!
//! An event that serves one project goes to the window of that project only, so
//! no window applies the event of another project. An event of the whole
//! application goes to every window. The code that emits an event runs in the
//! context of its project (`context`), and that decides where the event goes.

use serde::Serialize;
use tauri::{Emitter, Manager, Runtime};

use super::{context, PROJECT_ID_FIELD};
use crate::project_windows::ProjectWindows;

/// Whether an event belongs to the whole application. Settings, credentials,
/// voice, the log, the menu, and the windows themselves are not project data.
pub fn is_application_event(event: &str) -> bool {
    const APPLICATION_EVENTS: &[&str] = &[
        "agents-changed",
        "github-tokens-changed",
        "log-records-appended",
        "model-downgrade-detected",
        "notification-activated",
        "project-windows-changed",
        "project-window-close-requested",
    ];
    APPLICATION_EVENTS.contains(&event)
        || event.starts_with("menu:")
        || event.starts_with("voice-")
        || event.starts_with("settings-window:")
        || event.starts_with("test-")
}

/// The window label that an event of the running code goes to, or `None` for
/// every window.
pub fn target_window<R: Runtime, M: Manager<R>>(app: &M, event: &str) -> Option<String> {
    if is_application_event(event) {
        return None;
    }
    let project_id = context::current().filter(|id| !id.is_empty())?;
    app.try_state::<ProjectWindows>()?.label_of(&project_id)
}

/// The payload of a project event with the id of its project in the `projectId`
/// field (PWN-FR-AINZ). A payload without a value becomes an object that holds
/// the id only. A payload that is not an object stays as it is.
pub fn tag_payload<S: Serialize>(payload: S, project_id: &str) -> serde_json::Value {
    let mut value = serde_json::to_value(payload).unwrap_or(serde_json::Value::Null);
    if value.is_null() {
        value = serde_json::Value::Object(serde_json::Map::new());
    }
    if let serde_json::Value::Object(fields) = &mut value {
        fields
            .entry(PROJECT_ID_FIELD)
            .or_insert_with(|| serde_json::Value::String(project_id.to_string()));
    }
    value
}

/// The id of the project that the running code serves, for an event that
/// belongs to one project.
fn event_project_id(event: &str) -> Option<String> {
    if is_application_event(event) {
        return None;
    }
    context::current().filter(|id| !id.is_empty())
}

/// Emit an event to the window of the project that the running code serves, or
/// to every window when it serves no project. The payload carries the
/// `projectId`, and the frontend drops an event of another project.
pub trait EmitToProject<R: Runtime> {
    fn emit_project<S: Serialize + Clone>(&self, event: &str, payload: S) -> tauri::Result<()>;
}

impl<R: Runtime, M: Manager<R> + Emitter<R>> EmitToProject<R> for M {
    fn emit_project<S: Serialize + Clone>(&self, event: &str, payload: S) -> tauri::Result<()> {
        let Some(project_id) = event_project_id(event) else {
            return self.emit(event, payload);
        };
        let tagged = tag_payload(payload, &project_id);
        match target_window::<R, M>(self, event) {
            Some(label) => self.emit_to(label.as_str(), event, tagged),
            None => self.emit(event, tagged),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_windows::ProjectWindowEntry;
    use tauri::test::mock_app;

    fn entry(id: &str, label: &str) -> ProjectWindowEntry {
        ProjectWindowEntry {
            project_id: id.to_string(),
            name: id.to_string(),
            window_label: label.to_string(),
            needs_attention: false,
        }
    }

    #[test]
    fn application_events_reach_every_window() {
        assert!(is_application_event("agents-changed"));
        assert!(is_application_event("menu:exit-requested"));
        assert!(is_application_event("voice-ai-changed"));
        assert!(!is_application_event("project-tree-changed"));
    }

    #[test]
    fn an_event_goes_to_the_window_of_its_project() {
        let app = mock_app();
        let windows = ProjectWindows::default();
        windows.lock().open(entry("one", "main"));
        windows.lock().open(entry("two", "project-1"));
        app.manage(windows);
        let handle = app.handle();
        let to_one =
            context::run_in("one", || target_window::<_, _>(handle, "project-tree-changed"));
        let to_two =
            context::run_in("two", || target_window::<_, _>(handle, "project-tree-changed"));
        assert_eq!(to_one.as_deref(), Some("main"));
        assert_eq!(to_two.as_deref(), Some("project-1"));
    }

    #[test]
    fn an_application_event_has_no_target_even_in_a_project() {
        let app = mock_app();
        let windows = ProjectWindows::default();
        windows.lock().open(entry("one", "main"));
        app.manage(windows);
        let target =
            context::run_in("one", || target_window::<_, _>(app.handle(), "agents-changed"));
        assert_eq!(target, None);
    }

    #[test]
    fn code_with_no_context_emits_to_every_window() {
        let app = mock_app();
        app.manage(ProjectWindows::default());
        assert_eq!(target_window::<_, _>(app.handle(), "project-tree-changed"), None);
    }
}
