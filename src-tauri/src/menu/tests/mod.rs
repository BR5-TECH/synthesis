//! The test scenarios of the native application menu
//! (`specifications/ui/SNV-shell-navigation.md`).
//!
//! The headless app builders that both topic files use live here.

use super::*;

/// A headless app with the managed state the File-menu paths touch.
///
/// `mock_app` starts with no windows, which is precisely the state
/// `begin_exit` must refuse to hold in — so any test about a *held* quit has
/// to open one first (`mock_app_with_window`).
#[cfg(test)]
fn mock_app_with_project(root: &std::path::Path) -> tauri::App<tauri::test::MockRuntime> {
    use tauri::Manager;
    let app = tauri::test::mock_app();
    app.manage(ProjectState::default());
    app.manage(ProjectWatcher::default());
    app.manage(ContentTracker::default());
    app.manage(crate::draft_watcher::DraftsWatcher::default());
    app.manage(ExitGate::default());
    // PRG-FR-13: the teardown terminates operations scoped to the outgoing
    // content root, so the registry has to be managed like every other store.
    app.manage(crate::progress::ProgressRegistry::default());
    // ASC-FR-17 / SCC-FR-12: the close tears the candidate list down and
    // stops any running search, so both stores have to be managed too.
    app.manage(crate::scanning::CandidateStore::default());
    // ASC-FR-14: the close tears the attribution baseline down with the rest
    // of the content root's in-memory state.
    app.manage(crate::scanning::AttributionBaseline::default());
    app.manage(crate::search::SearchRegistry::default());
    // AGC-FR-20: the close cancels every turn in flight against the project
    // being closed, so the turn registry is managed like every other store.
    app.manage(crate::agent_conversations::TurnRegistry::default());
    app.state::<ProjectState>().set_root(root.to_path_buf());
    app
}

/// The same app with a webview window open — i.e. a frontend that can be
/// asked to flush and that can answer with `finish_exit`.
#[cfg(test)]
fn mock_app_with_window(root: &std::path::Path) -> tauri::App<tauri::test::MockRuntime> {
    let app = mock_app_with_project(root);
    tauri::WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
        .build()
        .unwrap();
    app
}


mod items;
mod teardown;
