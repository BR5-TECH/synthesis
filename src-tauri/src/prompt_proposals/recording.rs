//! Recording a prompt change proposal — the **only** path by which one comes
//! into existence (`PCP-prompt-change-proposals.md` PCP-FR-05, PCP-FR-CNVT).

use super::*;

/// What a caller hands [`record_prompt_proposal`]. A struct rather than five
/// positional arguments, three of which are strings that would transpose
/// silently.
pub struct NewPromptProposal<'a> {
    pub artifact_id: &'a str,
    /// The complete new text, or the hunks (PCP-FR-CNVT).
    pub change: ProposedChange<'a>,
    pub rationale: &'a str,
    pub agent: &'a Participant,
    /// The conversation the turn belongs to (PCP-FR-05's `origin`).
    pub thread_id: &'a str,
}

/// PCP-FR-05: the **only** path by which a proposal comes into existence.
///
/// Internal by construction — no Tauri command creates one, so no frontend call
/// can record a proposal, forge one against an artifact, or attribute one to an
/// agent, exactly as none can attribute a comment to one (CMS-FR-41). The one
/// caller is `crate::tools::propose_prompt_changes`.
///
/// PCP-FR-06: the two writes and the append succeed together or leave nothing
/// behind. The hunks and the record reach disk before the comment is appended,
/// so no folded comment ever names a proposal that is not there; and a failure
/// to append removes both files before returning.
pub fn record_prompt_proposal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    // CMS-FR-01: the proposal lives in the worktree; the comment announcing it
    // is a conversation and lives in the repository machine store.
    store: &fs::RootFs,
    new: NewPromptProposal<'_>,
) -> Result<PromptChangeProposal, RecordRefusal>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    // PCP-FR-04: the one-pending check below is a read followed by a write, and
    // AGC-FR-03 puts several turns in flight at once.
    let _guard = artifact_lock(new.artifact_id);

    // PCP-FR-07, checked in the order a caller can act on.
    let artifact_path =
        require_prompt_artifact(root, new.artifact_id).map_err(|e| match e.as_str() {
            ERR_NOT_A_PROMPT_ARTIFACT => RecordRefusal::NotAPromptArtifact,
            _ => RecordRefusal::ArtifactNotFound,
        })?;
    if pending_for(root, new.artifact_id).is_some() {
        return Err(RecordRefusal::ProposalPending);
    }
    let current = root.read_text(&artifact_path).map_err(|_| RecordRefusal::ArtifactNotFound)?;
    // PCP-FR-CNVT: both forms become hunks here, before any write.
    let document = plan_change(&current, new.change)?;
    // PCP-FR-07: a proposal that changes nothing is nothing to decide. The
    // comparison is against what the write path would actually put on disk
    // (PST-FR-22), so a change of line endings alone changes no byte.
    if current == composed(root, &current, &document)? {
        return Err(RecordRefusal::NoChange);
    }

    let (thread_artifact, discussion) =
        locate_thread(root, store, new.thread_id).map_err(|_| RecordRefusal::NotRecorded)?;
    let target = thread_ref(&thread_artifact, discussion);
    // CMS-FR-17: a resolved conversation takes no comment.
    if comments::thread_at(store, target, new.thread_id)
        .map_err(|_| RecordRefusal::NotRecorded)?
        .resolved
    {
        return Err(RecordRefusal::ThreadResolved);
    }

    let dir = ensure_folder(root).map_err(|_| RecordRefusal::NotRecorded)?;
    let id = new_note_id();
    let now = now_rfc3339();

    // PCP-FR-CNVT: the hunk bodies beside the record rather than in it, because
    // listing reads records and never a document (PCP-FR-10).
    root.write_text_atomic(at(&dir, &id, HUNKS_EXT), &document.to_toml())
        .map_err(|_| RecordRefusal::NotRecorded)?;

    let ledger: Vec<HunkLedgerRow> = document.hunks.iter().map(HunkLedgerRow::new).collect();
    let origin_checksums = document
        .hunks
        .iter()
        .map(|h| (h.id.clone(), fs::sha256_bytes(h.after_text().as_bytes())))
        .collect();
    // PCP-FR-06: the comment id is minted **here**, before either write, so the
    // record can name the comment it will be announced by.
    let comment_id = new_note_id();
    let mut record = PromptChangeProposal {
        id: id.clone(),
        artifact_id: new.artifact_id.to_string(),
        path: new.artifact_id.to_string(),
        rationale: new.rationale.to_string(),
        thread_id: new.thread_id.to_string(),
        comment_id: comment_id.clone(),
        state: PromptProposalState::Pending,
        origin_checksum: None,
        candidate_edited: false,
        comment_owed: false,
        legacy: false,
        base_sha256: Some(fs::sha256_bytes(current.as_bytes())),
        hunk_count: 0,
        created_at: now.clone(),
        decided_at: None,
        counts: HunkCounts::default(),
        ledger,
        origin_checksums,
        agent: new.agent.clone(),
    };
    ledger::resettle(&mut record);

    let cleanup = || {
        let _ = root.delete_under(&dir, format!("{id}.{HUNKS_EXT}"), false);
        let _ = root.delete_under(&dir, format!("{id}.{RECORD_EXT}"), false);
    };

    if root.write_toml_atomic(at(&dir, &id, RECORD_EXT), &record).is_err() {
        cleanup();
        return Err(RecordRefusal::NotRecorded);
    }

    let attachment = Attachment::PromptProposal {
        proposal_id: id.clone(),
        artifact_id: new.artifact_id.to_string(),
        path: new.artifact_id.to_string(),
    };
    let appended = match comments::append_agent_comment_with(
        store,
        target,
        new.thread_id,
        comment_id,
        new.rationale.to_string(),
        vec![attachment],
        comments::Stamp { by: new.agent, at: &now },
    ) {
        Ok(thread) => thread,
        Err(error) => {
            // PCP-FR-06: a failure to append removes **both** files, so the
            // artifact's one pending slot is free again.
            cleanup();
            return Err(if error == comments::ERR_DISCUSSION_RESOLVED {
                RecordRefusal::ThreadResolved
            } else {
                RecordRefusal::NotRecorded
            });
        }
    };

    // CMS-FR-51 / PCP-FR-06: the conversation announces itself the moment the
    // offer is in it.
    comments::emit_discussion_changed(app, &appended);
    // PCP-FR-17: emitted when a proposal is **recorded**, not only when one is
    // decided.
    announce(app, &BUFFER, &record);
    Ok(record)
}
