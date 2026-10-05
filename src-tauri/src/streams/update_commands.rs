//! Starting an update, and driving one that stopped (WKS-FR-NRQT, WKS-FR-MJEB,
//! WKS-FR-GYHF, WKS-FR-CBXW, WKS-FR-DPNM, WKS-FR-LRAV, WKS-FR-WEJK).
//!
//! Every one of these answers from the stream's own update record, so an update
//! reads the same from the window that started it, from another window, and
//! from a launch after the one that ran it.

use tauri::Manager;

use super::commands::{announce, project_key, project_root, refuse, store, store_fs, stream_has_live_run};
use super::update_record::{StreamUpdateCommit, StreamUpdateStrategy};
use super::*;
use crate::graduation::GraduationEscalationAnswer;
use crate::log_fields;
use crate::logging::{self, Domain};

/// Read one stream of the open project, or the typed refusal.
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

/// The stream's update record, or the typed refusal where the stream is unknown.
fn record_of<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
) -> Result<Option<StreamUpdateRecord>, String> {
    let stream = stream_of(app, stream_id)?;
    let fs = store_fs(app)?;
    let store = store(app)?;
    Ok(update_record::read_update(&fs, &store, stream_id)
        .filter(|record| record.project_key == stream.project_key))
}

/// The commits the stream is missing from the pinned revision, for the record.
fn missing_at<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream: &WorkStream,
    pinned: &str,
) -> Vec<StreamUpdateCommit> {
    let Ok(root) = project_root(app) else {
        return Vec::new();
    };
    let Ok(main) = git::primary_repo(&root) else {
        return Vec::new();
    };
    let (Some(head), Ok(base)) = (
        update_git::branch_tip(&main, &stream.branch),
        git2::Oid::from_str(pinned),
    ) else {
        return Vec::new();
    };
    update_git::missing_commits(&main, head, base)
}

/// WKS-FR-NRQT / WKS-FR-MJEB: bring one stream up to a pinned revision of the
/// branch it was created from.
///
/// Returns the update's record in `running`. The update runs to rest on a thread
/// of its own, and what it settled is read back through `get_work_stream_update`.
#[tauri::command]
pub fn update_work_stream<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
    strategy: StreamUpdateStrategy,
    base_revision: String,
) -> Result<StreamUpdateRecord, String> {
    update_work_stream_with(
        &app,
        &stream_id,
        strategy,
        base_revision,
        |app, stream, strategy, base_revision, missing, decisions| {
            update_job::spawn(app, stream, strategy, base_revision, missing, decisions)
        },
    )
}

/// The start of an update job: the stream, the strategy, the pinned revision,
/// the commits the stream is missing, and the author's decisions.
pub(crate) trait UpdateStart<R: tauri::Runtime>:
    FnOnce(
        &tauri::AppHandle<R>,
        &WorkStream,
        StreamUpdateStrategy,
        String,
        Vec<StreamUpdateCommit>,
        Vec<StreamMergeDecision>,
    ) -> Result<StreamUpdateRecord, String>
{
}

impl<R: tauri::Runtime, F> UpdateStart<R> for F where
    F: FnOnce(
        &tauri::AppHandle<R>,
        &WorkStream,
        StreamUpdateStrategy,
        String,
        Vec<StreamUpdateCommit>,
        Vec<StreamMergeDecision>,
    ) -> Result<StreamUpdateRecord, String>
{
}

/// A start of the update job on a seam the test supplies, which returns once
/// the update has rested.
#[cfg(test)]
pub(crate) fn scripted_update_start<R: tauri::Runtime>(
    dispatch: std::sync::Arc<dyn crate::graduation::driver::GraduationDispatch<R>>,
) -> impl UpdateStart<R> {
    move |app: &tauri::AppHandle<R>,
          stream: &WorkStream,
          strategy: StreamUpdateStrategy,
          base_revision: String,
          missing: Vec<StreamUpdateCommit>,
          decisions: Vec<StreamMergeDecision>| {
        let (record, handle) = update_job::start_with(
            app,
            stream,
            strategy,
            base_revision,
            missing,
            decisions,
            dispatch,
        )?;
        handle
            .join()
            .map_err(|_| "the update thread did not rest".to_string())?;
        Ok(record)
    }
}

