//! The agent surface records (`AUI-agent-ui-surfaces.md` AUI-FR-IAAP to
//! AUI-FR-BPDT).

use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::Value;

use super::*;
use crate::agent_conversations::ConversationOrigin;
use crate::comments::{self, Attachment};
use crate::fs;
use crate::log_fields;
use crate::logging::{self, Domain, LogBuffer, LogSink};
use crate::notes::{new_note_id, now_rfc3339};
use crate::project_scope::events::EmitToProject;

/// AUI-FR-EGAU: one action at a time, so each surface is submitted once.
static SUBMIT_LOCK: Mutex<()> = Mutex::new(());

/// One comment this module appends: the agent's surface (AUI-FR-QRCX) or the
/// author's action (AUI-FR-MAXD).
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

/// AUI-FR-MAXD: the human write path (CMS-FR-11).
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

/// What `record_agent_surface` needs (AUI-FR-QRCX).
pub struct SurfaceInput<'a> {
    pub components: &'a [Value],
    pub data_model: &'a Value,
    /// The comment body: the agent's message, already prepared for display.
    pub message: &'a str,
    pub agent: &'a Participant,
    pub origin: &'a ConversationOrigin,
}

/// Why `record_agent_surface` refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordError {
    /// AUI-FR-ZHXO: the surface is not valid, and nothing was written.
    Invalid(SurfaceViolation),
    /// The discussion is resolved and takes no comment.
    DiscussionResolved,
    /// The record or the comment was not written, and neither remains.
    WriteFailed,
}

fn is_valid_id(id: &str) -> bool {
    crate::draft_proposals::is_valid_proposal_id(id)
}

fn record_path(id: &str) -> PathBuf {
    PathBuf::from(SURFACES_DIR).join(format!("{id}.json"))
}

fn write_record(store: &fs::RootFs, surface: &AgentSurface) -> Result<(), String> {
    let text = serde_json::to_string_pretty(surface).map_err(|_| ERR_WRITE_FAILED.to_string())?;
    store
        .write_text_atomic(record_path(&surface.id), &text)
        .map_err(|_| ERR_WRITE_FAILED.to_string())
}

fn announce<R: tauri::Runtime>(app: &tauri::AppHandle<R>, surface: &AgentSurface) {
    let _ = app.emit_project(AGENT_SURFACE_CHANGED, surface);
}

/// AUI-FR-XSAO / AUI-FR-DNZK: ids and counts alone.
fn log_warn<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    message: &str,
    surface_id: &str,
    error: &str,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    logging::log_warn(
        app,
        buffer,
        &[Domain::Backend],
        message,
        log_fields! { "surfaceId" => surface_id, "error" => error },
    );
}

/// AUI-FR-QRCX: validate, write an `open` record, then append the agent
/// comment that carries the one `agent_surface` attachment (CMS-FR-RWQQ).
pub fn record_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    store: &fs::RootFs,
    input: SurfaceInput<'_>,
) -> Result<AgentSurface, RecordError>
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
    input: SurfaceInput<'_>,
    append: Appender<'_>,
) -> Result<AgentSurface, RecordError>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    // AUI-FR-ZHXO: an invalid surface writes nothing.
    validate_surface(input.components, input.data_model).map_err(RecordError::Invalid)?;
    let origin = input.origin;
    let discussion = comments::locate_discussion_of(store, &origin.target, origin.discussion_id())
        .ok_or(RecordError::WriteFailed)?;
    if discussion.resolved {
        return Err(RecordError::DiscussionResolved);
    }
    let now = now_rfc3339();
    let comment_id = new_note_id();
    let surface = AgentSurface {
        id: new_note_id(),
        discussion_id: origin.discussion_id().to_string(),
        target: origin.target.clone(),
        comment_id: comment_id.clone(),
        catalog_id: CATALOG_ID.to_string(),
        components: input.components.to_vec(),
        data_model: input.data_model.clone(),
        authored_by: input.agent.clone(),
        created_at: now.clone(),
        state: SurfaceState::Open,
        submission: None,
    };
    write_record(store, &surface).map_err(|_| RecordError::WriteFailed)?;
    let appended = append(
        store,
        CommentWrite {
            discussion: &discussion,
            comment_id,
            body: input.message.to_string(),
            attachments: vec![Attachment::AgentSurface { surface_id: surface.id.clone() }],
            by: input.agent,
            at: &now,
        },
    );
    let thread = match appended {
        Ok(thread) => thread,
        Err(reason) => {
            // AUI-FR-ZHXO: neither the record nor the comment remains.
            let path = record_path(&surface.id);
            if store.delete_under(store.path(), &path, false).is_err() {
                log_warn(
                    app,
                    buffer,
                    "agent surface record not removed",
                    &surface.id,
                    ERR_WRITE_FAILED,
                );
            }
            let resolved = reason == comments::ERR_DISCUSSION_RESOLVED;
            let code = if resolved { ERR_DISCUSSION_RESOLVED } else { ERR_WRITE_FAILED };
            log_warn(app, buffer, "agent surface comment not appended", &surface.id, code);
            return Err(if resolved {
                RecordError::DiscussionResolved
            } else {
                RecordError::WriteFailed
            });
        }
    };
    comments::emit_discussion_changed(app, &thread);
    announce(app, &surface);
    // AUI-FR-XSAO.
    logging::log_info(
        app,
        buffer,
        &[Domain::Backend],
        "agent surface recorded",
        log_fields! {
            "surfaceId" => &surface.id,
            "discussionId" => &surface.discussion_id,
            "components" => surface.components.len(),
        },
    );
    Ok(surface)
}

