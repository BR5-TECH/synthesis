//! Deciding the hunks of a change edit proposal: accept (CEP-FR-OPGW), reject,
//! decline, hold, and edit (CEP-FR-UCVO).

use super::*;
use crate::prompt_proposals::HunkState;
use crate::text_hunks::anchors;
use crate::text_hunks::hunks as text;

/// Which way a decision went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Decision {
    Accept,
    Decline,
}

/// One decision: the proposal, the direction, the hunk it settles (`None` for a
/// decline of every undecided hunk), the feedback, and who decided.
pub(super) struct DecisionRequest<'a> {
    pub(super) proposal_id: &'a str,
    pub(super) decision: Decision,
    pub(super) hunk: Option<&'a str>,
    pub(super) feedback: Option<&'a str>,
    pub(super) by: &'a Participant,
}

/// One decision, whichever way it went.
pub(super) fn decide<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    request: DecisionRequest<'_>,
) -> Result<ChangeDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let DecisionRequest { proposal_id, decision, hunk, feedback, by } = request;
    let (held, _) = read_proposal(store, proposal_id)?;
    // PCP-FR-14: a second decision while an acceptance of this proposal runs
    // is refused rather than queued. An acceptance takes its slot here, before
    // the change lock, so a second acceptance does not wait behind the first.
    let _slot = match decision {
        Decision::Accept => Some(
            try_acceptance_slot(proposal_id)
                .ok_or_else(|| ERR_ACCEPTANCE_IN_PROGRESS.to_string())?,
        ),
        Decision::Decline if is_accepting(proposal_id) => {
            return Err(ERR_ACCEPTANCE_IN_PROGRESS.to_string());
        }
        Decision::Decline => None,
    };
    let _guard = change_lock(&held.stream_id, &held.change);
    reconcile_locked(app, store, sites, proposal_id)?;
    let (mut record, dir) = read_proposal(store, proposal_id)?;

    if record.state != ChangeProposalState::Pending {
        return Err(ERR_ALREADY_DECIDED.to_string());
    }
    if let Some(hunk_id) = hunk {
        let row = record.row(hunk_id).ok_or(ERR_HUNK_NOT_FOUND)?;
        if row.state == HunkState::Withdrawn {
            return Err(ERR_HUNK_WITHDRAWN.to_string());
        }
        let open = row.state.is_undecided()
            || (decision == Decision::Accept && row.state == HunkState::Rejected);
        if !open {
            return Err(ERR_HUNK_ALREADY_DECIDED.to_string());
        }
    }
    let resolves = match hunk {
        None => true,
        Some(hunk_id) => !record.ledger.iter().any(|r| r.id != hunk_id && r.state.is_undecided()),
    };
    let thread = locate_thread(store, &record.stream_id, &record.change, &record.thread_id)?;
    // PCP-FR-16: checked before anything is written.
    if thread.resolved {
        return Err(comments::ERR_DISCUSSION_RESOLVED.to_string());
    }

    if decision == Decision::Accept {
        let hunk_id = hunk.ok_or(ERR_HUNK_NOT_FOUND)?;
        // CEP-FR-ESQR: a run that works the stream's working copy refuses the
        // write, and the hunk stays undecided.
        if sites.busy_run(&record.stream_id).is_some() {
            return Err(ERR_STREAM_BUSY.to_string());
        }
        let site = sites.site(&record.stream_id, &record.change)?;
        let acceptance = Acceptance { hunk_id, resolves, thread: &thread, feedback, by };
        return accept_locked(app, store, &site, &dir, &record, acceptance);
    }

    // CEP-FR-UCVO: a rejection writes nothing into the working copy.
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
    resettle(&mut record);
    write_record(store, &dir, &record)?;
    let origin_kind = origin_kind_of(&thread);

    if !resolves {
        announce(app, &record, ChangeProposalCause::Decided);
        return Ok(ChangeDecisionOutcome {
            proposal: record,
            committed: false,
            resolved: false,
            comment_id: None,
            origin_kind,
        });
    }

    // CEP-FR-RWTQ: only the decision that closes the proposal appends the
    // decision comment.
    let comment_id = new_note_id();
    let appended = comments::add_comment_with_id(
        store,
        thread.log_ref(),
        &record.thread_id,
        comment_id.clone(),
        decision_body(&record.change, &record.counts, record.candidate_edited, feedback),
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
            log_refusal(app, "decline_comment", &reason, Subject::Proposal(proposal_id));
            None
        }
    };
    announce(app, &record, ChangeProposalCause::Decided);
    // CEP-FR-FOJW: a close with an earlier accepted hunk commits its files.
    let site = sites.site(&record.stream_id, &record.change).ok();
    let committed = commit_close(app, store, site.as_ref(), &record.id);
    record_close_statistics(app, store, &record.id);
    let (record, _) = read_proposal(store, proposal_id)?;
    Ok(ChangeDecisionOutcome {
        proposal: record,
        committed,
        resolved: true,
        comment_id,
        origin_kind,
    })
}

