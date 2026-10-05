//! The commands a work stream is managed through (WKS contract surface).

use tauri::{Emitter, Manager};

use super::*;
use crate::log_fields;
use crate::logging::{self, Domain};

/// WKS-FR-WULF: what every reader reloads its own listing on.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamsChangedPayload {
    project_key: String,
}

pub(super) fn announce<R: tauri::Runtime>(app: &tauri::AppHandle<R>, project_key: &str) {
    let _ = app.emit(
        WORK_STREAMS_CHANGED,
        StreamsChangedPayload {
            project_key: project_key.to_string(),
        },
    );
}

/// The store every operation here reads and writes.
///
/// `short_data_dir()` in the application, and whatever root the managed state
/// names where one is set.
pub(super) fn store<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<StreamStore, String> {
    if let Some(root) = app.try_state::<StreamState>().and_then(|state| state.root()) {
        return Ok(StreamStore::new(root));
    }
    let root = fsa::short_data_dir().map_err(|e| e.to_string())?;
    Ok(StreamStore::new(root))
}

/// The guarded filesystem instance, whose allowlist holds the store root.
pub(super) fn store_fs<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<std::sync::Arc<fsa::FsAccess>, String> {
    app.try_state::<fsa::FsAccessState>()
        .and_then(|state| state.get())
        .ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())
}

/// The open project's root, or the typed refusal.
pub(super) fn project_root<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<std::path::PathBuf, String> {
    let state = app
        .try_state::<crate::project::ProjectState>()
        .ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    state.root().ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())
}

pub(super) fn project_key<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> String {
    app.try_state::<crate::project::ProjectState>()
        .map(|state| state.slot_key())
        .unwrap_or_default()
}

/// WKS-FR-GKPX / WKS-FR-NRQT: whether a merge or an update of this stream is
/// refused because a run of it has not ended.
///
/// A run that rests holds no stream, but it keeps a base commit and commits of
/// its own, and a reconciliation that rewrote the branch under them would leave
/// the run measuring its work against a revision the branch has left. A merge
/// run is such a run: it stands on two pinned tips until it ends.
pub(super) fn stream_has_live_run<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream: &WorkStream,
) -> bool {
    stream.is_busy()
        || crate::graduation::project_queue(app).is_some_and(|queue| {
            queue
                .runs
                .iter()
                .any(|run| run.stream_id == stream.id && !run.state.is_terminal())
        })
}

/// WKS-FR-KDXF / WKS-FR-VTEY: create a stream, or create nothing.
#[tauri::command]
pub fn create_work_stream<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    name: String,
    base_branch: Option<String>,
) -> Result<WorkStream, String> {
    let buffer = &crate::logging::BUFFER;
    let root = project_root(&app)?;
    let fs = store_fs(&app)?;
    let store = store(&app)?;
    let main = git::primary_repo(&root)?;

    // The exclusive hold every write of this module takes, held across the
    // read, the check and the write together: without it two callers each
    // pass their own precondition before either mutates.
    let held = app.try_state::<StreamState>();
    let _write = held.as_ref().map(|state| state.write_guard());

    // WKS-FR-HRUZ: a name no other live stream of the project holds.
    let slug = slug_of(&name);
    if slug.is_empty() {
        return Err(refuse(&app, "work stream name refused", "", ERR_STREAM_NAME_INVALID.to_string()));
    }
    let branch = branch_name_for(&name);
    let key = project_key(&app);
    let existing = store::list_streams(&fs, &store, &key);
    if existing.iter().any(|s| s.branch == branch) {
        return Err(refuse(&app, "work stream name refused", "", ERR_STREAM_NAME_TAKEN.to_string()));
    }

    // WKS-FR-PWNR: the active worktree's branch where the request names none.
    let base_branch = match base_branch.filter(|b| !b.trim().is_empty()) {
        Some(branch) => branch,
        None => match git::current_branch(&root) {
            Some(branch) => branch,
            None => {
                return Err(refuse(
                    &app,
                    "work stream creation refused",
                    "",
                    ERR_BASE_BRANCH_REQUIRED.to_string(),
                ))
            }
        },
    };
    let base_revision = git::branch_revision(&main, &base_branch)?;

    let stream_id = store::new_stream_id();
    let target = store.worktree(&stream_id);
    let mut created = git::Created::default();

    logging::log_info(
        &app,
        buffer,
        &[Domain::Backend],
        "work stream creation started",
        log_fields! {
            "stream_id" => stream_id.clone(),
            "branch" => branch.clone(),
            "base_branch" => base_branch.clone(),
        },
    );

    let outcome = git::create_stream_checkout(
        &fs,
        &main,
        &stream_id,
        &branch,
        &base_revision,
        &target,
        &mut created,
    );
    if let Err(reason) = outcome {
        return Err(fail_and_reclaim(&app, &fs, &main, &store, created, &stream_id, reason));
    }

    let stream = WorkStream {
        id: stream_id.clone(),
        project_key: key.clone(),
        name,
        branch,
        worktree_path: target.to_string_lossy().into_owned(),
        base_branch,
        base_revision,
        created_at: crate::notes::now_rfc3339(),
        busy_run_id: None,
        is_missing: false,
    };
    if let Err(reason) = store::write_stream(&fs, &store, &stream) {
        return Err(fail_and_reclaim(&app, &fs, &main, &store, created, &stream_id, reason));
    }

    logging::log_info(
        &app,
        buffer,
        &[Domain::Backend],
        "work stream created",
        log_fields! { "stream_id" => stream.id.clone(), "branch" => stream.branch.clone() },
    );
    announce(&app, &key);
    // WKS-FR-SPVK: the stream's branch is new to every branch listing.
    crate::worktree::announce_branches_changed(&app, &root, "work stream creation");
    Ok(stream)
}

