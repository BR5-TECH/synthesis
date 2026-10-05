//! The commit history of the active worktree and what one commit changed
//! (`../../specifications/core/GTC-git.md` GTC-FR-BQNM, GTC-FR-RFLW,
//! GTC-FR-YCEV, GTC-FR-PDSK).
//!
//! Every read here uses local objects only and writes nothing.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use git2::{BranchType, Commit, Delta, DiffOptions, Oid, Repository, Sort, Tree};
use serde::Serialize;
use tauri::State;

use crate::changes;
use crate::log_fields;
use crate::logging::{self, LogBuffer, LogLevel, LogSink};
use crate::project::ProjectState;

use super::*;

/// GTC-FR-BQNM: the most commits one history or one branch listing carries.
pub const COMMIT_LIMIT: usize = 100;

/// GTC-FR-YCEV / GTC-FR-PDSK: the id names no commit of the repository.
pub const ERR_UNKNOWN_COMMIT: &str = "unknown_commit";

/// GTC-FR-PDSK: the commit did not change the path.
pub const ERR_PATH_NOT_IN_COMMIT: &str = "path_not_in_commit";

const MSG_HISTORY_FAILED: &str = "commit history failed";
const MSG_COMMIT_FILES_FAILED: &str = "commit file listing failed";
const MSG_COMMIT_DIFF_FAILED: &str = "commit file diff failed";

// ---------------------------------------------------------------------------
// Wire shapes (GTC-FR-RFLW)
// ---------------------------------------------------------------------------

/// GTC-FR-RFLW: one commit as the log rail and the branch overlay render it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitSummary {
    pub id: String,
    pub short_id: String,
    pub author_name: String,
    pub author_email: String,
    /// Unix seconds, UTC.
    pub authored_at: i64,
    /// The first line of the message.
    pub subject: String,
    /// The whole message.
    pub message: String,
    /// Short names of the local and remote-tracking branches whose tip this is.
    pub refs: Vec<String>,
}

/// GTC-FR-BQNM: the active worktree's latest commits, newest first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitHistory {
    /// Absent when `HEAD` is detached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub is_detached: bool,
    /// Absent when the branch has no commit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_id: Option<String>,
    pub commits: Vec<CommitSummary>,
}

/// GTC-FR-YCEV: how a commit changed one path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitFileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
}

/// GTC-FR-YCEV: one path a commit changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitFile {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_path: Option<String>,
    pub status: CommitFileStatus,
    pub is_binary: bool,
}

// ---------------------------------------------------------------------------
// Shared readers (GTC-FR-RFLW)
// ---------------------------------------------------------------------------

/// GTC-FR-RFLW: the short names of every local and remote-tracking branch, by
/// the commit each one points at.
///
/// Local branches come first and each group is sorted, so the order of `refs`
/// does not depend on how libgit2 walks the reference store. A remote's symbolic
/// `HEAD` pointer is not a branch and is skipped.
pub(crate) fn ref_tips(repo: &Repository) -> HashMap<Oid, Vec<String>> {
    let mut tips: HashMap<Oid, Vec<String>> = HashMap::new();
    for branch_type in [BranchType::Local, BranchType::Remote] {
        let Ok(iter) = repo.branches(Some(branch_type)) else {
            continue;
        };
        let mut named: Vec<(String, Oid)> = Vec::new();
        for item in iter {
            let Ok((branch, _)) = item else { continue };
            let Ok(Some(name)) = branch.name() else {
                continue;
            };
            if branch_type == BranchType::Remote && name.ends_with("/HEAD") {
                continue;
            }
            let Ok(commit) = branch.get().peel_to_commit() else {
                continue;
            };
            named.push((name.to_string(), commit.id()));
        }
        named.sort();
        for (name, id) in named {
            tips.entry(id).or_default().push(name);
        }
    }
    tips
}

/// GTC-FR-RFLW: one commit, with the branches whose tip it is.
pub(crate) fn summarize(commit: &Commit<'_>, tips: &HashMap<Oid, Vec<String>>) -> CommitSummary {
    let author = commit.author();
    let id = commit.id();
    let message = String::from_utf8_lossy(commit.message_bytes()).into_owned();
    CommitSummary {
        id: id.to_string(),
        short_id: id.to_string().chars().take(7).collect(),
        author_name: String::from_utf8_lossy(author.name_bytes()).into_owned(),
        author_email: String::from_utf8_lossy(author.email_bytes()).into_owned(),
        authored_at: author.when().seconds(),
        subject: message.lines().next().unwrap_or("").to_string(),
        message,
        refs: tips.get(&id).cloned().unwrap_or_default(),
    }
}

