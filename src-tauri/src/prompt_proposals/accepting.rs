//! Accepting one hunk of a prompt change proposal, as one transaction with one
//! commit point (`PCP-prompt-change-proposals.md` PCP-FR-14, PCP-FR-ZQAH).

use super::*;
use crate::text_hunks::hunks as text;

/// PCP-FR-RSLV: the decision comment's body — a sentence on where the proposal
/// ended, the tally of its hunks by final state, whether the author rewrote
/// any hunk, then the author's feedback where they wrote any.
///
/// Prose rather than a marker, because a reader scrolling the thread months
/// later reads the offer and the answer in sequence.
pub(super) fn decision_body(
    path: &str,
    counts: &HunkCounts,
    edited: bool,
    feedback: Option<&str>,
) -> String {
    let sentence = match (derive_state(counts), edited) {
        (PromptProposalState::Accepted, false) => {
            format!("Accepted the proposed change to `{path}`.")
        }
        (PromptProposalState::Accepted, true) => {
            format!("Accepted an edited version of the proposed change to `{path}`.")
        }
        (_, false) => format!("Declined the proposed change to `{path}`."),
        (_, true) => format!("Declined the proposed change to `{path}`, having edited it first."),
    };
    let sentence =
        format!("{sentence} {} accepted, {} rejected.", counts.accepted, counts.rejected);
    match feedback.map(str::trim).filter(|f| !f.is_empty()) {
        Some(feedback) => format!("{sentence}\n\n{feedback}"),
        None => sentence,
    }
}

/// The hunk an acceptance settles, the thread that receives the decision
/// comment, and the author's own part of the decision (PCP-FR-ZQAH,
/// PCP-FR-RSLV).
pub(super) struct Acceptance<'a> {
    pub(super) hunk_id: &'a str,
    /// PCP-FR-RSLV: whether this acceptance leaves no hunk undecided.
    pub(super) resolves: bool,
    pub(super) thread_artifact: &'a str,
    pub(super) discussion: bool,
    pub(super) feedback: Option<&'a str>,
    pub(super) by: &'a Participant,
}

