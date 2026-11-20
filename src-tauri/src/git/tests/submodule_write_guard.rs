//! The main branch write check on submodules (GSM-FR-GHFY, GSM-FR-KGOP,
//! GTC-FR-OVSD, GTC-FR-ZBAL).
//!
//! The main repository is on `feature`, which is not a main branch, and the
//! submodule is on `main`, its own default branch. The guard client answers
//! every GitHub request as unreachable, so no network is reached.

use tauri::Manager;

use crate::changes::repo_set::tests::fixture::{commit_all, write, Superproject};
use crate::git::write_guard::commands::{
    confirm_main_branch_write, get_branch_write_guard, WriteGuardState,
};
use crate::git::ERR_MAIN_BRANCH_WRITE_UNCONFIRMED;
use crate::project::ProjectState;

fn head_of(repo: &git2::Repository) -> git2::Oid {
    repo.head().unwrap().peel_to_commit().unwrap().id()
}

/// A superproject on `feature` with a submodule on `main`, and an application
/// that asks for the check.
fn guarded() -> (Superproject, git2::Repository, tauri::App<tauri::test::MockRuntime>) {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    let main = p.main();
    let head = main.head().unwrap().peel_to_commit().unwrap();
    main.branch("feature", &head, false).unwrap();
    main.set_head("refs/heads/feature").unwrap();
    let root = crate::changes::canonicalize_lenient(&p.root());
    let app = crate::tools::tests::mock_app();
    app.manage(ProjectState::default());
    app.manage(WriteGuardState::offline());
    app.manage(crate::global_settings::GlobalSettingsStore::in_memory());
    app.manage(crate::github_tokens::GithubTokens::default());
    app.manage(crate::artifacts::ContentTracker::default());
    let state = app.state::<ProjectState>();
    state.set_root(root.clone());
    state.set_store(root);
    (p, sub, app)
}

fn commit(
    app: &tauri::App<tauri::test::MockRuntime>,
    path: &str,
) -> Result<crate::git::CommitOutcome, String> {
    tauri::async_runtime::block_on(crate::git::commit_paths(
        "work".to_string(),
        vec![path.to_string()],
        None,
        app.handle().clone(),
        Default::default(),
    ))
}

// GSM-FR-GHFY, GSM-FR-KGOP, GTC-FR-OVSD: a commit that writes the default
// branch of a submodule is refused before any repository is written, and the
// refusal names the submodule.
#[test]
fn a_commit_on_a_submodule_main_branch_is_refused_and_names_it() {
    let (p, sub, app) = guarded();
    write(&sub, "a.md", "a\nmore\n");
    let heads = (head_of(&sub), head_of(&p.main()));

    let refused = commit(&app, "libs/sub/a.md").unwrap_err();
    let json = refused
        .strip_prefix(&format!("{ERR_MAIN_BRANCH_WRITE_UNCONFIRMED}: "))
        .unwrap_or_else(|| panic!("{refused}"));
    let guard: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(guard["repository"], "libs/sub");
    assert_eq!(guard["branch"], "main");
    assert_eq!(guard["isMain"], true);
    assert_eq!((head_of(&sub), head_of(&p.main())), heads, "nothing is written");
}

