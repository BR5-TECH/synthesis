//! Export requests (`DXP-document-export.md` DXP-FR-YEAQ to DXP-FR-IMSX).
//!
//! An agent can not choose where a file goes on the author's machine, so the
//! tool `crate::tools::request_document_export` does not export. It records a
//! request here and posts it into its conversation. The author exports from the
//! card, and the outcome comes back as a comment of the author.
//!
//! On-disk layout, under the store root of the conversation (DXP-FR-YEAQ):
//!
//! ```text
//! export-requests/<request-id>.toml
//! ```

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

use super::*;
use crate::agent_conversations::ConversationOrigin;
use crate::comments::{self, Attachment, DiscussionTarget, Participant};
use crate::fs;
use crate::global_settings::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, Domain, LogBuffer, LogSink, BUFFER};
use crate::notes::{new_note_id, now_rfc3339};
use crate::project::ProjectState;

#[cfg(test)]
mod tests;

/// DXP-FR-YEAQ: the folder of the records under the store root.
pub const REQUESTS_DIR: &str = "export-requests";

/// DXP-FR-FVTK: the event every change of a request emits.
pub const EXPORT_REQUEST_CHANGED: &str = "export-request-changed";

/// The typed error of a conversation that takes no further comment.
pub const ERR_DISCUSSION_RESOLVED: &str = comments::ERR_DISCUSSION_RESOLVED;

/// DXP-FR-YEAQ: the state of a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportRequestState {
    Pending,
    Exported,
    Cancelled,
    Failed,
}

impl ExportRequestState {
    pub fn as_str(self) -> &'static str {
        match self {
            ExportRequestState::Pending => "pending",
            ExportRequestState::Exported => "exported",
            ExportRequestState::Cancelled => "cancelled",
            ExportRequestState::Failed => "failed",
        }
    }
}

/// DXP-FR-YEAQ: one export request, as stored and as served.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub id: String,
    /// Project-relative path of the DOC artifact.
    pub path: String,
    /// `pdf` or `docx`: the format the agent gave.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    pub discussion_id: String,
    pub target: DiscussionTarget,
    pub state: ExportRequestState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<String>,
}

/// DXP-FR-VFFP: what a resolution returns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResolution {
    pub request: ExportRequest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_id: Option<String>,
    /// DXP-FR-UOFT: the outcome comment was not appended, so the conversation
    /// was not told.
    pub comment_failed: bool,
}

/// One comment this module appends: the agent's request (DXP-FR-XZVZ) or the
/// author's outcome (DXP-FR-VFFP).
pub(crate) struct CommentWrite<'a> {
    pub discussion: &'a comments::Discussion,
    pub comment_id: String,
    pub body: String,
    pub attachments: Vec<Attachment>,
    pub by: &'a Participant,
    pub at: &'a str,
}

/// The writer of one comment. The tests give one that fails.
pub(crate) type Appender<'a> =
    &'a dyn Fn(&fs::RootFs, CommentWrite<'_>) -> Result<comments::Discussion, String>;

/// DXP-FR-XZVZ: the agent comment that carries the request.
fn append_agent(store: &fs::RootFs, w: CommentWrite<'_>) -> Result<comments::Discussion, String> {
    comments::append_agent_comment_with(
        store,
        w.discussion.log_ref(),
        &w.discussion.id,
        w.comment_id,
        w.body,
        w.attachments,
        comments::Stamp { by: w.by, at: w.at },
    )
}

/// DXP-FR-VFFP: the author comment that states the outcome.
fn append_author(store: &fs::RootFs, w: CommentWrite<'_>) -> Result<comments::Discussion, String> {
    comments::add_comment_with_id(
        store,
        w.discussion.log_ref(),
        &w.discussion.id,
        w.comment_id,
        w.body,
        w.by,
        w.at,
    )
}

/// What `record_export_request` needs (DXP-FR-XZVZ).
pub struct ExportRequestInput<'a> {
    pub path: &'a str,
    pub format: Option<&'a str>,
    /// The comment body: the agent's message, already prepared for display.
    pub message: &'a str,
    pub agent: &'a Participant,
    pub origin: &'a ConversationOrigin,
}