/// GTC-FR-BQNM / GTC-FR-TGOI: at most [`COMMIT_LIMIT`] commits reachable from
/// `from`, newest first by commit time.
pub(crate) fn commits_from(repo: &Repository, from: Oid) -> Result<Vec<CommitSummary>, String> {
    let tips = ref_tips(repo);
    let mut walk = repo
        .revwalk()
        .map_err(|e| format!("failed to walk the history: {e}"))?;
    walk.set_sorting(Sort::TIME)
        .map_err(|e| format!("failed to walk the history: {e}"))?;
    walk.push(from)
        .map_err(|e| format!("failed to walk the history: {e}"))?;
    let mut commits = Vec::new();
    for id in walk.take(COMMIT_LIMIT) {
        let id = id.map_err(|e| format!("failed to walk the history: {e}"))?;
        let commit = repo
            .find_commit(id)
            .map_err(|e| format!("failed to read a commit: {e}"))?;
        commits.push(summarize(&commit, &tips));
    }
    Ok(commits)
}

// ---------------------------------------------------------------------------
// History (GTC-FR-BQNM)
// ---------------------------------------------------------------------------

/// GTC-FR-BQNM: the latest commits reachable from the active worktree's `HEAD`.
///
/// An unborn branch has no commit and returns an empty list under its own name.
pub fn commit_history_for(root: &Path) -> Result<CommitHistory, String> {
    let repo = changes::open_repo(root)?;
    let is_detached = repo.head_detached().unwrap_or(false);
    let branch = if is_detached {
        None
    } else {
        // The symbolic target names the branch even where it holds no commit.
        repo.find_reference("HEAD").ok().and_then(|head| {
            head.symbolic_target()
                .ok()
                .flatten()
                .and_then(|target| target.strip_prefix("refs/heads/"))
                .map(str::to_string)
        })
    };
    let head = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let Some(head) = head else {
        return Ok(CommitHistory {
            branch,
            is_detached,
            head_id: None,
            commits: Vec::new(),
        });
    };
    Ok(CommitHistory {
        branch,
        is_detached,
        head_id: Some(head.id().to_string()),
        commits: commits_from(&repo, head.id())?,
    })
}

// ---------------------------------------------------------------------------
// One commit's change (GTC-FR-YCEV / GTC-FR-PDSK)
// ---------------------------------------------------------------------------

/// The commit an id names, or `unknown_commit`.
fn find_commit<'r>(repo: &'r Repository, id: &str) -> Result<Commit<'r>, String> {
    Oid::from_str(id.trim())
        .ok()
        .and_then(|oid| repo.find_commit(oid).ok())
        .ok_or_else(|| ERR_UNKNOWN_COMMIT.to_string())
}

/// The commit's change against its first parent, or against the empty tree for
/// a root commit, with renames paired and scoped to the project.
fn commit_diff<'r>(
    repo: &'r Repository,
    commit: &Commit<'r>,
    prefix: &str,
    pathspecs: &[String],
) -> Result<git2::Diff<'r>, String> {
    let tree = commit
        .tree()
        .map_err(|e| format!("failed to read the commit tree: {e}"))?;
    let parent_tree = match commit.parent(0) {
        Ok(parent) => Some(
            parent
                .tree()
                .map_err(|e| format!("failed to read the parent tree: {e}"))?,
        ),
        Err(_) => None,
    };
    tree_diff(repo, parent_tree.as_ref(), &tree, prefix, pathspecs)
}

