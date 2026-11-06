//! The commands of the main branch write guard, and the one-line check that
//! each write command calls (`GTC-git.md` GTC-FR-JWOL, GTC-FR-EMLM).

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
use crate::project::ProjectState;

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
        app.try_state::<ProjectState>().map(|project| project.slot_key()).unwrap_or_default();
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
    app.try_state::<ProjectState>().ok_or_else(|| "no project open".to_string())?.require_root()
}

/// GTC `get_branch_write_guard(branch, refresh)` (GTC-FR-TTSV). With no
/// `branch`, the guard is of the branch the active worktree holds.
#[tauri::command]
pub async fn get_branch_write_guard<R: tauri::Runtime>(
    branch: Option<String>,
    refresh: bool,
    app: tauri::AppHandle<R>,
) -> Result<BranchWriteGuard, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let root = project_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
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
        })
        .ok_or_else(|| ERR_WRITE_GUARD_UNAVAILABLE.to_string())
    })
    .await
    .map_err(|error| format!("branch write guard read failed: {error}"))?
}

/// GTC `confirm_main_branch_write(branch)` (GTC-FR-YWRZ).
#[tauri::command]
pub async fn confirm_main_branch_write<R: tauri::Runtime>(
    branch: String,
    app: tauri::AppHandle<R>,
) -> Result<BranchWriteGuard, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let root = project_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        with_access(&app, &root, |session, access| {
            confirm_write(&app, &logging::BUFFER, session, access, &branch)
        })
        .unwrap_or_else(|| Err(ERR_WRITE_GUARD_UNAVAILABLE.to_string()))
    })
    .await
    .map_err(|error| format!("main branch write confirmation failed: {error}"))?
}
