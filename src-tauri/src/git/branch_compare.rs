//! What one branch changed against the branch it grows from
//! (`../../specifications/core/GTC-git.md` GTC-FR-YQVD, GTC-FR-MBBH,
//! GTC-FR-PYVV, GTC-FR-QVDE).
//!
//! The base of a work stream's branch is the stream's `base_branch`, and the
//! base of every other branch is the repository's default branch (CHC-FR-13).
//! The change runs from the merge base of the two tips to the branch tip, so it
//! holds what the branch did and nothing the base did since. Every read here
//! uses local objects only and writes nothing.

use std::path::Path;
use std::time::Instant;

use git2::{BranchType, Commit, Repository};
use serde::Serialize;
use tauri::State;

use crate::changes::{self, ERR_UNKNOWN_BRANCH};
use crate::log_fields;
use crate::logging::{self, LogBuffer, LogLevel};
use crate::project::ProjectState;

use super::history::{files_of, path_diff, tree_diff, CommitFile};
use super::*;

/// GTC-FR-QVDE: the base names no branch of the repository.
pub const ERR_NO_COMPARISON_BASE: &str = "no_comparison_base";

/// GTC-FR-QVDE: the base and the branch share no commit.
pub const ERR_NO_COMPARISON_MERGE_BASE: &str = "no_merge_base";

/// GTC-FR-PYVV: the comparison did not change the path.
pub const ERR_PATH_NOT_IN_COMPARISON: &str = "path_not_in_comparison";

const MSG_COMPARE_STARTED: &str = "branch comparison started";
const MSG_COMPARE_DIFF_STARTED: &str = "branch comparison diff started";
const MSG_COMPARE_FILES_FAILED: &str = "branch comparison failed";
const MSG_COMPARE_DIFF_FAILED: &str = "branch comparison diff failed";

/// GTC-FR-MBBH: one branch against its base.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchComparison {
    /// The compared branch, as named.
    pub branch: String,
    /// `"local"` or `"remote"`.
    pub kind: String,
    /// The base branch's short name.
    pub base: String,
    /// The full id of the merge base. Absent when `same_as_base` is true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_base: Option<String>,
    /// True when the branch is its own base. `files` is then empty.
    pub same_as_base: bool,
    /// The paths changed from the merge base to the branch tip.
    pub files: Vec<CommitFile>,
}

/// The two trees one comparison diffs, or the answer that there is none.
enum Sides<'r> {
    SameAsBase,
    Trees {
        merge_base: git2::Oid,
        old: git2::Tree<'r>,
        new: git2::Tree<'r>,
    },
}

/// The tip of the branch `name` of `kind`, or `"unknown branch"`.
fn branch_tip<'r>(repo: &'r Repository, name: &str, kind: &str) -> Result<Commit<'r>, String> {
    let branch_type = match kind {
        "local" => BranchType::Local,
        "remote" => BranchType::Remote,
        _ => return Err(ERR_UNKNOWN_BRANCH.to_string()),
    };
    if name.is_empty() || (branch_type == BranchType::Remote && name.ends_with("/HEAD")) {
        return Err(ERR_UNKNOWN_BRANCH.to_string());
    }
    repo.find_branch(name, branch_type)
        .ok()
        .and_then(|b| b.get().peel_to_commit().ok())
        .ok_or_else(|| ERR_UNKNOWN_BRANCH.to_string())
}

/// The tip of the base branch: the local branch of that name, else the primary
/// remote's branch of that name, so a clone that never checked the default
/// branch out still has a base. Neither is `no_comparison_base`.
fn base_tip<'r>(repo: &'r Repository, base: &str) -> Result<Commit<'r>, String> {
    let local = repo
        .find_branch(base, BranchType::Local)
        .ok()
        .and_then(|b| b.get().peel_to_commit().ok());
    if let Some(commit) = local {
        return Ok(commit);
    }
    changes::primary_remote_name(repo)
        .and_then(|remote| {
            repo.find_branch(&format!("{remote}/{base}"), BranchType::Remote)
                .ok()
        })
        .and_then(|b| b.get().peel_to_commit().ok())
        .ok_or_else(|| ERR_NO_COMPARISON_BASE.to_string())
}

/// GTC-FR-YQVD: the base of a branch. `stream_base` is the `base_branch` of the
/// work stream that owns the branch, when one does.
fn base_of(repo: &Repository, stream_base: Option<&str>) -> Result<String, String> {
    match stream_base {
        Some(base) if !base.trim().is_empty() => Ok(base.trim().to_string()),
        _ => changes::default_branch(repo),
    }
}

/// GTC-FR-MBBH / GTC-FR-QVDE: the base, and the two trees between the merge
/// base and the branch tip.
fn sides<'r>(
    repo: &'r Repository,
    name: &str,
    kind: &str,
    stream_base: Option<&str>,
) -> Result<(String, Sides<'r>), String> {
    let tip = branch_tip(repo, name, kind)?;
    let base = base_of(repo, stream_base)?;
    if kind == "local" && name == base {
        return Ok((base, Sides::SameAsBase));
    }
    let base_commit = base_tip(repo, &base)?;
    let merge_base = repo
        .merge_base(base_commit.id(), tip.id())
        .map_err(|_| ERR_NO_COMPARISON_MERGE_BASE.to_string())?;
    let old = repo
        .find_commit(merge_base)
        .and_then(|c| c.tree())
        .map_err(|e| format!("failed to read the merge base tree: {e}"))?;
    let new = tip
        .tree()
        .map_err(|e| format!("failed to read the branch tree: {e}"))?;
    Ok((base, Sides::Trees { merge_base, old, new }))
}

