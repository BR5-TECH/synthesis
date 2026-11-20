//! The commands of the main branch write guard, and the one-line check that
//! each write command calls (`GTC-git.md` GTC-FR-JWOL, GTC-FR-EMLM).

use crate::project::OpenProject as _;
use std::path::Path;
use std::sync::Arc;

use tauri::Manager;

use super::{
    branch_at, check_write, confirm_write, guard_for, BranchWriteGuard, WriteGuardSession,
};
use crate::git::pull_requests::client::{GithubPullRequests, HttpGithubPullRequests};
use crate::git::pull_requests::PullRequestAccess;
use crate::git::GithubAccess;
use crate::github_tokens::GithubTokens;
use crate::global_settings::GlobalSettingsStore;
use crate::logging::{self, LogSink};

/// The error of a check that cannot reach the guard. Production manages the
/// guard at start, so this shows only in a build that forgot to.
pub const ERR_WRITE_GUARD_UNAVAILABLE: &str = "main_branch_write_guard_unavailable";

/// The managed state of the guard: the session memory, and the client that
/// reads GitHub. A test replaces the client.
pub struct WriteGuardState {
    session: Arc<WriteGuardSession>,
    client: Box<dyn GithubPullRequests>,
}

impl Default for WriteGuardState {
    fn default() -> Self {
        WriteGuardState {
            session: Arc::new(WriteGuardSession::default()),
            client: Box::new(HttpGithubPullRequests),
        }
    }
}

impl WriteGuardState {
    /// A state that reads GitHub through `client`.
    #[cfg(test)]
    pub(crate) fn with_client(client: Box<dyn GithubPullRequests>) -> Self {
        WriteGuardState { session: Arc::new(WriteGuardSession::default()), client }
    }

    /// A state whose client answers every request as unreachable, so a test
    /// can never reach a network through it.
    #[cfg(test)]
    pub(crate) fn offline() -> Self {
        Self::with_client(Box::new(OfflineGithub))
    }

    /// The session memory, for the background writers (DRS-FR-MIYJ).
    pub fn session(&self) -> Arc<WriteGuardSession> {
        self.session.clone()
    }
}

/// A GitHub client of the tests that answers every request as unreachable.
#[cfg(test)]
struct OfflineGithub;

#[cfg(test)]
impl GithubPullRequests for OfflineGithub {
    fn get_json(
        &self,
        _secret: &str,
        _path: &str,
    ) -> Result<serde_json::Value, crate::git::pull_requests::client::GithubFailure> {
        Err(crate::git::pull_requests::client::GithubFailure::Unreachable)
    }
}

/// Run `work` with the session and the GitHub access of the project at `root`.
///
/// `None` when the application holds no guard. Under test that is a handle
/// that did not ask for the guard.
fn with_access<R, T>(
    app: &tauri::AppHandle<R>,
    root: &Path,
    work: impl FnOnce(&WriteGuardSession, &PullRequestAccess<'_>) -> T,
) -> Option<T>
where
    R: tauri::Runtime,
{
    let state = app.try_state::<WriteGuardState>()?;
    let store = app.try_state::<GlobalSettingsStore>()?;
    let tokens = app.try_state::<GithubTokens>()?;
    let project_key =
        app.open_project_state().map(|project| project.slot_key()).unwrap_or_default();
    let access = PullRequestAccess {
        project: GithubAccess { root, store: &store, tokens: &tokens, project_key: &project_key },
        client: state.client.as_ref(),
    };
    Some(work(&state.session, &access))
}

/// GTC-FR-JWOL: the check of a write to `branch` from the checkout at `root`.
/// The one line each write command calls before it writes anything.
pub(crate) fn require_branch_write<R>(
    app: &tauri::AppHandle<R>,
    root: &Path,
    branch: Option<&str>,
    operation: &str,
) -> Result<(), String>
where
    R: tauri::Runtime,
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    match with_access(app, root, |session, access| {
        check_write(app, &logging::BUFFER, session, access, branch, operation)
    }) {
        Some(result) => result,
        None => unavailable(app, operation),
    }
}

/// GTC-FR-OVSD / GSM-FR-GHFY: the check of a write to each repository in
/// `targets` — `(project-relative root, checkout)` — on the branch its
/// checkout holds, before the first write. The refusal of a submodule names it.
pub(crate) fn require_repositories_write<R>(
    app: &tauri::AppHandle<R>,
    targets: &[(String, std::path::PathBuf)],
    operation: &str,
) -> Result<(), String>
where
    R: tauri::Runtime,
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    for (repository, checkout) in targets {
        if let Err(e) = require_active_branch_write(app, checkout, operation) {
            // GSM-FR-WPKJ: the record of the check does not know the
            // repository, so a refusal of a submodule says which one.
            if !repository.is_empty() && super::is_write_guard_refusal(&e) {
                logging::log_warn(
                    app,
                    &logging::BUFFER,
                    super::super::LOCAL,
                    "main branch write refused in a submodule",
                    crate::log_fields! { "repository" => repository, "operation" => operation },
                );
            }
            return Err(super::refusal_in_repository(e, repository));
        }
    }
    Ok(())
}

