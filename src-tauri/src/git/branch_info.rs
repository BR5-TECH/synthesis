//! What one branch is and holds (`../../specifications/core/GTC-git.md`
//! GTC-FR-TGOI).
//!
//! Read-only. The worktree that has the branch checked out is read from the
//! worktree enumeration at the moment of the call (WTC-FR-BHFE), and the work
//! stream that owns it from the stream store.

use std::path::Path;
use std::time::Instant;

use git2::BranchType;
use serde::Serialize;
use tauri::State;

use crate::changes::{self, ERR_UNKNOWN_BRANCH};
use crate::log_fields;
use crate::logging::{self, LogBuffer, LogLevel};
use crate::project::ProjectState;
use crate::worktree::WorktreeEntry;

use super::history::{commits_from, summarize, ref_tips, CommitSummary};
use super::*;

const MSG_BRANCH_INFO_FAILED: &str = "branch information failed";

/// GTC-FR-TGOI: the worktree that has a branch checked out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchWorktreeRef {
    pub path: String,
    pub name: String,
    pub is_active: bool,
    pub is_primary: bool,
}

impl From<&WorktreeEntry> for BranchWorktreeRef {
    fn from(entry: &WorktreeEntry) -> Self {
        BranchWorktreeRef {
            path: entry.path.clone(),
            name: entry.name.clone(),
            is_active: entry.is_active,
            is_primary: entry.is_primary,
        }
    }
}

/// GTC-FR-TGOI: the work stream whose branch it is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchStreamRef {
    pub stream_id: String,
    pub stream_name: String,
}

/// GTC-FR-TGOI: what one branch is and holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInformation {
    /// The local branch name, or the remote-tracking branch's short name.
    pub name: String,
    /// `"local"` or `"remote"`.
    pub kind: String,
    /// The short name of the upstream. Local branches only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    /// Checked out in the active worktree.
    pub is_current: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<BranchWorktreeRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<BranchStreamRef>,
    pub tip: CommitSummary,
    /// At most 100 commits reachable from the tip, newest first.
    pub commits: Vec<CommitSummary>,
}

/// GTC-FR-TGOI: the answer, from the facts the caller read fresh.
///
/// `worktrees` is the enumeration of WTC-FR-04 and `stream` is the owner the
/// stream store names for a local branch. A name that is not a branch of the
/// requested kind is `"unknown branch"`.
pub fn branch_information_from(
    root: &Path,
    name: &str,
    kind: &str,
    worktrees: &[WorktreeEntry],
    stream: Option<BranchStreamRef>,
) -> Result<BranchInformation, String> {
    let branch_type = match kind {
        "local" => BranchType::Local,
        "remote" => BranchType::Remote,
        _ => return Err(ERR_UNKNOWN_BRANCH.to_string()),
    };
    let name = name.trim();
    let repo = changes::open_repo(root)?;
    if name.is_empty() || (branch_type == BranchType::Remote && name.ends_with("/HEAD")) {
        return Err(ERR_UNKNOWN_BRANCH.to_string());
    }
    let branch = repo
        .find_branch(name, branch_type)
        .map_err(|_| ERR_UNKNOWN_BRANCH.to_string())?;
    let tip = branch
        .get()
        .peel_to_commit()
        .map_err(|_| ERR_UNKNOWN_BRANCH.to_string())?;
    let is_local = branch_type == BranchType::Local;

    let upstream = if is_local {
        branch
            .upstream()
            .ok()
            .and_then(|u| u.name().ok().flatten().map(str::to_string))
    } else {
        None
    };
    let holder = if is_local {
        worktrees.iter().find(|w| w.branch.as_deref() == Some(name))
    } else {
        None
    };
    Ok(BranchInformation {
        name: name.to_string(),
        kind: kind.to_string(),
        upstream,
        is_current: holder.is_some_and(|w| w.is_active),
        worktree: holder.map(BranchWorktreeRef::from),
        stream: if is_local { stream } else { None },
        tip: summarize(&tip, &ref_tips(&repo)),
        commits: commits_from(&repo, tip.id())?,
    })
}

/// GTC-FR-TGOI: the information of one branch, and what came of the read.
///
/// Read-only, so `DEBUG`; a refusal is a `WARN` through `is_refusal`.
pub fn branch_information_reported<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    root: &Path,
    name: &str,
    kind: &str,
) -> Result<BranchInformation, String> {
    let started = Instant::now();
    let result = (|| {
        let mut worktrees = crate::worktree::context_for(root)?.worktrees;
        crate::streams::decorate_worktrees(app, &mut worktrees);
        let stream = crate::streams::stream_on_branch(app, name.trim()).map(|s| BranchStreamRef {
            stream_id: s.id,
            stream_name: s.name,
        });
        branch_information_from(root, name, kind, &worktrees, stream)
    })();
    match &result {
        Ok(info) => log_ok(
            app,
            buffer,
            LogLevel::Debug,
            LOCAL,
            "branch information read",
            log_fields! {
                "branch" => name.trim(),
                "kind" => kind,
                "commits" => info.commits.len(),
                "hasWorktree" => info.worktree.is_some(),
                "isStream" => info.stream.is_some(),
                "durationMs" => duration_ms(started),
            },
        ),
        Err(error) => log_failure(
            app,
            buffer,
            LOCAL,
            MSG_BRANCH_INFO_FAILED,
            error,
            log_fields! { "branch" => name.trim(), "kind" => kind },
        ),
    }
    result
}

/// GTC-FR-TGOI: what one branch is and holds.
#[tauri::command]
pub fn get_branch_information(
    name: String,
    kind: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<BranchInformation, String> {
    branch_information_reported(&app, &logging::BUFFER, &project.require_root()?, &name, &kind)
}
