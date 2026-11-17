//! Two open projects never share a filesystem instance (FSA-FR-TSHC,
//! FSA-FR-ZZNJ, PWN-FR-ISOL).
//!
//! Project "a" mounts first and project "b" mounts after it, as two project
//! tabs do. Each test then works in the context of one project, or in no
//! context, and checks which worktree and which Documents grants it reaches.

use std::path::{Path, PathBuf};

use rig::tool::PortableTool;
use tauri::Manager;
use tempfile::TempDir;

use super::{ide_instance, project_fs_state, FsAccessState, FsError, RootFs};
use crate::bm25_index::Bm25Indexer;
use crate::documents::{DocumentsCollection, SourceKind, StoredSource};
use crate::project::ProjectState;
use crate::project_scope::{context, ProjectScopes};
use crate::tools::tests::block_on;
use crate::tools::ToolRefusal;

type MockApp = tauri::App<tauri::test::MockRuntime>;

/// One project: its worktree, and a Documents folder outside the worktree.
struct Project {
    _worktree: TempDir,
    _docs: TempDir,
    root: PathBuf,
    docs: PathBuf,
}

impl Project {
    fn new(id: &str) -> Project {
        let worktree = TempDir::new().unwrap();
        let docs = TempDir::new().unwrap();
        let root = std::fs::canonicalize(worktree.path()).unwrap();
        let docs_root = std::fs::canonicalize(docs.path()).unwrap();
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join("docs/HLD.md"), format!("hld of {id}")).unwrap();
        std::fs::write(docs_root.join("ref.md"), format!("reference of {id}")).unwrap();
        Project { _worktree: worktree, _docs: docs, root, docs: docs_root }
    }

    fn hld(&self) -> PathBuf {
        self.root.join("docs/HLD.md")
    }

    fn reference(&self) -> PathBuf {
        self.docs.join("ref.md")
    }
}

/// Mount `project` in the scope `id`, as a project tab does at its open: the
/// IDE instance, the indexes, and the project root.
fn mount(app: &MockApp, id: &str, project: &Project) {
    let scopes = app.state::<ProjectScopes>();
    let fs = scopes.get::<FsAccessState>(id);
    fs.install_for_worktree(&project.root).unwrap();
    let access = fs.get().unwrap();
    scopes.get::<ProjectState>(id).set_root_with_access(project.root.clone(), Some(access.clone()));
    scopes.get::<Bm25Indexer>(id).mount(&RootFs::new(project.root.clone(), access));
}

/// Open the Documents collection of the scope `id` with the documents folder of
/// `project` as its one source, and refresh it.
fn select_documents(app: &MockApp, id: &str, project: &Project) {
    let scopes = app.state::<ProjectScopes>();
    let collection = scopes.get::<DocumentsCollection>(id);
    collection.open(id, Ok(Vec::new()));
    let source = StoredSource {
        kind: SourceKind::Folder,
        path: project.docs.to_string_lossy().into_owned(),
    };
    collection.add_sources(&[source], &|_, _| Ok(())).unwrap();
    collection.refresh(&scopes.get::<FsAccessState>(id), &app.handle().clone(), &|_| {});
}

/// Two project tabs: "a" mounted first, "b" mounted after it.
fn two_projects() -> (MockApp, Project, Project) {
    let app = tauri::test::mock_app();
    app.manage(ProjectScopes::default());
    let a = Project::new("a");
    let b = Project::new("b");
    mount(&app, "a", &a);
    mount(&app, "b", &b);
    (app, a, b)
}

fn document_id(path: &Path) -> String {
    crate::documents::document_id(&crate::documents::normalise_path(&path.to_string_lossy()))
}

fn read_file(app: &MockApp, session: &str, path: &str) -> Result<String, ToolRefusal> {
    let tool = crate::tools::file_read::FileReadTool::new(app.handle().clone(), session);
    block_on(tool.call(crate::tools::file_read::ReadFileArgs {
        path: path.to_string(),
        offset: None,
        limit: None,
    }))
}

