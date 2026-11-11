//! Recording a change action request (CAR-FR-KZOZ, CAR-FR-MNTP).

use super::*;

/// What a caller hands [`record_change_action_request`]: the change, the
/// action, the instruction of a rework, the rationale, the agent, and the
/// thread of the turn's conversation (the origin).
pub struct NewActionRequest<'a> {
    pub stream_id: &'a str,
    pub change: &'a str,
    pub action: ChangeAction,
    pub instruction: Option<&'a str>,
    pub rationale: &'a str,
    pub agent: &'a Participant,
    pub thread_id: &'a str,
}

/// CAR-FR-KZOZ: write the record, then append one agent comment with the
/// rationale and one `change_action_request` attachment. A failed append
/// removes the record. CAR-FR-MNTP: a refusal writes nothing.
pub fn record_change_action_request<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    new: NewActionRequest<'_>,
) -> Result<ChangeActionRequest, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let _guard = change_lock(new.stream_id, new.change);
    let outcome = record_locked(app, store, sites, &new);
    match &outcome {
        Ok(record) => log_record(app, "recorded", record),
        Err(code) => log_refusal(
            app,
            "record",
            code,
            Subject { stream_id: new.stream_id, change: new.change, request_id: None },
        ),
    }
    outcome
}

fn record_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    new: &NewActionRequest<'_>,
) -> Result<ChangeActionRequest, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    // CAR-FR-MNTP: in the order the requirement gives.
    if pending_request(store, new.stream_id, new.change).is_some() {
        return Err(ERR_ACTION_REQUEST_PENDING.to_string());
    }
    let thread = locate_thread(store, new.stream_id, new.change, new.thread_id)
        .ok_or_else(|| comments::ERR_DISCUSSION_NOT_FOUND.to_string())?;
    if thread.resolved {
        return Err(ERR_DISCUSSION_RESOLVED.to_string());
    }
    sites.site(new.stream_id, new.change)?;
    let instruction = match new.action {
        ChangeAction::Apply => None,
        ChangeAction::Rework => {
            let trimmed = new.instruction.map(str::trim).unwrap_or_default();
            if trimmed.is_empty() {
                return Err(ERR_BLANK_INSTRUCTION.to_string());
            }
            Some(trimmed.to_string())
        }
    };
    let dir = requests_dir(store, new.stream_id, new.change)?;
    let now = now_rfc3339();
    let record = ChangeActionRequest {
        id: new_note_id(),
        stream_id: new.stream_id.to_string(),
        change: new.change.to_string(),
        action: new.action,
        instruction,
        rationale: new.rationale.to_string(),
        thread_id: new.thread_id.to_string(),
        comment_id: new_note_id(),
        state: ChangeActionRequestState::Pending,
        run_id: None,
        last_refusal: None,
        created_at: now.clone(),
        decided_at: None,
        agent: new.agent.clone(),
    };
    let remove = || {
        let _ = store.delete_under(&dir, format!("{}.toml", record.id), false);
    };
    if write_record(store, &dir, &record).is_err() {
        remove();
        return Err(ERR_WRITE_FAILED.to_string());
    }
    let attachment = comments::Attachment::ChangeActionRequest {
        request_id: record.id.clone(),
        stream_id: record.stream_id.clone(),
        change: record.change.clone(),
        action: record.action.as_str().to_string(),
    };
    match append_comment(
        store,
        &thread,
        &record,
        vec![attachment],
        comments::Stamp { by: new.agent, at: &now },
    ) {
        Ok(appended) => comments::emit_discussion_changed(app, &appended),
        Err(error) => {
            remove();
            return Err(if error == comments::ERR_DISCUSSION_RESOLVED {
                ERR_DISCUSSION_RESOLVED.to_string()
            } else {
                ERR_WRITE_FAILED.to_string()
            });
        }
    }
    announce(app, &record, ChangeActionCause::Recorded);
    Ok(record)
}

/// The one agent comment of a recording (CAR-FR-KZOZ).
fn append_comment(
    store: &fs::RootFs,
    thread: &comments::Discussion,
    record: &ChangeActionRequest,
    attachments: Vec<comments::Attachment>,
    stamp: comments::Stamp<'_>,
) -> Result<comments::Discussion, String> {
    #[cfg(test)]
    if FAIL_APPEND.with(std::cell::Cell::take) {
        return Err(ERR_WRITE_FAILED.to_string());
    }
    comments::append_agent_comment_with(
        store,
        thread.log_ref(),
        &record.thread_id,
        record.comment_id.clone(),
        record.rationale.clone(),
        attachments,
        stamp,
    )
}

#[cfg(test)]
thread_local! {
    /// A test sets it to make the next comment append fail.
    pub(crate) static FAIL_APPEND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