/// WKS-FR-VTEY / WKS-FR-QMTV: reclaim what a failed creation made, and report
/// what could not be reclaimed.
fn fail_and_reclaim<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    fs: &fsa::FsAccess,
    main: &git2::Repository,
    store: &StreamStore,
    created: git::Created,
    stream_id: &str,
    original: String,
) -> String {
    let stranded = git::reclaim(fs, main, created);
    let _ = store::remove_stream_home(fs, store, stream_id);
    let reason = if stranded.is_empty() {
        original
    } else {
        // WKS-FR-SPVK: what could not be reclaimed may include the branch.
        if let Some(root) = main.workdir() {
            crate::worktree::announce_branches_changed(app, root, "work stream creation");
        }
        format!("{ERR_STREAM_CLEANUP_FAILED}: {}", stranded.join(", "))
    };
    logging::log_error(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "work stream creation failed",
        log_fields! { "stream_id" => stream_id.to_string(), "reason" => reason.clone() },
    );
    reason
}

/// WKS-FR-SGCM: every live stream, with what each holds.
#[tauri::command]
pub fn list_work_streams<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<Vec<WorkStreamSummary>, String> {
    let root = project_root(&app)?;
    let fs = store_fs(&app)?;
    let store = store(&app)?;
    let main = git::primary_repo(&root)?;
    let key = project_key(&app);
    // GRB-FR-UHFE: reclaim what a merge the application never finished left
    // behind. A listing is where it belongs: it is the first thing every reader
    // of this module does, and reclaiming costs nothing when there is nothing
    // stranded.
    attempts::sweep_stranded_attempts(&app);
    // WKS-FR-QFTH: and record the updates a stopped application left running.
    attempts::sweep_interrupted_updates(&app);
    // WKS-FR-YBST: the depth of each stream's queue, read from the project's
    // run order index. No run record is opened by a listing.
    let counts = crate::graduation::store_base(&app)
        .map(|base| crate::graduation::queued_counts_by_stream(&fs, &base, &key))
        .unwrap_or_default();
    let merge_runs = crate::graduation::store_base(&app)
        .map(|base| crate::graduation::merge_runs_by_stream(&fs, &base, &key))
        .unwrap_or_default();
    Ok(store::list_streams(&fs, &store, &key)
        .into_iter()
        .map(|stream| {
            let ahead_of_base = git::ahead_of_base(&main, &stream);
            // WKS-FR-RJPD / WKS-FR-UBGX: what Update is enabled by, and what the
            // stream update window lists, both read in this one listing.
            let (behind_base, base_tip_revision, missing_commits) =
                update_git::behind_base(&main, &stream);
            let queued_run_count = counts.get(&stream.id).copied().unwrap_or(0);
            // WKS-FR-KHJS: the stream's merge run, from the run order index.
            let merge_run = merge_runs.get(&stream.id).map(|(run_id, state)| StreamMergeRunLink {
                run_id: run_id.clone(),
                name: crate::graduation::merge_title(&stream.name),
                state: *state,
            });
            let update = update_record::read_update(&fs, &store, &stream.id)
                .filter(|record| record.project_key == key);
            WorkStreamSummary {
                stream,
                queued_run_count,
                ahead_of_base,
                behind_base,
                base_tip_revision,
                missing_commits,
                merge_run,
                update,
            }
        })
        .collect())
}

