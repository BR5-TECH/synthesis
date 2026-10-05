//! Git operations for the UI Git surface (`specifications/core/GTC-git.md`).
//!
//! Home of `get_diff`, `list_branches`, the `checkout_branch` primitive, and the
//! `fetch_remote_branches` primitive.
//!
//! `get_diff`'s scope is a path, the staged set, or a branch comparison: a
//! path taken against the merge-base of the current branch and a named target
//! branch, resolved exactly as `crate::changes` resolves it (CHC-FR-04). The
//! branch-comparison scope is what the Diff tab of `../ui/CHG-changes.md`
//! renders (CHG-FR-18).
//!
//! `list_branches` (GTC-FR-07) is unconditional — describing a branch is not
//! offering to check it out, so it returns the same set whichever worktree is
//! active. It is the flat branch listing the Git panel renders,
//! with the active worktree's branch marked current and no worktree association
//! — that richer picture is `WTC-worktree-context.md`'s. `checkout_branch_at`
//! (GTC-FR-07 / GTC-FR-08) is the codebase's single checkout primitive: it is
//! not itself a command, because every checkout the UI can ask for has to
//! re-root the project afterwards, and that is `WTC-worktree-context.md`'s
//! `check_out_branch_in_active_worktree` composing this. Routing both the Git
//! panel and the worktree selector through one command is what makes them
//! alternative routes to one behaviour (`../ui/GIT-git.md` GIT-FR-06).
//!
//! `fetch_remote_branches` (GTC-FR-12 .. GTC-FR-15) is the codebase's single
//! fetch primitive, and is not a command either: `WTC-worktree-context.md`
//! WTC-FR-22 composes it, so the top-chrome refresh control reaches a remote
//! through one implementation that resolves the credential in one place. It
//! writes only remote-tracking refs — a fetch changes what is *known* about the
//! remote, never what is checked out.
//!
//! Everything here but `checkout_branch_at` and `fetch_remote_branches` is
//! read-only.


pub mod index_lock;

pub mod branch_compare;
pub mod branch_deletion;
pub(crate) mod branch_deletion_remote;
pub mod branch_info;
pub mod branches;
pub mod commit;
pub mod diff;
pub mod fetch;
pub mod history;
pub mod pull_requests;
pub mod push;
pub mod rollback;
pub mod status;
pub mod telemetry;

// Re-exported flat, so every import site keeps naming `crate::git::X`. Each
// item keeps the visibility it had before the split — the helpers that were
// private to this file are `pub(crate)`, so `dead_code` still reaches them.
pub use branch_compare::*;
pub use branch_deletion::*;
pub use branch_info::*;
pub use branches::*;
pub use commit::*;
pub use diff::*;
pub use fetch::*;
pub use history::*;
pub use pull_requests::*;
pub use push::*;
pub use rollback::*;
pub use status::*;
// Every record helper is this crate's alone; nothing outside reports git.
pub(crate) use telemetry::*;

use std::path::Path;
use std::time::Instant;

use tauri::{Emitter, Manager, State};

use crate::changes::{self};
use crate::github_tokens::GithubTokens;
use crate::global_settings::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, Fields, LogBuffer, LogLevel, LogSink};
use crate::progress::ProgressRegistry;
use crate::project::ProjectState;

// ---------------------------------------------------------------------------
// Reported operations
// ---------------------------------------------------------------------------
//
// Each command is a thin wrapper over one of these: the operation, plus the
// record that explains it. The split is what makes the records testable — the
// buffer is a parameter here, exactly as it is in `logging::log` itself, so a
// test drives an operation against a buffer of its own while the command passes
// the session's (the pattern `ai_api.rs` established).

