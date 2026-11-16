//! Two open projects never share a store (`PWN-project-windows.md`
//! PWN-FR-ISOL, PWN-FR-CTXT).

use super::context;
use super::{ProjectScopes, ProjectStores, Scoped, DEFAULT_SCOPE};
use crate::agent_conversations::TurnRegistry;
use crate::bm25_index::Bm25Indexer;
use crate::project::{OpenProject, ProjectState};
use crate::watcher::ProjectWatcher;
use tauri::test::mock_app;
use tauri::Manager;

fn app_with_two_projects(
) -> (tauri::App<tauri::test::MockRuntime>, tempfile::TempDir, tempfile::TempDir) {
    let app = mock_app();
    app.manage(ProjectScopes::default());
    let one = tempfile::tempdir().unwrap();
    let two = tempfile::tempdir().unwrap();
    {
        let scopes = app.state::<ProjectScopes>();
        scopes.get::<ProjectState>("one").set_root(one.path().to_path_buf());
        scopes.get::<ProjectState>("two").set_root(two.path().to_path_buf());
    }
    (app, one, two)
}

#[project_scope_macros::project_scoped]
fn root_seen_by_a_command(
    project: Scoped<'_, ProjectState>,
    app: tauri::AppHandle<tauri::test::MockRuntime>,
) -> Option<std::path::PathBuf> {
    let _ = &project;
    app.open_project_state().and_then(|state| state.root())
}

#[project_scope_macros::project_scoped]
async fn root_seen_by_an_async_command(
    project: Scoped<'_, ProjectState>,
    app: tauri::AppHandle<tauri::test::MockRuntime>,
) -> Option<std::path::PathBuf> {
    let _ = &project;
    tokio::task::yield_now().await;
    app.open_project_state().and_then(|state| state.root())
}

#[test]
fn a_command_of_each_project_sees_that_project_only() {
    let (app, one, two) = app_with_two_projects();
    let handle = app.handle().clone();
    let scopes = app.state::<ProjectScopes>();
    let first = Scoped::Owned(scopes.get::<ProjectState>("one"), "one".to_string());
    let second = Scoped::Owned(scopes.get::<ProjectState>("two"), "two".to_string());
    assert_eq!(root_seen_by_a_command(first, handle.clone()).as_deref(), Some(one.path()));
    assert_eq!(root_seen_by_a_command(second, handle).as_deref(), Some(two.path()));
}

#[test]
fn an_async_command_of_each_project_sees_that_project_only() {
    let (app, one, two) = app_with_two_projects();
    let handle = app.handle().clone();
    let scopes = app.state::<ProjectScopes>();
    let first = Scoped::Owned(scopes.get::<ProjectState>("one"), "one".to_string());
    let second = Scoped::Owned(scopes.get::<ProjectState>("two"), "two".to_string());
    let (a, b) = tauri::async_runtime::block_on(async {
        tokio::join!(
            root_seen_by_an_async_command(first, handle.clone()),
            root_seen_by_an_async_command(second, handle.clone())
        )
    });
    assert_eq!(a.as_deref(), Some(one.path()));
    assert_eq!(b.as_deref(), Some(two.path()));
}

#[test]
fn code_with_no_context_gets_no_project_while_two_are_open() {
    let (app, _one, _two) = app_with_two_projects();
    assert!(app.handle().open_project_state().is_none());
}

#[test]
fn a_store_of_one_project_does_not_show_in_the_other() {
    let (app, _one, _two) = app_with_two_projects();
    let handle = app.handle();
    let in_one =
        context::run_in("one", || handle.project_store::<TurnRegistry>().shared().unwrap());
    let in_two =
        context::run_in("two", || handle.project_store::<TurnRegistry>().shared().unwrap());
    let again_one =
        context::run_in("one", || handle.project_store::<TurnRegistry>().shared().unwrap());
    assert!(!std::sync::Arc::ptr_eq(&in_one, &in_two));
    assert!(std::sync::Arc::ptr_eq(&in_one, &again_one));
}