/// GTC-FR-YQVD / GTC-FR-MBBH: the paths `name` changed against its base.
pub fn branch_comparison_for(
    root: &Path,
    name: &str,
    kind: &str,
    stream_base: Option<&str>,
) -> Result<BranchComparison, String> {
    let name = name.trim();
    let repo = changes::open_repo(root)?;
    let prefix = changes::project_prefix(&repo, root);
    let (base, sides) = sides(&repo, name, kind, stream_base)?;
    let (merge_base, files) = match sides {
        Sides::SameAsBase => (None, Vec::new()),
        Sides::Trees { merge_base, old, new } => {
            let diff = tree_diff(&repo, Some(&old), &new, &prefix, &[])?;
            (Some(merge_base.to_string()), files_of(&diff, &prefix))
        }
    };
    Ok(BranchComparison {
        branch: name.to_string(),
        kind: kind.to_string(),
        base,
        same_as_base: merge_base.is_none(),
        merge_base,
        files,
    })
}

/// GTC-FR-PYVV: the change of one path from the merge base to the branch tip.
pub fn branch_compare_file_diff_for(
    root: &Path,
    name: &str,
    kind: &str,
    path: &str,
    stream_base: Option<&str>,
) -> Result<DiffPayload, String> {
    let name = name.trim();
    let repo = changes::open_repo(root)?;
    let prefix = changes::project_prefix(&repo, root);
    let (_, sides) = sides(&repo, name, kind, stream_base)?;
    let payload = match sides {
        Sides::SameAsBase => Err(ERR_PATH_NOT_IN_COMPARISON.to_string()),
        Sides::Trees { old, new, .. } => {
            path_diff(&prefix, path, ERR_PATH_NOT_IN_COMPARISON, |specs| {
                tree_diff(&repo, Some(&old), &new, &prefix, specs)
            })
        }
    };
    payload
}

// ---------------------------------------------------------------------------
// Reported operations
// ---------------------------------------------------------------------------

/// GTC-FR-YQVD: the `base_branch` of the stream that owns a local branch.
fn stream_base_of<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    name: &str,
    kind: &str,
) -> Option<String> {
    if kind != "local" {
        return None;
    }
    crate::streams::stream_on_branch(app, name.trim()).map(|s| s.base_branch)
}

/// GTC-FR-QVDE: the comparison of one branch, its start, its file count, and
/// every error. Read-only, so `DEBUG`.
pub fn branch_comparison_reported<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    root: &Path,
    name: &str,
    kind: &str,
) -> Result<BranchComparison, String> {
    let started = Instant::now();
    let stream_base = stream_base_of(app, name, kind);
    log_ok(
        app,
        buffer,
        LogLevel::Debug,
        LOCAL,
        MSG_COMPARE_STARTED,
        log_fields! {
            "branch" => name.trim(),
            "kind" => kind,
            "isStream" => stream_base.is_some(),
        },
    );
    let result = branch_comparison_for(root, name, kind, stream_base.as_deref());
    match &result {
        Ok(comparison) => log_ok(
            app,
            buffer,
            LogLevel::Debug,
            LOCAL,
            "branch comparison read",
            log_fields! {
                "branch" => name.trim(),
                "kind" => kind,
                "base" => comparison.base.clone(),
                "sameAsBase" => comparison.same_as_base,
                "files" => comparison.files.len(),
                "durationMs" => duration_ms(started),
            },
        ),
        Err(error) => log_failure(
            app,
            buffer,
            LOCAL,
            MSG_COMPARE_FILES_FAILED,
            error,
            log_fields! { "branch" => name.trim(), "kind" => kind },
        ),
    }
    result
}

/// GTC-FR-QVDE: one path's change against the base, as its shape and never its
/// text.
pub fn branch_compare_file_diff_reported<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    root: &Path,
    name: &str,
    kind: &str,
    path: &str,
) -> Result<DiffPayload, String> {
    let started = Instant::now();
    let stream_base = stream_base_of(app, name, kind);
    log_ok(
        app,
        buffer,
        LogLevel::Debug,
        LOCAL,
        MSG_COMPARE_DIFF_STARTED,
        log_fields! {
            "branch" => name.trim(),
            "kind" => kind,
            "path" => path,
            "isStream" => stream_base.is_some(),
        },
    );
    let result = branch_compare_file_diff_for(root, name, kind, path, stream_base.as_deref());
    match &result {
        Ok(payload) => log_ok(
            app,
            buffer,
            LogLevel::Debug,
            LOCAL,
            "branch comparison diff computed",
            log_fields! {
                "branch" => name.trim(),
                "kind" => kind,
                "path" => path,
                "hunks" => payload.hunks.len(),
                "isBinary" => payload.is_binary,
                "durationMs" => duration_ms(started),
            },
        ),
        Err(error) => log_failure(
            app,
            buffer,
            LOCAL,
            MSG_COMPARE_DIFF_FAILED,
            error,
            log_fields! { "branch" => name.trim(), "kind" => kind, "path" => path },
        ),
    }
    result
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// GTC-FR-YQVD / GTC-FR-MBBH: the paths one branch changed against its base.
#[tauri::command]
pub fn list_branch_compare_files(
    name: String,
    kind: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<BranchComparison, String> {
    branch_comparison_reported(&app, &logging::BUFFER, &project.require_root()?, &name, &kind)
}

/// GTC-FR-PYVV: one branch's change to one path against its base.
#[tauri::command]
pub fn get_branch_compare_file_diff(
    name: String,
    kind: String,
    path: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<DiffPayload, String> {
    branch_compare_file_diff_reported(
        &app,
        &logging::BUFFER,
        &project.require_root()?,
        &name,
        &kind,
        &path,
    )
}
