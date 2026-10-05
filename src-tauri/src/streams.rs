//! Work streams: the branches and working copies graduation runs work in
//! (`WKS-work-streams.md`).
//!
//! A work stream is a named branch and linked worktree the application owns and
//! that lives longer than one run. Runs execute in the stream's working copy and
//! commit onto its branch, so each run starts from the commit the run before it
//! made. This module creates streams, enumerates them, merges one back into the
//! branch it came from, and removes it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fs as fsa;

mod artifact;
mod attempts;
mod conflict_types;
mod git;
mod lookup;
mod merge;
mod merge_apply;
mod merge_handoff;
mod merge_progress;
mod merge_snapshot;
mod semantic;
mod semantic_turn;
mod state;
mod store;
mod update_commands;
mod update_git;
mod update_job;
mod update_progress;
mod update_record;
mod updating;


pub use artifact::MergeConflict;
pub use conflict_types::{StreamMergeConflict, StreamMergeDecision};
#[cfg(test)]
pub(crate) use semantic_turn::merge_input_field_names;
pub use commands::*;
pub use lookup::{stream_at_path, stream_on_branch};
pub use merge_apply::{apply_merge_run, merge_tips_moved, MergeApplied, MergeApplyFailure};
pub use merge_handoff::*;
pub(crate) use merge_snapshot::{has_ref as has_snapshot_ref, release_ref as release_snapshot_ref};
pub use state::{Reconciliation, StreamState};
pub use store::StreamStore;
pub use update_commands::*;
pub use update_record::{
    StreamUpdateCommit, StreamUpdateOutcome, StreamUpdateRecord, StreamUpdateState,
    StreamUpdateStrategy,
};

mod commands;

// ---------------------------------------------------------------------------
// Typed errors (WKS contract surface)
// ---------------------------------------------------------------------------

pub const ERR_NOT_A_GIT_REPOSITORY: &str = "not_a_git_repository";
pub const ERR_BASE_BRANCH_REQUIRED: &str = "base_branch_required";
pub const ERR_STREAM_NAME_TAKEN: &str = "stream_name_taken";
pub const ERR_STREAM_NAME_INVALID: &str = "stream_name_invalid";
pub const ERR_STREAM_CREATION_FAILED: &str = "stream_creation_failed";
pub const ERR_STREAM_CLEANUP_FAILED: &str = "stream_cleanup_failed";
pub const ERR_UNKNOWN_STREAM: &str = "unknown_stream";
pub const ERR_STREAM_BUSY: &str = "stream_busy";
pub const ERR_STREAM_DIRTY: &str = "stream_dirty";
/// The base branch's own worktree holds uncommitted work (`GRB-graduation-rebase.md` GRB-FR-QIHE).
pub const ERR_BASE_DIRTY: &str = "base_dirty";
/// A stream whose working copy has gone (WKS-FR-AXRD).
pub const ERR_STREAM_MISSING: &str = "stream_missing";
/// An uncommitted merge was asked for while no worktree holds the base branch.
pub const ERR_BASE_NOT_CHECKED_OUT: &str = "base_not_checked_out";
pub const ERR_STREAM_UNMERGED: &str = "stream_unmerged";
pub const ERR_STREAM_HAS_RUNS: &str = "stream_has_runs";
/// WKS-FR-OVLQ: the stream's working copy is the project's active worktree.
pub const ERR_STREAM_ACTIVE: &str = "stream_active";
/// GRB-FR-JIRD: one merge of one stream runs at a time.
pub const ERR_MERGE_IN_PROGRESS: &str = "merge_in_progress";
/// A conflict a text turn cannot reconcile: a symbolic link or a submodule, or a
/// path that is a file on one side and a directory on the other. The merge is
/// refused at the check, and nothing is written.
pub const ERR_UNSUPPORTED_CONFLICT: &str = "unsupported_conflict";
/// A merge run's pinned tip is not the tip of its branch any more. The run does
/// not resume and does not apply, and nothing was written.
pub const ERR_MERGE_BRANCH_MOVED: &str = "merge_branch_moved";
/// GRB-FR-YPEX: the bundle a semantic turn reads could not be written.
pub const ERR_ARTIFACT_GENERATION_FAILED: &str = "artifact_generation_failed";
/// WKS-FR-KFVJ: the recorded base branch no longer points at the revision the
/// author's confirmation pinned. Nothing was written.
pub const ERR_STALE_BASE_REVISION: &str = "stale_base_revision";
/// GRB-FR-CLRO: three semantic turns of one update settled nothing.
pub const ERR_UPDATE_ATTEMPTS_EXHAUSTED: &str = "update_attempts_exhausted";
/// WKS-FR-TSOA: an update of this repository already holds the update guard.
pub const ERR_UPDATE_IN_PROGRESS: &str = "update_in_progress";
/// GRB-FR-TXVL: the author stopped the update. Neither branch moved.
pub const ERR_UPDATE_CANCELLED: &str = "update_cancelled";
/// WKS-FR-QFTH: the application stopped while this update was running.
pub const ERR_UPDATE_INTERRUPTED: &str = "update_interrupted";
/// WKS-FR-DPNM / WKS-FR-CBXW: the update does not rest where the request needs
/// it to. An escalated update is answered or cleared, never retried.
pub const ERR_UPDATE_STATE_NOT_PERMITTED: &str = "update_state_not_permitted";
pub const ERR_NO_PROJECT_OPEN: &str = "no project open";

