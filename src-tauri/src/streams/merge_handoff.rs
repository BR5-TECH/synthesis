//! Merging a stream into its base branch: the check Git makes first, and the
//! handoff of a conflict to a merge run (`WKS-work-streams.md`,
//! `GRB-graduation-rebase.md`).
//!
//! **Git settles what Git can settle.** The request merges the two branches in
//! memory under the repository update guard. A merge that is clean, and a stream
//! that holds nothing its base does not, finish here, with no run and no durable
//! record. A merge Git cannot settle leaves both branches and both working
//! copies exactly as they were, captures the merge snapshot, and creates a merge
//! graduation run, which is the only durable record of that merge. Everything
//! after the handoff is the run's: its queue, its passes, its review and its
//! apply.

use tauri::Manager;

use super::commands::{announce, project_key, project_root, refuse, store, store_fs, stream_has_live_run};
use super::merge_progress::MergeProgress;
use super::*;
use crate::graduation::MergeRunRequest;
use crate::log_fields;
use crate::logging::{self, Domain};

/// Read one stream of the open project, or the typed refusal.
///
/// WKS-FR-ZBHL: the project is checked before the stream, so a project outside
/// Git is told that rather than told the stream is unknown.
fn stream_of<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
) -> Result<WorkStream, String> {
    let root = project_root(app)?;
    git::primary_repo(&root)?;
    let fs = store_fs(app)?;
    let store = store(app)?;
    let key = project_key(app);
    store::read_stream(&fs, &store, stream_id)
        .filter(|stream| stream.project_key == key)
        .ok_or_else(|| refuse(app, "work stream not found", stream_id, ERR_UNKNOWN_STREAM.to_string()))
}

/// WKS-FR-GKPX: merge a stream into the branch it came from.
///
/// Answers when the check has settled. The check is Git work on the author's
/// repository, so it runs off the thread that serves the window.
#[tauri::command]
pub async fn merge_work_stream<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
    publication: StreamMergePublication,
) -> Result<StreamMergeResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        merge_work_stream_blocking(&app, &stream_id, publication)
    })
    .await
    .map_err(|error| format!("the merge check did not finish: {error}"))?
}

/// The request itself, on the calling thread.
pub(crate) fn merge_work_stream_blocking<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    publication: StreamMergePublication,
) -> Result<StreamMergeResult, String> {
    let stream = stream_of(app, stream_id)?;
    // WKS-FR-GKPX: refused while a run of the stream has not ended, a merge run
    // included.
    if stream_has_live_run(app, &stream) {
        return Err(refuse(
            app,
            "work stream merge refused",
            stream_id,
            ERR_STREAM_BUSY.to_string(),
        ));
    }

    // A test stands another request between the check above and the guard below.
    #[cfg(test)]
    before_guard::run();

    // WKS-FR-HLGN / WKS-FR-TSOA: the repository update guard. One merge check or
    // one update runs in a repository at a time.
    let state = app.try_state::<StreamState>();
    let hold = match state.as_ref() {
        Some(state) => match state.begin_repository_update(stream_id, Reconciliation::Merge) {
            Ok(hold) => Some(hold),
            Err(reason) => {
                return Err(refuse(app, "work stream merge refused", stream_id, reason))
            }
        },
        None => None,
    };
    // A second request that passed the check above before the first one held the
    // guard would hand off a second run, so the check is made again under it.
    let stream = stream_of(app, stream_id)?;
    if stream_has_live_run(app, &stream) {
        return Err(refuse(
            app,
            "work stream merge refused",
            stream_id,
            ERR_STREAM_BUSY.to_string(),
        ));
    }
    let progress = MergeProgress::begin(app, &stream.name);
    logging::log_info(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "work stream merge check started",
        log_fields! {
            "stream_id" => stream_id.to_string(),
            "base_branch" => stream.base_branch.clone(),
        },
    );

    let settled = check(app, &stream, &publication);
    let result = match settled {
        Ok(result) => result,
        Err(reason) => return Err(refuse(app, "work stream merge refused", stream_id, reason)),
    };

    // The guard is released before the queue is offered a dispatch: the claim of
    // a run is refused while a merge or an update of its stream holds the guard
    // (WKS-FR-RQVM), and a run offered a slot under it would wait for the next
    // change to be offered another.
    drop(progress);
    drop(hold);
    announce(app, &stream.project_key);
    if matches!(result, StreamMergeResult::Conflicted { .. }) {
        crate::graduation::advance_stream_queue(app, stream_id);
    }
    Ok(result)
}