/// The change from `old` (the empty tree when `None`) to `new`, with renames
/// paired and scoped to the project, or to the literal `pathspecs` when given.
///
/// GTC-FR-YCEV and GTC-FR-MBBH read a change through this one diff, so a commit
/// and a branch comparison report a path in the same shape.
pub(crate) fn tree_diff<'r>(
    repo: &'r Repository,
    old: Option<&Tree<'r>>,
    new: &Tree<'r>,
    prefix: &str,
    pathspecs: &[String],
) -> Result<git2::Diff<'r>, String> {
    let mut opts = DiffOptions::new();
    opts.include_typechange(true).ignore_submodules(true);
    if pathspecs.is_empty() {
        changes::scope_to_project(&mut opts, prefix);
    } else {
        // A literal path, so a name holding `*` or `[` matches itself alone.
        opts.disable_pathspec_match(true);
        for spec in pathspecs {
            opts.pathspec(spec);
        }
    }
    let mut diff = repo
        .diff_tree_to_tree(old, Some(new), Some(&mut opts))
        .map_err(|e| format!("failed to diff the trees: {e}"))?;
    changes::find_renames(&mut diff)?;
    Ok(diff)
}

/// A repository-relative path as the project names it, or `None` outside the
/// project.
fn project_relative(prefix: &str, path: &str) -> Option<String> {
    if prefix.is_empty() {
        return Some(path.to_string());
    }
    path.strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('/'))
        .map(str::to_string)
}

fn status_of(delta: Delta) -> Option<CommitFileStatus> {
    match delta {
        Delta::Added => Some(CommitFileStatus::Added),
        Delta::Modified => Some(CommitFileStatus::Modified),
        Delta::Deleted => Some(CommitFileStatus::Deleted),
        Delta::Renamed => Some(CommitFileStatus::Renamed),
        Delta::Copied => Some(CommitFileStatus::Copied),
        Delta::Typechange => Some(CommitFileStatus::TypeChanged),
        _ => None,
    }
}

fn path_text(file: git2::DiffFile<'_>) -> Option<String> {
    file.path().map(|p| changes::to_forward(p))
}

/// GTC-FR-YCEV: the paths the commit changed against its first parent.
pub fn commit_files_for(root: &Path, commit_id: &str) -> Result<Vec<CommitFile>, String> {
    let repo = changes::open_repo(root)?;
    let prefix = changes::project_prefix(&repo, root);
    let commit = find_commit(&repo, commit_id)?;
    let diff = commit_diff(&repo, &commit, &prefix, &[])?;
    Ok(files_of(&diff, &prefix))
}