/// GTC-FR-19 / GTC-FR-20: commit exactly the named paths, and say what came of
/// it.
pub fn commit_paths_reported<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    root: &crate::fs::RootFs,
    message: &str,
    paths: &[String],
    expected_worktree: Option<&str>,
) -> Result<CommitOutcome, String> {
    let started = Instant::now();
    let result = commit_named_paths(root, message, paths, expected_worktree);
    // How many paths were named, never which — a path is safe in a record, but a
    // commit routinely names hundreds and the buffer is a bounded ring. The
    // message is the author's own prose and appears nowhere.
    let fields = log_fields! { "pathCount" => paths.len(), "durationMs" => duration_ms(started) };
    match result {
        Ok(outcome) => {
            let mut fields = fields;
            fields.insert("commit".to_string(), serde_json::json!(outcome.commit_id));
            // How many the commit actually recorded, which a caller chasing "why
            // did that Diff tab not close" needs and cannot get from `pathCount`
            // — a rename records two, and a path that changed nothing records
            // none.
            fields.insert(
                "recordedCount".to_string(),
                serde_json::json!(outcome.committed_paths.len()),
            );
            log_ok(sink, buffer, LogLevel::Info, LOCAL, "commit created", fields);
            Ok(outcome)
        }
        Err(error) => {
            log_failure(sink, buffer, LOCAL, MSG_COMMIT_FAILED, &error, fields);
            Err(error)
        }
    }
}

/// GTC-FR-23 – GTC-FR-25: the rollback, and what it reached.
///
/// A rollback destroys uncommitted work on purpose, so it is the one operation
/// here whose *outcome* a reader is most likely to come looking for — "which of
/// the files I discarded actually went". The counts are what answer that; the
/// paths themselves are not recorded, a rollback routinely naming hundreds and
/// the session buffer being a bounded ring. A run in which any path failed is a
/// `WARN` rather than an `INFO`: the operation returned normally, but the user is
/// looking at a panel reporting failures and the record should say so.
pub fn rollback_paths_reported<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    root: &crate::fs::RootFs,
    paths: &[String],
) -> Result<RollbackOutcome, String> {
    let started = Instant::now();
    let result = rollback_named_paths(root, paths);
    let fields = log_fields! { "pathCount" => paths.len(), "durationMs" => duration_ms(started) };
    match result {
        Ok((outcome, index_error)) => {
            // The working tree is correct and the outcome describes it; the
            // index simply did not catch up, so the restored paths read as
            // staged until the next write. Nothing downstream can infer this
            // from the outcome, so it is recorded here or nowhere.
            if let Some(error) = &index_error {
                logging::log_error(
                    sink,
                    buffer,
                    LOCAL,
                    "rollback restored the working tree but could not write the index",
                    log_fields! { "error" => error.clone() },
                );
            }
            let restored: usize = outcome
                .entries
                .iter()
                .map(|e| e.restored_paths.len())
                .sum();
            let removed: usize = outcome.entries.iter().map(|e| e.removed_paths.len()).sum();
            let failed = outcome
                .entries
                .iter()
                .filter(|e| e.outcome == ROLLBACK_FAILED)
                .count();
            let mut fields = fields;
            fields.insert("restoredCount".to_string(), serde_json::json!(restored));
            fields.insert("removedCount".to_string(), serde_json::json!(removed));
            fields.insert("failedEntries".to_string(), serde_json::json!(failed));
            log_ok(
                sink,
                buffer,
                if failed > 0 {
                    LogLevel::Warn
                } else {
                    LogLevel::Info
                },
                LOCAL,
                if failed > 0 {
                    "rollback completed with failures"
                } else {
                    "rollback completed"
                },
                fields,
            );
            Ok(outcome)
        }
        Err(error) => {
            log_failure(sink, buffer, LOCAL, "rollback failed", &error, fields);
            Err(error)
        }
    }
}

/// GTC-FR-21: the current branch's standing against its upstream.
///
/// `DEBUG` for the reason every read here is: the Changes panel re-reads this
/// whenever the change set moves, and the session buffer is a bounded ring — a
/// routine read at `INFO` would evict the records explaining whatever the reader
/// is actually investigating.
pub fn upstream_sync_state_reported<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    root: &Path,
) -> Result<UpstreamSyncState, String> {
    let result = upstream_sync_state(root);
    match &result {
        Ok(state) => log_ok(
            sink,
            buffer,
            LogLevel::Debug,
            LOCAL,
            "upstream sync state read",
            log_fields! {
                "hasRemote" => state.has_remote,
                "hasUpstream" => state.has_upstream,
                "ahead" => state.ahead,
                "behind" => state.behind,
            },
        ),
        Err(error) => log_failure(
            sink,
            buffer,
            LOCAL,
            "failed to read the upstream sync state",
            error,
            Fields::new(),
        ),
    }
    result
}

