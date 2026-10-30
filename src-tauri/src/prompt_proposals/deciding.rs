//! Deciding the hunks of a prompt change proposal
//! (`PCP-prompt-change-proposals.md` PCP-FR-ZQAH, PCP-FR-RJHN, PCP-FR-HLDQ,
//! PCP-FR-RSLV).

use super::*;

/// Which way a decision went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Decision {
    Accept,
    Decline,
}

/// What the author decided about one proposal: the proposal, the direction,
/// the one hunk it settles (`None` for a decline of every undecided hunk), the
/// feedback the author wrote, and who decided.
pub(super) struct DecisionRequest<'a> {
    pub(super) proposal_id: &'a str,
    pub(super) decision: Decision,
    pub(super) hunk: Option<&'a str>,
    pub(super) feedback: Option<&'a str>,
    pub(super) by: &'a Participant,
}

/// One decision, whichever way it went (PCP-FR-ZQAH, PCP-FR-RJHN, PCP-FR-RSLV).
pub(super) fn decide<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
    request: DecisionRequest<'_>,
    tracker: &ContentTracker,
) -> Result<PromptDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let DecisionRequest { proposal_id, decision, hunk, feedback, by } = request;
    let held = read_proposal(root, proposal_id)?;
    // PCP-FR-14: checked before the artifact lock is taken, so a decision made
    // while an acceptance of this proposal is in flight is refused rather than
    // queued behind it.
    if is_accepting(proposal_id) {
        return Err(ERR_ACCEPTANCE_IN_PROGRESS.to_string());
    }
    // PCP-FR-18 / PCP-FR-14: held for the whole decision.
    let _guard = artifact_lock(&held.artifact_id);
    // PCP-FR-14: reconciled before the proposal is reported in any state.
    reconcile_locked(app, root, store, proposal_id)?;
    let mut record = read_proposal(root, proposal_id)?;

    // PCP-FR-18: a resolved proposal takes no further decision, of either kind.
    // Completing an owed comment is `complete_prompt_change_decision`'s.
    if record.state != PromptProposalState::Pending {
        return Err(ERR_ALREADY_DECIDED.to_string());
    }
    // PCP-FR-ZQAH / PCP-FR-RJHN: a rejection settles an undecided hunk, and an
    // acceptance an undecided or a rejected one. An accepted hunk is final.
    if let Some(hunk_id) = hunk {
        let row = record.ledger.iter().find(|row| row.id == hunk_id).ok_or(ERR_HUNK_NOT_FOUND)?;
        let open = row.state.is_undecided()
            || (decision == Decision::Accept && row.state == HunkState::Rejected);
        if !open {
            return Err(ERR_HUNK_ALREADY_DECIDED.to_string());
        }
    }
    // PCP-FR-RSLV: only the decision that leaves no hunk undecided appends a
    // comment.
    let resolves = match hunk {
        None => true,
        Some(hunk_id) => {
            !record.ledger.iter().any(|row| row.id != hunk_id && row.state.is_undecided())
        }
    };

    // PCP-FR-12 / PCP-FR-19: an acceptance refuses a target that is gone or no
    // longer a prompt; a rejection clears such a proposal.
    if decision == Decision::Accept {
        require_prompt_artifact(root, &record.artifact_id)?;
    }

    let (thread_artifact, discussion) = locate_thread(root, store, &record.thread_id)?;
    // PCP-FR-16: the conditions that would make the append impossible are
    // checked **before anything is written**.
    if comments::thread_at(store, thread_ref(&thread_artifact, discussion), &record.thread_id)?
        .resolved
    {
        return Err(comments::ERR_DISCUSSION_RESOLVED.to_string());
    }

    if decision == Decision::Accept {
        let hunk_id = hunk.ok_or(ERR_HUNK_NOT_FOUND)?;
        return accept_locked(
            app,
            root,
            store,
            &record,
            Acceptance {
                hunk_id,
                resolves,
                thread_artifact: &thread_artifact,
                discussion,
                feedback,
                by,
            },
            tracker,
        );
    }

    // PCP-FR-RJHN: a rejection **writes nothing into the project** — no
    // artifact write, no journal begun. One naming a hunk rejects that hunk;
    // one naming none rejects every hunk still undecided.
    //
    // The record moves before the comment is appended, so a comment that will
    // not append leaves the decision made and the conversation one line short,
    // rather than a conversation saying the proposal was declined while the
    // record still reads pending.
    let dir = ensure_folder(root)?;
    let now = now_rfc3339();
    for row in record.ledger.iter_mut() {
        let settles = match hunk {
            Some(hunk_id) => row.id == hunk_id,
            None => row.state.is_undecided(),
        };
        if settles {
            row.state = HunkState::Rejected;
            row.decided_at = Some(now.clone());
        }
    }
    record.comment_owed = false;
    ledger::resettle(&mut record);
    root.write_toml_atomic(at(&dir, proposal_id, RECORD_EXT), &record)
        .map_err(|e| e.to_string())?;

    if !resolves {
        announce(app, &BUFFER, &record);
        return Ok(PromptDecisionOutcome {
            proposal: record,
            resolved: false,
            comment_id: None,
            origin_kind: origin_kind_of(discussion),
        });
    }

    // Minted here so the caller can name it as the fresh turn's trigger
    // (PCR-FR-14) without re-folding the thread to find what was just appended.
    let comment_id = new_note_id();
    let appended = comments::add_comment_with_id(
        store,
        thread_ref(&thread_artifact, discussion),
        &record.thread_id,
        comment_id.clone(),
        decision_body(&record.path, &record.counts, record.candidate_edited, feedback),
        by,
        &now_rfc3339(),
    );
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
            log_refusal(app, "decline_comment", &reason);
            None
        }
    };

    announce(app, &BUFFER, &record);
    Ok(PromptDecisionOutcome {
        proposal: record,
        resolved: true,
        comment_id,
        origin_kind: origin_kind_of(discussion),
    })
}

/// PCP-FR-HLDQ: hold one hunk for discussion, or release it. It decides
/// nothing: a held hunk is undecided and still occupies the artifact's one
/// slot.
pub fn hold_hunk_impl(
    root: &fs::RootFs,
    proposal_id: &str,
    hunk_id: &str,
    discussing: bool,
) -> Result<PromptChangeProposal, String> {
    let held = read_proposal(root, proposal_id)?;
    let _guard = artifact_lock(&held.artifact_id);
    // Read again inside the lock, so a decision that landed meanwhile is not
    // overwritten by a copy read before it.
    let mut record = read_proposal(root, proposal_id)?;
    let row = record.ledger.iter_mut().find(|row| row.id == hunk_id).ok_or(ERR_HUNK_NOT_FOUND)?;
    if !row.state.is_undecided() {
        return Err(ERR_HUNK_ALREADY_DECIDED.to_string());
    }
    row.state = if discussing { HunkState::Discussing } else { HunkState::Pending };
    row.decided_at = None;
    ledger::resettle(&mut record);
    let dir = ensure_folder(root)?;
    root.write_toml_atomic(at(&dir, proposal_id, RECORD_EXT), &record)
        .map_err(|e| e.to_string())?;
    Ok(record)
}