#[test]
fn every_project_store_is_separate_per_project() {
    let (app, _one, _two) = app_with_two_projects();
    let handle = app.handle();
    let watcher_one =
        context::run_in("one", || handle.project_store::<ProjectWatcher>().shared().unwrap());
    let watcher_two =
        context::run_in("two", || handle.project_store::<ProjectWatcher>().shared().unwrap());
    assert!(!std::sync::Arc::ptr_eq(&watcher_one, &watcher_two));
    let index_one =
        context::run_in("one", || handle.project_store::<Bm25Indexer>().shared().unwrap());
    let index_two =
        context::run_in("two", || handle.project_store::<Bm25Indexer>().shared().unwrap());
    assert!(!std::sync::Arc::ptr_eq(&index_one, &index_two));
}

#[test]
fn a_thread_started_in_a_project_reads_the_stores_of_that_project() {
    let (app, _one, _two) = app_with_two_projects();
    let handle = app.handle().clone();
    let expected =
        context::run_in("two", || handle.project_store::<TurnRegistry>().shared().unwrap());
    let from_thread = context::run_in("two", || {
        let handle = handle.clone();
        context::thread_spawn(move || handle.project_store::<TurnRegistry>().shared().unwrap())
            .join()
            .unwrap()
    });
    assert!(std::sync::Arc::ptr_eq(&expected, &from_thread));
}

#[test]
fn code_with_no_context_and_one_open_project_gets_that_project() {
    let app = mock_app();
    app.manage(ProjectScopes::default());
    let dir = tempfile::tempdir().unwrap();
    app.state::<ProjectScopes>().get::<ProjectState>("only").set_root(dir.path().to_path_buf());
    let handle = app.handle();
    let sole = handle.project_store::<TurnRegistry>().shared().unwrap();
    let named =
        context::run_in("only", || handle.project_store::<TurnRegistry>().shared().unwrap());
    assert!(std::sync::Arc::ptr_eq(&sole, &named));
}

#[test]
fn no_context_and_two_open_projects_use_the_default_scope() {
    let (app, _one, _two) = app_with_two_projects();
    let handle = app.handle();
    let none = handle.project_store::<TurnRegistry>().shared().unwrap();
    let default_scope = app.state::<ProjectScopes>().get::<TurnRegistry>(DEFAULT_SCOPE);
    assert!(std::sync::Arc::ptr_eq(&none, &default_scope));
}

fn close(app: &tauri::App<tauri::test::MockRuntime>, id: &str) {
    let scopes = app.state::<ProjectScopes>();
    crate::project::close_project(
        app.handle().clone(),
        Scoped::Owned(scopes.get(id), id.to_string()),
        Scoped::Owned(scopes.get(id), id.to_string()),
        Scoped::Owned(scopes.get(id), id.to_string()),
        Scoped::Owned(scopes.get(id), id.to_string()),
        Scoped::Owned(scopes.get(id), id.to_string()),
        Scoped::Owned(scopes.get(id), id.to_string()),
        Scoped::Owned(scopes.get(id), id.to_string()),
        Scoped::Owned(scopes.get(id), id.to_string()),
    );
}

// LGC-FR-15: the diagnostic buffer serves every open project. Closing one of
// two projects keeps the records of the other, and the close of the last
// project clears it.
#[test]
fn closing_one_of_two_projects_keeps_the_log_of_the_other() {
    use crate::logging::BUFFER;
    let (app, _one, _two) = app_with_two_projects();
    let before = BUFFER.state().generation;

    context::run_in("one", || close(&app, "one"));
    assert_eq!(BUFFER.state().generation, before, "the project two is still open");

    context::run_in("two", || close(&app, "two"));
    assert_eq!(BUFFER.state().generation, before + 1, "the last close clears the buffer");
}

