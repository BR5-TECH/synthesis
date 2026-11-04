//! Recording and updating a new-draft proposal (DCP-FR-TGRN, DCP-FR-OBAW,
//! DCP-FR-CKWC, DCP-FR-GDTC, DCP-FR-ZJRW).

use crate::comments::{self, Attachment, NewDraftUpdateAction, Participant, ThreadRef};
use crate::draft_proposals::draft_lock;
use crate::notes::{new_note_id, now_rfc3339};

use super::*;

/// DCP-FR-OBAW: a blank name or body, a position less than 1, and a total less
/// than the position are each a typed validation error.
fn validate(name: &str, body: &str, position: i64, total: i64) -> Result<(u32, u32), String> {
    if name.trim().is_empty() {
        return Err(ERR_BLANK_NAME.into());
    }
    if body.trim().is_empty() {
        return Err(ERR_BLANK_BODY.into());
    }
    if position < 1 || total < position || total > i64::from(u32::MAX) {
        return Err(ERR_BAD_POSITION.into());
    }
    Ok((position as u32, total as u32))
}

/// DCP-FR-CKWC: the set id of the draft's decomposition, minted by the first
/// recording and read from the existing records afterwards.
fn set_id_of(root: &fs::RootFs, draft_id: &str) -> String {
    list_impl(root, draft_id)
        .ok()
        .and_then(|all| all.last().map(|p| p.set_id.clone()))
        .unwrap_or_else(new_note_id)
}

/// Append the agent comment that carries the one `new_draft_proposal`
/// attachment (DCP-FR-TGRN, CMS-FR-LIBT).
fn append_reference(
    store: &fs::RootFs,
    target: ThreadRef<'_>,
    thread_id: &str,
    comment_id: String,
    rationale: &str,
    agent: &Participant,
    reference: (&str, &str, Option<NewDraftUpdateAction>),
) -> Result<comments::Discussion, String> {
    let (proposal_id, draft_id, update) = reference;
    let attachment = Attachment::NewDraftProposal {
        proposal_id: proposal_id.to_string(),
        draft_id: draft_id.to_string(),
        update,
    };
    comments::append_agent_comment_with(
        store,
        target,
        thread_id,
        comment_id,
        rationale.to_string(),
        vec![attachment],
        comments::Stamp { by: agent, at: &now_rfc3339() },
    )
    .map_err(|e| {
        if e == comments::ERR_DISCUSSION_RESOLVED {
            ERR_DISCUSSION_RESOLVED.to_string()
        } else {
            ERR_WRITE_FAILED.to_string()
        }
    })
}

/// DCP-FR-TGRN: the **only** path by which a new-draft proposal comes into
/// existence. The body, then the record, then the comment; a failed append
/// removes both files. Called by `crate::tools::propose_new_draft` alone.
pub fn record_new_draft_proposal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
    input: NewDraftInput<'_>,
) -> Result<NewDraftProposal, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let draft_id = input.draft_id;
    // DCP-FR-KUSI: the pending check and the writes are one act per draft.
    let _guard = draft_lock(draft_id);
    let outcome = (|| {
        require_open_original(app, root, draft_id)?;
        if pending_for(root, draft_id).is_some() {
            return Err(ERR_PROPOSAL_PENDING.to_string());
        }
        let (position, total) = validate(input.name, input.body, input.position, input.total)?;
        let dir = dir_of(root, draft_id)?;
        let id = new_note_id();
        let comment_id = new_note_id();
        let record = NewDraftProposal {
            id: id.clone(),
            set_id: set_id_of(root, draft_id),
            draft_id: draft_id.to_string(),
            name: input.name.trim().to_string(),
            position,
            total,
            rationale: input.rationale.to_string(),
            thread_id: input.thread_id.to_string(),
            comment_id: comment_id.clone(),
            state: NewDraftState::Pending,
            revision: 1,
            created_draft_id: None,
            created_at: now_rfc3339(),
            decided_at: None,
            agent: input.agent.clone(),
        };
        let cleanup = || {
            let _ = root.delete_under(&dir, format!("{id}.{BODY_EXT}"), false);
            let _ = root.delete_under(&dir, format!("{id}.{RECORD_EXT}"), false);
        };
        root.write_text_atomic(body_path(&dir, &id), input.body)
            .map_err(|_| ERR_WRITE_FAILED.to_string())?;
        if root.write_toml_atomic(record_path(&dir, &id), &record).is_err() {
            cleanup();
            return Err(ERR_WRITE_FAILED.to_string());
        }
        // DCP-FR-VQWK: private draft storage, kept out of Git.
        drafts::git_storage::ensure_private_ignored(root);
        let thread = append_reference(
            store,
            input.target,
            input.thread_id,
            comment_id,
            input.rationale,
            input.agent,
            (&id, draft_id, None),
        )
        .inspect_err(|_| cleanup())?;
        Ok((record, thread))
    })();
    match outcome {
        Ok((record, thread)) => {
            comments::emit_discussion_changed(app, &thread);
            announce(app, draft_id, Some(&record), NewDraftChangeCause::Recorded);
            Ok(record)
        }
        Err(reason) => {
            log_refusal(app, "record", draft_id, &reason);
            Err(reason)
        }
    }
}

