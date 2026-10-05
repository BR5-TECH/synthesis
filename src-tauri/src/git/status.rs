//! Working-tree status (GTC-FR-29 .. GTC-FR-32) and the upstream sync
//! state (GTC-FR-21).

use std::path::Path;
use std::time::Instant;

use git2::BranchType;
use serde::{Deserialize, Serialize};

use crate::changes::{self};
use crate::log_fields;
use crate::logging::{LogBuffer, LogLevel, LogSink};

use super::*;

// ---------------------------------------------------------------------------
// Working-tree status (GTC-FR-29 … GTC-FR-32)
// ---------------------------------------------------------------------------

/// The `expected_worktree` a caller carried is not the project's active
/// worktree (GTC-FR-31). Carries the expected path and the active one.
pub const ERR_WORKTREE_IDENTITY_CHANGED: &str = "worktree_identity_changed";

/// What one side of the index holds for a path (GTC-FR-30).
///
/// Untracked is an unstaged-side value alone: a path Git has never been told
/// about has nothing in the index to describe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    TypeChanged,
    Untracked,
}

/// One changed path, with **what Git reports on each side of the index kept
/// apart** (GTC-FR-30).
///
/// Neither status stands in for the other and neither is collapsed into one
/// overall state: a path staged and then changed again carries both, which is
/// the combined state Git reports as two letters and which neither side alone
/// describes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkingTreeEntry {
    /// Project-relative, at the path's current location.
    pub path: String,
    /// What the index holds against `HEAD`, or null where it holds nothing.
    pub staged_status: Option<PathStatus>,
    /// What the working tree holds against the index, or null where they agree.
    pub unstaged_status: Option<PathStatus>,
    /// The pre-rename path; renamed entries only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_path: Option<String>,
}

/// The active worktree's complete uncommitted state (GTC-FR-29).
///
/// `worktree_path` travels with the entries so a caller that must act on the
/// same checkout it read can compare the two rather than assume they agree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkingTreeStatus {
    /// Absolute path of the worktree the status describes.
    pub worktree_path: String,
    pub entries: Vec<WorkingTreeEntry>,
}

/// GTC-FR-31: the typed refusal, carrying both paths as data rather than as
/// prose — what the author reads is the calling surface's to compose.
pub fn worktree_identity_error(expected: &str, active: &str) -> String {
    format!(
        "{ERR_WORKTREE_IDENTITY_CHANGED}: {}",
        serde_json::json!({ "expected": expected, "active": active })
    )
}

/// Whether `error` is the identity refusal, whatever pair of paths it carries.
pub fn is_worktree_identity_changed(error: &str) -> bool {
    error.starts_with(ERR_WORKTREE_IDENTITY_CHANGED)
}

/// GTC-FR-31: bind an operation to one checkout **before it reads or writes
/// anything**, without resolving anything a caller that supplied no expectation
/// does not need.
///
/// The short circuit matters: `commit_paths` is served without an expectation
/// everywhere but the graduation preflight, and resolving the worktree for it
/// would put a repository discovery — and, outside a repository, a "not a git
/// repository" refusal — ahead of the message and path checks GTC-FR-20 orders.
pub fn require_worktree_identity(
    active_root: &Path,
    expected: Option<&str>,
) -> Result<(), String> {
    if expected.is_none() {
        return Ok(());
    }
    resolve_worktree_identity(active_root, expected).map(|_| ())
}

/// GTC-FR-31: the same guard, reporting the active worktree's path either way —
/// which is what `get_working_tree_status` returns beside its entries
/// (GTC-FR-29).
pub fn resolve_worktree_identity(
    active_root: &Path,
    expected: Option<&str>,
) -> Result<String, String> {
    let active = crate::worktree::active_entry_for(active_root)?.path;
    let Some(expected) = expected else {
        return Ok(active);
    };
    if changes::canonicalize_lenient(Path::new(expected))
        == changes::canonicalize_lenient(Path::new(&active))
    {
        Ok(active)
    } else {
        Err(worktree_identity_error(expected, &active))
    }
}