/// GTC-FR-ZBAL: the checkout of the member that `repository` names, or the
/// content root for none. A path that is not a member is `"unknown repository"`.
fn checkout_of(root: &Path, repository: Option<&str>) -> Result<std::path::PathBuf, String> {
    let Some(repository) = repository.filter(|r| !r.is_empty()) else {
        return Ok(root.to_path_buf());
    };
    let set = crate::changes::repo_set::RepoSet::open(root)?;
    set.submodule_at(repository)
        .and_then(|member| member.repo.workdir().map(Path::to_path_buf))
        .ok_or_else(|| ERR_UNKNOWN_REPOSITORY.to_string())
}

/// GTC-FR-ZBAL: a `repository` that is not a member of the repository set.
pub const ERR_UNKNOWN_REPOSITORY: &str = "unknown repository";

/// GTC-FR-TTSV: read the guard of `branch` into the session memory, so that a
/// later check of it reads no network. It refuses nothing.
pub(crate) fn prime_branch_guard<R>(app: &tauri::AppHandle<R>, root: &Path, branch: Option<&str>)
where
    R: tauri::Runtime,
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let Some(branch) = branch.filter(|name| !name.is_empty()) else {
        return;
    };
    let _ = with_access(app, root, |session, access| {
        guard_for(app, &logging::BUFFER, session, access, branch, false)
    });
}

/// GTC-FR-JWOL: the check of a write to the branch the checkout at `root`
/// holds. A detached `HEAD` names no branch and passes.
pub(crate) fn require_active_branch_write<R>(
    app: &tauri::AppHandle<R>,
    root: &Path,
    operation: &str,
) -> Result<(), String>
where
    R: tauri::Runtime,
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    require_branch_write(app, root, branch_at(root).as_deref(), operation)
}

/// GTC-FR-JWOL: the check of a write to the branch the project's active
/// worktree holds. With no project open there is nothing to write to, and the
/// write refuses on its own.
pub(crate) fn require_project_branch_write<R>(
    app: &tauri::AppHandle<R>,
    operation: &str,
) -> Result<(), String>
where
    R: tauri::Runtime,
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    match project_root(app) {
        Ok(root) => require_active_branch_write(app, &root, operation),
        Err(_) => Ok(()),
    }
}

#[cfg(test)]
fn unavailable<R: tauri::Runtime>(
    _app: &tauri::AppHandle<R>,
    _operation: &str,
) -> Result<(), String> {
    // A test handle that manages no guard did not ask for the check.
    Ok(())
}

#[cfg(not(test))]
fn unavailable<R>(app: &tauri::AppHandle<R>, operation: &str) -> Result<(), String>
where
    R: tauri::Runtime,
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    logging::log_error(
        app,
        &logging::BUFFER,
        super::LOCAL,
        "main branch write guard unavailable",
        crate::log_fields! { "operation" => operation },
    );
    Err(ERR_WRITE_GUARD_UNAVAILABLE.to_string())
}

/// The project root of `app`, or the typed "no project open" error.
fn project_root<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<crate::fs::RootFs, String> {
    app.open_project_state().ok_or_else(|| "no project open".to_string())?.require_root()
}

/// GTC `get_branch_write_guard(branch, refresh)` (GTC-FR-TTSV). With no
/// `branch`, the guard is of the branch the active worktree holds.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub async fn get_branch_write_guard<R: tauri::Runtime>(
    branch: Option<String>,
    refresh: bool,
    repository: Option<String>,
    app: tauri::AppHandle<R>,
) -> Result<BranchWriteGuard, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let content_root = project_root(&app)?;
    crate::project_scope::context::spawn_blocking(move || {
        let label = repository.clone().unwrap_or_default();
        let root = checkout_of(&content_root, repository.as_deref())?;
        let branch = branch.filter(|name| !name.is_empty()).or_else(|| branch_at(&root));
        with_access(&app, &root, |session, access| {
            guard_for(
                &app,
                &logging::BUFFER,
                session,
                access,
                branch.as_deref().unwrap_or(""),
                refresh,
            )
            .in_repository(&label)
        })
        .ok_or_else(|| ERR_WRITE_GUARD_UNAVAILABLE.to_string())
    })
    .await
    .map_err(|error| format!("branch write guard read failed: {error}"))?
}

/// GTC `confirm_main_branch_write(branch)` (GTC-FR-YWRZ).
#[project_scope_macros::project_scoped]
#[tauri::command]
pub async fn confirm_main_branch_write<R: tauri::Runtime>(
    branch: String,
    repository: Option<String>,
    app: tauri::AppHandle<R>,
) -> Result<BranchWriteGuard, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let content_root = project_root(&app)?;
    crate::project_scope::context::spawn_blocking(move || {
        let label = repository.clone().unwrap_or_default();
        let root = checkout_of(&content_root, repository.as_deref())?;
        with_access(&app, &root, |session, access| {
            confirm_write(&app, &logging::BUFFER, session, access, &branch)
                .map(|guard| guard.in_repository(&label))
        })
        .unwrap_or_else(|| Err(ERR_WRITE_GUARD_UNAVAILABLE.to_string()))
    })
    .await
    .map_err(|error| format!("main branch write confirmation failed: {error}"))?
}