/// DXP-FR-LHAG: the outcome a resolution asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportOutcome {
    Exported { destination: String },
    Cancelled,
    Failed { error: String },
}

impl ExportOutcome {
    /// The outcome of the wire arguments, or `invalid_outcome`.
    pub fn parse(
        outcome: &str,
        destination: Option<String>,
        error: Option<String>,
    ) -> Result<Self, String> {
        let text = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        match outcome {
            "exported" => text(destination)
                .map(|destination| ExportOutcome::Exported { destination })
                .ok_or_else(|| ERR_INVALID_OUTCOME.to_string()),
            "cancelled" => Ok(ExportOutcome::Cancelled),
            "failed" => Ok(ExportOutcome::Failed {
                error: text(error).unwrap_or_else(|| "unknown error".to_string()),
            }),
            _ => Err(ERR_INVALID_OUTCOME.to_string()),
        }
    }

    fn state(&self) -> ExportRequestState {
        match self {
            ExportOutcome::Exported { .. } => ExportRequestState::Exported,
            ExportOutcome::Cancelled => ExportRequestState::Cancelled,
            ExportOutcome::Failed { .. } => ExportRequestState::Failed,
        }
    }
}

/// One resolution at a time, so a request is resolved once (DXP-FR-LHAG).
static RESOLVE_LOCK: Mutex<()> = Mutex::new(());

fn is_valid_id(id: &str) -> bool {
    crate::draft_proposals::is_valid_proposal_id(id)
}

fn record_path(id: &str) -> PathBuf {
    PathBuf::from(REQUESTS_DIR).join(format!("{id}.toml"))
}

/// The file name of a project-relative path, for a comment body.
fn file_name(path: &str) -> &str {
    path.rsplit('/').next().filter(|n| !n.is_empty()).unwrap_or(path)
}

/// DXP-FR-VFFP: the body of the outcome comment.
pub fn outcome_body(path: &str, outcome: &ExportOutcome) -> String {
    let name = file_name(path);
    match outcome {
        ExportOutcome::Exported { destination } => {
            format!("Exported “{name}” to `{destination}`.")
        }
        ExportOutcome::Cancelled => format!("Cancelled the export of “{name}”."),
        ExportOutcome::Failed { error } => {
            // The error is a sentence of its own, so it keeps its own stop.
            let error = error.trim().trim_end_matches('.');
            format!("The export of “{name}” failed: {error}.")
        }
    }
}

fn announce<R: tauri::Runtime>(app: &tauri::AppHandle<R>, request: &ExportRequest) {
    let _ = app.emit(EXPORT_REQUEST_CHANGED, request);
}

/// DXP-FR-IMSX: the request id, the format, and the outcome; never the path,
/// the destination, the error text, or the message.
fn log_request<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    message: &str,
    request: &ExportRequest,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    logging::log_info(
        app,
        buffer,
        &[Domain::Backend],
        message,
        log_fields! {
            "requestId" => &request.id,
            "format" => request.format.as_deref().unwrap_or("any"),
            "state" => request.state.as_str(),
        },
    );
}

fn log_refusal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    operation: &str,
    request_id: &str,
    reason: &str,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    logging::log_warn(
        app,
        buffer,
        &[Domain::Backend],
        "export request refused",
        log_fields! {
            "operation" => operation,
            "requestId" => request_id,
            "reason" => reason,
        },
    );
}

/// DXP-FR-XZVZ: write a `pending` record, then append the agent comment that
/// carries the one `export_request` attachment (CMS-FR-OKDW). A failed append
/// removes the record, so neither remains.
pub fn record_export_request<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    input: ExportRequestInput<'_>,
) -> Result<ExportRequest, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    record_with(app, &BUFFER, store, input)
}

pub(crate) fn record_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    store: &fs::RootFs,
    input: ExportRequestInput<'_>,
) -> Result<ExportRequest, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    record_using(app, buffer, store, input, &append_agent)
}