/// Read the staged side of a status flag set (GTC-FR-30).
pub(crate) fn staged_status_of(status: git2::Status) -> Option<PathStatus> {
    if status.contains(git2::Status::INDEX_NEW) {
        Some(PathStatus::Added)
    } else if status.contains(git2::Status::INDEX_DELETED) {
        Some(PathStatus::Deleted)
    } else if status.contains(git2::Status::INDEX_RENAMED) {
        Some(PathStatus::Renamed)
    } else if status.contains(git2::Status::INDEX_TYPECHANGE) {
        Some(PathStatus::TypeChanged)
    } else if status.contains(git2::Status::INDEX_MODIFIED) {
        Some(PathStatus::Modified)
    } else {
        None
    }
}

/// Read the unstaged side of a status flag set (GTC-FR-30).
///
/// A path with an unresolved conflict reads as a working-tree modification, the
/// reading `crate::changes` gives it too (`Delta::Conflicted` at `map_status`).
/// libgit2 reports such a path as `CONFLICTED` **alone**, with no `WT_` bit
/// beside it, so without this a worktree stopped mid-merge would report the
/// conflict nowhere — and `GRD-graduation.md`'s preflight, which refuses on a
/// non-empty set, would find that worktree clean and graduate it (GSU-FR-MLEJ).
pub(crate) fn unstaged_status_of(status: git2::Status) -> Option<PathStatus> {
    if status.contains(git2::Status::CONFLICTED) {
        Some(PathStatus::Modified)
    } else if status.contains(git2::Status::WT_NEW) {
        Some(PathStatus::Untracked)
    } else if status.contains(git2::Status::WT_DELETED) {
        Some(PathStatus::Deleted)
    } else if status.contains(git2::Status::WT_RENAMED) {
        Some(PathStatus::Renamed)
    } else if status.contains(git2::Status::WT_TYPECHANGE) {
        Some(PathStatus::TypeChanged)
    } else if status.contains(git2::Status::WT_MODIFIED) {
        Some(PathStatus::Modified)
    } else {
        None
    }
}

/// GTC-FR-29 / GTC-FR-30 / GTC-FR-32: the single working-tree status primitive.
///
/// Complete and unclassified: every path Git reports for the active worktree is
/// present, the only omission being a path the repository's ignore rules
/// exclude. It resolves no artifact type, applies no lens, and drops no path for
/// any other reason — which is what makes it the primitive a precondition is
/// read from rather than a surface's change set (`GRD-graduation.md` GSU-FR-MLEJ).
///
/// Read-only under GTC-FR-06: no ref, no index entry, and no working-tree file
/// is written.
pub fn working_tree_status(root: &crate::fs::RootFs) -> Result<Vec<WorkingTreeEntry>, String> {
    let repo = changes::open_repo(root)?;
    let prefix = changes::project_prefix(&repo, root);

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .include_unmodified(false)
        .exclude_submodules(false)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);
    let statuses = repo
        .statuses(Some(&mut opts))
        .map_err(|e| format!("failed to read the working tree status: {e}"))?;

    let mut entries: Vec<WorkingTreeEntry> = Vec::new();
    for entry in statuses.iter() {
        let flags = entry.status();
        let staged_status = staged_status_of(flags);
        let unstaged_status = unstaged_status_of(flags);
        // GTC-FR-30: at least one of the two is non-null. A flag set that
        // describes neither side is not a change this reports.
        if staged_status.is_none() && unstaged_status.is_none() {
            continue;
        }
        // The **current** location, which is each delta's *new* file. A status
        // entry's own `path()` is its old side, so a renamed path reported
        // through it would be named where it no longer is — the one place this
        // set is read, the author is deciding what to commit and needs the
        // location on disk.
        let head_to_index = entry.head_to_index();
        let index_to_workdir = entry.index_to_workdir();
        let path = index_to_workdir
            .as_ref()
            .and_then(|d| d.new_file().path())
            .map(changes::to_forward)
            .or_else(|| {
                head_to_index
                    .as_ref()
                    .and_then(|d| d.new_file().path())
                    .map(changes::to_forward)
            })
            .or_else(|| entry.path().ok().map(str::to_string));
        // A path libgit2 cannot report as UTF-8 on either side names a file no
        // surface downstream could render, let alone commit.
        let Some(path) = path else { continue };
        // A rename carries its pre-rename path beside whichever side reports it
        // (GTC-FR-30). The index side is read first: where both sides report one,
        // the committed location is the one the author recognises.
        let previous = [&head_to_index, &index_to_workdir]
            .into_iter()
            .flatten()
            .filter(|delta| {
                matches!(delta.status(), git2::Delta::Renamed | git2::Delta::Copied)
            })
            .find_map(|delta| delta.old_file().path().map(changes::to_forward))
            .filter(|old| *old != path);

        // Repository-relative to project-relative. A path outside the project's
        // content root names a file no surface downstream could act on, so it is
        // dropped rather than reported (the same rule `crate::changes` applies).
        let Some(path) = changes::to_project_rel(&prefix, &path) else {
            continue;
        };
        let previous_path = previous
            .as_deref()
            .and_then(|old| changes::to_project_rel(&prefix, old));
        entries.push(WorkingTreeEntry {
            path,
            staged_status,
            unstaged_status,
            previous_path,
        });
    }
    // Deterministic, so a surface rendering the set twice renders it the same
    // way twice. Git's own status order is not one the author can predict.
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