/// WKS-FR-SGCM: one stream, or the typed refusal.
#[tauri::command]
pub fn get_work_stream<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
) -> Result<WorkStream, String> {
    let root = project_root(&app)?;
    // WKS-FR-ZBHL: the typed refusal comes from every operation here, so a
    // project outside Git is told that rather than told the stream is unknown.
    git::primary_repo(&root)?;
    let fs = store_fs(&app)?;
    let store = store(&app)?;
    let key = project_key(&app);
    store::read_stream(&fs, &store, &stream_id)
        .filter(|stream| stream.project_key == key)
        .ok_or_else(|| refuse(&app, "work stream not found", &stream_id, ERR_UNKNOWN_STREAM.to_string()))
}

/// WKS-FR-DHOP: a refusal is a failure path, and every one of them is recorded.
///
/// The reason and the stream are fields rather than prose, so the panel can be
/// filtered on either. No file content and no path of the author's reaches one.
pub(super) fn refuse<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    message: &str,
    stream_id: &str,
    reason: String,
) -> String {
    logging::log_warn(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        message,
        log_fields! { "stream_id" => stream_id.to_string(), "reason" => reason.clone() },
    );
    reason
}

/// WKS-FR-EIBC / WKS-FR-OVLQ / WKS-FR-DBXN: remove a stream, its branch and its
/// working copy.
///
/// The checks run in a fixed order: unknown stream, busy, has runs, active,
/// unmerged (without `force`), dirty (without `discard_uncommitted`). `force`
/// answers the unmerged check alone and `discard_uncommitted` the dirty check
/// alone; neither reaches the three before them.
#[tauri::command]
pub fn delete_work_stream<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
    force: bool,
    discard_uncommitted: bool,
) -> Result<(), String> {
    let buffer = &crate::logging::BUFFER;
    let root = project_root(&app)?;
    let fs = store_fs(&app)?;
    let store = store(&app)?;
    let main = git::primary_repo(&root)?;
    let key = project_key(&app);
    let stream = store::read_stream(&fs, &store, &stream_id)
        .filter(|stream| stream.project_key == key)
        .ok_or_else(|| refuse(&app, "work stream not found", &stream_id, ERR_UNKNOWN_STREAM.to_string()))?;

    // The exclusive hold every write of this module takes, held across the
    // read, the check and the write together: without it two callers each
    // pass their own precondition before either mutates.
    let held = app.try_state::<StreamState>();
    let _write = held.as_ref().map(|state| state.write_guard());

    // WKS-FR-OVLQ: refused while a run holds it.
    if stream.is_busy() {
        return Err(refuse(&app, "work stream deletion refused", &stream_id, ERR_STREAM_BUSY.to_string()));
    }
    // WKS-FR-OVLQ: refused while the project holds a run of it that has not
    // ended, a direct run that targets its working copy included. `force`
    // reaches neither this nor the busy refusal above.
    if crate::graduation::project_queue(&app).is_some_and(|queue| {
        queue
            .runs
            .iter()
            .any(|run| run.stream_id == stream.id && !run.state.is_terminal())
    }) {
        return Err(refuse(
            &app,
            "work stream deletion refused",
            &stream_id,
            ERR_STREAM_HAS_RUNS.to_string(),
        ));
    }
    // WKS-FR-OVLQ: refused while its working copy roots the project. Read from
    // the worktree enumeration now, so a switch since the last listing counts.
    let active = crate::worktree::active_entry_for(&root)?;
    if crate::changes::canonicalize_lenient(&stream.worktree())
        == std::path::Path::new(&active.path)
    {
        return Err(refuse(
            &app,
            "work stream deletion refused",
            &stream_id,
            ERR_STREAM_ACTIVE.to_string(),
        ));
    }
    // WKS-FR-EIBC: without force, a stream carrying unmerged commits is
    // refused, and the refusal names how many. Counted with force too, so the
    // record of a forced deletion says how many commits it dropped.
    let ahead = git::ahead_of_base(&main, &stream);
    if !force {
        if ahead > 0 {
            return Err(refuse(
                &app,
                "work stream deletion refused",
                &stream_id,
                format!("{ERR_STREAM_UNMERGED}: {ahead}"),
            ));
        }
    }

    // WKS-FR-DBXN: uncommitted paths are discarded only on the author's word.
    // A working copy that is missing reports none (WKS-FR-ETKW).
    let uncommitted = git::uncommitted_paths(&stream.worktree(), false);
    if !uncommitted.is_empty() && !discard_uncommitted {
        // The count only: a path is the author's content.
        logging::log_warn(
            &app,
            buffer,
            &[Domain::Backend],
            "work stream deletion refused: uncommitted paths",
            log_fields! { "stream_id" => stream_id.clone(), "path_count" => uncommitted.len() },
        );
        return Err(format!("{ERR_STREAM_DIRTY}: {}", uncommitted.join(", ")));
    }

    let mut stranded =
        git::remove_worktree(&fs, &main, &stream.worktree(), Some(&stream.id))
            .err()
            .map(|_| vec![stream.worktree_path.clone()])
            .unwrap_or_default();
    if git::delete_branch(&main, &stream.branch).is_err() {
        stranded.push(stream.branch.clone());
    }
    if let Err(reason) = store::remove_stream_home(&fs, &store, &stream_id) {
        stranded.push(reason);
    }
    // WKS-FR-EIBC: the update record goes with the stream it belongs to.
    if let Err(reason) = update_record::remove_update(&fs, &store, &stream_id) {
        stranded.push(reason);
    }
    // WKS-FR-SPVK: the branch set changed, whether or not every part went.
    crate::worktree::announce_branches_changed(&app, &root, "work stream deletion");
    if !stranded.is_empty() {
        let reason = format!("{ERR_STREAM_CLEANUP_FAILED}: {}", stranded.join(", "));
        logging::log_error(
            &app,
            buffer,
            &[Domain::Backend],
            "work stream deletion could not reclaim everything",
            log_fields! {
                "stream_id" => stream_id.clone(),
                "reason" => reason.clone(),
                "force" => force,
                "ahead_of_base" => ahead,
                "discard_uncommitted" => discard_uncommitted,
                "uncommitted_count" => uncommitted.len(),
            },
        );
        return Err(reason);
    }

    // A forced deletion that dropped commits is the record a reader looks for
    // after "my commits are gone", so it stands out as a warning. Counts and
    // flags only: no path and no content.
    let fields = log_fields! {
        "stream_id" => stream_id.clone(),
        "force" => force,
        "ahead_of_base" => ahead,
        "discard_uncommitted" => discard_uncommitted,
        "uncommitted_count" => uncommitted.len(),
    };
    if force && ahead > 0 {
        logging::log_warn(
            &app,
            buffer,
            &[Domain::Backend],
            "work stream deleted with unmerged commits",
            fields,
        );
    } else {
        logging::log_info(&app, buffer, &[Domain::Backend], "work stream deleted", fields);
    }
    announce(&app, &project_key(&app));
    Ok(())
}

/// WKS-FR-GQSU: the complete uncommitted path set of a stream's working copy.
///
/// Empty where the working copy is missing. Writes nothing, and is the read a
/// deletion confirmation names the discarded paths from.
#[tauri::command]
pub fn get_work_stream_uncommitted_paths<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
) -> Result<Vec<String>, String> {
    let root = project_root(&app)?;
    git::primary_repo(&root)?;
    let fs = store_fs(&app)?;
    let store = store(&app)?;
    let key = project_key(&app);
    let stream = store::read_stream(&fs, &store, &stream_id)
        .filter(|stream| stream.project_key == key)
        .ok_or_else(|| refuse(&app, "work stream not found", &stream_id, ERR_UNKNOWN_STREAM.to_string()))?;
    let paths = git::uncommitted_paths(&stream.worktree(), false);
    logging::log_debug(
        &app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "work stream uncommitted paths read",
        log_fields! { "stream_id" => stream_id.clone(), "path_count" => paths.len() },
    );
    Ok(paths)
}
