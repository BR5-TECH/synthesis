//! The main branch write guard (`../../specifications/core/GTC-git.md`
//! GTC-FR-UOUS, GTC-FR-VZJR, GTC-FR-TTSV, GTC-FR-YWRZ, GTC-FR-JWOL,
//! GTC-FR-LRNB, GTC-FR-EMLM).
//!
//! A **main branch** is the default branch of the repository or a branch that
//! GitHub protects. The application writes to a main branch only after the
//! author confirms it, once per branch for the application session. A branch
//! that GitHub protects needs the same confirmation. It is not blocked: the
//! author owns the choice, and GitHub refuses a push on its own.
//!
//! This module holds three things:
//!
//! - the [`BranchWriteGuard`] read, which tells a surface whether a write needs
//!   a confirmation before the surface asks for the write;
//! - the session memory of the protection reads and of the confirmations. It is
//!   in memory only and is never written to disk (GTC-FR-YWRZ);
//! - the **main branch write check** (GTC-FR-JWOL), which each write command
//!   calls in one line before it writes anything.
//!
//! The GitHub reads go through the `GithubPullRequests` trait, so a test
//! replaces them and no test reaches a network.

pub(crate) mod commands;
pub(crate) mod off_main;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::log_fields;
use crate::logging::{self, LogBuffer, LogSink};

use super::pull_requests::protection::{read_github_view, read_protection, GithubView};
use super::pull_requests::PullRequestAccess;
use super::telemetry::{LOCAL, TRANSFER};

/// GTC-FR-JWOL: a write to a main branch that the author did not confirm.
/// Carries the [`BranchWriteGuard`] as JSON.
pub const ERR_MAIN_BRANCH_WRITE_UNCONFIRMED: &str = "main_branch_write_unconfirmed";

/// GTC-FR-VZJR: the four values of `protection`.
pub const PROTECTION_PROTECTED: &str = "protected";
pub const PROTECTION_UNPROTECTED: &str = "unprotected";
pub const PROTECTION_UNKNOWN: &str = "unknown";
pub const PROTECTION_NOT_APPLICABLE: &str = "not_applicable";

/// GTC `BranchWriteGuard`: whether a branch is a main branch, how GitHub
/// protects it, and whether the author confirmed a write to it in this session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchWriteGuard {
    /// The branch. Empty when the write names no branch (a detached `HEAD`).
    pub branch: String,
    /// GTC-FR-UOUS.
    pub is_default: bool,
    /// GTC-FR-VZJR: `protected`, `unprotected`, `unknown`, or `not_applicable`.
    pub protection: String,
    /// GTC-FR-VZJR: the typed cause of `unknown`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protection_error: Option<String>,
    /// GTC-FR-TTSV.
    pub is_main: bool,
    /// GTC-FR-YWRZ.
    pub confirmed: bool,
}

impl BranchWriteGuard {
    /// The guard of a write that names no branch: it is never a main branch.
    fn no_branch() -> BranchWriteGuard {
        BranchWriteGuard {
            branch: String::new(),
            is_default: false,
            protection: PROTECTION_NOT_APPLICABLE.to_string(),
            protection_error: None,
            is_main: false,
            confirmed: false,
        }
    }
}

/// The typed refusal `code`, carrying `guard` as JSON data.
pub fn refusal(code: &str, guard: &BranchWriteGuard) -> String {
    format!("{code}: {}", serde_json::to_string(guard).unwrap_or_else(|_| "{}".to_string()))
}

/// Whether `error` is the refusal of the check.
pub fn is_write_guard_refusal(error: &str) -> bool {
    error.starts_with(ERR_MAIN_BRANCH_WRITE_UNCONFIRMED)
}

// ---------------------------------------------------------------------------
// Session memory (GTC-FR-TTSV, GTC-FR-YWRZ)
// ---------------------------------------------------------------------------

/// One protection read, kept for the session (GTC-FR-TTSV).
#[derive(Clone, Debug)]
struct ProtectionRead {
    protection: &'static str,
    error: Option<String>,
}

