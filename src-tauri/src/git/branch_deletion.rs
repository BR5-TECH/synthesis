//! Deleting a local branch together with its linked worktree
//! (`../../specifications/core/GTC-git.md` GTC-FR-UMXA, GTC-FR-WNZH,
//! GTC-FR-AVKD, GTC-FR-LEBC, GTC-FR-JOWX, GTC-FR-NPCT).
//!
//! `inspect_branch_deletion` and `delete_branch` share one resolution of the
//! state a deletion stands in, so the plan an author confirms and the checks the
//! deletion makes cannot drift apart. Both read the worktree enumeration and the
//! stream store at the moment of the call (WTC-FR-BHFE): a result read earlier
//! is never trusted. The remote leg lives in `branch_deletion_remote.rs`.

use std::path::Path;
use std::time::Instant;

use git2::BranchType;
use serde::Serialize;
use tauri::{Manager, State};

use crate::changes::{self, ERR_UNKNOWN_BRANCH};
use crate::github_tokens::GithubTokens;
use crate::global_settings::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, LogBuffer, LogLevel};
use crate::project::ProjectState;
use crate::worktree::WorktreeEntry;

use super::branch_deletion_remote::{remote_target, RemoteTarget};
use super::branch_info::BranchWorktreeRef;
use super::*;

/// GTC-FR-WNZH: the name is a remote-tracking branch, not a local one.
pub const ERR_NOT_A_LOCAL_BRANCH: &str = "not_a_local_branch";
/// GTC-FR-WNZH: the primary worktree has the branch checked out.
pub const ERR_BRANCH_IN_PRIMARY_WORKTREE: &str = "branch_in_primary_worktree";
/// GTC-FR-WNZH: the active worktree has the branch checked out.
pub const ERR_BRANCH_IN_ACTIVE_WORKTREE: &str = "branch_in_active_worktree";
/// GTC-FR-JOWX: the branch is a work stream's. Followed by `: <stream id>`.
pub const ERR_BRANCH_BELONGS_TO_WORK_STREAM: &str = "branch_belongs_to_work_stream";
/// GTC-FR-WNZH: the worktree holds uncommitted paths. Followed by the paths.
pub const ERR_WORKTREE_DIRTY: &str = "worktree_dirty";

/// GTC-FR-FSRQ: the three states of the remote part of a deletion.
pub const REMOTE_NOT_REQUESTED: &str = "not_requested";
pub const REMOTE_DELETED: &str = "deleted";
pub const REMOTE_FAILED: &str = "failed";

const MSG_INSPECT_FAILED: &str = "branch deletion inspection failed";
const MSG_DELETE_REFUSED: &str = "branch deletion refused";
const MSG_DELETE_FAILED: &str = "branch deletion failed";

// ---------------------------------------------------------------------------
// Wire shapes (GTC payload shapes)
// ---------------------------------------------------------------------------

/// GTC-FR-JOWX: the stream a plan names, and whether a run holds it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchDeletionStream {
    pub stream_id: String,
    pub stream_name: String,
    /// The run holding the stream, or `null`.
    pub busy_run_id: Option<String>,
    /// GTC-FR-JOWX: the commits the stream holds that its base branch does
    /// not, counted as `WKS-work-streams.md` WKS-FR-EIBC counts them.
    pub ahead_of_base: u32,
}

/// GTC-FR-UMXA: what deleting a local branch would remove and discard.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchDeletionPlan {
    pub branch: String,
    /// The linked worktree the deletion removes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<BranchWorktreeRef>,
    /// Present for a work stream's branch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<BranchDeletionStream>,
    /// The complete set the deletion would discard.
    pub uncommitted_paths: Vec<String>,
    /// The short name of the associated remote-tracking branch, when one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_branch: Option<String>,
}