/// DCP-FR-GDTC: change or withdraw the draft's pending new-draft proposal, and
/// append one comment into the turn's conversation, as one act.
#[allow(clippy::too_many_arguments)] // The arguments are the contract surface's, one each.
pub fn update_new_draft_proposal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
    draft_id: &str,
    update: NewDraftUpdate,
    rationale: &str,
    agent: &Participant,
    conversation: (ThreadRef<'_>, &str),
) -> Result<NewDraftProposal, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let _guard = draft_lock(draft_id);
    let outcome = (|| {
        require_open_original(app, root, draft_id)?;
        let mut record = pending_for(root, draft_id).ok_or(ERR_NO_OPEN_PROPOSAL)?;
        let dir = dir_of(root, draft_id)?;
        let old_record = root.read_text(record_path(&dir, &record.id)).ok();
        let old_body = root.read_text(body_path(&dir, &record.id)).ok();
        let (action, next_body) = match update {
            NewDraftUpdate::Withdraw => {
                record.state = NewDraftState::Withdrawn;
                record.decided_at = Some(now_rfc3339());
                (NewDraftUpdateAction::Withdraw, None)
            }
            NewDraftUpdate::Change { name, body, position, total } => {
                let current_body = old_body.clone().unwrap_or_default();
                let next_name = name.unwrap_or_else(|| record.name.clone());
                let next_body = body.unwrap_or_else(|| current_body.clone());
                let next_position = position.unwrap_or(i64::from(record.position));
                let next_total = total.unwrap_or(i64::from(record.total));
                let (p, t) = validate(&next_name, &next_body, next_position, next_total)?;
                let next_name = next_name.trim().to_string();
                if next_name == record.name
                    && next_body == current_body
                    && p == record.position
                    && t == record.total
                {
                    return Err(ERR_NO_CHANGE.to_string());
                }
                record.name = next_name;
                record.position = p;
                record.total = t;
                record.revision += 1;
                (NewDraftUpdateAction::Change, Some(next_body))
            }
        };
        let restore = || {
            if let Some(text) = &old_body {
                let _ = root.write_text_atomic(body_path(&dir, &record.id), text);
            }
            if let Some(text) = &old_record {
                let _ = root.write_text_atomic(record_path(&dir, &record.id), text);
            }
        };
        if let Some(body) = &next_body {
            root.write_text_atomic(body_path(&dir, &record.id), body)
                .map_err(|_| ERR_WRITE_FAILED.to_string())?;
        }
        if root.write_toml_atomic(record_path(&dir, &record.id), &record).is_err() {
            restore();
            return Err(ERR_WRITE_FAILED.to_string());
        }
        let (target, thread_id) = conversation;
        let thread = append_reference(
            store,
            target,
            thread_id,
            new_note_id(),
            rationale,
            agent,
            (&record.id, draft_id, Some(action)),
        )
        .inspect_err(|_| restore())?;
        Ok((record, thread))
    })();
    match outcome {
        Ok((record, thread)) => {
            comments::emit_discussion_changed(app, &thread);
            announce(app, draft_id, Some(&record), NewDraftChangeCause::Updated);
            Ok(record)
        }
        Err(reason) => {
            log_refusal(app, "update", draft_id, &reason);
            Err(reason)
        }
    }
}