/// PCP-FR-ZQAH / PCP-FR-14: accept one hunk of a pending proposal, as one
/// transaction with one commit point.
///
/// The hunk is resolved against the file **as it stands on disk now** and
/// spliced into it. No baseline checksum is compared. A legacy proposal's one
/// hunk is accepted as a whole-file replacement (PCP-FR-LGXQ).
pub(super) fn accept_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
    record: &PromptChangeProposal,
    acceptance: Acceptance<'_>,
    tracker: &ContentTracker,
) -> Result<PromptDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let Acceptance { hunk_id, resolves, thread_artifact, discussion, feedback, by } = acceptance;
    // PCP-FR-14: one acceptance of a proposal at a time.
    let Some(_slot) = try_acceptance_slot(&record.id) else {
        return Err(ERR_ACCEPTANCE_IN_PROGRESS.to_string());
    };

    let dir = ensure_folder(root)?;
    let artifact_path = require_prompt_artifact(root, &record.artifact_id)?;
    let current = root.read_text(&artifact_path).map_err(|_| ERR_ARTIFACT_NOT_FOUND.to_string())?;
    let (doc, legacy) = read_hunk_document(root, &dir, &record.id, &current)?;
    let candidate = if legacy {
        // PCP-FR-LGXQ: the whole candidate, verbatim, as it always meant.
        root.read_text(at(&dir, &record.id, CONTENT_EXT))
            .map_err(|_| ERR_PROPOSAL_NOT_FOUND.to_string())?
    } else {
        let hunk = doc.find(hunk_id).ok_or(ERR_HUNK_NOT_FOUND)?;
        let before = anchors::normalise_newlines(hunk.before_text());
        let placed = anchors::resolve(
            &anchors::normalise_newlines(&current),
            hunk.kind,
            &before,
            &hunk.anchor,
        );
        // PCP-FR-ZQAH: re-resolved here against disk rather than trusting what
        // the surface last showed. A lost hunk is refused and nothing is
        // written.
        text::apply_hunk(&current, hunk, placed).ok_or(ERR_ANCHOR_LOST)?
    };

    let prior_row = record.ledger.iter().find(|row| row.id == hunk_id).ok_or(ERR_HUNK_NOT_FOUND)?;
    let mut decided = record.clone();
    if let Some(row) = decided.ledger.iter_mut().find(|row| row.id == hunk_id) {
        row.state = HunkState::Accepted;
        row.decided_at = Some(now_rfc3339());
    }
    ledger::resettle(&mut decided);
    decided.comment_owed = resolves;

    // PCP-FR-14: the comment's identity and its `event_id` are minted in
    // advance, so a re-append after a crash contributes nothing a second time
    // (CMS-FR-06). An acceptance that does not resolve the proposal owes no
    // comment (PCP-FR-RSLV), so its journal carries no body.
    let journal = Journal {
        operation_id: new_note_id(),
        proposal_id: record.id.clone(),
        artifact_id: record.artifact_id.clone(),
        path: record.path.clone(),
        prior_sha256: String::new(),
        candidate_sha256: fs::sha256_bytes(normalise_for_write(root, &candidate).as_bytes()),
        comment_id: new_note_id(),
        event_id: new_note_id(),
        thread_id: record.thread_id.clone(),
        comment_body: if resolves {
            decision_body(&record.path, &decided.counts, decided.candidate_edited, feedback)
        } else {
            String::new()
        },
        created_at: now_rfc3339(),
        hunk_id: Some(hunk_id.to_string()),
        prior_state: prior_row.state,
        prior_decided_at: prior_row.decided_at.clone(),
        resolves,
        comment_by: by.clone(),
    };

    // Everything from here to the commit point either lands or is undone whole,
    // and the artifact is never written by any of it.
    let prepared = || -> Result<Journal, String> {
        // PCP-FR-14: the artifact's whole current bytes, byte-for-byte and
        // unnormalised, written and then read back and checksummed.
        let prior = root.read_bytes(&artifact_path).map_err(|e| e.to_string())?;
        root.write_bytes_atomic(at(&dir, &record.id, PRIOR_EXT), &prior)
            .map_err(|e| e.to_string())?;
        let prior_sha256 =
            root.sha256_file(at(&dir, &record.id, PRIOR_EXT)).map_err(|e| e.to_string())?;
        let journal = Journal { prior_sha256, ..journal.clone() };
        root.write_toml_atomic(at(&dir, &record.id, JOURNAL_EXT), &journal)
            .map_err(|e| e.to_string())?;
        // The record moves before the artifact does, so a journal beside a
        // record reading the hunk `accepted` and an artifact still holding its
        // own bytes is exactly the "did not commit" reading of PCP-FR-14.
        root.write_toml_atomic(at(&dir, &record.id, RECORD_EXT), &decided)
            .map_err(|e| e.to_string())?;
        Ok(journal)
    };

    let journal = match prepared() {
        Ok(journal) => journal,
        Err(reason) => {
            // PCP-FR-14: the artifact was never written, so only this module's
            // own record is put back.
            let _ = roll_back(root, &dir, &journal);
            log_recovery(app, "rollback", record, &reason);
            return Err(ERR_WRITE_FAILED.to_string());
        }
    };

    // ---- THE COMMIT POINT: the artifact write, atomic and last ---------------
    // PCP-FR-12 / PCP-FR-ZQAH: through `crate::artifacts`' own write path, which
    // stays the only write path for an artifact's file.
    if let Err(reason) =
        crate::artifacts::save_validated_artifact(root, &record.artifact_id, &candidate, tracker)
    {
        let _ = roll_back(root, &dir, &journal);
        log_recovery(app, "rollback", record, &reason);
        return Err(ERR_WRITE_FAILED.to_string());
    }

    // Past the commit point a read that fails must not answer as a refusal:
    // the record written a moment ago is what the acceptance settled.
    let accepted = read_proposal(root, &record.id).unwrap_or_else(|_| decided.clone());
    // PCP-FR-17: emitted **after** the commit point and never before.
    announce(app, &BUFFER, &accepted);
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "prompt change acceptance committed",
        log_fields! {
            "artifactId" => &record.artifact_id,
            "proposalId" => &record.id,
            "hunkId" => hunk_id,
            "path" => &record.path,
            "operationId" => &journal.operation_id,
            "resolves" => resolves,
        },
    );

    // PCP-FR-RSLV: an acceptance that leaves a hunk undecided appends nothing.
    if !resolves {
        finish(root, &dir, &record.id);
        return Ok(PromptDecisionOutcome {
            proposal: accepted,
            resolved: false,
            comment_id: None,
            origin_kind: origin_kind_of(discussion),
        });
    }

    // PCP-FR-14 / PCP-FR-16: the append is the tail of a successful acceptance.
    match append_decision(app, store, &journal, thread_artifact, discussion) {
        Ok(()) => {
            let settled = clear_owed(root, &dir, &record.id).unwrap_or(accepted);
            finish(root, &dir, &record.id);
            Ok(PromptDecisionOutcome {
                proposal: settled,
                resolved: true,
                comment_id: Some(journal.comment_id),
                origin_kind: origin_kind_of(discussion),
            })
        }
        Err(reason) => {
            // PCP-FR-14: the acceptance landed and the comment is owed. The
            // journal and `.prior` stay on disk.
            log_recovery(app, "comment_owed", record, &reason);
            Err(ERR_ACCEPTANCE_INCOMPLETE.to_string())
        }
    }
}