/// WKS-FR-WULF: tell every reader of the project's streams to reload.
///
/// Called by the graduation module when a merge run changes state, because a
/// stream's row names its merge run.
pub fn announce_changed<R: tauri::Runtime>(app: &tauri::AppHandle<R>, project_key: &str) {
    commands::announce(app, project_key);
}

/// The event every consumer reloads its own listing on (WKS-FR-WULF).
pub const WORK_STREAMS_CHANGED: &str = "work-streams-changed";

/// The event a running update reports itself through (WKS-FR-FQLS).
pub const WORK_STREAM_UPDATE_PROGRESS: &str = "work-stream-update-progress";

/// WKS-FR-KDXF: the branch every stream this application makes is named under.
pub const STREAM_BRANCH_PREFIX: &str = "synthesis/stream/";

/// WKS-FR-KDXF: how much of a name reaches the branch.
const SLUG_LIMIT: usize = 48;

// ---------------------------------------------------------------------------
// Record shapes (WKS contract surface)
// ---------------------------------------------------------------------------

/// WKS-FR-QMTV: what one stream is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkStream {
    pub id: String,
    /// WKS-FR-SGCM: the project this stream belongs to, so one machine's store
    /// can hold the streams of every project without one project seeing
    /// another's.
    #[serde(default)]
    pub project_key: String,
    /// The author's own name for the stream.
    pub name: String,
    /// `synthesis/stream/<slug>`.
    pub branch: String,
    /// Absolute, under `short_data_dir()/w/<id>/`.
    pub worktree_path: String,
    /// The branch the stream was created from and merges back into.
    pub base_branch: String,
    /// That branch's revision when the stream was created.
    pub base_revision: String,
    pub created_at: String,
    /// WKS-FR-JQJA: the run holding the stream, or `None`. Durable.
    #[serde(default)]
    pub busy_run_id: Option<String>,
    /// WKS-FR-AXRD: Git lists it but its directory has gone.
    #[serde(default, skip_deserializing)]
    pub is_missing: bool,
}

