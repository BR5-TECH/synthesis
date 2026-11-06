//! A write command on an unconfirmed main branch is refused and writes nothing
//! (GTC-FR-EMLM, PST-FR-ZHVE, DRS-FR-TMZX).
//!
//! The repository has no remote, so its local `main` or `master` is the
//! default branch, and the guard client answers every GitHub request as
//! unreachable. No network is reached.

use tauri::Manager;

use crate::git::write_guard::commands::WriteGuardState;
use crate::git::ERR_MAIN_BRANCH_WRITE_UNCONFIRMED;
use crate::project::ProjectState;

/// A repository with one commit, and an application that asks for the check.
fn guarded_app() -> (tempfile::TempDir, tauri::App<tauri::test::MockRuntime>) {
    let dir = tempfile::TempDir::new().unwrap();
    let root = crate::changes::canonicalize_lenient(dir.path());
    let repo = git2::Repository::init(&root).unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    std::fs::write(root.join("README.md"), "seed\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("README.md")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let signature = repo.signature().unwrap();
    repo.commit(Some("HEAD"), &signature, &signature, "seed", &tree, &[]).unwrap();

    let app = crate::tools::tests::mock_app();
    app.manage(ProjectState::default());
    app.manage(WriteGuardState::offline());
    app.manage(crate::global_settings::GlobalSettingsStore::in_memory());
    app.manage(crate::github_tokens::GithubTokens::default());
    app.manage(crate::artifacts::ContentTracker::default());
    let state = app.state::<ProjectState>();
    state.set_root(root.clone());
    state.set_store(root);
    (dir, app)
}

fn head_of(dir: &tempfile::TempDir) -> git2::Oid {
    let repo = git2::Repository::open(dir.path()).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap().id();
    head
}

fn assert_unconfirmed(result: Result<impl std::fmt::Debug, String>) {
    let refused = result.unwrap_err();
    assert!(refused.starts_with(ERR_MAIN_BRANCH_WRITE_UNCONFIRMED), "{refused}");
}

// GTC-FR-EMLM: a commit on an unconfirmed main branch is refused, and `HEAD`
// and the index stay as they were.
#[test]
fn a_commit_on_an_unconfirmed_main_branch_moves_no_head() {
    let (dir, app) = guarded_app();
    std::fs::write(dir.path().join("README.md"), "changed\n").unwrap();
    let before = head_of(&dir);

    assert_unconfirmed(tauri::async_runtime::block_on(crate::git::commit_paths(
        "Change the readme".to_string(),
        vec!["README.md".to_string()],
        None,
        app.handle().clone(),
    )));

    assert_eq!(head_of(&dir), before, "the refused commit moved HEAD");
    let repo = git2::Repository::open(dir.path()).unwrap();
    let status = repo.status_file(std::path::Path::new("README.md")).unwrap();
    assert!(status.contains(git2::Status::WT_MODIFIED), "the change is not staged");
}

// PST-FR-ZHVE: a save on an unconfirmed main branch is refused, and the file
// keeps its bytes.
#[test]
fn a_save_on_an_unconfirmed_main_branch_keeps_the_bytes() {
    let (dir, app) = guarded_app();
    std::fs::write(dir.path().join("spec.md"), "# Spec\n").unwrap();

    assert_unconfirmed(tauri::async_runtime::block_on(crate::artifacts::save_artifact_contents(
        app.handle().clone(),
        "spec.md".to_string(),
        "# Changed\n".to_string(),
    )));

    assert_eq!(std::fs::read_to_string(dir.path().join("spec.md")).unwrap(), "# Spec\n");
}

// DRS-FR-TMZX: a draft deletion on an unconfirmed main branch is refused, and
// the draft stays on disk.
#[test]
fn a_draft_deletion_on_an_unconfirmed_main_branch_keeps_the_draft() {
    let (dir, app) = guarded_app();
    let root = app.state::<ProjectState>().require_root().unwrap();
    let created = crate::drafts::create_draft_impl(&root, Some("spec"), None).unwrap();
    let draft = dir.path().join(crate::storage_floor::DRAFTS_REL).join(&created.draft.id);
    assert!(draft.is_dir());

    assert_unconfirmed(tauri::async_runtime::block_on(crate::drafts::delete_draft(
        created.draft.id.clone(),
        app.handle().clone(),
    )));

    assert!(draft.is_dir(), "the refused deletion removed the draft");
}