/// GTC `get_diff(scope)`, and the shape of what it answered.
pub fn diff_reported<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    root: &Path,
    scope: &DiffScope,
) -> Result<DiffPayload, String> {
    let started = Instant::now();
    let result = diff_for_scope(root, scope);
    match &result {
        Ok(payload) => {
            let mut fields = scope_fields(scope);
            // The shape of the answer, not the answer: a hunk count and the
            // binary marker are what explain a Diff tab that rendered nothing,
            // while the diff text itself is the author's content.
            fields.insert("hunks".to_string(), serde_json::json!(payload.hunks.len()));
            fields.insert("isBinary".to_string(), serde_json::json!(payload.is_binary));
            fields.insert(
                "durationMs".to_string(),
                serde_json::json!(duration_ms(started)),
            );
            log_ok(sink, buffer, LogLevel::Debug, LOCAL, "diff computed", fields);
        }
        Err(error) => log_failure(
            sink,
            buffer,
            LOCAL,
            MSG_DIFF_FAILED,
            error,
            scope_fields(scope),
        ),
    }
    result
}

/// GTC-FR-16: both sides of the comparison for one file, and which of them the
/// comparison actually had.
pub fn file_revisions_reported<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    root: &crate::fs::RootFs,
    scope: &DiffScope,
) -> Result<FileRevisions, String> {
    let started = Instant::now();
    let result = file_revisions_for_scope(root, scope);
    match &result {
        Ok(revisions) => {
            let mut fields = scope_fields(scope);
            // Whether each side exists, and how much of it there is — the
            // distinction between an addition, a deletion, and a file that
            // exists and is empty (GTC-FR-16) is exactly what a reader chasing a
            // wrongly-rendered comparison needs. The text is never logged.
            fields.insert(
                "hasOld".to_string(),
                serde_json::json!(revisions.old.is_some()),
            );
            fields.insert(
                "hasNew".to_string(),
                serde_json::json!(revisions.new.is_some()),
            );
            fields.insert(
                "oldChars".to_string(),
                serde_json::json!(revisions.old.as_ref().map(String::len)),
            );
            fields.insert(
                "newChars".to_string(),
                serde_json::json!(revisions.new.as_ref().map(String::len)),
            );
            fields.insert(
                "isBinary".to_string(),
                serde_json::json!(revisions.is_binary),
            );
            fields.insert(
                "durationMs".to_string(),
                serde_json::json!(duration_ms(started)),
            );
            log_ok(
                sink,
                buffer,
                LogLevel::Debug,
                LOCAL,
                "file revisions read",
                fields,
            );
        }
        Err(error) => log_failure(
            sink,
            buffer,
            LOCAL,
            MSG_REVISIONS_FAILED,
            error,
            scope_fields(scope),
        ),
    }
    result
}