fn get_document(app: &MockApp, id: &str) -> Result<String, ToolRefusal> {
    let tool = crate::tools::document_get::GetDocumentTool::new(app.handle().clone());
    block_on(tool.call(crate::tools::document_get::GetDocumentArgs {
        id: id.to_string(),
        byte_offset: None,
        byte_length: None,
        line_offset: None,
        line_limit: None,
    }))
}

// FSA-FR-29, FSA-FR-ZZNJ, CVL-FR-24: an agent session of the project that
// mounted first is rooted at its own worktree, not at the worktree of the
// project that mounted last.
#[test]
fn an_agent_session_is_rooted_at_the_worktree_of_its_project() {
    let (app, a, b) = two_projects();
    context::run_in("a", || {
        let state = project_fs_state(app.handle()).unwrap();
        let session = state.open_agent_session("turn-a").unwrap();
        assert_eq!(session.read_text(a.hld()).unwrap(), "hld of a");
        assert!(matches!(session.read_text(b.hld()), Err(FsError::EscapesAllowedRoots { .. })));
    });
}

// FSA-FR-TSHC, RFT regression: `read_file` of a project path reads the file of
// the project of the turn while another project is open.
#[test]
fn read_file_reads_the_project_of_the_turn() {
    let (app, _a, _b) = two_projects();
    context::run_in("a", || {
        project_fs_state(app.handle()).unwrap().open_agent_session("turn-a").unwrap();
        assert_eq!(read_file(&app, "turn-a", "docs/HLD.md").unwrap(), "hld of a");
    });
    context::run_in("b", || {
        project_fs_state(app.handle()).unwrap().open_agent_session("turn-b").unwrap();
        assert_eq!(read_file(&app, "turn-b", "docs/HLD.md").unwrap(), "hld of b");
        // The session of project "a" is not a session of project "b".
        assert_eq!(read_file(&app, "turn-a", "docs/HLD.md"), Err(ToolRefusal::NoProjectOpen));
    });
}

// GDT regression, FSA-FR-TSHC: `get_document` reads a document of the project
// of the turn after another project refreshed its own Documents collection.
#[test]
fn get_document_reads_a_document_after_another_project_refreshed() {
    let (app, a, b) = two_projects();
    select_documents(&app, "a", &a);
    select_documents(&app, "b", &b);
    let id_a = document_id(&a.reference());
    let id_b = document_id(&b.reference());
    context::run_in("a", || {
        assert_eq!(get_document(&app, &id_a).unwrap(), "reference of a");
        assert_eq!(get_document(&app, &id_b), Err(ToolRefusal::DocumentNotFound));
    });
    context::run_in("b", || {
        assert_eq!(get_document(&app, &id_b).unwrap(), "reference of b");
    });
}

// FSA-FR-21, FSA-FR-29, FSA-FR-TSHC: the close of one project discards its own
// instances and keeps every instance of the other project.
#[test]
fn a_close_discards_the_instances_of_that_project_only() {
    let (app, a, b) = two_projects();
    select_documents(&app, "a", &a);
    select_documents(&app, "b", &b);
    let scopes = app.state::<ProjectScopes>();
    scopes.get::<FsAccessState>("a").open_agent_session("turn-a").unwrap();
    scopes.get::<FsAccessState>("b").open_agent_session("turn-b").unwrap();

    scopes.get::<FsAccessState>("b").discard_project();

    let closed = scopes.get::<FsAccessState>("b");
    assert!(closed.get().is_none());
    assert!(closed.documents().is_none());
    assert_eq!(closed.agent_session_count(), 0);
    assert!(matches!(closed.open_agent_session("late"), Err(super::FsAccessBuildError::NoRoots)));
    let open = scopes.get::<FsAccessState>("a");
    assert_eq!(open.get().unwrap().read_text(a.hld()).unwrap(), "hld of a");
    assert_eq!(open.documents().unwrap().read_text(a.reference()).unwrap(), "reference of a");
    assert_eq!(open.agent_session("turn-a").unwrap().read_text(a.hld()).unwrap(), "hld of a");
}

