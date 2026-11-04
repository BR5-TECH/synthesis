//! Accepting and rejecting a new-draft proposal (DCP-FR-KOOA, DCP-FR-AQDM,
//! DCP-FR-FZEP, DCP-FR-PDCP, DCP-FR-EEDK, DCP-FR-EJWZ).
//!
//! The Tauri commands and `crate::tools::decide_draft_changes` both call the
//! functions here, so a decision by tool passes the same checks, emits the
//! same event, and appends the same decision comment (DCP-FR-EJWZ).

use crate::comments::{self, DiscussionTarget, Participant};
use crate::draft_proposals::{
    draft_lock, thread_ref, ORIGIN_DRAFT_COMMENT, ORIGIN_DRAFT_DISCUSSION,
};
use crate::notes::{new_note_id, now_rfc3339};

use super::*;

/// Which way a decision went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewDraftDecision {
    Accept,
    Reject,
}

/// DCP-FR-PDCP: the body of the decision comment. It states the decision, the
/// proposed name, the position and the total, and carries any feedback.
pub fn decision_body(
    decision: NewDraftDecision,
    proposal: &NewDraftProposal,
    feedback: Option<&str>,
) -> String {
    let (word, tail) = match decision {
        NewDraftDecision::Accept => ("Accepted", "The new draft is created."),
        NewDraftDecision::Reject => ("Rejected", "No draft is created."),
    };
    let mut body = format!(
        "{word} the proposed draft “{}”, part {} of {}. {tail}",
        proposal.name, proposal.position, proposal.total
    );
    if let Some(text) = feedback.map(str::trim).filter(|t| !t.is_empty()) {
        body.push_str("\n\n");
        body.push_str(text);
    }
    body
}

/// DCP-FR-KOOA / DCP-FR-FZEP: decide one pending new-draft proposal as the
/// participant `by`.
pub fn decide_new_draft_in_process<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
    proposal_id: &str,
    decision: NewDraftDecision,
    feedback: Option<&str>,
    by: &Participant,
) -> Result<NewDraftDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let operation = match decision {
        NewDraftDecision::Accept => "accept",
        NewDraftDecision::Reject => "reject",
    };
    let Some(found) = find(root, proposal_id) else {
        log_refusal(app, operation, "", ERR_PROPOSAL_NOT_FOUND);
        return Err(ERR_PROPOSAL_NOT_FOUND.into());
    };
    let draft_id = found.draft_id.clone();
    // DCP-FR-KUSI: the state check and the writes are one act per draft, so
    // two acceptances never create two drafts from one proposal.
    let guard = draft_lock(&draft_id);
    let outcome = decide_locked(app, root, store, proposal_id, decision, feedback, by);
    drop(guard);
    match outcome {
        Ok(outcome) => {
            announce(app, &draft_id, Some(&outcome.proposal), NewDraftChangeCause::Decided);
            Ok(outcome)
        }
        Err(reason) => {
            log_refusal(app, operation, &draft_id, &reason);
            Err(reason)
        }
    }
}

fn decide_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
    proposal_id: &str,
    decision: NewDraftDecision,
    feedback: Option<&str>,
    by: &Participant,
) -> Result<NewDraftDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let mut record = find(root, proposal_id).ok_or(ERR_PROPOSAL_NOT_FOUND)?;
    let draft_id = record.draft_id.clone();
    // DCP-FR-EEDK: decided once.
    if record.state != NewDraftState::Pending {
        return Err(ERR_ALREADY_DECIDED.into());
    }
    // DCP-FR-LTEZ.
    let original = require_open_original(app, root, &draft_id)?;
    // The decision comment's conversation, checked before any write, so a
    // resolved conversation refuses with nothing changed.
    let target = DiscussionTarget::Draft { draft_id: draft_id.clone() };
    // A thread that is gone does not hold the decision up: the decision lands
    // and its comment is absent (DCP contract surface, `comment_id`).
    let thread = comments::locate_discussion_of(store, &target, &record.thread_id);
    if thread.as_ref().is_some_and(|t| t.resolved) {
        return Err(ERR_DISCUSSION_RESOLVED.into());
    }
    let thread_found = thread.is_some();
    let (file_rel, origin_kind) = match thread.and_then(|t| t.fragment_target) {
        Some(fragment) => (Some(fragment.path), ORIGIN_DRAFT_COMMENT),
        None => (None, ORIGIN_DRAFT_DISCUSSION),
    };
    let dir = dir_of(root, &draft_id)?;
    let now = now_rfc3339();

    let created = match decision {
        NewDraftDecision::Accept => {
            // DCP-FR-KOOA: in the folder that holds the original draft now.
            let body = root
                .read_text(body_path(&dir, proposal_id))
                .map_err(|_| ERR_WRITE_FAILED.to_string())?;
            let folder = drafts::draft_folder_of(root, &draft_id)
                .map_err(|_| ERR_WRITE_FAILED.to_string())?;
            let sequence = drafts::DraftSequence {
                set_id: record.set_id.clone(),
                position: record.position,
                total: record.total,
                original_draft_id: draft_id.clone(),
                original_name: original.name.clone(),
            };
            let created =
                drafts::create_draft_with_prompt(root, &record.name, &folder, &body, sequence)
                    .map_err(|_| ERR_WRITE_FAILED.to_string())?;
            record.state = NewDraftState::Accepted;
            record.created_draft_id = Some(created.draft.id.clone());
            Some(created)
        }
        NewDraftDecision::Reject => {
            record.state = NewDraftState::Rejected;
            None
        }
    };
    record.decided_at = Some(now.clone());
    if root.write_toml_atomic(record_path(&dir, proposal_id), &record).is_err() {
        // DCP-FR-AQDM: a record that will not write takes the created draft
        // back with it.
        if let Some(created) = &created {
            let _ = drafts::delete_draft_impl(root, store, &created.draft.id);
        }
        return Err(ERR_WRITE_FAILED.into());
    }
    drafts::git_storage::ensure_private_ignored(root);
    if let Some(created) = &created {
        // DCP-FR-ZDSP: the creation's "drafts changed" comes before this
        // module's own event.
        crate::storage_floor::commit::offer_sink(app);
        drafts::announce(
            app,
            drafts::DraftChange::to(&created.draft.id, vec![created.file.clone()]),
        );
    }

    // DCP-FR-PDCP: one comment into the proposal's own thread, through the
    // human write path.
    let comment_id = new_note_id();
    let appended = if !thread_found {
        Err(comments::ERR_DISCUSSION_NOT_FOUND.to_string())
    } else {
        comments::add_comment_with_id(
            store,
            thread_ref(&draft_id, file_rel.as_ref()),
            &record.thread_id,
            comment_id.clone(),
            decision_body(decision, &record, feedback),
            by,
            &now,
        )
    };
    let comment_id = match appended {
        Ok(thread) => {
            crate::agent_conversations::retire_recoverable_for_human_comment(
                app,
                &record.thread_id,
            );
            comments::emit_discussion_changed(app, &thread);
            Some(comment_id)
        }
        Err(reason) => {
            log_refusal(app, "decision_comment", &draft_id, &reason);
            None
        }
    };
    Ok(NewDraftDecisionOutcome {
        proposal: record,
        created_draft: created.map(|c| c.draft),
        comment_id,
        origin_kind,
    })
}
