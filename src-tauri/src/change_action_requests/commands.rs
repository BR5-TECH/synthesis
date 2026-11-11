//! The Tauri commands, the event, and the logs of change action requests
//! (CAR contract surface, CAR-FR-ZSIQ, CAR-FR-UCSJ, CAR-FR-ENOZ).

use super::*;
use crate::global_settings::GlobalSettingsStore;
use crate::project::ProjectState;
use tauri::{Emitter, Manager, State};

/// CAR-FR-ZSIQ: `"change action requests changed"`.
pub const CHANGE_ACTION_REQUESTS_CHANGED: &str = "change-action-requests-changed";

/// CAR-FR-ZSIQ: what the event is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeActionCause {
    Recorded,
    Decided,
    Refused,
}

/// CAR-FR-ZSIQ: the payload.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeActionRequestsChanged {
    pub stream_id: String,
    pub change: String,
    pub request_id: String,
    pub cause: ChangeActionCause,
}

/// CAR-FR-ZSIQ: emitted after the write is durable.
pub(super) fn announce<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    record: &ChangeActionRequest,
    cause: ChangeActionCause,
) {
    let _ = app.emit(
        CHANGE_ACTION_REQUESTS_CHANGED,
        ChangeActionRequestsChanged {
            stream_id: record.stream_id.clone(),
            change: record.change.clone(),
            request_id: record.id.clone(),
            cause,
        },
    );
}

/// What a log record names about the request it is about.
#[derive(Clone, Copy)]
pub(super) struct Subject<'a> {
    pub(super) stream_id: &'a str,
    pub(super) change: &'a str,
    pub(super) request_id: Option<&'a str>,
}

impl<'a> Subject<'a> {
    pub(super) fn of(record: &'a ChangeActionRequest) -> Self {
        Subject {
            stream_id: &record.stream_id,
            change: &record.change,
            request_id: Some(&record.id),
        }
    }

    /// A request id that names no record: the id alone, where it is well formed.
    pub(super) fn unknown(request_id: &'a str) -> Self {
        let id = is_valid_id(request_id).then_some(request_id);
        Subject { stream_id: "", change: "", request_id: id }
    }
}

/// CAR-FR-ENOZ: one `INFO` record. Ids, the action, and the state; no
/// instruction, no rationale, and no feedback.
pub(super) fn log_record<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    operation: &'static str,
    record: &ChangeActionRequest,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let mut fields = log_fields! {
        "operation" => operation,
        "streamId" => &record.stream_id,
        "change" => &record.change,
        "requestId" => &record.id,
        "action" => record.action.as_str(),
        "state" => record.state.as_str(),
    };
    if let Some(run_id) = &record.run_id {
        fields.insert("runId".to_string(), serde_json::json!(run_id));
    }
    if let Some(code) = &record.last_refusal {
        fields.insert("code".to_string(), serde_json::json!(code));
    }
    logging::log_info(app, &BUFFER, &[Domain::Backend], "change action request changed", fields);
}

/// CAR-FR-ENOZ: one `WARN` record for a refusal, with its code alone.
pub(super) fn log_refusal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    operation: &'static str,
    code: &str,
    subject: Subject<'_>,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let mut fields = log_fields! {
        "operation" => operation,
        "streamId" => subject.stream_id,
        "change" => subject.change,
        "code" => refusal_code(code),
    };
    if let Some(id) = subject.request_id {
        fields.insert("requestId".to_string(), serde_json::json!(id));
    }
    logging::log_warn(app, &BUFFER, &[Domain::Backend], "change action request refused", fields);
}

/// An `ERROR` record for a write that failed after a run started.
pub(super) fn log_error<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    operation: &'static str,
    code: &str,
    record: &ChangeActionRequest,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let mut fields = log_fields! {
        "operation" => operation,
        "streamId" => &record.stream_id,
        "change" => &record.change,
        "requestId" => &record.id,
        "code" => code,
    };
    if let Some(run_id) = &record.run_id {
        fields.insert("runId".to_string(), serde_json::json!(run_id));
    }
    logging::log_error(
        app,
        &BUFFER,
        &[Domain::Backend],
        "change action request not stored",
        fields,
    );
}

/// CAR-FR-UCSJ: the store of the open project, or `no_project_open`.
fn require_store(project: &ProjectState) -> Result<fs::RootFs, String> {
    project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    project.require_store()
}

/// CAR-FR-UCSJ / CMS-FR-12: the store and the author of a decision.
pub(super) fn decision_context<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<(fs::RootFs, Participant), String> {
    let project = app.try_state::<ProjectState>().ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    let store = require_store(&project)?;
    let settings =
        app.try_state::<GlobalSettingsStore>().ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    let by = comments::acting_participant(&settings, &project)?;
    Ok((store, by))
}

/// CAR-FR-ALHA: `"list change action requests"`.
#[tauri::command(async)]
pub fn list_change_action_requests(
    stream_id: String,
    change: String,
    project: State<'_, ProjectState>,
) -> Result<Vec<ChangeActionRequest>, String> {
    let store = require_store(&project)?;
    Ok(list_requests_impl(&store, &stream_id, &change))
}

/// CAR-FR-MMPV: `"confirm change action request"`. On a blocking thread,
/// because the start takes the graduation write guard and can reach GitHub
/// (GRD-FR-WJLO).
#[tauri::command]
pub async fn confirm_change_action_request<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    request_id: String,
) -> Result<ChangeActionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let (store, by) = decision_context(&app).inspect_err(|code| {
            log_refusal(&app, "confirm", code, Subject::unknown(&request_id))
        })?;
        confirm_action_request(&app, &store, &AppStarter { app: &app }, &request_id, &by)
    })
    .await
    .map_err(|_| ERR_WRITE_FAILED.to_string())?
}

/// CAR-FR-WXDA: `"decline change action request"`.
#[tauri::command(async)]
pub fn decline_change_action_request<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    request_id: String,
    feedback: Option<String>,
) -> Result<ChangeActionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let (store, by) = decision_context(&app)
        .inspect_err(|code| log_refusal(&app, "decline", code, Subject::unknown(&request_id)))?;
    decline_action_request(&app, &store, &request_id, feedback.as_deref(), &by)
}