/// GTC-FR-07: the repository's branches, counted rather than itemised — the
/// listing runs on every render of the Git panel, and one record per branch
/// would cost the buffer far more than it tells a reader.
pub fn branches_reported<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    root: &Path,
) -> Result<Vec<GitBranch>, String> {
    let result = branches_for(root);
    match &result {
        Ok(branches) => log_ok(
            sink,
            buffer,
            LogLevel::Debug,
            LOCAL,
            "branches listed",
            log_fields! {
                "local" => branches.iter().filter(|b| b.kind == "local").count(),
                "remote" => branches.iter().filter(|b| b.kind == "remote").count(),
                "current" => branches.iter().find(|b| b.is_current).map(|b| b.name.as_str()).unwrap_or("none"),
            },
        ),
        Err(error) => log_failure(
            sink,
            buffer,
            LOCAL,
            MSG_LISTING_FAILED,
            error,
            Fields::new(),
        ),
    }
    result
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// GTC-FR-19 / GTC-FR-20: commit exactly the named paths. The commit the
/// application makes, invoked by `../ui/CMW-commit-message.md`.
#[tauri::command]
pub fn commit_paths(
    message: String,
    paths: Vec<String>,
    expected_worktree: Option<String>,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<CommitOutcome, String> {
    let root = project.require_root()?;
    let outcome = commit_paths_reported(
        &app,
        &logging::BUFFER,
        &root,
        &message,
        &paths,
        expected_worktree.as_deref(),
    )?;
    // CHC-FR-16: the change set the commit just emptied is reloaded by every
    // surface that follows the event (`../ui/CHG-changes.md` CHG-FR-41). Emitted
    // here rather than left to the watcher, which does not watch `.git`.
    if let Err(e) = app.emit(
        changes::CHANGES_UPDATED,
        changes::ChangesUpdatedPayload {
            change_count: paths.len(),
        },
    ) {
        // The commit is made; an undelivered announcement must not undo it. It
        // is still worth a record: every surface following the event is now
        // showing a change set the commit already emptied, and nothing else
        // explains why.
        logging::log_warn(
            &app,
            &logging::BUFFER,
            LOCAL,
            "commit made but the changes-updated event was not delivered",
            log_fields! { "event" => changes::CHANGES_UPDATED, "error" => e.to_string() },
        );
    }
    Ok(outcome)
}

/// GTC-FR-29 … GTC-FR-32: the active worktree's complete uncommitted state.
///
/// No UI consumer: the change set the Changes panel renders is `crate::changes`'
/// classified one, and what reads this is `GRD-graduation.md`'s start preflight
/// (GSU-FR-MLEJ), which needs the whole of what Git reports rather than a
/// classified and filterable change set.
#[tauri::command]
pub fn get_working_tree_status(
    expected_worktree: Option<String>,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<WorkingTreeStatus, String> {
    let root = project.require_root()?;
    working_tree_status_reported(
        &app,
        &logging::BUFFER,
        &root,
        expected_worktree.as_deref(),
    )
}

/// GTC-FR-23: return the named paths to `HEAD`, per path rather than atomically.
#[tauri::command]
pub fn rollback_paths(
    paths: Vec<String>,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<RollbackOutcome, String> {
    let root = project.require_root()?;
    // GTC-FR-28: this operation emits **nothing of its own**. A rollback writes
    // the working tree and the index, and `CHC-changes.md` CHC-FR-16 already
    // watches both — `changes::is_git_meta_rel` names `index`, `HEAD` and
    // `refs/`, and the scanner's content channel covers the working-tree half —
    // so the ordinary debounced `"changes updated"` follows on its own. Emitting
    // a second, un-debounced event here would race the caller's own reload
    // (CHG-FR-64) rather than coalescing with it.
    rollback_paths_reported(&app, &logging::BUFFER, &root, &paths)
}

/// GTC-FR-21: the current branch's standing against its upstream, from local
/// refs alone.
#[tauri::command]
pub fn get_upstream_sync_state(
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<UpstreamSyncState, String> {
    upstream_sync_state_reported(&app, &logging::BUFFER, &project.require_root()?)
}

/// GTC-FR-04 / GTC-FR-22: push the current branch, streaming its output.
#[tauri::command]
pub async fn push_current_branch(
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<(), String> {
    let root = project.require_root()?;
    let project_key = project.slot_key();

    // The managed state is resolved inside the closure rather than taken as
    // parameters: a `State<'_, T>` borrows the invoke context and cannot cross
    // onto another thread, while the `AppHandle` can and reaches the same values.
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<GlobalSettingsStore>();
        let tokens = app.state::<GithubTokens>();
        let registry = app.state::<ProgressRegistry>();
        let emit = |name: &str, payload: serde_json::Value| {
            // An event that cannot be delivered is dropped rather than
            // propagated: reporting must never take down the transfer.
            let _ = app.emit(name, payload);
        };
        push_branch_at(
            &app,
            &logging::BUFFER,
            &emit,
            &registry,
            &root,
            &store,
            &tokens,
            &project_key,
        )
    })
    .await
    .map_err(|e| format!("push failed: {e}"))?
}

#[tauri::command]
pub fn get_diff(
    scope: DiffScope,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<DiffPayload, String> {
    diff_reported(&app, &logging::BUFFER, &project.require_root()?, &scope)
}

/// GTC-FR-16: both sides of the comparison for one file, whole. What the
/// side-by-side, final, and rich modes of `../ui/DFV-diff-viewer.md` render
/// (DFV-FR-25).
#[tauri::command]
pub fn get_file_revisions(
    scope: DiffScope,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<FileRevisions, String> {
    file_revisions_reported(&app, &logging::BUFFER, &project.require_root()?, &scope)
}

#[tauri::command]
pub fn list_branches(
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<Vec<GitBranch>, String> {
    branches_reported(&app, &logging::BUFFER, &project.require_root()?)
}

#[cfg(test)]
mod tests;