/// WKS-FR-SGCM: one stream as a listing reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkStreamSummary {
    pub stream: WorkStream,
    /// Runs assigned to this stream that are not terminal.
    pub queued_run_count: u32,
    /// Commits the base branch does not hold.
    pub ahead_of_base: u32,
    /// WKS-FR-RJPD: commits the recorded base branch holds and this stream does
    /// not. Zero means the stream is up to date with its base.
    pub behind_base: u32,
    /// WKS-FR-KFVJ: the base branch's tip when this listing was read, which is
    /// the revision an Update confirmation pins. Empty where the branch names
    /// no commit.
    pub base_tip_revision: String,
    /// WKS-FR-UBGX: the first of the commits counted by `behind_base`, newest
    /// first, cut to the bound a listing carries.
    pub missing_commits: Vec<StreamUpdateCommit>,
    /// WKS-FR-KHJS: the stream's newest merge run that is neither discarded nor
    /// archived, or nothing.
    ///
    /// Read from the project's run order index alone, so a listing opens no run
    /// record (WKS-FR-YBST).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_run: Option<StreamMergeRunLink>,
    /// WKS-FR-SGCM: what this stream's last update did, or nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<StreamUpdateRecord>,
}

/// WKS-FR-KHJS: what a stream's row knows of its merge run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamMergeRunLink {
    pub run_id: String,
    /// `Merge <stream name>`.
    pub name: String,
    pub state: crate::graduation::GraduationRunState,
}

/// WKS-FR-GKPX: how a merge lands on the base branch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StreamMergePublication {
    /// Apply into the base worktree and stop.
    Uncommitted,
    /// Apply and create one commit holding it.
    Commit { message: String },
}

/// WKS-FR-GKPX: what the request of a merge settled.
///
/// A clean merge and a stream that holds nothing its base does not finish in the
/// request and leave no record. A merge Git cannot settle names the run it was
/// handed to, which is the merge's only durable record (WKS-FR-VQDE).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamMergeResult {
    /// The stream holds nothing the base branch does not. Nothing was written.
    NothingToMerge,
    /// Git settled every path and the base branch holds the stream's work.
    Merged {
        /// Project-relative paths the merge wrote.
        #[serde(rename = "mergedPaths")]
        merged_paths: Vec<String>,
        /// The merge commit, where the publication made one.
        commit: Option<String>,
    },
    /// Git could not settle these paths. Both branches and both working copies
    /// are as they were, and a merge run carries the merge on.
    Conflicted {
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "conflictedPaths")]
        conflicted_paths: Vec<String>,
    },
}

/// GRB-FR-YRHM: how many semantic turns one stream update request may spend.
///
/// The author's own retry grants three more, an update being performed at the
/// author's request and at no other moment (GRB-FR-BQNF).
pub const UPDATE_ATTEMPTS_MAX: u32 = 3;

// ---------------------------------------------------------------------------
// Naming (WKS-FR-KDXF, WKS-FR-HRUZ)
// ---------------------------------------------------------------------------

/// WKS-FR-KDXF: a name reduced to path-safe characters and cut to 48.
///
/// Every character that is not a letter, a digit, a hyphen or an underscore
/// becomes a hyphen; runs of hyphens collapse; leading and trailing hyphens go.
/// The cut is applied last, so a long name and its prefix do not collide by
/// accident of where a separator fell.
pub fn slug_of(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_was_sep = true;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch.to_ascii_lowercase());
            last_was_sep = false;
        } else if !last_was_sep {
            out.push('-');
            last_was_sep = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.len() > SLUG_LIMIT {
        out.truncate(SLUG_LIMIT);
        while out.ends_with('-') {
            out.pop();
        }
    }
    out
}

/// WKS-FR-KDXF: the branch a stream of this name owns.
pub fn branch_name_for(name: &str) -> String {
    format!("{STREAM_BRANCH_PREFIX}{}", slug_of(name))
}

/// WKS-FR-MFDW: whether a branch is a work stream's.
///
/// A stream's branch is listed like any other; this tells a reader which of the
/// listed branches is a stream so it can be labelled as one.
pub fn is_stream_branch(name: &str) -> bool {
    name.starts_with(STREAM_BRANCH_PREFIX)
}

impl WorkStream {
    /// WKS-FR-CYAG: a run holds this stream.
    pub fn is_busy(&self) -> bool {
        self.busy_run_id.is_some()
    }