/// GTC-FR-FSRQ: how the remote part of a deletion ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDeletionOutcome {
    /// The caller's `delete_remote` choice.
    pub requested: bool,
    /// `"not_requested"`, `"deleted"` or `"failed"`.
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The typed error, when `state` is `"failed"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// GTC-FR-WNZH / GTC-FR-FSRQ: the result of a branch deletion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchDeletionOutcome {
    pub branch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removed_worktree_path: Option<String>,
    pub remote: RemoteDeletionOutcome,
}

// ---------------------------------------------------------------------------
// The shared resolution (GTC-FR-WNZH, GTC-FR-UMXA)
// ---------------------------------------------------------------------------

/// Everything a deletion stands in, read once and read fresh.
struct Resolved {
    branch: String,
    /// The linked worktree that has the branch checked out.
    worktree: Option<WorktreeEntry>,
    stream: Option<crate::streams::WorkStream>,
    remote: Option<RemoteTarget>,
    uncommitted: Vec<String>,
}

/// The typed code at the head of an error, which carries no path and no id.
pub(crate) fn typed_code(error: &str) -> &str {
    error.split(':').next().unwrap_or(error).trim()
}

/// Whether an error is one of the refusals of this module, which declines
/// before anything changes.
pub(crate) fn is_deletion_refusal(error: &str) -> bool {
    matches!(
        typed_code(error),
        ERR_NOT_A_LOCAL_BRANCH
            | ERR_BRANCH_IN_PRIMARY_WORKTREE
            | ERR_BRANCH_IN_ACTIVE_WORKTREE
            | ERR_BRANCH_BELONGS_TO_WORK_STREAM
            | ERR_WORKTREE_DIRTY
            | ERR_UNKNOWN_COMMIT
            | ERR_PATH_NOT_IN_COMMIT
            | crate::worktree::ERR_DIRECT_GRADUATION_ACTIVE
    )
}

/// GTC-FR-WNZH: the refusals in their fixed order, up to and including the one
/// about a direct graduation run. `plan` reads for `inspect_branch_deletion`,
/// which returns a work stream's branch as a plan rather than refusing it.
fn resolve<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &Path,
    name: &str,
    plan: bool,
) -> Result<Resolved, String> {
    let repo = changes::open_repo(root)?;
    let name = name.trim();
    if name.is_empty() || repo.find_branch(name, BranchType::Local).is_err() {
        // A remote-tracking branch is a branch, only not one this deletes.
        if !name.is_empty() && repo.find_branch(name, BranchType::Remote).is_ok() {
            return Err(ERR_NOT_A_LOCAL_BRANCH.to_string());
        }
        return Err(ERR_UNKNOWN_BRANCH.to_string());
    }

    // WTC-FR-BHFE: the enumeration as it stands now, with the stream facts.
    let mut worktrees = crate::worktree::context_for(root)?.worktrees;
    crate::streams::decorate_worktrees(app, &mut worktrees);
    let holder = worktrees
        .into_iter()
        .find(|w| w.branch.as_deref() == Some(name));
    // The primary worktree is checked before the active one: the primary is
    // usually the active one too, and it is the stronger statement.
    if let Some(held) = &holder {
        if held.is_primary {
            return Err(ERR_BRANCH_IN_PRIMARY_WORKTREE.to_string());
        }
        if held.is_active {
            return Err(ERR_BRANCH_IN_ACTIVE_WORKTREE.to_string());
        }
    }
    let stream = crate::streams::stream_on_branch(app, name);
    if let Some(owner) = &stream {
        if !plan {
            return Err(format!("{ERR_BRANCH_BELONGS_TO_WORK_STREAM}: {}", owner.id));
        }
    } else {
        crate::graduation::require_no_dispatched_direct_run(app)?;
    }

    // GTC-FR-LEBC: a directory that is gone holds no uncommitted path.
    let tree = match (&holder, &stream) {
        (Some(held), _) if !held.is_missing => Some(std::path::PathBuf::from(&held.path)),
        (None, Some(owner)) => Some(owner.worktree()),
        _ => None,
    };
    let uncommitted = tree
        .map(|path| crate::streams::uncommitted_paths_of(&path))
        .unwrap_or_default();
    Ok(Resolved {
        branch: name.to_string(),
        worktree: holder,
        stream,
        remote: remote_target(&repo, name),
        uncommitted,
    })
}

/// GTC-FR-UMXA: read-only. The plan a deletion would carry out, or the typed
/// refusal `delete_branch` would return at this moment.
pub fn inspect_branch_deletion_for<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &Path,
    name: &str,
) -> Result<BranchDeletionPlan, String> {
    let resolved = resolve(app, root, name, true)?;
    Ok(BranchDeletionPlan {
        branch: resolved.branch,
        worktree: resolved.worktree.as_ref().map(BranchWorktreeRef::from),
        stream: resolved.stream.map(|s| BranchDeletionStream {
            ahead_of_base: crate::streams::ahead_of_base_of(root, &s),
            stream_id: s.id,
            stream_name: s.name,
            busy_run_id: s.busy_run_id,
        }),
        uncommitted_paths: resolved.uncommitted,
        remote_branch: resolved.remote.map(|r| r.tracking),
    })
}

/// GTC-FR-UMXA: the inspection, and its record. `DEBUG`, because an author
/// opening a confirmation reads it and nothing changes.
pub fn inspect_branch_deletion_reported<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    root: &Path,
    name: &str,
) -> Result<BranchDeletionPlan, String> {
    let result = inspect_branch_deletion_for(app, root, name);
    match &result {
        Ok(plan) => log_ok(
            app,
            buffer,
            LogLevel::Debug,
            LOCAL,
            "branch deletion inspected",
            log_fields! {
                "branch" => plan.branch.as_str(),
                "hasWorktree" => plan.worktree.is_some(),
                "isStream" => plan.stream.is_some(),
                "uncommittedCount" => plan.uncommitted_paths.len(),
                "hasRemoteBranch" => plan.remote_branch.is_some(),
            },
        ),
        // Typed code only: a dirty refusal carries paths, which are the
        // author's content and stay out of a record (GTC-FR-NPCT).
        Err(error) => log_failure(
            app,
            buffer,
            LOCAL,
            MSG_INSPECT_FAILED,
            typed_code(error),
            log_fields! { "branch" => name.trim() },
        ),
    }
    result
}

// ---------------------------------------------------------------------------
// The deletion (GTC-FR-WNZH, GTC-FR-AVKD, GTC-FR-FSRQ, GTC-FR-NPCT)
// ---------------------------------------------------------------------------

/// GTC-FR-WNZH: delete a local branch, its linked worktree, and optionally its
/// remote branch.
///
/// Re-reads everything and refuses, before any change, in the order of
/// GTC-FR-WNZH. A branch checked out in another, non-active linked worktree
/// goes with that worktree (GTC-FR-AVKD), through `remove_linked_worktree`. The
/// operation switches no branch and no worktree.
///
/// GTC-FR-FSRQ: with `delete_remote`, the credential is resolved before any
/// change, so a project that resolves none is refused with the typed token
/// error and nothing is deleted. The remote leg runs last, and its failure is
/// reported in the outcome while the local results stay.
///
/// GTC-FR-NPCT: emits `"branches changed"` once when the branch was removed,
/// including when the remote leg failed, and nothing on a refusal.
#[allow(clippy::too_many_arguments)]
pub fn delete_branch_at<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    root: &Path,
    project_key: &str,
    store: &GlobalSettingsStore,
    tokens: &GithubTokens,
    name: &str,
    delete_remote: bool,
    discard_uncommitted: bool,
) -> Result<BranchDeletionOutcome, String> {
    let started = Instant::now();
    logging::log_info(
        app,
        buffer,
        LOCAL,
        "deleting branch",
        log_fields! {
            "branch" => name.trim(),
            "deleteRemote" => delete_remote,
            "discardUncommitted" => discard_uncommitted,
        },
    );

    let resolved = resolve(app, root, name, false).inspect_err(|error| {
        log_failure(
            app,
            buffer,
            LOCAL,
            MSG_DELETE_REFUSED,
            typed_code(error),
            log_fields! { "branch" => name.trim() },
        )
    })?;
    if !resolved.uncommitted.is_empty() && !discard_uncommitted {
        logging::log_warn(
            app,
            buffer,
            LOCAL,
            MSG_DELETE_REFUSED,
            log_fields! {
                "branch" => resolved.branch.as_str(),
                "error" => ERR_WORKTREE_DIRTY,
                "uncommittedCount" => resolved.uncommitted.len(),
            },
        );
        return Err(format!(
            "{ERR_WORKTREE_DIRTY}: {}",
            resolved.uncommitted.join(", ")
        ));
    }

    // GTC-FR-FSRQ: before any change. Only a remote that authenticates with the
    // project's token needs one (GTC-FR-09), and only when there is a remote
    // branch to delete.
    let token = match (&resolved.remote, delete_remote) {
        (Some(target), true) => crate::github_tokens::resolve_remote_token(
            store,
            tokens,
            project_key,
            &target.url,
        )
        .inspect_err(|error| {
            log_failure(
                app,
                buffer,
                TRANSFER,
                MSG_DELETE_REFUSED,
                error,
                log_fields! { "branch" => resolved.branch.as_str() },
            )
        })?,
        _ => None,
    };

    let outcome = carry_out(app, buffer, root, &resolved, delete_remote, token.as_deref())
        .inspect_err(|error| {
            log_failure(
                app,
                buffer,
                LOCAL,
                MSG_DELETE_FAILED,
                error,
                log_fields! { "branch" => resolved.branch.as_str() },
            )
        })?;

    // GTC-FR-NPCT: once, now that the branch set has changed.
    crate::worktree::announce_branches_changed(app, root, "branch deletion");
    log_ok(
        app,
        buffer,
        LogLevel::Info,
        LOCAL,
        "branch deleted",
        log_fields! {
            "branch" => outcome.branch.as_str(),
            "removedWorktree" => outcome.removed_worktree_path.is_some(),
            "discardedCount" => resolved.uncommitted.len(),
            "remoteState" => outcome.remote.state.as_str(),
            "durationMs" => duration_ms(started),
        },
    );
    Ok(outcome)
}

/// The changes themselves: the worktree, the local branch, then the remote.
fn carry_out<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    root: &Path,
    resolved: &Resolved,
    delete_remote: bool,
    token: Option<&str>,
) -> Result<BranchDeletionOutcome, String> {
    // GTC-FR-AVKD: the worktree goes first, because Git refuses to delete a
    // branch a worktree still has checked out.
    let removed_worktree_path = match &resolved.worktree {
        Some(held) => {
            crate::worktree::remove_linked_worktree(app, root, Path::new(&held.path))?;
            Some(held.path.clone())
        }
        None => None,
    };
    let main = crate::streams::primary_repo_of(root)?;
    let mut local = main
        .find_branch(&resolved.branch, BranchType::Local)
        .map_err(|_| ERR_UNKNOWN_BRANCH.to_string())?;
    local
        .delete()
        .map_err(|e| format!("failed to delete the branch: {e}"))?;

    let remote = match (&resolved.remote, delete_remote) {
        (Some(target), true) => {
            super::branch_deletion_remote::delete_remote_branch(app, buffer, &main, target, token)
        }
        (None, true) => RemoteDeletionOutcome {
            requested: true,
            state: REMOTE_NOT_REQUESTED.to_string(),
            branch: None,
            error: None,
        },
        (_, false) => RemoteDeletionOutcome {
            requested: false,
            state: REMOTE_NOT_REQUESTED.to_string(),
            branch: None,
            error: None,
        },
    };
    Ok(BranchDeletionOutcome {
        branch: resolved.branch.clone(),
        removed_worktree_path,
        remote,
    })
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// GTC-FR-UMXA: read-only. What deleting a local branch would remove and
/// discard, or the typed refusal.
#[tauri::command]
pub fn inspect_branch_deletion(
    name: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<BranchDeletionPlan, String> {
    inspect_branch_deletion_reported(&app, &logging::BUFFER, &project.require_root()?, &name)
}

/// GTC-FR-WNZH: delete a local branch, its linked worktree, and optionally its
/// remote branch.
///
/// Runs on the blocking pool, because the remote leg is a blocking transfer that
/// would otherwise hold the main thread for as long as an unreachable remote
/// takes to time out.
#[tauri::command]
pub async fn delete_branch(
    name: String,
    delete_remote: bool,
    discard_uncommitted: bool,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<BranchDeletionOutcome, String> {
    let root = project.require_root()?;
    let project_key = project.slot_key();
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<GlobalSettingsStore>();
        let tokens = app.state::<GithubTokens>();
        delete_branch_at(
            &app,
            &logging::BUFFER,
            &root,
            &project_key,
            &store,
            &tokens,
            &name,
            delete_remote,
            discard_uncommitted,
        )
    })
    .await
    .map_err(|e| format!("branch deletion failed: {e}"))?
}