// FSA-FR-ZZNJ, PWN-FR-CTXT: with two projects open, code with no context gets
// no project instance, and the tools refuse rather than read either project.
#[test]
fn code_with_no_context_reaches_no_project_while_two_are_open() {
    let (app, a, b) = two_projects();
    select_documents(&app, "a", &a);
    select_documents(&app, "b", &b);
    app.state::<ProjectScopes>().get::<FsAccessState>("a").open_agent_session("turn-a").unwrap();

    let state = project_fs_state(app.handle()).unwrap();
    assert!(state.get().is_none());
    assert!(state.documents().is_none());
    assert!(state.agent_session("turn-a").is_none());
    assert!(matches!(state.open_agent_session("stray"), Err(super::FsAccessBuildError::NoRoots)));
    assert_eq!(read_file(&app, "turn-a", "docs/HLD.md"), Err(ToolRefusal::NoProjectOpen));
    assert_eq!(get_document(&app, &document_id(&a.reference())), Err(ToolRefusal::NoProjectOpen));
}

// FSA-FR-ZZNJ, FSA-FR-21: with no context and two projects open, the IDE
// instance is the pre-project instance, which reaches no worktree of a project.
#[test]
fn the_ide_instance_with_no_context_is_the_pre_project_instance() {
    let (app, a, b) = two_projects();
    assert!(ide_instance(app.handle()).is_none(), "no pre-project instance yet");

    let application = TempDir::new().unwrap();
    let application = std::fs::canonicalize(application.path()).unwrap();
    std::fs::write(application.join("settings.toml"), "x = 1").unwrap();
    app.manage(FsAccessState::default());
    app.state::<FsAccessState>().install_for_worktree(&application).unwrap();

    let instance = ide_instance(app.handle()).unwrap();
    assert_eq!(instance.read_text(application.join("settings.toml")).unwrap(), "x = 1");
    assert!(matches!(instance.read_text(a.hld()), Err(FsError::EscapesAllowedRoots { .. })));
    assert!(matches!(instance.read_text(b.hld()), Err(FsError::EscapesAllowedRoots { .. })));
    context::run_in("a", || {
        let own = ide_instance(app.handle()).unwrap();
        assert_eq!(own.read_text(a.hld()).unwrap(), "hld of a");
    });
}

// FSA-FR-ZZNJ: with one project open, code with no context gets the state of
// that project.
#[test]
fn code_with_no_context_gets_the_one_open_project() {
    let app = tauri::test::mock_app();
    app.manage(ProjectScopes::default());
    let a = Project::new("a");
    mount(&app, "a", &a);
    let state = project_fs_state(app.handle()).unwrap();
    assert_eq!(state.get().unwrap().read_text(a.hld()).unwrap(), "hld of a");
    let session = state.open_agent_session("turn-a").unwrap();
    assert_eq!(session.read_text(a.hld()).unwrap(), "hld of a");
}

// FSA-FR-21, FSA-FR-ZZNJ: a project whose instance is not built yet gets the
// pre-project instance and no agent session, never the instance of the other
// project.
#[test]
fn a_project_not_yet_installed_reaches_no_other_project() {
    let app = tauri::test::mock_app();
    app.manage(ProjectScopes::default());
    let a = Project::new("a");
    let b = Project::new("b");
    mount(&app, "a", &a);
    app.state::<ProjectScopes>().get::<ProjectState>("b").set_root(b.root.clone());
    context::run_in("b", || {
        assert!(ide_instance(app.handle()).is_none());
        let state = project_fs_state(app.handle()).unwrap();
        assert!(matches!(state.open_agent_session("t"), Err(super::FsAccessBuildError::NoRoots)));
    });
}

