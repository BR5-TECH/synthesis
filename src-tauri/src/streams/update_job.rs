//! An update as a job of its own (WKS-FR-MJEB).
//!
//! The request writes the stream's update record `running` and returns it, and the update itself runs on a thread and writes
//! what it settled back to that same record. Every surface then learns what an
//! update did the one way — by reading the record.

use std::sync::Arc;

use tauri::Manager;

use super::update_record::{StreamUpdateCommit, StreamUpdateStrategy};
use super::*;
use crate::graduation::driver::GraduationDispatch;
use crate::log_fields;
use crate::logging::{self, Domain};

/// WKS-FR-MJEB: start an update of one stream and return at once.
pub(super) fn spawn<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream: &WorkStream,
    strategy: StreamUpdateStrategy,
    base_revision: String,
    missing_commits: Vec<StreamUpdateCommit>,
    decisions: Vec<StreamMergeDecision>,
) -> Result<StreamUpdateRecord, String> {
    spawn_with(
        app,
        stream,
        strategy,
        base_revision,
        missing_commits,
        decisions,
        Arc::new(crate::graduation::driver::AgentCliDispatch),
    )
}

/// The same, with the dispatch seam supplied, so a test drives the production
/// job with the execution agent removed.
pub(super) fn spawn_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream: &WorkStream,
    strategy: StreamUpdateStrategy,
    base_revision: String,
    missing_commits: Vec<StreamUpdateCommit>,
    decisions: Vec<StreamMergeDecision>,
    dispatch: Arc<dyn GraduationDispatch<R>>,
) -> Result<StreamUpdateRecord, String> {
    start_with(
        app,
        stream,
        strategy,
        base_revision,
        missing_commits,
        decisions,
        dispatch,
    )
    .map(|(record, _)| record)
}

/// The same again, handing back the thread the update runs on.
///
/// Nothing in production joins it — the record and `"work streams changed"` are
/// how a surface learns — so the handle is dropped there and taken here by a
/// test that would otherwise assert against an update still running.
pub(super) fn start_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream: &WorkStream,
    strategy: StreamUpdateStrategy,
    base_revision: String,
    missing_commits: Vec<StreamUpdateCommit>,
    decisions: Vec<StreamMergeDecision>,
    dispatch: Arc<dyn GraduationDispatch<R>>,
) -> Result<(StreamUpdateRecord, std::thread::JoinHandle<()>), String> {
    let fs = commands::store_fs(app)?;
    let store = commands::store(app)?;
    let state = app.try_state::<StreamState>();

    // WKS-FR-QFTH: the job is marked live before its record is written and stays
    // marked until its outcome is written, so a sweep running beside it never
    // calls it stranded. The claim is also what refuses a second request: the
    // update's own hold on the repository update guard is taken later, on the
    // thread, which leaves a window two requests could both pass.
    let record = {
        let _guard = state.as_ref().map(|state| state.write_guard());
        let claimed = state
            .as_ref()
            .map(|state| state.claim_update_job(&stream.id))
            .unwrap_or(true);
        if !claimed {
            return Err(commands::refuse(
                app,
                "work stream update refused",
                &stream.id,
                ERR_UPDATE_IN_PROGRESS.to_string(),
            ));
        }
        let record = StreamUpdateRecord::starting(
            stream,
            strategy,
            &base_revision,
            missing_commits,
            decisions.clone(),
        );
        match update_record::write_update(&fs, &store, &record) {
            Ok(()) => record,
            Err(reason) => {
                if let Some(state) = state.as_ref() {
                    state.release_update_job(&stream.id);
                }
                return Err(reason);
            }
        }
    };
    commands::announce(app, &stream.project_key);

    logging::log_info(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "work stream update started",
        log_fields! {
            "stream_id" => stream.id.clone(),
            "base_branch" => stream.base_branch.clone(),
            "strategy" => strategy.as_str(),
            "decisions" => decisions.len() as i64,
        },
    );

    let app = app.clone();
    let stream_id = stream.id.clone();
    let project_key = stream.project_key.clone();
    let started = record.clone();
    let handle = std::thread::spawn(move || {
        let _job = UpdateJobRelease {
            app: app.clone(),
            stream_id: stream_id.clone(),
        };
        update_blocking(
            &app,
            &stream_id,
            &project_key,
            started,
            strategy,
            base_revision,
            decisions,
            dispatch,
        );
    });
    Ok((record, handle))
}

#[allow(clippy::too_many_arguments)]
fn update_blocking<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    project_key: &str,
    mut record: StreamUpdateRecord,
    strategy: StreamUpdateStrategy,
    base_revision: String,
    decisions: Vec<StreamMergeDecision>,
    dispatch: Arc<dyn GraduationDispatch<R>>,
) {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        // The update never ran, so the record must not be left claiming it is.
        record.state = update_record::StreamUpdateState::Failed;
        record.failure = ERR_UPDATE_ATTEMPTS_EXHAUSTED.to_string();
        record.updated_at = crate::notes::now_rfc3339();
        publish(app, project_key, &record);
        return;
    };
    let report = runtime.block_on(async {
        updating::run(app, stream_id, strategy, base_revision, decisions, dispatch).await
    });

    // WKS-FR-TSOA: a request refused because the repository update guard was
    // already held settles nothing of the work that holds it, so it never writes
    // over that work's record.
    let refused = report.outcome.as_ref().err().map(String::as_str);
    if refused == Some(ERR_UPDATE_IN_PROGRESS) || refused == Some(ERR_MERGE_IN_PROGRESS) {
        // The refusal is still the answer to *this* request, so it is recorded
        // rather than dropped: the record it would write over belongs to the
        // same stream only where the guard is held by that stream's own work.
        update_record::settle(&mut record, &report.conflicts, &report.outcome);
        publish(app, project_key, &record);
        return;
    }

    update_record::settle(&mut record, &report.conflicts, &report.outcome);
    logging::log_info(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "work stream update settled",
        log_fields! {
            "stream_id" => stream_id.to_string(),
            "state" => record.state.as_str(),
            "attempt_id" => record.attempt_id.clone(),
            "conflicts" => record.conflicts.len() as i64,
        },
    );
    publish(app, project_key, &record);
}

/// Write what the update settled and tell every surface to reload.
fn publish<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project_key: &str,
    record: &StreamUpdateRecord,
) {
    let written = commands::store_fs(app)
        .and_then(|fs| commands::store(app).map(|store| (fs, store)))
        .and_then(|(fs, store)| update_record::write_update(&fs, &store, record));
    if let Err(reason) = written {
        // WKS-FR-DHOP: a record that could not be written leaves the update
        // unreachable to every surface, so it is never a silent failure.
        logging::log_error(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "work stream update record could not be written",
            log_fields! {
                "stream_id" => record.stream_id.clone(),
                "reason" => reason,
            },
        );
    }
    commands::announce(app, project_key);
}

/// Releases the job mark `start_with` took, when the update's thread unwinds.
struct UpdateJobRelease<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    stream_id: String,
}

impl<R: tauri::Runtime> Drop for UpdateJobRelease<R> {
    fn drop(&mut self) {
        if let Some(state) = self.app.try_state::<StreamState>() {
            state.release_update_job(&self.stream_id);
        }
        // WKS-FR-RQVM: a run whose claim this update refused waits in the
        // stream's queue, and the stream is free for it now.
        crate::graduation::advance_stream_queue(&self.app, &self.stream_id);
    }
}