/// The Git half of the request, under the guard.
fn check<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream: &WorkStream,
    publication: &StreamMergePublication,
) -> Result<StreamMergeResult, String> {
    let buffer = &crate::logging::BUFFER;
    let root = project_root(app)?;
    let main = git::primary_repo(&root)?;
    // The merge writes into the tree that holds the base branch, which need not
    // be the primary worktree: a stream's base is whatever the author stood on
    // when they made it.
    let base_repo = git::worktree_holding(&main, &stream.base_branch);
    let base_worktree = base_repo
        .as_ref()
        .and_then(|repo| repo.workdir().map(std::path::PathBuf::from));
    // PST-FR-RONA: the base worktree is what the merge checks out, so every
    // draft changed there is saved before anything is compared.
    if let Some(base_worktree) = base_worktree.as_deref() {
        let access = store_fs(app)?;
        crate::storage_floor::save::save_drafts_before_checkout(&access, base_worktree)?;
    }
    merge::refuse_dirty(&stream.worktree(), base_worktree.as_deref())?;

    let mut prepared = merge::prepare(&main, stream)?;
    if prepared.up_to_date {
        logging::log_info(
            app,
            buffer,
            &[Domain::Backend],
            "work stream merge found nothing to merge",
            log_fields! { "stream_id" => stream.id.clone() },
        );
        return Ok(StreamMergeResult::NothingToMerge);
    }

    if prepared.conflicted_paths.is_empty() {
        let commit = merge::apply(&main, base_repo.as_ref(), stream, &mut prepared, publication)?;
        logging::log_info(
            app,
            buffer,
            &[Domain::Backend],
            "work stream merged",
            log_fields! {
                "stream_id" => stream.id.clone(),
                "paths" => prepared.merged_paths.len() as i64,
                "committed" => commit.is_some(),
            },
        );
        return Ok(StreamMergeResult::Merged {
            merged_paths: prepared.merged_paths.clone(),
            commit,
        });
    }

    // GRB-FR-QIHE: an `uncommitted` publication applies into the worktree that
    // holds the base branch. Where none does, the apply could never land, so the
    // request is refused now and no run is made for it.
    if base_repo.is_none() && matches!(publication, StreamMergePublication::Uncommitted) {
        return Err(ERR_BASE_NOT_CHECKED_OUT.to_string());
    }

    // WKS-FR-UCMR: a conflict no text turn can reconcile is refused before the
    // preflight, so it is not told as a machine that cannot execute an agent.
    merge_snapshot::refuse_unsupported_between(&main, prepared.base_oid, prepared.stream_oid)?;

    // GSU-FR-GLVQ: a machine that cannot execute an agent costs no run, so the
    // preflight is asked before anything is captured or created. A clean merge
    // needs no agent and never reaches it.
    crate::graduation::image_preflight(app)?;

    let run_id = crate::graduation::new_run_id();
    let snapshot = merge_snapshot::capture(&main, prepared.base_oid, prepared.stream_oid, &run_id)?;
    let request = MergeRunRequest {
        run_id: run_id.clone(),
        project_key: stream.project_key.clone(),
        stream_id: stream.id.clone(),
        stream_name: stream.name.clone(),
        stream_branch: stream.branch.clone(),
        base_branch: stream.base_branch.clone(),
        base_tip: prepared.base_oid.to_string(),
        stream_tip: prepared.stream_oid.to_string(),
        merge_base: snapshot.merge_base.to_string(),
        snapshot_commit: snapshot.commit.to_string(),
        publication: publication.clone(),
        changed_paths: snapshot.changed_paths.clone(),
        unresolved_paths: snapshot.unresolved_paths.clone(),
        conflicts: snapshot.conflicts.clone(),
    };
    match crate::graduation::enqueue_merge_run(app, request) {
        Ok(run) => {
            logging::log_info(
                app,
                buffer,
                &[Domain::Backend],
                "work stream merge handed to a run",
                log_fields! {
                    "stream_id" => stream.id.clone(),
                    "run_id" => run.id.clone(),
                    "unresolved" => snapshot.unresolved_paths.len() as i64,
                },
            );
            Ok(StreamMergeResult::Conflicted {
                run_id: run.id,
                conflicted_paths: snapshot.unresolved_paths,
            })
        }
        Err(reason) => {
            // The snapshot belongs to a run that does not exist, so it goes.
            if let Err(release) = merge_snapshot::release_ref(&main, &run_id) {
                logging::log_warn(
                    app,
                    buffer,
                    &[Domain::Backend],
                    "work stream merge could not remove an unused snapshot",
                    log_fields! { "stream_id" => stream.id.clone(), "reason" => release },
                );
            }
            Err(reason)
        }
    }
}

/// A seam for tests: what another request does after this one passed its first
/// check of the stream's runs and before it takes the repository update guard.
/// The hook belongs to the calling thread, so tests do not meet each other.
#[cfg(test)]
pub(crate) mod before_guard {
    use std::cell::RefCell;

    thread_local! {
        static HOOK: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
    }

    /// Set what runs at the seam on this thread.
    pub(crate) fn set(hook: impl Fn() + 'static) {
        HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
    }

    pub(super) fn run() {
        // The hook is taken out while it runs, so it can call the request again.
        let hook = HOOK.with(|slot| slot.borrow_mut().take());
        if let Some(hook) = hook {
            hook();
        }
    }
}