// FSA-FR-21, FSA-FR-TSHC: a second close changes nothing, and a project opened
// again after its close gets a new instance while the other project keeps its
// own.
#[test]
fn a_project_closed_twice_and_opened_again_leaves_the_other_alone() {
    let (app, _a, b) = two_projects();
    let scopes = app.state::<ProjectScopes>();
    let instance_a = scopes.get::<FsAccessState>("a").get().unwrap();
    let old_b = scopes.get::<FsAccessState>("b").get().unwrap();
    scopes.get::<FsAccessState>("b").discard_project();
    scopes.get::<FsAccessState>("b").discard_project();
    scopes.get::<FsAccessState>("empty").discard_project();
    mount(&app, "b", &b);
    let new_b = scopes.get::<FsAccessState>("b").get().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&old_b, &new_b));
    assert_eq!(new_b.read_text(b.hld()).unwrap(), "hld of b");
    assert!(std::sync::Arc::ptr_eq(&instance_a, &scopes.get::<FsAccessState>("a").get().unwrap()));
}

// The fixtures of other modules manage one `FsAccessState` and no scopes. With
// no `ProjectScopes`, the state of the project is that managed value.
#[test]
fn with_no_scopes_the_managed_state_is_the_project_state() {
    let app = tauri::test::mock_app();
    app.manage(FsAccessState::default());
    let a = Project::new("a");
    app.state::<FsAccessState>().install_for_worktree(&a.root).unwrap();
    let state = project_fs_state(app.handle()).unwrap();
    assert_eq!(state.get().unwrap().read_text(a.hld()).unwrap(), "hld of a");
    assert_eq!(ide_instance(app.handle()).unwrap().read_text(a.hld()).unwrap(), "hld of a");
}

// DCL-FR-FTMO, FSA-FR-DMKC, FSA-FR-WBKZ: a refresh and a close of the Documents
// collection of one project, through the lifecycle of the application, keep
// the documents instance of the other project, so its agent still reads its
// documents.
#[test]
fn the_documents_lifecycle_of_one_project_keeps_the_instance_of_the_other() {
    let (app, a, b) = two_projects();
    select_documents(&app, "a", &a);
    let scopes = app.state::<ProjectScopes>();
    let before = scopes.get::<FsAccessState>("a").documents().unwrap();
    let collection_b = scopes.get::<DocumentsCollection>("b");
    collection_b.open("b", Ok(Vec::new()));
    let source =
        StoredSource { kind: SourceKind::Folder, path: b.docs.to_string_lossy().into_owned() };
    collection_b.add_sources(&[source], &|_, _| Ok(())).unwrap();
    // No index pass of "b" is wanted here: the test is about the instances.
    scopes.get::<Bm25Indexer>("b").clear();

    context::run_in("b", || crate::documents::lifecycle::run_refresh(app.handle()));
    let docs_b = scopes.get::<FsAccessState>("b").documents().unwrap();
    assert_eq!(docs_b.read_text(b.reference()).unwrap(), "reference of b");
    assert!(matches!(docs_b.read_text(a.reference()), Err(FsError::EscapesAllowedRoots { .. })));
    let after_refresh = scopes.get::<FsAccessState>("a").documents().unwrap();
    assert!(std::sync::Arc::ptr_eq(&before, &after_refresh));

    context::run_in("b", || crate::documents::lifecycle::on_project_closed(app.handle()));
    assert!(scopes.get::<FsAccessState>("b").documents().is_none());
    let after_close = scopes.get::<FsAccessState>("a").documents().unwrap();
    assert!(std::sync::Arc::ptr_eq(&before, &after_close));
    context::run_in("a", || {
        assert_eq!(get_document(&app, &document_id(&a.reference())).unwrap(), "reference of a");
    });
}
