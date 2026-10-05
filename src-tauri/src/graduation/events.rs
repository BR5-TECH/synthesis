//! What the frontend is told when a run or a queue moves (GRD-FR-EFAU).

use super::*;

pub const GRADUATION_QUEUE_CHANGED: &str = "graduation-queue-changed";
pub const GRADUATION_RUN_CHANGED: &str = "graduation-run-changed";
/// GRS-FR-UCZL: records of one run and one stream became durable.
pub const GRADUATION_LOG_RECORDS_APPENDED: &str = "graduation-log-records-appended";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueueChangedPayload {
    project_key: String,
    /// GRD-FR-EFAU: every queue-change event names the queue that changed, so a
    /// reader reloads one listing rather than all of them. Empty for the queue
    /// of an ordinary worktree.
    stream_id: String,
    /// GRD-FR-EFAU: the worktree a direct run's queue belongs to; empty for a
    /// stream run.
    worktree_path: String,
}

/// GRS-FR-UCZL: the run and the stream that grew, and no record content.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogRecordsAppendedPayload {
    run_id: String,
    stream: logs::GraduationLogStream,
    latest_sequence: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunChangedPayload {
    run_id: String,
    state: GraduationRunState,
}

/// GRS-FR-UCZL / GRS-FR-KCAK: emitted after records become durable, naming the
/// run and the **stream** that grew and carrying no record content, so a
/// consumer re-reads under its own cursor rather than rendering an event.
pub(super) fn announce_log_records<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    stream: logs::GraduationLogStream,
    latest_sequence: u64,
) {
    let _ = app.emit(
        GRADUATION_LOG_RECORDS_APPENDED,
        LogRecordsAppendedPayload {
            run_id: run_id.to_string(),
            stream,
            latest_sequence,
        },
    );
}

/// GRD-FR-EFAU: emitted after the change is durable, so a listener that
/// re-reads on it sees the result rather than racing it.
pub(super) fn announce_queue<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &GraduationRun) {
    let _ = app.emit(
        GRADUATION_QUEUE_CHANGED,
        QueueChangedPayload {
            project_key: run.project_key.clone(),
            stream_id: run.stream_id.clone(),
            worktree_path: run
                .direct_target
                .as_ref()
                .map(|target| target.worktree_path.clone())
                .unwrap_or_default(),
        },
    );
}

pub(super) fn announce_run<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &GraduationRun) {
    let _ = app.emit(
        GRADUATION_RUN_CHANGED,
        RunChangedPayload {
            run_id: run.id.clone(),
            state: run.state,
        },
    );
}