/// What the session knows about one repository.
#[derive(Default)]
struct RepositoryMemory {
    /// GitHub's view of the repository, or `None` before the first read.
    github: Option<GithubView>,
    /// Each branch's protection read, by branch name.
    protection: HashMap<String, ProtectionRead>,
    /// The branches the author confirmed writes to (GTC-FR-YWRZ).
    confirmed: HashSet<String>,
}

/// The session memory of the guard: protection reads and confirmations, for
/// each repository. A project is identified by its repository, so the
/// confirmation of a branch holds in every worktree of the project.
#[derive(Default)]
pub struct WriteGuardSession {
    repositories: Mutex<HashMap<String, RepositoryMemory>>,
}

impl WriteGuardSession {
    fn with<T>(&self, key: &str, read: impl FnOnce(&mut RepositoryMemory) -> T) -> T {
        let mut map = self.repositories.lock().unwrap_or_else(|e| e.into_inner());
        read(map.entry(key.to_string()).or_default())
    }

    fn is_confirmed(&self, key: &str, branch: &str) -> bool {
        self.with(key, |memory| memory.confirmed.contains(branch))
    }

    fn record_confirmation(&self, key: &str, branch: &str) {
        self.with(key, |memory| memory.confirmed.insert(branch.to_string()));
    }
}

/// The session the background writers read (DRS-FR-MIYJ). Set once at start,
/// by [`install`]. A test build never sets it, so a test that does not ask for
/// the check is not refused.
static INSTALLED: OnceLock<Arc<WriteGuardSession>> = OnceLock::new();

/// Make `session` the one the background writers read.
pub fn install(session: Arc<WriteGuardSession>) {
    let _ = INSTALLED.set(session);
}

/// The session the background writers read, when one is installed.
pub fn installed() -> Option<Arc<WriteGuardSession>> {
    INSTALLED.get().cloned()
}

// ---------------------------------------------------------------------------
// Local reads
// ---------------------------------------------------------------------------

/// The key of the repository that holds `root`: its common Git directory, which
/// is the same for every worktree of the repository.
pub(crate) fn repository_key(root: &Path) -> Option<String> {
    let repo = crate::changes::open_repo(root).ok()?;
    Some(crate::changes::canonicalize_lenient(repo.commondir()).to_string_lossy().into_owned())
}

/// The branch that the checkout at `root` holds, or `None` for a detached or
/// unborn `HEAD` and for a path outside a repository.
pub(crate) fn branch_at(root: &Path) -> Option<String> {
    let repo = crate::changes::open_repo(root).ok()?;
    let head = repo.find_reference("HEAD").ok()?;
    let target = head.symbolic_target().ok().flatten()?.to_string();
    let name = target.strip_prefix("refs/heads/")?;
    // A `HEAD` that points at a branch with no commit yet is unborn. It holds
    // no branch to write to.
    repo.find_branch(name, git2::BranchType::Local).ok()?;
    Some(name.to_string())
}