/// GTC-FR-29 / GTC-FR-31: the status, bound to one checkout, with a record of
/// what it read.
pub fn working_tree_status_reported<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    root: &crate::fs::RootFs,
    expected_worktree: Option<&str>,
) -> Result<WorkingTreeStatus, String> {
    let started = Instant::now();
    let result = (|| -> Result<WorkingTreeStatus, String> {
        // GTC-FR-31: ahead of the read, so a refusal has read no status.
        let worktree_path = resolve_worktree_identity(root.path(), expected_worktree)?;
        let entries = working_tree_status(root)?;
        Ok(WorkingTreeStatus {
            worktree_path,
            entries,
        })
    })();
    // How many paths, never which: a working tree routinely holds hundreds and
    // the session buffer is a bounded ring. No path and no file content appears.
    match result {
        Ok(status) => {
            let fields = log_fields! {
                "entryCount" => status.entries.len(),
                "durationMs" => duration_ms(started),
            };
            log_ok(
                sink,
                buffer,
                LogLevel::Debug,
                LOCAL,
                "working tree status read",
                fields,
            );
            Ok(status)
        }
        Err(error) => {
            let fields = log_fields! { "durationMs" => duration_ms(started) };
            log_failure(
                sink,
                buffer,
                LOCAL,
                "working tree status could not be read",
                &error,
                fields,
            );
            Err(error)
        }
    }
}

// ---------------------------------------------------------------------------
// Upstream sync state (GTC-FR-21)
// ---------------------------------------------------------------------------

/// The current branch's standing against its upstream (GTC-FR-21).
///
/// Resolved from local refs alone — no network — so it describes the remote as
/// the last fetch left it. `ahead` / `behind` are `None` rather than `0` when
/// there is no upstream to count against, because "level with the remote" and
/// "never published" are the opposite answers to whether a push is offered
/// (`../ui/CHG-changes.md` CHG-FR-37).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamSyncState {
    pub has_remote: bool,
    pub has_upstream: bool,
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
}

/// GTC-FR-21: read the active worktree's branch against its upstream. Makes no
/// network request and writes nothing.
pub fn upstream_sync_state(root: &Path) -> Result<UpstreamSyncState, String> {
    let repo = changes::open_repo(root)?;
    let has_remote = changes::primary_remote_name(&repo).is_some();
    let mut state = UpstreamSyncState {
        has_remote,
        ..UpstreamSyncState::default()
    };

    // A detached HEAD tracks nothing, and neither does an unborn branch.
    let Ok(head) = repo.head() else {
        return Ok(state);
    };
    if repo.head_detached().unwrap_or(false) {
        return Ok(state);
    }
    let Ok(name) = head.shorthand() else {
        return Ok(state);
    };
    let Ok(branch) = repo.find_branch(name, BranchType::Local) else {
        return Ok(state);
    };
    let Ok(upstream) = branch.upstream() else {
        // A remote is configured but this branch has never been published, so a
        // push would publish it.
        return Ok(state);
    };
    let (Some(local_oid), Some(upstream_oid)) = (
        branch.get().target(),
        upstream.get().target(),
    ) else {
        return Ok(state);
    };
    let (ahead, behind) = repo
        .graph_ahead_behind(local_oid, upstream_oid)
        .map_err(|e| format!("failed to compare with the upstream: {e}"))?;
    state.has_upstream = true;
    state.ahead = Some(ahead);
    state.behind = Some(behind);
    Ok(state)
}