    /// The stream's working copy as a path.
    pub fn worktree(&self) -> PathBuf {
        PathBuf::from(&self.worktree_path)
    }
}

/// WKS-FR-AXRD: a stream Git still knows whose directory has gone.
pub(crate) fn mark_missing(stream: &mut WorkStream) {
    stream.is_missing = !Path::new(&stream.worktree_path).is_dir();
}

/// WTC-FR-QKZD: the stream each worktree belongs to, filled in on a listing.
///
/// The enumeration in `worktree.rs` answers for a repository and holds no store,
/// so the stream facts are added here, where the store is reachable.
pub fn decorate_worktrees<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    entries: &mut [crate::worktree::WorktreeEntry],
) {
    let Some(store) = store_for(app) else { return };
    let Some(fs) = access_for(app) else { return };
    let key = project_key_of(app);
    for stream in store::list_streams(&fs, &store, &key) {
        let target = Path::new(&stream.worktree_path);
        for entry in entries.iter_mut() {
            if Path::new(&entry.path) == target {
                entry.stream = Some(crate::worktree::WorktreeStream {
                    stream_id: stream.id.clone(),
                    stream_name: stream.name.clone(),
                    busy_run_id: stream.busy_run_id.clone(),
                });
            }
        }
    }
}

/// WKS-FR-CYAG: the stream a run holds at this path, if a run holds one.
///
/// What `activate_worktree` and `check_out_branch_in_active_worktree` refuse
/// on: an agent turn is writing that tree.
pub fn busy_stream_at<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    path: &Path,
) -> Option<(String, String)> {
    let store = store_for(app)?;
    let fs = access_for(app)?;
    let key = project_key_of(app);
    store::list_streams(&fs, &store, &key)
        .into_iter()
        .find(|stream| Path::new(&stream.worktree_path) == path)
        .and_then(|stream| stream.busy_run_id.clone().map(|run| (stream.id, run)))
}

/// WKS-FR-CYAG: the stream a run holds whose branch is `branch`, if one does.
pub fn busy_stream_on_branch<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    branch: &str,
) -> Option<(String, String)> {
    let store = store_for(app)?;
    let fs = access_for(app)?;
    let key = project_key_of(app);
    store::list_streams(&fs, &store, &key)
        .into_iter()
        .find(|stream| stream.branch == branch)
        .and_then(|stream| stream.busy_run_id.clone().map(|run| (stream.id, run)))
}

/// WKS-FR-JQJA: mark a stream busy for a run, durably.
///
/// Written before the run's first turn starts, so a stream a run held when the
/// application stopped is recognisable at the next launch.
pub fn claim_stream<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    run_id: &str,
) -> Result<(), String> {
    let store = store_for(app).ok_or_else(|| ERR_UNKNOWN_STREAM.to_string())?;
    let fs = access_for(app).ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    let mut stream =
        store::read_stream(&fs, &store, stream_id).ok_or_else(|| ERR_UNKNOWN_STREAM.to_string())?;
    if stream.busy_run_id.as_deref().is_some_and(|held| held != run_id) {
        return Err(ERR_STREAM_BUSY.to_string());
    }
    // WKS-FR-RQVM: a merge or an update of this stream writes its branch and
    // its working copy from its request until its outcome is written, so no
    // run takes the stream in that time.
    use tauri::Manager;
    if let Some(state) = app.try_state::<StreamState>() {
        if state.has_update_job(stream_id)
            || state.is_merging(stream_id)
            || state.is_updating(stream_id)
        {
            return Err(ERR_STREAM_BUSY.to_string());
        }
    }
    stream.busy_run_id = Some(run_id.to_string());
    store::write_stream(&fs, &store, &stream)
}