mod events_to_windows {
    use super::*;
    use crate::progress::{self, ProgressRegistry, OPERATION_PROGRESS};
    use crate::project_windows::{ProjectWindowEntry, ProjectWindows};
    use std::sync::{Arc, Mutex};
    use tauri::{Listener, WebviewUrl, WebviewWindowBuilder};

    fn entry(id: &str, label: &str) -> ProjectWindowEntry {
        ProjectWindowEntry {
            project_id: id.to_string(),
            name: id.to_string(),
            window_label: label.to_string(),
            needs_attention: false,
        }
    }

    type Seen = Arc<Mutex<Vec<serde_json::Value>>>;

    fn listen_in(app: &tauri::App<tauri::test::MockRuntime>, label: &str) -> Seen {
        listen_for(app, label, OPERATION_PROGRESS)
    }

    fn listen_for(app: &tauri::App<tauri::test::MockRuntime>, label: &str, event: &str) -> Seen {
        let window = WebviewWindowBuilder::new(app, label, WebviewUrl::default()).build().unwrap();
        let seen: Seen = Arc::default();
        let sink = seen.clone();
        window.listen(event, move |event| {
            let payload = serde_json::from_str(event.payload()).unwrap_or_default();
            sink.lock().unwrap().push(payload);
        });
        seen
    }

    #[test]
    fn the_progress_of_a_project_reaches_the_window_of_that_project_only() {
        let (app, _one, _two) = app_with_two_projects();
        let windows = ProjectWindows::default();
        windows.lock().open(entry("one", "main"));
        windows.lock().open(entry("two", "project-1"));
        app.manage(windows);
        let in_one = listen_in(&app, "main");
        let in_two = listen_in(&app, "project-1");
        let handle = app.handle().clone();

        for id in ["one", "two"] {
            context::run_in(id, || {
                let registry = handle.project_store::<ProgressRegistry>();
                progress::register_and_publish(&handle, &registry, "scan", id, None, None);
            });
        }

        let one = in_one.lock().unwrap();
        let two = in_two.lock().unwrap();
        assert_eq!(one.len(), 1, "the window of the project one gets one event");
        assert_eq!(two.len(), 1, "the window of the project two gets one event");
        assert_eq!(one[0]["projectId"], "one");
        assert_eq!(one[0]["label"], "one");
        assert_eq!(two[0]["projectId"], "two");
        assert_eq!(two[0]["label"], "two");
    }

    #[test]
    fn a_statistics_append_queued_from_a_project_reaches_the_window_of_that_project_only() {
        use crate::change_statistics::{record_change_statistics_event, CHANGE_STATISTICS_CHANGED};
        use crate::statistics::{wait_for_writer, EventBody};

        let (app, _one, _two) = app_with_two_projects();
        let windows = ProjectWindows::default();
        windows.lock().open(entry("one", "main"));
        windows.lock().open(entry("two", "project-1"));
        app.manage(windows);
        let in_one = listen_for(&app, "main", CHANGE_STATISTICS_CHANGED);
        let in_two = listen_for(&app, "project-1", CHANGE_STATISTICS_CHANGED);
        let handle = app.handle().clone();

        context::run_in("two", || {
            record_change_statistics_event(
                &handle,
                "stream-1",
                "change-1",
                EventBody::EditingInterval {
                    interval_id: "i1".to_string(),
                    started_at: "2026-03-14T12:00:00.000Z".to_string(),
                    ended_at: "2026-03-14T12:01:00.000Z".to_string(),
                },
            );
        });
        wait_for_writer();

        assert!(in_one.lock().unwrap().is_empty(), "the window of the project one gets nothing");
        let two = in_two.lock().unwrap();
        assert_eq!(two.len(), 1, "the window of the project two gets one event");
        assert_eq!(two[0]["projectId"], "two");
        assert_eq!(two[0]["streamId"], "stream-1");
    }
}