/// GTC-FR-YCEV: the project paths one diff changed, sorted by path, each with
/// its previous path when renamed or copied and its binary flag.
pub(crate) fn files_of(diff: &git2::Diff<'_>, prefix: &str) -> Vec<CommitFile> {
    let mut files = Vec::new();
    for index in 0..diff.deltas().len() {
        let Some(delta) = diff.get_delta(index) else {
            continue;
        };
        let Some(status) = status_of(delta.status()) else {
            continue;
        };
        let Some(new_path) = path_text(delta.new_file()).and_then(|p| project_relative(prefix, &p))
        else {
            continue;
        };
        let previous_path = match status {
            CommitFileStatus::Renamed | CommitFileStatus::Copied => {
                path_text(delta.old_file()).and_then(|p| project_relative(prefix, &p))
            }
            _ => None,
        };
        // The binary flag is set once the content has been read, which a patch
        // does. A delta that yields no patch holds no content to be binary.
        let is_binary = git2::Patch::from_diff(diff, index)
            .ok()
            .flatten()
            .map(|patch| patch.delta().flags().is_binary())
            .unwrap_or_else(|| delta.flags().is_binary());
        files.push(CommitFile {
            path: new_path,
            previous_path,
            status,
            is_binary,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

/// GTC-FR-PDSK: the commit's change to one path, in the shape `get_diff` uses.
pub fn commit_file_diff_for(
    root: &Path,
    commit_id: &str,
    path: &str,
) -> Result<DiffPayload, String> {
    let repo = changes::open_repo(root)?;
    let prefix = changes::project_prefix(&repo, root);
    let commit = find_commit(&repo, commit_id)?;
    path_diff(&prefix, path, ERR_PATH_NOT_IN_COMMIT, |specs| {
        commit_diff(&repo, &commit, &prefix, specs)
    })
}

/// GTC-FR-PDSK / GTC-FR-PYVV: one path's change, read from `diff_with`, which
/// returns the whole change for no pathspec and that path's change for some.
/// A path the whole change does not hold is `not_found`.
pub(crate) fn path_diff<'r>(
    prefix: &str,
    path: &str,
    not_found: &str,
    diff_with: impl Fn(&[String]) -> Result<git2::Diff<'r>, String>,
) -> Result<DiffPayload, String> {
    let wanted = repo_pathspec(prefix, path.trim());

    // The whole change first: a rename is only paired when both of its paths
    // are in view, so the path alone cannot be asked of Git directly.
    let whole = diff_with(&[])?;
    let mut specs: Option<Vec<String>> = None;
    for index in 0..whole.deltas().len() {
        let Some(delta) = whole.get_delta(index) else {
            continue;
        };
        if status_of(delta.status()).is_none() {
            continue;
        }
        if path_text(delta.new_file()).as_deref() != Some(wanted.as_str()) {
            continue;
        }
        let mut found = vec![wanted.clone()];
        if let Some(old) = path_text(delta.old_file()) {
            if old != wanted {
                found.push(old);
            }
        }
        specs = Some(found);
        break;
    }
    let specs = specs.ok_or_else(|| not_found.to_string())?;

    let diff = diff_with(&specs)?;
    format_diff(&diff)
}

// ---------------------------------------------------------------------------
// Reported operations
// ---------------------------------------------------------------------------

/// GTC-FR-BQNM: the history, counted rather than itemised. `DEBUG` for the
/// reason every read here is: the panel re-reads it whenever the branch moves.
pub fn commit_history_reported<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    root: &Path,
) -> Result<CommitHistory, String> {
    let started = Instant::now();
    let result = commit_history_for(root);
    match &result {
        Ok(history) => log_ok(
            sink,
            buffer,
            LogLevel::Debug,
            LOCAL,
            "commit history read",
            log_fields! {
                "commits" => history.commits.len(),
                "isDetached" => history.is_detached,
                "durationMs" => duration_ms(started),
            },
        ),
        Err(error) => log_failure(sink, buffer, LOCAL, MSG_HISTORY_FAILED, error, Fields::new()),
    }
    result
}

/// GTC-FR-YCEV: the files of one commit, counted. The commit id is not secret.
pub fn commit_files_reported<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    root: &Path,
    commit_id: &str,
) -> Result<Vec<CommitFile>, String> {
    let started = Instant::now();
    let result = commit_files_for(root, commit_id);
    match &result {
        Ok(files) => log_ok(
            sink,
            buffer,
            LogLevel::Debug,
            LOCAL,
            "commit files listed",
            log_fields! {
                "commit" => commit_id,
                "files" => files.len(),
                "durationMs" => duration_ms(started),
            },
        ),
        Err(error) => log_failure(
            sink,
            buffer,
            LOCAL,
            MSG_COMMIT_FILES_FAILED,
            error,
            log_fields! { "commit" => commit_id },
        ),
    }
    result
}

/// GTC-FR-PDSK: one commit's change to one path, as its shape and never its
/// text.
pub fn commit_file_diff_reported<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    root: &Path,
    commit_id: &str,
    path: &str,
) -> Result<DiffPayload, String> {
    let started = Instant::now();
    let result = commit_file_diff_for(root, commit_id, path);
    match &result {
        Ok(payload) => log_ok(
            sink,
            buffer,
            LogLevel::Debug,
            LOCAL,
            "commit file diff computed",
            log_fields! {
                "commit" => commit_id,
                "path" => path,
                "hunks" => payload.hunks.len(),
                "isBinary" => payload.is_binary,
                "durationMs" => duration_ms(started),
            },
        ),
        Err(error) => log_failure(
            sink,
            buffer,
            LOCAL,
            MSG_COMMIT_DIFF_FAILED,
            error,
            log_fields! { "commit" => commit_id, "path" => path },
        ),
    }
    result
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// GTC-FR-BQNM: at most the 100 latest commits reachable from the active
/// worktree's `HEAD`.
#[tauri::command]
pub fn list_commit_history(
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<CommitHistory, String> {
    commit_history_reported(&app, &logging::BUFFER, &project.require_root()?)
}

/// GTC-FR-YCEV: the paths one commit changed.
#[tauri::command]
pub fn list_commit_files(
    commit_id: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<Vec<CommitFile>, String> {
    commit_files_reported(&app, &logging::BUFFER, &project.require_root()?, &commit_id)
}

/// GTC-FR-PDSK: one commit's change to one path.
#[tauri::command]
pub fn get_commit_file_diff(
    commit_id: String,
    path: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<DiffPayload, String> {
    commit_file_diff_reported(
        &app,
        &logging::BUFFER,
        &project.require_root()?,
        &commit_id,
        &path,
    )
}