/// The same as `update_work_stream`, with the start of the update job
/// supplied, so a test drives the production request on the scripted seam
/// (GXD-FR-QLFA).
pub(crate) fn update_work_stream_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    strategy: StreamUpdateStrategy,
    base_revision: String,
    start: impl UpdateStart<R>,
) -> Result<StreamUpdateRecord, String> {
    let app = app.clone();
    let stream_id = stream_id.to_string();
    let stream = stream_of(&app, &stream_id)?;
    // WKS-FR-NRQT: refused while a run of the stream has not ended. Answered
    // here rather than on the thread, so the author learns it from the call
    // they made.
    if stream_has_live_run(&app, &stream) {
        return Err(refuse(
            &app,
            "work stream update refused",
            &stream_id,
            ERR_STREAM_BUSY.to_string(),
        ));
    }
    if base_revision.trim().is_empty() {
        return Err(refuse(
            &app,
            "work stream update refused",
            &stream_id,
            ERR_STALE_BASE_REVISION.to_string(),
        ));
    }
    let missing = missing_at(&app, &stream, &base_revision);
    start(&app, &stream, strategy, base_revision, missing, Vec::new())
}

/// WKS-FR-GYHF: what one stream's last update did, or nothing.
#[tauri::command]
pub fn get_work_stream_update<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
) -> Result<Option<StreamUpdateRecord>, String> {
    record_of(&app, &stream_id)
}

/// WKS-FR-CBXW: the author's answers to an update that asked.
#[tauri::command]
pub fn answer_work_stream_update_escalation<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
    answers: Vec<GraduationEscalationAnswer>,
) -> Result<StreamUpdateRecord, String> {
    answer_work_stream_update_escalation_with(
        &app,
        &stream_id,
        answers,
        |app, stream, strategy, base_revision, missing, decisions| {
            update_job::spawn(app, stream, strategy, base_revision, missing, decisions)
        },
    )
}

/// The same, with the start of the update job supplied.
pub(crate) fn answer_work_stream_update_escalation_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    answers: Vec<GraduationEscalationAnswer>,
    start: impl UpdateStart<R>,
) -> Result<StreamUpdateRecord, String> {
    let app = app.clone();
    let stream_id = stream_id.to_string();
    let stream = stream_of(&app, &stream_id)?;
    let mut record = record_of(&app, &stream_id)?.ok_or_else(|| {
        refuse(
            &app,
            "work stream update answer refused",
            &stream_id,
            ERR_UPDATE_STATE_NOT_PERMITTED.to_string(),
        )
    })?;
    let Some(escalation) = record.escalation.clone().filter(|_| record.state.waits_on_author())
    else {
        return Err(refuse(
            &app,
            "work stream update answer refused",
            &stream_id,
            ERR_UPDATE_STATE_NOT_PERMITTED.to_string(),
        ));
    };

    // WKS-FR-NRQT: the answers start the update again, on the same terms.
    if stream_has_live_run(&app, &stream) {
        return Err(refuse(
            &app,
            "work stream update answer refused",
            &stream_id,
            ERR_STREAM_BUSY.to_string(),
        ));
    }

    let mut positions: Vec<u32> = answers.iter().map(|answer| answer.position).collect();
    positions.sort_unstable();
    let expected: Vec<u32> = escalation.questions.iter().map(|q| q.position).collect();
    if positions != expected || answers.iter().any(|a| a.answer.trim().is_empty()) {
        logging::log_warn(
            &app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "work stream update answers do not cover every question",
            log_fields! {
                "stream_id" => stream_id.clone(),
                "answered" => answers.len() as i64,
                "questions" => expected.len() as i64,
            },
        );
        return Err("the answers do not cover every question".to_string());
    }

    // GRB-FR-KMXT: the question travels with the answer.
    let decisions: Vec<StreamMergeDecision> = escalation
        .questions
        .iter()
        .filter_map(|question| {
            let answer = answers.iter().find(|a| a.position == question.position)?;
            Some(StreamMergeDecision {
                position: question.position,
                question: question.question.clone(),
                answer: answer.answer.clone(),
                summary: answer.summary.clone(),
            })
        })
        .collect();

    record.escalation = None;
    record.reason = String::new();
    // WKS-FR-AMWE: the pinned revision the record retains, never the branch tip
    // read again. A revision the branch has since left is refused by the update
    // itself with `stale_base_revision`.
    start(
        &app,
        &stream,
        record.strategy,
        record.base_revision.clone(),
        record.missing_commits.clone(),
        decisions,
    )
}