/// AUI-FR-QBXZ: the record of `surface_id`.
pub fn get_impl(store: &fs::RootFs, surface_id: &str) -> Result<AgentSurface, String> {
    if !is_valid_id(surface_id) {
        return Err(ERR_SURFACE_NOT_FOUND.to_string());
    }
    let text =
        store.read_text(record_path(surface_id)).map_err(|_| ERR_SURFACE_NOT_FOUND.to_string())?;
    serde_json::from_str(&text).map_err(|_| ERR_SURFACE_NOT_FOUND.to_string())
}

/// AUI-FR-KBVY to AUI-FR-GTXG: accept one action as the participant `by`.
pub fn submit_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    store: &fs::RootFs,
    surface_id: &str,
    action: SurfaceAction,
    by: &Participant,
) -> Result<SurfaceSubmitResult, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    submit_using(app, buffer, store, surface_id, action, by, &append_author)
}

/// `submit_impl`, with the comment written by `append`.
pub(crate) fn submit_using<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    store: &fs::RootFs,
    surface_id: &str,
    action: SurfaceAction,
    by: &Participant,
    append: Appender<'_>,
) -> Result<SurfaceSubmitResult, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let guard = SUBMIT_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let outcome = submit_locked(store, surface_id, action, by, append);
    drop(guard);
    match outcome {
        Ok((result, thread)) => {
            crate::agent_conversations::retire_recoverable_for_human_comment(app, &thread.id);
            comments::emit_discussion_changed(app, &thread);
            announce(app, &result.surface);
            logging::log_info(
                app,
                buffer,
                &[Domain::Backend],
                "agent surface action accepted",
                log_fields! { "surfaceId" => &result.surface.id },
            );
            Ok(result)
        }
        Err(reason) => {
            log_warn(app, buffer, "agent surface action refused", surface_id, &reason);
            Err(reason)
        }
    }
}

fn submit_locked(
    store: &fs::RootFs,
    surface_id: &str,
    action: SurfaceAction,
    by: &Participant,
    append: Appender<'_>,
) -> Result<(SurfaceSubmitResult, comments::Discussion), String> {
    let open = get_impl(store, surface_id)?;
    if open.state != SurfaceState::Open {
        return Err(ERR_ALREADY_SUBMITTED.to_string());
    }
    let body = super::actions::accept(&open, &action)?;
    // AUI-FR-GTXG: a discussion that is gone or resolved takes no action.
    let discussion = comments::locate_discussion_of(store, &open.target, &open.discussion_id)
        .filter(|d| !d.resolved)
        .ok_or_else(|| ERR_DISCUSSION_RESOLVED.to_string())?;
    let now = now_rfc3339();
    let comment_id = new_note_id();
    let mut submitted = open.clone();
    submitted.state = SurfaceState::Submitted;
    submitted.submission = Some(SurfaceSubmission {
        name: action.name,
        source_component_id: action.source_component_id,
        context: action.context,
        data_model: action.data_model,
        by: by.clone(),
        at: now.clone(),
        comment_id: comment_id.clone(),
    });
    write_record(store, &submitted)?;
    let appended = append(
        store,
        CommentWrite {
            discussion: &discussion,
            comment_id: comment_id.clone(),
            body,
            attachments: Vec::new(),
            by,
            at: &now,
        },
    );
    match appended {
        Ok(thread) => Ok((SurfaceSubmitResult { surface: submitted, comment_id }, thread)),
        Err(reason) => {
            // AUI-FR-FWQX: the record goes back to `open`. Where even that
            // write fails, the caller learns `write_failed` and the log says so.
            write_record(store, &open)?;
            if reason == comments::ERR_DISCUSSION_RESOLVED {
                return Err(ERR_DISCUSSION_RESOLVED.to_string());
            }
            Err(ERR_COMMENT_FAILED.to_string())
        }
    }
}
