//! The end of a stream: what a deletion and a sweep share, and the sweep that
//! removes a stream whose working copy is gone (`WKS-work-streams.md`
//! WKS-FR-APUE, WKS-FR-RVSU, WKS-FR-AUSI, WKS-FR-VYIW, WKS-FR-YLBX,
//! WKS-FR-DBWC).

use std::collections::BTreeSet;

use tauri::Manager;

use super::*;
use crate::log_fields;
use crate::logging::{self, Domain};

/// WKS-FR-YLBX: what the author reads once about a stream the sweep removed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovedStreamNotice {
    pub stream_id: String,
    pub name: String,
    pub branch: String,
    /// The branch is still in the repository (WKS-FR-VYIW).
    pub branch_survives: bool,
}

/// WKS-FR-EIBC, WKS-FR-APUE: what goes with a stream however it ended, with its
/// record already gone. The failure of the update record's removal, where one
/// happened.
pub(super) fn end_stream<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    fs: &fsa::FsAccess,
    store: &StreamStore,
    project_key: &str,
    stream_id: &str,
) -> Result<(), String> {
    let update = update_record::remove_update(fs, store, stream_id);
    // EAC-FR-ABHG: the stream has ended, so its sandbox VM stops.
    crate::tools::agent_exec::sandbox::stop_scope_in_background(
        app,
        project_key.to_string(),
        stream_id.to_string(),
    );
    update
}

/// WKS-FR-AUSI: only a positive answer says the working copy is gone. An error
/// of any other kind says nothing about what stands on the disk, and neither
/// does a record that names another path than the stream's own home in the store.
fn working_copy_not_found<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    fs: &fsa::FsAccess,
    store: &StreamStore,
    stream: &WorkStream,
) -> bool {
    [stream.worktree(), store.worktree(&stream.id)].iter().all(|path| match fs.file_info(path) {
        Err(fsa::FsError::NotFound { .. }) => true,
        Ok(_) => false,
        Err(reason) => {
            logging::log_debug(
                app,
                &crate::logging::BUFFER,
                &[Domain::Backend],
                "work stream sweep kept a stream it could not read",
                log_fields! { "stream_id" => stream.id.clone(), "reason" => reason.to_string() },
            );
            false
        }
    })
}

/// WKS-FR-RVSU: the streams the project holds a non-terminal run of, read from
/// the run order index alone (WKS-FR-YBST). `None` where the index exists and
/// cannot be read (WKS-FR-AUSI).
fn streams_with_live_runs<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    fs: &fsa::FsAccess,
    project_key: &str,
) -> Option<BTreeSet<String>> {
    let base = crate::graduation::store_base(app).ok()?;
    let index = crate::graduation::try_read_queue_index(fs, &base, project_key).ok()?;
    Some(
        index
            .runs
            .into_iter()
            .filter(|entry| !entry.state.is_terminal() && !entry.stream_id.is_empty())
            .map(|entry| entry.stream_id)
            .collect(),
    )
}

/// WKS-FR-RVSU: what keeps a stream whose working copy is gone.
pub(super) fn is_held(
    state: Option<&StreamState>,
    stream: &WorkStream,
    live_runs: &BTreeSet<String>,
    active_root: &Path,
) -> bool {
    stream.is_busy()
        || live_runs.contains(&stream.id)
        || active_root.starts_with(crate::changes::canonicalize_lenient(&stream.worktree()))
        || state.is_some_and(|state| {
            state.has_update_job(&stream.id)
                || state.is_merging(&stream.id)
                || state.is_updating(&stream.id)
        })
}