// GTC-FR-ZBAL, GSM-FR-KGOP: the guard of a submodule is read and confirmed by
// its repository, and after the confirmation the commit goes ahead.
#[test]
fn a_confirmed_submodule_main_branch_takes_the_commit() {
    let (p, sub, app) = guarded();
    write(&sub, "a.md", "a\nmore\n");
    let handle = app.handle().clone();
    let read = tauri::async_runtime::block_on(get_branch_write_guard(
        None,
        false,
        Some("libs/sub".into()),
        handle.clone(),
        Default::default(),
    ))
    .unwrap();
    assert_eq!(read.branch, "main");
    assert!(read.is_main && !read.confirmed);
    assert_eq!(read.repository.as_deref(), Some("libs/sub"));
    // The confirmation of the main repository's `main` is not the submodule's.
    let main_guard = tauri::async_runtime::block_on(get_branch_write_guard(
        Some("main".into()),
        false,
        None,
        handle.clone(),
        Default::default(),
    ))
    .unwrap();
    assert_eq!(main_guard.repository, None);

    let confirmed = tauri::async_runtime::block_on(confirm_main_branch_write(
        "main".into(),
        Some("libs/sub".into()),
        handle,
        Default::default(),
    ))
    .unwrap();
    assert!(confirmed.confirmed);
    assert_eq!(confirmed.repository.as_deref(), Some("libs/sub"));

    let outcome = commit(&app, "libs/sub/a.md").unwrap();
    assert_eq!(outcome.repositories.len(), 2);
    assert_eq!(head_of(&sub).to_string(), outcome.repositories[0].commit_id);
    drop(p);
}

// GTC-FR-ZBAL: a repository that is not a member is refused.
#[test]
fn an_unknown_repository_is_refused() {
    let (_p, _sub, app) = guarded();
    let err = tauri::async_runtime::block_on(get_branch_write_guard(
        None,
        false,
        Some("not/a/sub".into()),
        app.handle().clone(),
        Default::default(),
    ))
    .unwrap_err();
    assert_eq!(err, "unknown repository");
}

// GSM-FR-GHFY, GTC-FR-OVSD: a push that would write the default branch of a
// submodule is refused before anything is pushed.
#[test]
fn a_push_to_a_submodule_main_branch_is_refused_before_any_push() {
    let (p, sub, app) = guarded();
    let sub_remote = p.bare_remote(&sub);
    write(&sub, "a.md", "a\nmore\n");
    commit_all(&sub, "sub work");
    let before =
        git2::Repository::open_bare(&sub_remote).unwrap().refname_to_id("refs/heads/main").unwrap();

    // The command runs exactly these two calls before `push_affected_at`
    // (`write_guard_wiring.rs`); its own runtime is not the test runtime.
    let targets = crate::git::push_targets(&p.root()).unwrap();
    let repos: Vec<&str> = targets.iter().map(|(r, _)| r.as_str()).collect();
    assert_eq!(repos, ["libs/sub"], "the main repository has no remote, so it is skipped");
    let refused = crate::git::write_guard::commands::require_repositories_write(
        app.handle(),
        &targets,
        "push_current_branch",
    )
    .unwrap_err();
    assert!(refused.starts_with(ERR_MAIN_BRANCH_WRITE_UNCONFIRMED), "{refused}");
    assert!(refused.contains("\"repository\":\"libs/sub\""), "{refused}");
    let after =
        git2::Repository::open_bare(&sub_remote).unwrap().refname_to_id("refs/heads/main").unwrap();
    assert_eq!(before, after, "nothing is pushed");
}

fn confirm(
    app: &tauri::App<tauri::test::MockRuntime>,
    repository: Option<&str>,
) -> Result<crate::git::BranchWriteGuard, String> {
    tauri::async_runtime::block_on(confirm_main_branch_write(
        "main".into(),
        repository.map(str::to_string),
        app.handle().clone(),
        Default::default(),
    ))
}

fn guard_of(
    app: &tauri::App<tauri::test::MockRuntime>,
    repository: Option<&str>,
) -> crate::git::BranchWriteGuard {
    tauri::async_runtime::block_on(get_branch_write_guard(
        Some("main".into()),
        false,
        repository.map(str::to_string),
        app.handle().clone(),
        Default::default(),
    ))
    .unwrap()
}