/// `record_with`, with the comment written by `append`.
pub(crate) fn record_using<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    store: &fs::RootFs,
    input: ExportRequestInput<'_>,
    append: Appender<'_>,
) -> Result<ExportRequest, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let origin = input.origin;
    let discussion = comments::locate_discussion_of(store, &origin.target, origin.discussion_id())
        .ok_or_else(|| ERR_WRITE_FAILED.to_string())?;
    if discussion.resolved {
        return Err(ERR_DISCUSSION_RESOLVED.to_string());
    }
    let now = now_rfc3339();
    let request = ExportRequest {
        id: new_note_id(),
        path: input.path.to_string(),
        format: input.format.map(str::to_string),
        discussion_id: origin.discussion_id().to_string(),
        target: origin.target.clone(),
        state: ExportRequestState::Pending,
        destination: None,
        error: None,
        created_at: now.clone(),
        resolved_at: None,
    };
    let path = record_path(&request.id);
    store.write_toml_atomic(&path, &request).map_err(|_| ERR_WRITE_FAILED.to_string())?;
    let attachment = Attachment::ExportRequest {
        request_id: request.id.clone(),
        path: request.path.clone(),
        format: request.format.clone(),
    };
    let appended = append(
        store,
        CommentWrite {
            discussion: &discussion,
            comment_id: new_note_id(),
            body: input.message.to_string(),
            attachments: vec![attachment],
            by: input.agent,
            at: &now,
        },
    );
    let thread = match appended {
        Ok(thread) => thread,
        Err(reason) => {
            // DXP-FR-XZVZ: neither the record nor the comment remains.
            if store.delete_under(store.path(), &path, false).is_err() {
                log_refusal(app, buffer, "record", &request.id, "record_not_removed");
            }
            log_refusal(app, buffer, "record", &request.id, append_failure(&reason));
            if reason == comments::ERR_DISCUSSION_RESOLVED {
                return Err(ERR_DISCUSSION_RESOLVED.to_string());
            }
            return Err(ERR_WRITE_FAILED.to_string());
        }
    };
    comments::emit_discussion_changed(app, &thread);
    announce(app, &request);
    log_request(app, buffer, "export request recorded", &request);
    Ok(request)
}

/// DXP-FR-NXGQ: the record of `request_id`.
pub fn get_export_request_impl(
    store: &fs::RootFs,
    request_id: &str,
) -> Result<ExportRequest, String> {
    if !is_valid_id(request_id) {
        return Err(ERR_REQUEST_NOT_FOUND.to_string());
    }
    store
        .read_toml::<ExportRequest>(record_path(request_id))
        .map_err(|_| ERR_REQUEST_NOT_FOUND.to_string())
}

/// DXP-FR-LHAG / DXP-FR-VFFP / DXP-FR-FVTK: resolve a `pending` request as the
/// participant `by`.
pub fn resolve_export_request_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    store: &fs::RootFs,
    request_id: &str,
    outcome: ExportOutcome,
    by: &Participant,
) -> Result<ExportResolution, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    resolve_using(app, buffer, store, request_id, outcome, by, &append_author)
}

/// `resolve_export_request_impl`, with the comment written by `append`.
pub(crate) fn resolve_using<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    store: &fs::RootFs,
    request_id: &str,
    outcome: ExportOutcome,
    by: &Participant,
    append: Appender<'_>,
) -> Result<ExportResolution, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let guard = RESOLVE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let outcome = resolve_locked(store, request_id, outcome, by, append);
    drop(guard);
    match outcome {
        Ok((resolution, thread, append_error)) => {
            if let Some(reason) = append_error {
                // DXP-FR-UOFT: the state landed, and the conversation was not
                // told. The request id and the typed code alone.
                logging::log_warn(
                    app,
                    buffer,
                    &[Domain::Backend],
                    "the outcome comment of an export request was not appended",
                    log_fields! {
                        "requestId" => &resolution.request.id,
                        "error" => append_failure(&reason),
                    },
                );
            }
            if let Some(thread) = &thread {
                crate::agent_conversations::retire_recoverable_for_human_comment(app, &thread.id);
                comments::emit_discussion_changed(app, thread);
            }
            announce(app, &resolution.request);
            log_request(app, buffer, "export request resolved", &resolution.request);
            Ok(resolution)
        }
        Err(reason) => {
            log_refusal(app, buffer, "resolve", request_id, &reason);
            Err(reason)
        }
    }
}

