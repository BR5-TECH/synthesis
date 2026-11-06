//! The main branch write guard (GTC-FR-UOUS, GTC-FR-VZJR, GTC-FR-TTSV,
//! GTC-FR-YWRZ, GTC-FR-JWOL, GTC-FR-LRNB).
//!
//! GitHub is the scripted fake of the pull request tests, so "no request is
//! made" is an assertion about what it was asked.

use super::pull_requests::{
    repo_with_origin, token_missing, token_ready, FakeGithub, PLATFORM, SECRET,
};
use super::*;
use crate::git::pull_requests::client::GithubFailure;
use crate::git::write_guard::{
    background_refusal, check_write, confirm_write, guard_for, WriteGuardSession,
    ERR_MAIN_BRANCH_PROTECTED, ERR_MAIN_BRANCH_WRITE_UNCONFIRMED,
};
use serde_json::json;

const REPO: &str = "/repos/acme/platform";

fn branch_path(branch: &str) -> String {
    format!("{REPO}/branches/{branch}")
}

/// A fake GitHub whose repository reports `default` as its default branch.
fn github_with_default(default: &str) -> FakeGithub {
    let github = FakeGithub::new();
    github.answer(REPO, Ok(json!({ "default_branch": default })));
    github
}

/// Run `work` with an access to the fixture's repository.
fn with_access<T>(
    f: &WorktreeFixture,
    github: &FakeGithub,
    tokens: &(GlobalSettingsStore, GithubTokens),
    work: impl FnOnce(&PullRequestAccess<'_>) -> T,
) -> T {
    let root = f.root();
    let access = PullRequestAccess {
        project: GithubAccess {
            root: &root,
            store: &tokens.0,
            tokens: &tokens.1,
            project_key: "/dev/acme",
        },
        client: github,
    };
    work(&access)
}

fn guard(
    f: &WorktreeFixture,
    github: &FakeGithub,
    tokens: &(GlobalSettingsStore, GithubTokens),
    session: &WriteGuardSession,
    branch: &str,
    refresh: bool,
) -> BranchWriteGuard {
    with_access(f, github, tokens, |access| {
        guard_for(&NullSink, &SCRATCH_BUFFER, session, access, branch, refresh)
    })
}

fn check(
    f: &WorktreeFixture,
    github: &FakeGithub,
    tokens: &(GlobalSettingsStore, GithubTokens),
    session: &WriteGuardSession,
    branch: Option<&str>,
    buffer: &'static LogBuffer,
) -> Result<(), String> {
    with_access(f, github, tokens, |access| {
        check_write(&NullSink, buffer, session, access, branch, "commit_paths")
    })
}

fn confirm(
    f: &WorktreeFixture,
    github: &FakeGithub,
    tokens: &(GlobalSettingsStore, GithubTokens),
    session: &WriteGuardSession,
    branch: &str,
) -> Result<BranchWriteGuard, String> {
    with_access(f, github, tokens, |access| {
        confirm_write(&NullSink, &SCRATCH_BUFFER, session, access, branch)
    })
}

/// The guard a refusal carries.
fn carried(error: &str) -> BranchWriteGuard {
    let (_, data) = error.split_once(": ").expect("the refusal carries data");
    serde_json::from_str(data).expect("the refusal carries a guard")
}

// GTC-FR-UOUS / GTC-FR-TTSV: the default branch is the one GitHub reports, and
// it is a main branch whatever the local branch names say.
#[test]
fn the_default_branch_is_the_one_github_reports() {
    let f = repo_with_origin(PLATFORM);
    f.branch("trunk");
    let github = github_with_default("trunk");
    github.answer(&branch_path("trunk"), Ok(json!({ "name": "trunk", "protected": false })));
    github.answer(&branch_path(&f.current()), Ok(json!({ "protected": false })));
    let tokens = token_ready();
    let session = WriteGuardSession::default();

    let trunk = guard(&f, &github, &tokens, &session, "trunk", false);
    assert!(trunk.is_default && trunk.is_main && !trunk.confirmed);
    assert_eq!(trunk.protection, "unprotected");

    let local = guard(&f, &github, &tokens, &session, &f.current(), false);
    assert!(!local.is_default, "a local main is not the default when GitHub names another");
    assert!(!local.is_main);
}

// GTC-FR-UOUS: with no GitHub read, the default branch is the primary remote's
// `HEAD`, then a local `main`, then a local `master`.
#[test]
fn the_default_branch_falls_back_to_the_remote_head_then_main_then_master() {
    let f = WorktreeFixture::new();
    let github = FakeGithub::new();
    let tokens = token_missing();
    let session = WriteGuardSession::default();
    let current = f.current();
    let first = guard(&f, &github, &tokens, &session, &current, false);
    assert!(first.is_default, "{current} is the local main or master");
    assert_eq!(first.protection, "not_applicable", "no remote is no GitHub remote");
    assert!(github.requests().is_empty(), "no GitHub remote, no request");

    f.remote_ref("origin/develop");
    f.repo()
        .reference_symbolic("refs/remotes/origin/HEAD", "refs/remotes/origin/develop", true, "t")
        .unwrap();
    f.branch("develop");
    let fresh = WriteGuardSession::default();
    assert!(guard(&f, &github, &tokens, &fresh, "develop", false).is_default);
    assert!(!guard(&f, &github, &tokens, &fresh, &current, false).is_default);
}

// GTC-FR-UOUS: when no candidate exists, no branch is the default branch.
#[test]
fn no_branch_is_the_default_when_no_candidate_exists() {
    let f = WorktreeFixture::new();
    let repo = f.repo();
    let mut current = repo.find_branch(&f.current(), BranchType::Local).unwrap();
    current.rename("trunk", false).unwrap();
    drop(current);
    repo.set_head("refs/heads/trunk").unwrap();
    let github = FakeGithub::new();
    let guard = guard(&f, &github, &token_missing(), &WriteGuardSession::default(), "trunk", false);
    assert!(!guard.is_default && !guard.is_main);
}

// GTC-FR-VZJR / GTC-FR-TTSV: a branch GitHub protects is a main branch; a
// branch GitHub does not hold is unprotected.
#[test]
fn protection_is_what_github_reports_and_a_missing_branch_is_unprotected() {
    let f = repo_with_origin(PLATFORM);
    let github = github_with_default("main");
    github.answer(&branch_path("release/1.0"), Ok(json!({ "protected": true })));
    github.answer(&branch_path("gone"), Err(GithubFailure::NotFound));
    let tokens = token_ready();
    let session = WriteGuardSession::default();

    let release = guard(&f, &github, &tokens, &session, "release/1.0", false);
    assert_eq!(release.protection, "protected");
    assert!(release.is_main && !release.is_default);

    let gone = guard(&f, &github, &tokens, &session, "gone", false);
    assert_eq!(gone.protection, "unprotected");
    assert!(!gone.is_main);
    assert!(github.requests().iter().all(|(secret, _)| secret == SECRET));
}

// GTC-FR-VZJR: a remote that is not on GitHub has no protection to read.
#[test]
fn a_remote_that_is_not_on_github_is_not_applicable() {
    let f = repo_with_origin("https://gitlab.com/acme/platform.git");
    let github = FakeGithub::new();
    let guard = guard(&f, &github, &token_ready(), &WriteGuardSession::default(), "x", false);
    assert_eq!(guard.protection, "not_applicable");
    assert!(github.requests().is_empty());
}

// GTC-FR-VZJR: a read that fails is `unknown`, with its typed cause.
#[test]
fn a_failed_read_is_unknown_with_its_cause() {
    let f = repo_with_origin(PLATFORM);
    let github = github_with_default("main");
    github.answer(&branch_path("feature"), Err(GithubFailure::Unreachable));
    let session = WriteGuardSession::default();
    let feature = guard(&f, &github, &token_ready(), &session, "feature", false);
    assert_eq!(feature.protection, "unknown");
    assert_eq!(feature.protection_error.as_deref(), Some("github_unreachable"));

    // No token: the repository is not read at all, and the cause is the token's.
    let other = repo_with_origin(PLATFORM);
    let silent = FakeGithub::new();
    let read = guard(&other, &silent, &token_missing(), &WriteGuardSession::default(), "x", false);
    assert_eq!(read.protection, "unknown");
    assert!(read.protection_error.as_deref().is_some_and(|e| !e.is_empty()));
    assert!(silent.requests().is_empty());
}

// GTC-FR-TTSV: each read is kept for the session, and `refresh` reads again.
#[test]
fn protection_reads_are_kept_for_the_session_and_refresh_reads_again() {
    let f = repo_with_origin(PLATFORM);
    let github = github_with_default("main");
    github.answer(&branch_path("main"), Ok(json!({ "protected": false })));
    let tokens = token_ready();
    let session = WriteGuardSession::default();
    guard(&f, &github, &tokens, &session, "main", false);
    let asked = github.requests().len();
    assert_eq!(asked, 2, "the repository and the branch");

    guard(&f, &github, &tokens, &session, "main", false);
    assert_eq!(github.requests().len(), asked, "a second read asks GitHub nothing");

    github.answer(&branch_path("main"), Ok(json!({ "protected": true })));
    let refreshed = guard(&f, &github, &tokens, &session, "main", true);
    assert_eq!(github.requests().len(), asked * 2);
    assert_eq!(refreshed.protection, "protected");
}

// GTC-FR-JWOL / GTC-FR-YWRZ: an unconfirmed default branch is refused with
// the guard; a confirmation lets the next write through for the session.
#[test]
fn the_default_branch_is_refused_until_the_author_confirms() {
    let f = WorktreeFixture::new();
    let github = FakeGithub::new();
    let tokens = token_missing();
    let session = WriteGuardSession::default();
    let branch = f.current();
    let buffer = own_buffer();

    let refused = check(&f, &github, &tokens, &session, Some(&branch), buffer).unwrap_err();
    assert!(refused.starts_with(ERR_MAIN_BRANCH_WRITE_UNCONFIRMED), "{refused}");
    let carried = carried(&refused);
    assert_eq!(carried.branch, branch);
    assert!(carried.is_main && !carried.confirmed);
    let record = record_for(buffer, "main branch write refused");
    assert_eq!(record.fields["operation"], "commit_paths");

    let before = f.snapshot_all();
    let confirmed = confirm(&f, &github, &tokens, &session, &branch).unwrap();
    assert!(confirmed.confirmed);
    assert_eq!(f.snapshot_all(), before, "a confirmation writes nothing to disk");
    assert!(check(&f, &github, &tokens, &session, Some(&branch), buffer).is_ok());
    assert!(guard(&f, &github, &tokens, &session, &branch, false).confirmed);

    // GTC-FR-YWRZ: the memory is the session's, and a new session holds none.
    let relaunched = WriteGuardSession::default();
    assert!(check(&f, &github, &tokens, &relaunched, Some(&branch), buffer).is_err());
}

// GTC-FR-JWOL / GTC-FR-YWRZ: a protected branch is refused always, and a
// confirmation of it records nothing.
#[test]
fn a_protected_branch_is_refused_and_cannot_be_confirmed() {
    let f = repo_with_origin(PLATFORM);
    let github = github_with_default("main");
    github.answer(&branch_path("main"), Ok(json!({ "protected": true })));
    let tokens = token_ready();
    let session = WriteGuardSession::default();

    let buffer = own_buffer();
    let refused = check(&f, &github, &tokens, &session, Some("main"), buffer).unwrap_err();
    assert!(refused.starts_with(ERR_MAIN_BRANCH_PROTECTED), "{refused}");
    // The token was presented to GitHub, and no record or refusal holds it.
    assert!(github.requests().iter().any(|(secret, _)| secret == SECRET));
    assert!(!buffer_text(buffer).contains(SECRET));
    assert!(!refused.contains(SECRET));
    assert_eq!(carried(&refused).branch, "main");

    let confirmation = confirm(&f, &github, &tokens, &session, "main").unwrap_err();
    assert!(confirmation.starts_with(ERR_MAIN_BRANCH_PROTECTED));
    assert!(!guard(&f, &github, &tokens, &session, "main", false).confirmed);
    assert!(check(&f, &github, &tokens, &session, Some("main"), own_buffer()).is_err());
}

// GTC-FR-JWOL: a write that names no branch passes, and a branch that is not a
// main branch passes.
#[test]
fn a_write_with_no_branch_or_to_another_branch_passes() {
    let f = WorktreeFixture::new();
    f.branch("feature");
    let github = FakeGithub::new();
    let tokens = token_missing();
    let session = WriteGuardSession::default();
    assert!(check(&f, &github, &tokens, &session, None, own_buffer()).is_ok());
    assert!(check(&f, &github, &tokens, &session, Some("feature"), own_buffer()).is_ok());
}

// GTC-FR-LRNB: an unknown protection passes for another branch, with a
// warning, and needs the confirmation for the default branch.
#[test]
fn an_unknown_protection_passes_another_branch_and_holds_the_default_branch() {
    let f = repo_with_origin(PLATFORM);
    let github = FakeGithub::new();
    let tokens = token_missing();
    let session = WriteGuardSession::default();
    f.branch("feature");
    let buffer = own_buffer();

    assert!(check(&f, &github, &tokens, &session, Some("feature"), buffer).is_ok());
    let warning = record_for(buffer, "branch protection unknown; write allowed");
    assert_eq!(warning.level, crate::logging::LogLevel::Warn);
    assert_eq!(warning.fields["branch"], "feature");

    let default = f.current();
    let refused = check(&f, &github, &tokens, &session, Some(&default), buffer).unwrap_err();
    assert!(refused.starts_with(ERR_MAIN_BRANCH_WRITE_UNCONFIRMED));
    assert_eq!(carried(&refused).protection, "unknown");
}

// GTC-FR-VZJR: a branch name that is not a valid one is never put in a request.
#[test]
fn an_invalid_branch_name_is_never_sent_to_github() {
    let f = repo_with_origin(PLATFORM);
    let github = github_with_default("main");
    let read = guard(&f, &github, &token_ready(), &WriteGuardSession::default(), "a..b", false);
    assert_eq!(read.protection, "unknown");
    assert!(github.paths().iter().all(|path| !path.contains("a..b")));
}

// DRS-FR-MIYJ: the background check reads only the session and the local
// repository, and holds the default branch until it is confirmed.
#[test]
fn the_background_check_holds_an_unconfirmed_default_branch() {
    let f = WorktreeFixture::new();
    let github = FakeGithub::new();
    let tokens = token_missing();
    let session = WriteGuardSession::default();
    assert_eq!(background_refusal(&session, &f.root()), Some(ERR_MAIN_BRANCH_WRITE_UNCONFIRMED));
    confirm(&f, &github, &tokens, &session, &f.current()).unwrap();
    assert_eq!(background_refusal(&session, &f.root()), None);

    let repo = f.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();
    repo.set_head("refs/heads/feature").unwrap();
    assert_eq!(background_refusal(&WriteGuardSession::default(), &f.root()), None);
}

// GTC-FR-TTSV / GTC-FR-YWRZ: the two commands, as the frontend reaches them.
// With no branch named, the guard is of the branch the active worktree holds.
#[test]
fn the_commands_read_the_active_branch_and_record_a_confirmation() {
    use crate::git::write_guard::commands::{
        confirm_main_branch_write, get_branch_write_guard, WriteGuardState,
    };
    use tauri::Manager;
    let f = repo_with_origin(PLATFORM);
    let current = f.current();
    let github = github_with_default(&current);
    github.answer(&branch_path(&current), Ok(json!({ "protected": false })));
    let (store, tokens) = token_ready();

    let app = tauri::test::mock_app();
    app.manage(crate::project::ProjectState::default());
    app.manage(WriteGuardState::with_client(Box::new(github)));
    app.manage(store);
    app.manage(tokens);
    app.state::<crate::project::ProjectState>().set_root(f.root());
    let handle = app.handle().clone();

    let read = tauri::async_runtime::block_on(get_branch_write_guard(None, false, handle.clone()))
        .expect("a guard");
    assert_eq!(read.branch, current);
    assert!(read.is_default && read.is_main && !read.confirmed);

    let confirmed =
        tauri::async_runtime::block_on(confirm_main_branch_write(current.clone(), handle.clone()))
            .expect("confirmed");
    assert!(confirmed.confirmed);
    let again = tauri::async_runtime::block_on(get_branch_write_guard(
        Some(current.clone()),
        false,
        handle,
    ))
    .expect("a guard");
    assert!(again.confirmed);
}

// GTC-FR-TTSV: the wire shape of `BranchWriteGuard` is camelCase, and
// `protectionError` is left out when there is none.
#[test]
fn the_guard_crosses_the_wire_in_camel_case() {
    let guard = BranchWriteGuard {
        branch: "main".into(),
        is_default: true,
        protection: "unknown".into(),
        protection_error: Some("github_unreachable".into()),
        is_main: true,
        confirmed: false,
    };
    assert_eq!(
        serde_json::to_value(&guard).unwrap(),
        json!({
            "branch": "main",
            "isDefault": true,
            "protection": "unknown",
            "protectionError": "github_unreachable",
            "isMain": true,
            "confirmed": false,
        })
    );
    let none = BranchWriteGuard { protection_error: None, ..guard };
    assert!(serde_json::to_value(&none).unwrap().get("protectionError").is_none());
}

// GTC-FR-YWRZ: a confirmation is held for the repository and the branch. It
// reaches a second worktree of the same repository, and it does not reach
// another repository that has a branch of the same name.
#[test]
fn a_confirmation_reaches_the_worktrees_of_its_repository_and_no_other_repository() {
    let f = WorktreeFixture::new();
    let other = WorktreeFixture::new();
    let github = FakeGithub::new();
    let tokens = token_missing();
    let session = WriteGuardSession::default();
    let branch = f.current();
    assert_eq!(other.current(), branch, "both repositories hold a branch of the same name");
    let buffer = own_buffer();

    confirm(&f, &github, &tokens, &session, &branch).unwrap();

    // A second worktree of the same repository, on a branch of its own.
    let repo = f.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let side = repo.branch("side", &head, false).unwrap();
    let linked = f.sibling("linked");
    let mut options = git2::WorktreeAddOptions::new();
    options.reference(Some(side.get()));
    repo.worktree("linked", &linked, Some(&options)).unwrap();
    let in_linked = with_access(&f, &github, &tokens, |access| {
        let access = PullRequestAccess {
            project: GithubAccess { root: &linked, ..access.project },
            client: access.client,
        };
        check_write(&NullSink, buffer, &session, &access, Some(&branch), "commit_paths")
    });
    assert!(in_linked.is_ok(), "the confirmation reaches the second worktree: {in_linked:?}");

    let refused = check(&other, &github, &tokens, &session, Some(&branch), buffer).unwrap_err();
    assert!(refused.starts_with(ERR_MAIN_BRANCH_WRITE_UNCONFIRMED), "{refused}");
    assert!(!carried(&refused).confirmed);
}