/// The hunk an acceptance settles and the author's part of the decision.
pub(super) struct Acceptance<'a> {
    pub(super) hunk_id: &'a str,
    pub(super) resolves: bool,
    pub(super) thread: &'a comments::Discussion,
    pub(super) feedback: Option<&'a str>,
    pub(super) by: &'a Participant,
}

/// The text the hunk gives its file, resolved against the file as it stands in
/// the working copy (CEP-FR-OPGW, CEP-FR-HRCX).
fn candidate_of(site: &ChangeSite, file: &HunkSetFile, hunk_id: &str) -> Result<String, String> {
    let hunk = file.document.find(hunk_id).ok_or(ERR_HUNK_NOT_FOUND)?;
    if file.new_file {
        if site.exists(&file.path) {
            return Err(ERR_FILE_EXISTS.to_string());
        }
        return Ok(hunk.after_text().to_string());
    }
    let current = site.read_file(&file.path).ok_or(ERR_ANCHOR_LOST)?;
    let before = anchors::normalise_newlines(hunk.before_text());
    let placed =
        anchors::resolve(&anchors::normalise_newlines(&current), hunk.kind, &before, &hunk.anchor);
    text::apply_hunk(&current, hunk, placed).ok_or_else(|| ERR_ANCHOR_LOST.to_string())
}