/// The typed code of a failed append, for the log. Any other text is a
/// message of the file system, which can carry a path.
fn append_failure(reason: &str) -> &'static str {
    if reason == comments::ERR_DISCUSSION_RESOLVED {
        comments::ERR_DISCUSSION_RESOLVED
    } else {
        ERR_WRITE_FAILED
    }
}

type Resolved = (ExportResolution, Option<comments::Discussion>, Option<String>);

fn resolve_locked(
    store: &fs::RootFs,
    request_id: &str,
    outcome: ExportOutcome,
    by: &Participant,
    append: Appender<'_>,
) -> Result<Resolved, String> {
    let mut request = get_export_request_impl(store, request_id)?;
    if request.state != ExportRequestState::Pending {
        return Err(ERR_ALREADY_RESOLVED.to_string());
    }
    let now = now_rfc3339();
    request.state = outcome.state();
    match &outcome {
        ExportOutcome::Exported { destination } => request.destination = Some(destination.clone()),
        ExportOutcome::Failed { error } => request.error = Some(error.clone()),
        ExportOutcome::Cancelled => {}
    }
    request.resolved_at = Some(now.clone());
    store
        .write_toml_atomic(record_path(&request.id), &request)
        .map_err(|_| ERR_WRITE_FAILED.to_string())?;
    // DXP-FR-FVTK: a discussion that is gone or resolved takes no comment, and
    // the state lands all the same.
    let Some(discussion) =
        comments::locate_discussion_of(store, &request.target, &request.discussion_id)
            .filter(|d| !d.resolved)
    else {
        let resolution = ExportResolution { request, comment_id: None, comment_failed: false };
        return Ok((resolution, None, None));
    };
    let comment_id = new_note_id();
    let appended = append(
        store,
        CommentWrite {
            discussion: &discussion,
            comment_id: comment_id.clone(),
            body: outcome_body(&request.path, &outcome),
            attachments: Vec::new(),
            by,
            at: &now,
        },
    );
    Ok(match appended {
        Ok(thread) => (
            ExportResolution { request, comment_id: Some(comment_id), comment_failed: false },
            Some(thread),
            None,
        ),
        // DXP-FR-FVTK: the discussion was resolved after the check above. It
        // takes no comment, as a discussion that was resolved before does not.
        Err(reason) if reason == comments::ERR_DISCUSSION_RESOLVED => {
            (ExportResolution { request, comment_id: None, comment_failed: false }, None, None)
        }
        // DXP-FR-UOFT: the state stands, and the caller learns that the
        // conversation was not told.
        Err(reason) => (
            ExportResolution { request, comment_id: None, comment_failed: true },
            None,
            Some(reason),
        ),
    })
}

/// DXP contract surface: `"get export request (id)"`.
#[tauri::command]
pub fn get_export_request(
    request_id: String,
    project: State<'_, ProjectState>,
) -> Result<ExportRequest, String> {
    let store = project.require_store().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    get_export_request_impl(&store, &request_id)
}

/// DXP contract surface: `"resolve export request (id, outcome, destination, error)"`.
#[tauri::command]
pub fn resolve_export_request<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    request_id: String,
    outcome: String,
    destination: Option<String>,
    error: Option<String>,
    settings: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<ExportResolution, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let store = project.require_store().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    let outcome = ExportOutcome::parse(&outcome, destination, error)?;
    let by = comments::acting_participant(&settings, &project)?;
    resolve_export_request_impl(&app, &BUFFER, &store, &request_id, outcome, &by)
}