/// WKS-FR-JQJA: clear the busy mark, where this run is what holds it.
///
/// Releasing a stream a different run holds is a no-op rather than a silent
/// steal.
pub fn release_stream<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    run_id: &str,
) -> Result<(), String> {
    let store = store_for(app).ok_or_else(|| ERR_UNKNOWN_STREAM.to_string())?;
    let fs = access_for(app).ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    let Some(mut stream) = store::read_stream(&fs, &store, stream_id) else {
        return Ok(());
    };
    if stream.busy_run_id.as_deref() != Some(run_id) {
        return Ok(());
    }
    stream.busy_run_id = None;
    store::write_stream(&fs, &store, &stream)
}

/// WKS-FR-JGCA: the working copy a stream owns.
pub fn worktree_of<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
) -> Option<PathBuf> {
    let store = store_for(app)?;
    let fs = access_for(app)?;
    let stream = store::read_stream(&fs, &store, stream_id)?;
    let path = stream.worktree();
    path.is_dir().then_some(path)
}

/// The stream, whether or not a run holds it.
pub fn stream_of<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
) -> Option<WorkStream> {
    let store = store_for(app)?;
    let fs = access_for(app)?;
    store::read_stream(&fs, &store, stream_id)
}

/// WKS-FR-UZHT: the complete set of uncommitted paths a worktree holds, for a
/// gate that does not check the worktree out. Per WKS-FR-JVLM no path of the
/// application-owned storage is in it.
pub fn uncommitted_paths_of(worktree: &Path) -> Vec<String> {
    git::uncommitted_paths(worktree, false)
}

/// WKS-FR-EIBC / `GTC-git.md` GTC-FR-JOWX: how many commits the stream holds
/// that its base branch does not, counted as the deletion counts them. 0 when
/// the repository or either branch cannot be read.
pub fn ahead_of_base_of(root: &Path, stream: &WorkStream) -> u32 {
    git::primary_repo(root)
        .map(|main| git::ahead_of_base(&main, stream))
        .unwrap_or(0)
}

/// The repository a worktree belongs to, opened at its primary worktree.
pub fn primary_repo_of(path: &Path) -> Result<git2::Repository, String> {
    git::primary_repo(path)
}

/// Remove a linked worktree this application made, with the registration that
/// names it and the branch it was made on.
///
/// Deleting the directory alone is not a removal: Git keeps the admin
/// registration under the primary checkout and keeps the branch, so a later
/// worktree of the same name is refused. Every step is idempotent, because what
/// is already gone is the state the caller wanted.
pub(crate) fn reclaim_linked_worktree(
    fs: &fsa::FsAccess,
    main: &git2::Repository,
    path: &Path,
    registration: &str,
    branch: Option<&str>,
) -> Result<(), String> {
    git::remove_worktree(fs, main, path, Some(registration))?;
    if let Some(branch) = branch {
        git::delete_branch(main, branch)?;
    }
    Ok(())
}

/// Whether the primary repository still registers a linked worktree by name.
pub(crate) fn registers_worktree(main: &git2::Repository, name: &str) -> bool {
    main.worktrees()
        .map(|list| {
            list.iter()
                .filter_map(|entry| entry.ok().flatten())
                .any(|entry| entry == name)
        })
        .unwrap_or(false)
}

fn store_for<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<StreamStore> {
    use tauri::Manager;
    if let Some(root) = app.try_state::<StreamState>().and_then(|state| state.root()) {
        return Some(StreamStore::new(root));
    }
    fsa::short_data_dir().ok().map(StreamStore::new)
}

fn access_for<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Option<std::sync::Arc<fsa::FsAccess>> {
    use tauri::Manager;
    app.try_state::<fsa::FsAccessState>()
        .and_then(|state| state.get())
}

fn project_key_of<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> String {
    use tauri::Manager;
    app.try_state::<crate::project::ProjectState>()
        .map(|state| state.slot_key())
        .unwrap_or_default()
}

// The module's own tests. Declared last so the production half of this file
// is the whole of it, which is what the filesystem-access guard sweeps.
#[cfg(test)]
mod tests;