/// CEP-FR-OPGW / PCP-FR-14: accept one hunk as one transaction. The file write
/// is the commit point and the last fallible step, so no path here restores a
/// file of the working copy.
pub(super) fn accept_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    site: &ChangeSite,
    dir: &Path,
    record: &ChangeEditProposal,
    acceptance: Acceptance<'_>,
) -> Result<ChangeDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let Acceptance { hunk_id, resolves, thread, feedback, by } = acceptance;
    // The caller holds the acceptance slot of this proposal (PCP-FR-14).
    let set = read_hunks(store, dir, &record.id)?;
    let (file, _) = set.find(hunk_id).ok_or(ERR_HUNK_NOT_FOUND)?;
    let candidate = site.normalise(&candidate_of(site, file, hunk_id)?);
    let target = site.file_path(&file.path)?;
    let prior_sha256 =
        if file.new_file { None } else { site.working_copy.sha256_file(&target).ok() };

    let prior_row = record.row(hunk_id).ok_or(ERR_HUNK_NOT_FOUND)?;
    let mut decided = record.clone();
    if let Some(row) = decided.row_mut(hunk_id) {
        row.state = HunkState::Accepted;
        row.decided_at = Some(now_rfc3339());
    }
    let path_was_written = decided.written_paths.contains(&file.path);
    if !path_was_written {
        decided.written_paths.push(file.path.clone());
    }
    resettle(&mut decided);
    decided.comment_owed = resolves;

    let journal = Journal {
        operation_id: new_note_id(),
        proposal_id: record.id.clone(),
        hunk_id: hunk_id.to_string(),
        path: file.path.clone(),
        prior_sha256,
        candidate_sha256: fs::sha256_bytes(candidate.as_bytes()),
        prior_state: prior_row.state,
        prior_decided_at: prior_row.decided_at.clone(),
        path_was_written,
        resolves,
        comment_id: new_note_id(),
        event_id: new_note_id(),
        thread_id: record.thread_id.clone(),
        comment_body: if resolves {
            decision_body(&record.change, &decided.counts, decided.candidate_edited, feedback)
        } else {
            String::new()
        },
        comment_by: by.clone(),
    };

    // The journal and the record move before the file does.
    let prepared = store
        .write_toml_atomic(at(dir, &record.id, JOURNAL_EXT), &journal)
        .map_err(|e| e.to_string())
        .and_then(|()| write_record(store, dir, &decided));
    if let Err(reason) = prepared {
        let _ = roll_back(store, dir, &journal);
        log_recovery(app, "rollback", record, &reason);
        return Err(ERR_WRITE_FAILED.to_string());
    }

    // ---- THE COMMIT POINT: the file write, atomic and last ------------------
    if let Err(reason) = site.working_copy.write_text_atomic(&target, &candidate) {
        let _ = roll_back(store, dir, &journal);
        log_recovery(app, "rollback", record, &reason.to_string());
        return Err(ERR_WRITE_FAILED.to_string());
    }

    announce(app, &decided, ChangeProposalCause::Decided);
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "change edit acceptance committed",
        log_fields! {
            "streamId" => &record.stream_id,
            "change" => &record.change,
            "proposalId" => &record.id,
            "hunkId" => hunk_id,
            "resolves" => resolves,
        },
    );
    let origin_kind = origin_kind_of(thread);

    if !resolves {
        finish(store, dir, &record.id);
        return Ok(ChangeDecisionOutcome {
            proposal: decided,
            committed: false,
            resolved: false,
            comment_id: None,
            origin_kind,
        });
    }

    match append_decision(app, store, thread, &journal) {
        Ok(()) => {
            let settled = clear_owed(store, dir, &record.id).unwrap_or(decided);
            finish(store, dir, &record.id);
            announce(app, &settled, ChangeProposalCause::Decided);
            // CEP-FR-FOJW: the close commits the files of the accepted hunks.
            let committed = commit_close(app, store, Some(site), &record.id);
            record_close_statistics(app, store, &record.id);
            let proposal = read_proposal(store, &record.id).map(|(r, _)| r).unwrap_or(settled);
            Ok(ChangeDecisionOutcome {
                proposal,
                committed,
                resolved: true,
                comment_id: Some(journal.comment_id),
                origin_kind,
            })
        }
        Err(reason) => {
            // PCP-FR-14: the file is written and the comment is owed. The
            // journal stays.
            log_recovery(app, "comment_owed", record, &reason);
            Err(ERR_ACCEPTANCE_INCOMPLETE.to_string())
        }
    }
}

/// PCP-FR-14: undo an acceptance that never reached its commit point. Only the
/// one hunk's row goes back.
pub(super) fn roll_back(store: &fs::RootFs, dir: &Path, journal: &Journal) -> Result<(), String> {
    let (mut record, _) = read_proposal(store, &journal.proposal_id)?;
    if let Some(row) = record.row_mut(&journal.hunk_id) {
        row.state = journal.prior_state;
        row.decided_at = if journal.prior_state.is_undecided() {
            None
        } else {
            journal.prior_decided_at.clone()
        };
    }
    if !journal.path_was_written {
        record.written_paths.retain(|p| *p != journal.path);
    }
    record.comment_owed = false;
    record.decided_at = None;
    resettle(&mut record);
    write_record(store, dir, &record)?;
    finish(store, dir, &record.id);
    Ok(())
}

/// PCP-FR-14: the journal goes last of all.
pub(super) fn finish(store: &fs::RootFs, dir: &Path, proposal_id: &str) {
    let _ = store.delete_under(dir, format!("{proposal_id}.{JOURNAL_EXT}"), false);
}

/// PCP-FR-14: `comment_owed` is cleared once the append succeeded.
pub(super) fn clear_owed(
    store: &fs::RootFs,
    dir: &Path,
    proposal_id: &str,
) -> Result<ChangeEditProposal, String> {
    let (mut record, _) = read_proposal(store, proposal_id)?;
    if record.comment_owed {
        record.comment_owed = false;
        write_record(store, dir, &record)?;
    }
    Ok(record)
}

