//! The manager of a project scope (PWN-FR-ISOL, MCS-FR-NOKX).
//!
//! Every project scope makes its own manager, which starts with no observer. The
//! mount of the content root gives it the application's event and log channels.

use super::*;
use crate::mcp::config::ListedRecord;
use crate::mcp::manager::ServerInput;
use crate::mcp::status::{MachineFacts, STATUS_EVENT};
use crate::project_scope::{context, ProjectScopes, ProjectStores};
use crate::project_windows::{ProjectWindowEntry, ProjectWindows};
use std::sync::mpsc::channel;
use tauri::{Listener, Manager, WebviewUrl, WebviewWindowBuilder};

// A server with no secrets gets its status from the gate, with no network
// and no process.
fn input_without_secrets(id: &str) -> ServerInput {
    ServerInput {
        owner: "/repo".into(),
        listed: ListedRecord { id: id.to_string(), record: Ok(http_record(id, McpAuth::None)) },
        facts: MachineFacts {
            trusted: true,
            secrets_present: false,
            signed_in: true,
            vault_unavailable: false,
        },
    }
}

#[test]
fn a_status_change_in_a_project_scope_reaches_the_window_of_that_project() {
    let app = tauri::test::mock_app();
    app.manage(ProjectScopes::default());
    let windows = ProjectWindows::default();
    windows.lock().open(ProjectWindowEntry {
        project_id: "two".to_string(),
        name: "two".to_string(),
        window_label: "project-1".to_string(),
        needs_attention: false,
    });
    app.manage(windows);
    let window =
        WebviewWindowBuilder::new(&app, "project-1", WebviewUrl::default()).build().unwrap();
    let (sender, receiver) = channel();
    let sender = std::sync::Mutex::new(sender);
    window.listen(STATUS_EVENT, move |event| {
        let payload: serde_json::Value = serde_json::from_str(event.payload()).unwrap();
        let _ = sender.lock().unwrap().send(payload);
    });
    let handle = app.handle().clone();

    context::run_in("two", || {
        crate::mcp::on_content_root_mounted(&handle);
        let manager = handle.project_store::<McpManager>();
        manager.reconcile(
            crate::mcp::project_context("/repo", std::env::temp_dir()),
            vec![input_without_secrets("docs")],
            None,
        );
    });

    let payload = receiver.try_recv().expect("the status change reached the window");
    assert_eq!(payload["id"], "docs");
    assert_eq!(payload["projectId"], "two");
}