/// GTC-FR-UOUS: the default branch when GitHub cannot say. The first that
/// exists of the primary remote's `HEAD`, a local `main`, and a local
/// `master`. `None` when none of them exists.
pub(crate) fn local_default_branch(root: &Path) -> Option<String> {
    let repo = crate::changes::open_repo(root).ok()?;
    if let Some(remote) = crate::changes::primary_remote_name(&repo) {
        let head_ref = format!("refs/remotes/{remote}/HEAD");
        if let Ok(reference) = repo.find_reference(&head_ref) {
            if let Ok(Some(target)) = reference.symbolic_target() {
                if let Some(rest) = target.strip_prefix(&format!("refs/remotes/{remote}/")) {
                    if !rest.is_empty() {
                        return Some(rest.to_string());
                    }
                }
            }
        }
    }
    ["main", "master"]
        .into_iter()
        .find(|name| repo.find_branch(name, git2::BranchType::Local).is_ok())
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// The guard read (GTC-FR-UOUS, GTC-FR-VZJR, GTC-FR-TTSV)
// ---------------------------------------------------------------------------

/// GTC `get_branch_write_guard`: the guard of `branch` in the repository of
/// `access.project.root`. `refresh` reads GitHub again (GTC-FR-TTSV).
///
/// This never fails: a read of GitHub that fails is `unknown` protection with
/// its cause (GTC-FR-VZJR), and a path outside a repository holds no branch.
pub(crate) fn guard_for<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    session: &WriteGuardSession,
    access: &PullRequestAccess<'_>,
    branch: &str,
    refresh: bool,
) -> BranchWriteGuard {
    let root = access.project.root;
    let Some(key) = repository_key(root).filter(|_| !branch.is_empty()) else {
        return BranchWriteGuard::no_branch();
    };

    // GitHub's view of the repository: the default branch it reports, or why
    // there is none. The lock is not held while the network is read.
    // Whether this read reached GitHub. Only such a read is worth a record: a
    // read from the session memory runs at each save.
    let mut fetched = false;
    let cached_view = if refresh { None } else { session.with(&key, |m| m.github.clone()) };
    let view = match cached_view {
        Some(view) => view,
        None => {
            fetched = true;
            let view = read_github_view(access);
            if let GithubView::Failed(error) = &view {
                logging::log_warn(
                    sink,
                    buffer,
                    TRANSFER,
                    "repository read for the branch write guard failed",
                    log_fields! { "error" => error.clone() },
                );
            }
            session.with(&key, |m| m.github = Some(view.clone()));
            view
        }
    };

    let cached_read =
        if refresh { None } else { session.with(&key, |m| m.protection.get(branch).cloned()) };
    let read = match cached_read {
        Some(read) => read,
        None => {
            fetched = true;
            let read = match &view {
                GithubView::NotApplicable => {
                    ProtectionRead { protection: PROTECTION_NOT_APPLICABLE, error: None }
                }
                GithubView::Failed(error) => {
                    ProtectionRead { protection: PROTECTION_UNKNOWN, error: Some(error.clone()) }
                }
                GithubView::Repository { .. } => match read_protection(access, branch) {
                    Ok(true) => ProtectionRead { protection: PROTECTION_PROTECTED, error: None },
                    Ok(false) => ProtectionRead { protection: PROTECTION_UNPROTECTED, error: None },
                    Err(error) => {
                        logging::log_warn(
                            sink,
                            buffer,
                            TRANSFER,
                            "branch protection read failed",
                            log_fields! { "branch" => branch, "error" => error.clone() },
                        );
                        ProtectionRead { protection: PROTECTION_UNKNOWN, error: Some(error) }
                    }
                },
            };
            session.with(&key, |m| m.protection.insert(branch.to_string(), read.clone()));
            read
        }
    };

    let default = match &view {
        GithubView::Repository { default_branch: Some(name) } => Some(name.clone()),
        _ => local_default_branch(root),
    };
    let is_default = default.as_deref() == Some(branch);
    let guard = BranchWriteGuard {
        branch: branch.to_string(),
        is_default,
        protection: read.protection.to_string(),
        protection_error: read.error,
        is_main: is_default || read.protection == PROTECTION_PROTECTED,
        confirmed: session.is_confirmed(&key, branch),
    };
    if !fetched {
        return guard;
    }
    logging::log_debug(
        sink,
        buffer,
        LOCAL,
        "branch write guard read",
        log_fields! {
            "branch" => branch,
            "isDefault" => guard.is_default,
            "protection" => guard.protection.clone(),
            "isMain" => guard.is_main,
            "confirmed" => guard.confirmed,
            "refresh" => refresh,
        },
    );
    guard
}