// GSM-FR-KGOP, GTC-FR-YWRZ: a confirmation holds for one repository and one
// branch: the main repository's `main` and a submodule's `main` are apart.
#[test]
fn a_confirmation_holds_for_one_repository() {
    let (_p, sub, app) = guarded();
    confirm(&app, None).unwrap();
    assert!(guard_of(&app, None).confirmed);
    assert!(
        !guard_of(&app, Some("libs/sub")).confirmed,
        "the main confirmation is not the submodule's"
    );
    write(&sub, "a.md", "a\nmore\n");
    assert!(commit(&app, "libs/sub/a.md")
        .unwrap_err()
        .starts_with(ERR_MAIN_BRANCH_WRITE_UNCONFIRMED));

    let (_p, _sub, app) = guarded();
    confirm(&app, Some("libs/sub")).unwrap();
    assert!(!guard_of(&app, None).confirmed, "the submodule confirmation is not the main one");
}

// GTC-FR-ZBAL: a confirmation for a repository that is not a member is
// refused; an empty repository names the main one.
#[test]
fn a_confirmation_names_a_member_or_the_main_repository() {
    let (_p, _sub, app) = guarded();
    assert_eq!(confirm(&app, Some("nope")).unwrap_err(), "unknown repository");
    let main = confirm(&app, Some("")).unwrap();
    assert_eq!(main.repository, None);
    assert!(guard_of(&app, None).confirmed);
}

// GSM-FR-GHFY: a commit in a nested submodule is refused at the middle
// repository that is on its main branch, and nothing moves anywhere.
#[test]
fn a_nested_commit_is_refused_at_the_middle_repository() {
    let (p, sub, app) = guarded();
    let inner = p.add_submodule("libs/sub", "inner", &[("i.md", "i\n")]);
    // The fixture records the nested pointer on `feature`, which the main
    // repository holds.
    assert_eq!(p.main().head().unwrap().shorthand().ok(), Some("feature"));
    inner.branch("work", &inner.head().unwrap().peel_to_commit().unwrap(), false).unwrap();
    inner.set_head("refs/heads/work").unwrap();
    write(&inner, "i.md", "i\n2\n");
    let heads = (head_of(&inner), head_of(&sub), head_of(&p.main()));
    let refused = commit(&app, "libs/sub/inner/i.md").unwrap_err();
    assert!(refused.contains("\"repository\":\"libs/sub\""), "{refused}");
    assert_eq!((head_of(&inner), head_of(&sub), head_of(&p.main())), heads);
}

// GSM-FR-GHFY: a commit of a pointer alone writes the parent only, so a
// submodule on its main branch does not refuse it.
#[test]
fn a_pointer_commit_does_not_check_the_submodule() {
    let (p, sub, app) = guarded();
    write(&sub, "a.md", "a\nmore\n");
    let moved = commit_all(&sub, "moved outside the app");
    let outcome = commit(&app, "libs/sub").unwrap();
    assert_eq!(outcome.repositories.len(), 1);
    assert_eq!(head_of(&sub), moved);
    drop(p);
}

// GTC-FR-OVSD: the tag of a refusal leaves other errors and refusals of the
// main repository as they are.
#[test]
fn a_refusal_names_a_submodule_only() {
    use crate::git::write_guard::{refusal, refusal_in_repository, BranchWriteGuard};
    let guard: BranchWriteGuard = serde_json::from_value(serde_json::json!({
        "branch": "main", "isDefault": true, "protection": "unknown", "isMain": true, "confirmed": false
    }))
    .unwrap();
    let refused = refusal(ERR_MAIN_BRANCH_WRITE_UNCONFIRMED, &guard);
    assert_eq!(refusal_in_repository(refused.clone(), ""), refused);
    assert_eq!(refusal_in_repository("other".into(), "libs/sub"), "other");
    let bad = format!("{ERR_MAIN_BRANCH_WRITE_UNCONFIRMED}: not json");
    assert_eq!(refusal_in_repository(bad.clone(), "libs/sub"), bad);
    assert!(refusal_in_repository(refused, "libs/sub").contains("\"repository\":\"libs/sub\""));
    assert_eq!(guard.in_repository("").repository, None);
}