/// WKS-FR-DPNM: run the update again, with three fresh semantic turns.
#[tauri::command]
pub fn retry_work_stream_update<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
) -> Result<StreamUpdateRecord, String> {
    retry_work_stream_update_with(
        &app,
        &stream_id,
        |app, stream, strategy, base_revision, missing, decisions| {
            update_job::spawn(app, stream, strategy, base_revision, missing, decisions)
        },
    )
}

/// The same, with the start of the update job supplied.
pub(crate) fn retry_work_stream_update_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    start: impl UpdateStart<R>,
) -> Result<StreamUpdateRecord, String> {
    let app = app.clone();
    let stream_id = stream_id.to_string();
    let stream = stream_of(&app, &stream_id)?;
    let record = record_of(&app, &stream_id)?.ok_or_else(|| {
        refuse(
            &app,
            "work stream update retry refused",
            &stream_id,
            ERR_UPDATE_STATE_NOT_PERMITTED.to_string(),
        )
    })?;
    if record.state.is_running() {
        return Err(refuse(
            &app,
            "work stream update retry refused",
            &stream_id,
            ERR_UPDATE_IN_PROGRESS.to_string(),
        ));
    }
    // WKS-FR-DPNM: an escalated update is answered or cleared. A retry of one
    // would spend three more turns on the question the author already holds.
    if !record.state.is_retryable() {
        return Err(refuse(
            &app,
            "work stream update retry refused",
            &stream_id,
            ERR_UPDATE_STATE_NOT_PERMITTED.to_string(),
        ));
    }
    if stream_has_live_run(&app, &stream) {
        return Err(refuse(
            &app,
            "work stream update retry refused",
            &stream_id,
            ERR_STREAM_BUSY.to_string(),
        ));
    }
    start(
        &app,
        &stream,
        record.strategy,
        record.base_revision.clone(),
        record.missing_commits.clone(),
        record.decisions.clone(),
    )
}

/// WKS-FR-LRAV: forget what an update settled.
#[tauri::command]
pub fn clear_work_stream_update<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
) -> Result<(), String> {
    let stream = stream_of(&app, &stream_id)?;
    let fs = store_fs(&app)?;
    let store = store(&app)?;
    let held = app.try_state::<StreamState>();
    let _write = held.as_ref().map(|state| state.write_guard());

    let running = update_record::read_update(&fs, &store, &stream_id)
        .map(|record| record.state.is_running())
        .unwrap_or(false)
        || held
            .as_ref()
            .map(|state| state.is_updating(&stream_id))
            .unwrap_or(false);
    if running {
        return Err(refuse(
            &app,
            "work stream update clear refused",
            &stream_id,
            ERR_UPDATE_IN_PROGRESS.to_string(),
        ));
    }
    update_record::remove_update(&fs, &store, &stream_id)?;
    logging::log_info(
        &app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "work stream update record cleared",
        log_fields! { "stream_id" => stream_id.clone() },
    );
    announce(&app, &stream.project_key);
    Ok(())
}

/// WKS-FR-WEJK / GRB-FR-TXVL: stop the update of one stream.
///
/// A stream with no update running is answered without an error: an update that
/// settled between the author reading the row and pressing the control is not a
/// failure of the request.
#[tauri::command]
pub fn cancel_work_stream_update<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
) -> Result<(), String> {
    let stopped = app
        .try_state::<StreamState>()
        .map(|state| state.cancel_update(&stream_id))
        .unwrap_or(false);
    logging::log_info(
        &app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "work stream update cancellation requested",
        log_fields! { "stream_id" => stream_id.clone(), "was_running" => stopped },
    );
    Ok(())
}