/// PCP-FR-14 / PCP-FR-ZQAH: undo a transaction that never reached its commit
/// point.
///
/// It puts the one hunk the journal names back to its prior row and leaves
/// every other hunk exactly as it stands — a whole-proposal rollback here would
/// destroy decisions that already committed. The rollback is read back and
/// verified before the journal and `.prior` are removed.
pub(super) fn roll_back(root: &fs::RootFs, dir: &Path, journal: &Journal) -> Result<(), String> {
    let mut record = read_proposal(root, &journal.proposal_id)?;
    let hunk_id = journal.hunk();
    if let Some(row) = record.ledger.iter_mut().find(|row| row.id == hunk_id) {
        row.state = journal.prior_state;
        row.decided_at = if journal.prior_state.is_undecided() {
            None
        } else {
            journal.prior_decided_at.clone()
        };
    }
    record.comment_owed = false;
    record.decided_at = None;
    ledger::resettle(&mut record);
    root.write_toml_atomic(at(dir, &record.id, RECORD_EXT), &record).map_err(|e| e.to_string())?;
    let verified = read_proposal(root, &record.id)?;
    let row_back = verified.ledger.iter().find(|row| row.id == hunk_id).map(|row| row.state);
    if row_back.is_some_and(|state| state != journal.prior_state) || verified.comment_owed {
        return Err("rollback not verified".to_string());
    }
    finish(root, dir, &record.id);
    Ok(())
}

/// PCP-FR-14: the journal and `.prior` go, the journal **last of all**, so a
/// folder holding a journal is always a folder with something still to finish.
pub(super) fn finish(root: &fs::RootFs, dir: &Path, proposal_id: &str) {
    let _ = root.delete_under(dir, format!("{proposal_id}.{PRIOR_EXT}"), false);
    let _ = root.delete_under(dir, format!("{proposal_id}.{JOURNAL_EXT}"), false);
}

/// PCP-FR-14: `comment_owed` is cleared only once the append has succeeded.
pub(super) fn clear_owed(
    root: &fs::RootFs,
    dir: &Path,
    proposal_id: &str,
) -> Result<PromptChangeProposal, String> {
    let mut record = read_proposal(root, proposal_id)?;
    if !record.comment_owed {
        return Ok(record);
    }
    record.comment_owed = false;
    root.write_toml_atomic(at(dir, proposal_id, RECORD_EXT), &record).map_err(|e| e.to_string())?;
    Ok(record)
}

/// PCP-FR-14 / PCP-FR-16: append the decision comment under the ids the journal
/// minted, so a repeat contributes nothing a second time (CMS-FR-06).
///
/// The resolution the conversation may since have taken is deliberately not
/// consulted: it was checked before the transaction began, and the transaction
/// has committed.
pub(super) fn append_decision<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    journal: &Journal,
    thread_artifact: &str,
    discussion: bool,
) -> Result<(), String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let thread = comments::append_decision_comment_event(
        store,
        thread_ref(thread_artifact, discussion),
        &journal.thread_id,
        journal.event_id.clone(),
        journal.comment_id.clone(),
        journal.comment_body.clone(),
        comments::Stamp { by: &journal.comment_by, at: &now_rfc3339() },
    )?;
    // AGC-FR-31 / CTA-FR-RUVS: the author's decision is a human-authored
    // comment, so it retires this conversation's offer to retry a failed turn.
    crate::agent_conversations::retire_recoverable_for_human_comment(app, &journal.thread_id);
    // CMS-FR-51: the decision's own comment reaches the conversation's surface.
    comments::emit_discussion_changed(app, &thread);
    Ok(())
}
