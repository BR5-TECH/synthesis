//! A command that has no `Scoped<ProjectState>` parameter acts on the project
//! of its call only (PWN-FR-CTXT, PWN-FR-ISOL).

use super::*;
use crate::project_scope::{ProjectKey, ProjectScopes};

fn app_with_two_services() -> tauri::App<tauri::test::MockRuntime> {
    let app = tauri::test::mock_app();
    app.manage(ProjectScopes::default());
    for id in ["one", "two"] {
        let (factory, _log) = FakeFactory::new();
        let service = EmbeddingsService::new(Box::new(factory), false);
        service.apply_settings(EmbeddingsRecord { enabled: true, model: None });
        service.mount(MountInput {
            key: id.to_string(),
            git: false,
            root: std::path::PathBuf::from(id),
            store_dir: None,
            mode: ProjectEmbeddingsMode::On,
            generation: 0,
        });
        app.state::<ProjectScopes>().set(id, service);
    }
    app
}

fn requested(app: &tauri::App<tauri::test::MockRuntime>, id: &str) -> Option<ReindexScope> {
    let service = app.state::<ProjectScopes>().get::<EmbeddingsService>(id);
    let st = service.lock();
    st.project.as_ref().and_then(|project| project.pending.reindex)
}

#[test]
fn start_reindex_acts_on_the_project_of_its_call() {
    let app = app_with_two_services();
    let handle = app.handle().clone();
    start_reindex(handle.clone(), ReindexScope::WholeTrace, ProjectKey("two".to_string()))
        .expect("the project two accepts the request");
    assert_eq!(requested(&app, "one"), None, "the project one asked for nothing");
    assert_eq!(requested(&app, "two"), Some(ReindexScope::WholeTrace));

    start_reindex(handle, ReindexScope::CurrentBranch, ProjectKey("one".to_string()))
        .expect("the project one accepts the request");
    assert_eq!(requested(&app, "one"), Some(ReindexScope::CurrentBranch));
    assert_eq!(requested(&app, "two"), Some(ReindexScope::WholeTrace));
}

fn scope_app() -> (tauri::App<tauri::test::MockRuntime>, tempfile::TempDir) {
    let app = tauri::test::mock_app();
    app.manage(ProjectScopes::default());
    let settings_dir = tempfile::tempdir().unwrap();
    app.manage(crate::global_settings::GlobalSettingsStore::with_path(
        settings_dir.path().join("synthesis.toml"),
    ));
    for id in ["one", "two"] {
        let root = tempfile::tempdir().unwrap();
        let scopes = app.state::<ProjectScopes>();
        scopes.get::<crate::project::ProjectState>(id).set_root(root.keep());
        let (factory, _log) = FakeFactory::new();
        scopes.set(id, EmbeddingsService::new(Box::new(factory), false));
    }
    (app, settings_dir)
}

// EMB-FR-VXIS: the start of the application reaches the default scope alone.
// A project scope follows the Global switch when its content root mounts.
#[test]
fn a_mounted_project_follows_the_global_switch() {
    let (app, _settings) = scope_app();
    app.state::<crate::global_settings::GlobalSettingsStore>()
        .save_embeddings_config(&EmbeddingsRecord { enabled: true, model: None })
        .unwrap();
    let handle = app.handle().clone();

    crate::project_scope::context::run_in("two", || {
        crate::embeddings::on_content_root_mounted(&handle)
    });

    let scopes = app.state::<ProjectScopes>();
    assert!(scopes.get::<EmbeddingsService>("two").status().enabled);
    assert!(
        !scopes.get::<EmbeddingsService>("one").status().enabled,
        "no other project is touched"
    );
}

// EMB-FR-JFOV: a Global change reaches every open project, not only the project
// of the calling window.
#[test]
fn saving_the_global_settings_reaches_every_open_project() {
    let (app, _settings) = scope_app();
    let handle = app.handle().clone();
    let model = crate::embeddings::list_models().remove(0).id;

    crate::project_scope::context::run_in("one", || {
        crate::embeddings::save_settings(
            &handle,
            crate::embeddings::EmbeddingsSettings { enabled: true, model: model.clone() },
        )
        .unwrap();
    });

    let scopes = app.state::<ProjectScopes>();
    for id in ["one", "two"] {
        assert!(scopes.get::<EmbeddingsService>(id).status().enabled, "{id} follows the switch");
    }
}

fn root_check_pending(app: &tauri::App<tauri::test::MockRuntime>, id: &str) -> bool {
    let service = app.state::<ProjectScopes>().get::<EmbeddingsService>(id);
    let st = service.lock();
    st.project.as_ref().is_some_and(|project| project.pending.root_check)
}

// EMB-FR-VQSK, PWN-FR-CTXT: the focus of the window of one project requests a
// root check on the service of that project only.
#[test]
fn the_focus_of_a_project_window_requests_a_root_check_of_its_project_only() {
    use crate::project_windows::{ProjectWindowEntry, ProjectWindows};
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    let app = app_with_two_services();
    let windows = ProjectWindows::default();
    for (id, label) in [("one", "main"), ("two", "project-1")] {
        windows.lock().open(ProjectWindowEntry {
            project_id: id.to_string(),
            name: id.to_string(),
            window_label: label.to_string(),
            needs_attention: false,
        });
        WebviewWindowBuilder::new(&app, label, WebviewUrl::default()).build().unwrap();
    }
    app.manage(windows);
    assert!(!root_check_pending(&app, "one") && !root_check_pending(&app, "two"));

    let window = app.get_webview_window("project-1").unwrap();
    let window = window.as_ref().window();
    crate::menu::handle_window_event(&window, &tauri::WindowEvent::Focused(true));

    assert!(!root_check_pending(&app, "one"), "the project one asked for no check");
    assert!(root_check_pending(&app, "two"), "the project two asked for a check");
}