/// PCP-FR-14 / PCP-FR-16: the decision comment, under the ids the journal
/// minted, so a repeat adds nothing a second time (CMS-FR-06).
pub(super) fn append_decision<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    thread: &comments::Discussion,
    journal: &Journal,
) -> Result<(), String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let appended = comments::append_decision_comment_event(
        store,
        thread.log_ref(),
        &journal.thread_id,
        journal.event_id.clone(),
        journal.comment_id.clone(),
        journal.comment_body.clone(),
        comments::Stamp { by: &journal.comment_by, at: &now_rfc3339() },
    )?;
    crate::agent_conversations::retire_recoverable_for_human_comment(app, &journal.thread_id);
    comments::emit_discussion_changed(app, &appended);
    Ok(())
}

/// CEP-FR-UCVO (PCP-FR-HLDQ): hold one hunk for discussion, or release it.
pub fn hold_hunk_impl(
    store: &fs::RootFs,
    proposal_id: &str,
    hunk_id: &str,
    discussing: bool,
) -> Result<ChangeEditProposal, String> {
    let (held, _) = read_proposal(store, proposal_id)?;
    let _guard = change_lock(&held.stream_id, &held.change);
    let (mut record, dir) = read_proposal(store, proposal_id)?;
    let row = record.row_mut(hunk_id).ok_or(ERR_HUNK_NOT_FOUND)?;
    if row.state == HunkState::Withdrawn {
        return Err(ERR_HUNK_WITHDRAWN.to_string());
    }
    if !row.state.is_undecided() {
        return Err(ERR_HUNK_ALREADY_DECIDED.to_string());
    }
    row.state = if discussing { HunkState::Discussing } else { HunkState::Pending };
    row.decided_at = None;
    resettle(&mut record);
    write_record(store, &dir, &record)?;
    Ok(record)
}

/// CEP-FR-UCVO (PCP-FR-EDTW): the author's own text over one undecided hunk.
/// It writes the hunk set and the record, never the working copy.
pub fn edit_hunk_impl(
    store: &fs::RootFs,
    proposal_id: &str,
    hunk_id: &str,
    after: &str,
    baseline_checksum: &str,
) -> Result<ChangeCandidateSaved, String> {
    let (held, _) = read_proposal(store, proposal_id)?;
    let _guard = change_lock(&held.stream_id, &held.change);
    let (mut record, dir) = read_proposal(store, proposal_id)?;
    let mut set = read_hunks(store, &dir, proposal_id)?;
    let row = record.row(hunk_id).ok_or(ERR_HUNK_NOT_FOUND)?;
    if row.state == HunkState::Withdrawn {
        return Err(ERR_HUNK_WITHDRAWN.to_string());
    }
    if !row.state.is_undecided() {
        return Err(ERR_HUNK_ALREADY_DECIDED.to_string());
    }
    if set.checksum() != baseline_checksum {
        return Err(ERR_CANDIDATE_STALE.to_string());
    }
    let slot = set
        .files
        .iter_mut()
        .flat_map(|file| file.document.hunks.iter_mut())
        .find(|hunk| hunk.id == hunk_id)
        .ok_or(ERR_HUNK_NOT_FOUND)?;
    slot.after = (!after.is_empty()).then(|| after.to_string());
    let checksum = set.checksum();
    let prior = store.read_text(at(&dir, proposal_id, HUNKS_EXT)).ok();
    let text = set.to_toml().map_err(|_| ERR_WRITE_FAILED.to_string())?;
    store
        .write_text_atomic(at(&dir, proposal_id, HUNKS_EXT), &text)
        .map_err(|_| ERR_WRITE_FAILED.to_string())?;
    let edited = record
        .origin_checksums
        .get(hunk_id)
        .map(|origin| *origin != fs::sha256_bytes(after.as_bytes()))
        .unwrap_or(true);
    if let Some(row) = record.row_mut(hunk_id) {
        row.edited = edited;
    }
    resettle(&mut record);
    // The hunk set and the ledger agree, or neither changes: a record that
    // does not write puts the earlier hunk set back, and the caller logs the
    // refusal.
    if write_record(store, &dir, &record).is_err() {
        if let Some(prior) = prior {
            let _ = store.write_text_atomic(at(&dir, proposal_id, HUNKS_EXT), &prior);
        }
        return Err(ERR_WRITE_FAILED.to_string());
    }
    Ok(ChangeCandidateSaved { checksum })
}
