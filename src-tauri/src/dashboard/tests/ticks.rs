//! The timer tick with several projects open (PWN-FR-CTXT).
//!
//! One part of `../tests/mod.rs`.

use super::*;
use crate::project::ProjectState;
use crate::project_scope::context;
use crate::project_scope::ProjectScopes;
use crate::project_windows::{ProjectWindowEntry, ProjectWindows};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Mutex;
use tauri::{Listener, Manager, WebviewUrl, WebviewWindowBuilder};

fn entry(id: &str, label: &str) -> ProjectWindowEntry {
    ProjectWindowEntry {
        project_id: id.to_string(),
        name: id.to_string(),
        window_label: label.to_string(),
        needs_attention: false,
    }
}

/// A channel of the refresh events that reach the window.
fn listen_in(
    app: &tauri::App<tauri::test::MockRuntime>,
    label: &str,
) -> Receiver<serde_json::Value> {
    let window = WebviewWindowBuilder::new(app, label, WebviewUrl::default()).build().unwrap();
    let (sender, receiver) = channel();
    let sender = Mutex::new(sender);
    for event in [DASHBOARD_PENDING_GIT_CHANGED, DASHBOARD_REFRESH_FAILED] {
        let sender = sender.lock().unwrap().clone();
        let sender = Mutex::new(sender);
        window.listen(event, move |event| {
            let payload = serde_json::from_str(event.payload()).unwrap_or_default();
            let _ = sender.lock().unwrap().send(payload);
        });
    }
    receiver
}

// The timer starts with the application and has no project of its own. One tick
// refreshes the open project of each window, and the result reaches that window
// only.
#[test]
fn one_tick_refreshes_every_open_project() {
    let app = tauri::test::mock_app();
    app.manage(ProjectScopes::default());
    let one = tempfile::tempdir().unwrap();
    let two = tempfile::tempdir().unwrap();
    {
        let scopes = app.state::<ProjectScopes>();
        scopes.get::<ProjectState>("one").set_root(one.path().to_path_buf());
        scopes.get::<ProjectState>("two").set_root(two.path().to_path_buf());
    }
    let windows = ProjectWindows::default();
    windows.lock().open(entry("one", "main"));
    windows.lock().open(entry("two", "project-1"));
    app.manage(windows);
    let in_one = listen_in(&app, "main");
    let in_two = listen_in(&app, "project-1");

    tick(app.handle());

    // `recv` waits for the event itself, so the result does not depend on how
    // fast the refresh thread runs.
    let first = in_one.recv().expect("a refresh event for the project one");
    let second = in_two.recv().expect("a refresh event for the project two");
    assert_eq!(first["projectId"], "one");
    assert_eq!(second["projectId"], "two");
    assert!(context::current().is_none(), "the tick leaves no context behind");
}