/// WKS-FR-APUE, WKS-FR-VYIW, WKS-FR-STSZ: remove one stream's record and what
/// goes with it, and no branch and no directory.
fn remove_gone_stream<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    fs: &fsa::FsAccess,
    store: &StreamStore,
    main: &git2::Repository,
    project_key: &str,
    stream: &WorkStream,
) -> Result<RemovedStreamNotice, String> {
    let buffer = &crate::logging::BUFFER;
    // A read that fails says nothing, so the notice promises no branch on it.
    let branch_survives = main.find_branch(&stream.branch, git2::BranchType::Local).is_ok();
    // WKS-FR-STSZ: a lock marks a path as offline on purpose, and a lock names the
    // registration as the author's. It stays, and it holds no more than a name.
    let locked = crate::worktree::worktree_lock_at(app, main, &stream.worktree()).is_some();
    let pruned = if locked {
        Ok(())
    } else {
        crate::worktree::prune_dead_registration(fs, main, &stream.worktree(), Some(&stream.id))
    };
    if let Err(reason) = pruned {
        // The registration stays, and it holds no more than a name. The stream
        // still leaves: its working copy is gone either way.
        logging::log_warn(
            app,
            buffer,
            &[Domain::Backend],
            "work stream registration could not be pruned",
            log_fields! { "stream_id" => stream.id.clone(), "reason" => reason },
        );
    }
    store::remove_stream_record(fs, store, &stream.id)?;
    if let Err(reason) = end_stream(app, fs, store, project_key, &stream.id) {
        logging::log_warn(
            app,
            buffer,
            &[Domain::Backend],
            "work stream update record could not be removed",
            log_fields! { "stream_id" => stream.id.clone(), "reason" => reason },
        );
    }
    logging::log_info(
        app,
        buffer,
        &[Domain::Backend],
        "work stream removed: its working copy is gone",
        log_fields! {
            "stream_id" => stream.id.clone(),
            "branch" => stream.branch.clone(),
            "branch_survives" => branch_survives,
        },
    );
    Ok(RemovedStreamNotice {
        stream_id: stream.id.clone(),
        name: stream.name.clone(),
        branch: stream.branch.clone(),
        branch_survives,
    })
}

/// WKS-FR-APUE: remove each stream of the project whose working copy is gone.
///
/// Called by a listing, and it costs one file read per stream where nothing is
/// gone. It reads the records under the hold a creation writes under, so it
/// never sees a stream that is half made (WKS-FR-KDXF).
pub(super) fn sweep_gone_streams<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    main: &git2::Repository,
    root: &Path,
) {
    let (Ok(fs), Ok(store)) = (commands::store_fs(app), commands::store(app)) else {
        return;
    };
    let key = commands::project_key(app);
    let gone = |stream: &WorkStream| working_copy_not_found(app, &fs, &store, stream);
    if !store::list_streams(&fs, &store, &key).iter().any(gone) {
        return;
    }

    let state = app.try_state::<StreamState>();
    let active_root = crate::changes::canonicalize_lenient(root);
    let mut removed = false;
    {
        let _write = state.as_ref().map(|state| state.write_guard());
        let Some(live_runs) = streams_with_live_runs(app, &fs, &key) else {
            logging::log_warn(
                app,
                &crate::logging::BUFFER,
                &[Domain::Backend],
                "work stream sweep skipped: the run order index cannot be read",
                log_fields! {},
            );
            return;
        };
        for stream in store::list_streams(&fs, &store, &key) {
            let holder = state.as_deref();
            if !gone(&stream) || is_held(holder, &stream, &live_runs, &active_root) {
                continue;
            }
            match remove_gone_stream(app, &fs, &store, main, &key, &stream) {
                Ok(notice) => {
                    removed = true;
                    if let Some(state) = holder {
                        state.record_removed(&key, notice);
                    }
                }
                // The next listing tries again.
                Err(reason) => logging::log_error(
                    app,
                    &crate::logging::BUFFER,
                    &[Domain::Backend],
                    "work stream record could not be removed",
                    log_fields! { "stream_id" => stream.id.clone(), "reason" => reason },
                ),
            }
        }
    }
    // WKS-FR-DBWC: a listing reloads itself on this event, so it is emitted only
    // where a record left.
    if removed {
        commands::announce(app, &key);
        let repository = main.workdir().unwrap_or(root);
        crate::worktree::announce_branches_changed(app, repository, "work stream sweep");
    }
}

/// WKS-FR-YLBX: the notices of the open project. Reading consumes none of them:
/// a listing runs on any surface, and a remount of the selector reads again.
#[tauri::command]
pub fn list_removed_work_streams<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<Vec<RemovedStreamNotice>, String> {
    let key = commands::project_key(&app);
    Ok(app.try_state::<StreamState>().map(|state| state.removed_notices(&key)).unwrap_or_default())
}

/// WKS-FR-YLBX: drop one notice. An id with no notice is no error.
#[tauri::command]
pub fn dismiss_removed_work_stream<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
) -> Result<(), String> {
    let key = commands::project_key(&app);
    if let Some(state) = app.try_state::<StreamState>() {
        state.dismiss_removed(&key, &stream_id);
    }
    Ok(())
}