/// GTC-FR-JWOL / GTC-FR-LRNB: the main branch write check. `branch` is the
/// branch the write lands on, and `operation` names the write in the record.
///
/// It writes nothing. A write that names no branch passes.
pub(crate) fn check_write<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    session: &WriteGuardSession,
    access: &PullRequestAccess<'_>,
    branch: Option<&str>,
    operation: &str,
) -> Result<(), String> {
    let Some(branch) = branch.filter(|name| !name.is_empty()) else {
        return Ok(());
    };
    let guard = guard_for(sink, buffer, session, access, branch, false);
    // GTC-FR-JWOL: a main branch with no confirmation is refused, also a branch
    // that GitHub protects. GTC-FR-LRNB: an `unknown` protection of the default
    // branch needs the confirmation, as an unprotected one does.
    if guard.is_main && !guard.confirmed {
        logging::log_warn(
            sink,
            buffer,
            LOCAL,
            "main branch write refused",
            log_fields! {
                "branch" => branch,
                "operation" => operation,
                "reason" => ERR_MAIN_BRANCH_WRITE_UNCONFIRMED,
                "protection" => guard.protection.clone(),
            },
        );
        return Err(refusal(ERR_MAIN_BRANCH_WRITE_UNCONFIRMED, &guard));
    }
    if guard.protection == PROTECTION_UNKNOWN && !guard.is_default {
        // GTC-FR-LRNB: a branch that is not the default branch passes on an
        // unknown protection, and the cause is worth a record.
        logging::log_warn(
            sink,
            buffer,
            LOCAL,
            "branch protection unknown; write allowed",
            log_fields! {
                "branch" => branch,
                "operation" => operation,
                "error" => guard.protection_error.clone().unwrap_or_default(),
            },
        );
    }
    Ok(())
}

/// GTC `confirm_main_branch_write` (GTC-FR-YWRZ): record the confirmation for
/// the repository and the branch, in memory. A protected branch is recorded as
/// any other main branch.
pub(crate) fn confirm_write<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    session: &WriteGuardSession,
    access: &PullRequestAccess<'_>,
    branch: &str,
) -> Result<BranchWriteGuard, String> {
    let mut guard = guard_for(sink, buffer, session, access, branch, false);
    let Some(key) = repository_key(access.project.root).filter(|_| !branch.is_empty()) else {
        return Ok(guard);
    };
    session.record_confirmation(&key, branch);
    guard.confirmed = true;
    logging::log_info(
        sink,
        buffer,
        LOCAL,
        "main branch write confirmed",
        log_fields! {
            "branch" => branch,
            "isDefault" => guard.is_default,
            "protection" => guard.protection.clone(),
        },
    );
    Ok(guard)
}

/// DRS-FR-MIYJ: the check of a background write, which must not wait on a
/// network. It reads only what the session already knows, and the local
/// default branch where GitHub's view was never read.
///
/// Returns the refusal code, or `None` when the write may go ahead.
pub fn background_refusal(session: &WriteGuardSession, root: &Path) -> Option<&'static str> {
    let branch = branch_at(root)?;
    let key = repository_key(root)?;
    let (view, read, confirmed) = session.with(&key, |m| {
        (m.github.clone(), m.protection.get(&branch).cloned(), m.confirmed.contains(&branch))
    });
    let protected = read.as_ref().is_some_and(|read| read.protection == PROTECTION_PROTECTED);
    let default = match view {
        Some(GithubView::Repository { default_branch: Some(name) }) => Some(name),
        _ => local_default_branch(root),
    };
    let is_main = protected || default.as_deref() == Some(branch.as_str());
    if is_main && !confirmed {
        return Some(ERR_MAIN_BRANCH_WRITE_UNCONFIRMED);
    }
    None
}

/// Record that GitHub protects `branch` in the repository of `root`, for a test
/// that has no GitHub to read.
#[cfg(test)]
pub(crate) fn protect_for_test(session: &WriteGuardSession, root: &Path, branch: &str) {
    let key = repository_key(root).expect("a repository");
    session.with(&key, |memory| {
        memory.protection.insert(
            branch.to_string(),
            ProtectionRead { protection: PROTECTION_PROTECTED, error: None },
        )
    });
}

/// Record a confirmation of `branch` in the repository of `root`, for a test
/// that has no GitHub to read.
#[cfg(test)]
pub(crate) fn confirm_for_test(session: &WriteGuardSession, root: &Path, branch: &str) {
    let key = repository_key(root).expect("a repository");
    session.record_confirmation(&key, branch);
}
