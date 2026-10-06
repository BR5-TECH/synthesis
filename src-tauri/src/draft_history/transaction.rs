//! The acceptance transaction, and the reconciliation that finishes an
//! interrupted one (`DHS-draft-history.md` DHS-FR-17 … DHS-FR-23).
//!
//! One accepted change is one transaction with **one commit point**: a journal
//! written first saying what is about to happen, the entries and the prompt
//! written next, then one atomic rewrite of that journal. Everything before the
//! commit point is rolled back on any failure and the draft is byte-for-byte
//! what it was; everything after it is owed and is completed by whoever reads
//! the journal next, including a process that was not the one that wrote it.
//!
//! It is here on its own because it is read end to end or not at all — and
//! because the ordering within it is the whole correctness argument.

use super::*;

// ---------------------------------------------------------------------------
// Writing entries
// ---------------------------------------------------------------------------

/// Write one entry: payload first, then manifest.
///
/// The order is what DHS-FR-12 rests on outside a transaction too — a payload
/// with no manifest is listed by nothing, where a manifest with no payload would
/// be a version the rail offers and cannot open.
pub(crate) fn write_entry(
    root: &fs::RootFs,
    hist: &Path,
    entry_id: &str,
    draft_id: &str,
    seq: u32,
    path: &str,
    source: DraftHistorySource,
    bytes: &[u8],
) -> Result<DraftHistoryEntry, String> {
    root.write_bytes_atomic(snapshot_path(hist, entry_id), bytes)
        .map_err(|e| e.to_string())?;
    // Digested from the file that just landed rather than from the slice in
    // memory, so the manifest describes what a later read will actually verify.
    let sha256 = root
        .sha256_file(snapshot_path(hist, entry_id))
        .map_err(|e| e.to_string())?;
    let entry = DraftHistoryEntry {
        id: entry_id.to_string(),
        draft_id: draft_id.to_string(),
        seq,
        path: path.to_string(),
        created_at: now_rfc3339(),
        byte_len: bytes.len() as u64,
        sha256,
        source,
    };
    root.write_toml_atomic(manifest_path(hist, entry_id), &entry)
        .map_err(|e| e.to_string())?;
    // DRS-FR-WYIN / DRS-FR-JDRY: a settled version is private draft storage.
    // The ignore file keeps it out of Git, and no commit names it.
    crate::drafts::git_storage::ensure_private_ignored(root);
    Ok(entry)
}

// ---------------------------------------------------------------------------
// The acceptance transaction (DHS-FR-13 … DHS-FR-21)
// ---------------------------------------------------------------------------

/// What `apply_acceptance` is handed (DHS-FR-15).
///
/// Everything it needs to run the whole transaction, composed by
/// `crate::draft_proposals` before the transaction begins (DCP-FR-11): the
/// candidate as it stands, the proposal record as it is to read once accepted,
/// and the decision comment's body and its human identity.
pub struct Acceptance<'a> {
    pub proposal: &'a DraftChangeProposal,
    /// The change being accepted, or `None` for a whole-proposal acceptance.
    pub hunk_id: Option<String>,
    /// Whether this decision resolves the proposal (DHS-FR-15).
    pub resolves: bool,
    /// The candidate's text exactly as proposal storage holds it. The line-ending
    /// convention is settled by the write rather than here (DCP-FR-08).
    pub candidate: &'a str,
    pub comment_body: String,
    pub comment_by: &'a Participant,
}

/// What an acceptance produced (DHS-FR-15).
#[derive(Clone, Debug)]
pub struct AcceptedOutcome {
    /// Absent where the acceptance changed no persisted byte (DHS-FR-16).
    pub entry: Option<DraftHistoryEntry>,
    /// DHS-FR-07: the `Original` this acceptance settled on its way past — the
    /// prompt as it stood before it. Present only for the first acceptance a
    /// draft takes, every one after it finding an `Original` already recorded.
    pub original: Option<DraftHistoryEntry>,
    pub proposal: DraftChangeProposal,
    /// DHS-FR-15: absent where this decision resolved nothing and therefore
    /// owed the conversation no comment.
    pub comment_id: Option<String>,
}

/// DHS-FR-15: the whole acceptance, in the one fixed order that makes it
/// recoverable.
///
/// Called by `crate::draft_proposals` (DCP-FR-11) and by nothing else. Its caller
/// has already made every check that can be made in advance — that the proposal
/// is pending, that its `path` still names the prompt, that an identity resolves,
/// and that the conversation is unlocked (DCP-FR-15) — so a failure here is an
/// I/O failure rather than an ordinary refusal.
pub fn apply_acceptance<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    // CMS-FR-01: the decision comment goes into the repository machine
    // store, while the prompt, the entry, and the journal stay in the worktree.
    store: &fs::RootFs,
    input: Acceptance<'_>,
) -> Result<AcceptedOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let _guard = draft_proposals::draft_lock(&input.proposal.draft_id);
    apply_acceptance_locked(app, root, store, input)
}

/// [`apply_acceptance`] for a caller already holding the draft's lock
/// (DHS-FR-23) — which `DCP-draft-change-proposals.md`'s decision path is,
/// because its already-decided check and this transaction are one act.
pub(crate) fn apply_acceptance_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    // CMS-FR-01: the decision comment goes into the repository machine
    // store, while the prompt, the entry, and the journal stay in the worktree.
    store: &fs::RootFs,
    input: Acceptance<'_>,
) -> Result<AcceptedOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let draft_id = input.proposal.draft_id.as_str();
    // DHS-FR-18: reconciliation runs first, so the refusal below is reached only
    // where recovery itself is blocked rather than for every interrupted run.
    reconcile_locked(app, root, store, draft_id)?;

    let dir = draft_dir(root, draft_id)?;
    let prompt = drafts::require_prompt(root, draft_id)?;
    let hist = dir.join(drafts::HISTORY_DIR);

    // DHS-FR-23: one journal is always enough, so two acceptances never
    // interleave over one prompt.
    if read_journal(root, &hist)?.is_some() {
        return Err(ERR_ACCEPTANCE_IN_PROGRESS.to_string());
    }

    // DHS-FR-15, step one: normalise the candidate on the terms the save path
    // will persist it, and read the prompt's current bytes.
    let normalised = drafts::normalise_for_write(root, input.candidate);
    let prompt_abs = drafts::draft_file_abs_path(root, draft_id, &prompt)?;
    let prior = root
        .read_bytes(&prompt_abs)
        .map_err(|_| ERR_WRITE_FAILED.to_string())?;

    // DHS-FR-16: an acceptance whose normalised candidate is byte-identical to
    // what the prompt holds creates no entry. The transaction still runs — the
    // proposal is accepted, the comment appended, and the draft's own save path
    // still called so its `updated_at` and its indexing follow.
    let changes = normalised.as_bytes() != prior.as_slice();
    let entry_id = changes.then(new_note_id);
    let held = manifests(root, &hist);
    // DHS-FR-07: the `Original` is the prompt as it stood before the first
    // change was accepted, so it is written by that acceptance — over the very
    // bytes this one is about to supersede — and by nothing else. A draft
    // nobody has proposed a change to holds no entry, its live prompt being its
    // `Original`; an acceptance that changes nothing supersedes nothing and so
    // settles nothing either.
    let original_id = (changes && held.is_empty()).then(new_note_id);
    let base_seq = held.iter().map(|e| e.seq).max().unwrap_or(0);
    let original_seq = base_seq + 1;
    let next_seq = if original_id.is_some() {
        base_seq + 2
    } else {
        base_seq + 1
    };

    let journal = Journal {
        operation_id: new_note_id(),
        draft_id: draft_id.to_string(),
        proposal_id: input.proposal.id.clone(),
        hunk_id: input.hunk_id.clone(),
        resolves: input.resolves,
        path: prompt.clone(),
        prior_sha256: fs::sha256_bytes(&prior),
        candidate_sha256: fs::sha256_bytes(normalised.as_bytes()),
        entry_id: entry_id.clone(),
        original_entry_id: original_id.clone(),
        entry_seq: next_seq,
        comment_id: new_note_id(),
        event_id: new_note_id(),
        thread_id: input.proposal.thread_id.clone(),
        comment_body: input.comment_body,
        created_at: now_rfc3339(),
        phase: Phase::Prepared,
        agent: input.proposal.agent.clone(),
        comment_by: input.comment_by.clone(),
    };

    // Anything from here to the commit point that fails is undone whole.
    type Prepared = (Option<DraftHistoryEntry>, Option<DraftHistoryEntry>);
    let prepared = || -> Result<Prepared, String> {
        root.write_bytes_atomic(hist.join(JOURNAL_PRIOR), &prior)
            .map_err(|e| e.to_string())?;
        root.write_toml_atomic(hist.join(JOURNAL_FILE), &journal)
            .map_err(|e| e.to_string())?;
        // DHS-FR-07: the superseded prompt is settled before the version that
        // supersedes it, so the two never land out of order and a rollback that
        // catches the operation between them has one entry to remove rather than
        // an `Original` that outlives the acceptance it belonged to.
        let original = match original_id.as_deref() {
            Some(id) => Some(write_entry(
                root,
                &hist,
                id,
                draft_id,
                original_seq,
                &prompt,
                DraftHistorySource::Original,
                &prior,
            )?),
            None => None,
        };
        let entry = match entry_id.as_deref() {
            Some(id) => Some(write_entry(
                root,
                &hist,
                id,
                draft_id,
                next_seq,
                &prompt,
                DraftHistorySource::ProposalAccepted {
                    proposal_id: input.proposal.id.clone(),
                    hunk_id: input.hunk_id.clone(),
                    agent: input.proposal.agent.clone(),
                },
                normalised.as_bytes(),
            )?),
            None => None,
        };
        // DRS-FR-12: written through the draft's own save path, which stays the
        // only write path for a prompt whoever composed the text.
        drafts::save_prompt_at(root, &dir, draft_id, &prompt, input.candidate)?;
        match input.hunk_id.as_deref() {
            // DCP-FR-PWSF: one hunk's row moves and the record resettles around
            // it, so hunks decided by an earlier operation are untouched.
            Some(hunk_id) => {
                draft_proposals::set_hunk_state(
                    root,
                    draft_id,
                    &input.proposal.id,
                    hunk_id,
                    draft_proposals::hunks::HunkState::Accepted,
                )?;
            }
            None => {
                draft_proposals::set_decided(
                    root,
                    draft_id,
                    &input.proposal.id,
                    ProposalState::Accepted,
                )?;
            }
        }
        Ok((original, entry))
    };

    let (original, entry) = match prepared() {
        Ok(entries) => entries,
        Err(reason) => {
            // DHS-FR-17 / DHS-FR-19: the prompt is byte-for-byte what it was, no
            // entry exists, the proposal is still `pending`, and no comment was
            // appended — none can have been, the append standing after the
            // commit point.
            let rolled = roll_back(root, &dir, &hist, &journal);
            log_recovery(app, "rollback", draft_id, &journal, &reason);
            if rolled.is_err() {
                return Err(ERR_HISTORY_RECOVERY_FAILED.to_string());
            }
            withdraw(app, draft_id, &journal);
            return Err(ERR_WRITE_FAILED.to_string());
        }
    };

    // ---- THE COMMIT POINT: one atomic rewrite of the journal ----------------
    let committed = Journal {
        phase: Phase::Committed,
        ..journal.clone()
    };
    if root
        .write_toml_atomic(hist.join(JOURNAL_FILE), &committed)
        .is_err()
    {
        let rolled = roll_back(root, &dir, &hist, &journal);
        log_recovery(app, "rollback", draft_id, &journal, "journal commit failed");
        if rolled.is_err() {
            return Err(ERR_HISTORY_RECOVERY_FAILED.to_string());
        }
        withdraw(app, draft_id, &journal);
        return Err(ERR_WRITE_FAILED.to_string());
    }

    // DCP-FR-16 / DHS-FR-22: the prompt's own write is announced first, being
    // the ordinary announcement of an ordinary write, and the version it
    // produced second — so a surface redrawing on the proposal (which its own
    // command announces after this returns) already holds both.
    drafts::announce(
        app,
        drafts::DraftChange::to(draft_id, vec![prompt.clone()]),
    );
    // DHS-FR-22: one event per entry that became exposed, oldest first — the
    // `Original` this acceptance settled, where it settled one, and then the
    // version it produced.
    if let Some(entry) = original.as_ref() {
        announce_entry(app, draft_id, Some(entry));
    }
    if let Some(entry) = entry.as_ref() {
        announce_entry(app, draft_id, Some(entry));
    }
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "draft acceptance committed",
        log_fields! {
            "draftId" => draft_id,
            "proposalId" => &journal.proposal_id,
            "operationId" => &journal.operation_id,
            "entryId" => journal.entry_id.clone().unwrap_or_default(),
            // DHS-FR-07: a first acceptance settles two versions, and a record
            // naming one of them would leave the other unaccounted for by the
            // one channel this is debuggable from.
            "originalEntryId" => journal.original_entry_id.clone().unwrap_or_default(),
        },
    );
    let proposal = draft_proposals::read_proposal(root, draft_id, &input.proposal.id)?;

    // DHS-FR-15: only the decision that resolves the proposal owes the
    // conversation a comment. An intermediate acceptance finishes here with
    // nothing owed, so `acceptance_incomplete` is reachable from a resolving
    // decision alone.
    if !committed.resolves {
        finish(root, &hist, &committed);
        return Ok(AcceptedOutcome {
            entry,
            original,
            proposal,
            comment_id: None,
        });
    }

    // DHS-FR-17: the append is retried once inside the call; where the retry also
    // fails the operation is left journaled at `committed` and the acceptance is
    // reported as landed-but-incomplete, because it did land and reporting
    // otherwise would be a lie about the author's own draft.
    match append_decision(app, store, &committed) {
        Ok(()) => {
            finish(root, &hist, &committed);
            Ok(AcceptedOutcome {
                entry,
                original,
                proposal,
                comment_id: Some(committed.comment_id),
            })
        }
        Err(_) => match append_decision(app, store, &committed) {
            Ok(()) => {
                finish(root, &hist, &committed);
                Ok(AcceptedOutcome {
                    entry,
                    original,
                    proposal,
                    comment_id: Some(committed.comment_id),
                })
            }
            Err(reason) => {
                log_recovery(app, "comment_owed", draft_id, &committed, &reason);
                Err(ERR_ACCEPTANCE_INCOMPLETE.to_string())
            }
        },
    }
}

/// The last step of a committed operation: the journal reaches `complete` and is
/// then removed with the prior payload it no longer needs (DHS-FR-14).
///
/// The `complete` write is made rather than skipped straight to the removal
/// because a crash between the two must leave a journal that reconciles to
/// "nothing left to do" rather than one that re-appends the comment — which
/// would be harmless (CMS-FR-06 folds it once) but would also be a lie about
/// what still had to happen.
fn finish(root: &fs::RootFs, hist: &Path, journal: &Journal) {
    let done = Journal {
        phase: Phase::Complete,
        ..journal.clone()
    };
    let _ = root.write_toml_atomic(hist.join(JOURNAL_FILE), &done);
    let _ = root.delete_under(hist, JOURNAL_FILE, false);
    let _ = root.delete_under(hist, JOURNAL_PRIOR, false);
}

/// DHS-FR-22: announce that a rolled-back operation's entry is gone again, so a
/// surface holding it is told rather than left showing a version the application
/// has just withdrawn.
///
/// Silent where the operation was never going to create one (DHS-FR-16): there
/// is nothing to withdraw, and an event saying otherwise would have a surface
/// re-read for a change that never happened.
///
/// One event however many entries went, the payload being null: what a surface
/// does with it is re-read the list, and it re-reads that list once whether one
/// version was withdrawn or two.
fn withdraw<R: tauri::Runtime>(app: &tauri::AppHandle<R>, draft_id: &str, journal: &Journal) {
    if journal.entry_id.is_some() || journal.original_entry_id.is_some() {
        announce_entry(app, draft_id, None);
    }
}

/// DHS-FR-19: undo an operation that never committed.
///
/// Idempotent in every step, because it is run both inline and by a
/// reconciliation that may itself be interrupted: the prompt is restored only
/// where it currently holds the candidate's digest, and each removal tolerates
/// a file that is already gone.
fn roll_back(
    root: &fs::RootFs,
    dir: &Path,
    hist: &Path,
    journal: &Journal,
) -> Result<(), String> {
    let prompt_abs = fs::resolve_under(dir.join(drafts::FILES_DIR), &journal.path)
        .map_err(|e| e.to_string())?;
    let current = root.sha256_file(&prompt_abs).map_err(|e| e.to_string())?;
    if current == journal.candidate_sha256 && current != journal.prior_sha256 {
        let prior = root
            .read_text(hist.join(JOURNAL_PRIOR))
            .map_err(|_| ERR_HISTORY_RECOVERY_FAILED.to_string())?;
        drafts::save_prompt_at(root, dir, &journal.draft_id, &journal.path, &prior)
            .map_err(|_| ERR_HISTORY_RECOVERY_FAILED.to_string())?;
    }
    // DRS-FR-WYIN / DRS-FR-JDRY: a settled version is private draft storage.
    // The ignore file keeps it out of Git, and no commit names it.
    crate::drafts::git_storage::ensure_private_ignored(root);
    for id in [
        journal.entry_id.as_deref(),
        journal.original_entry_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        for name in [
            format!("{id}.{SNAPSHOT_EXT}"),
            format!("{id}.{MANIFEST_EXT}"),
        ] {
            let _ = root.delete_under(hist, &name, false);
            if root.file_info(hist.join(&name)).is_ok() {
                return Err(ERR_HISTORY_RECOVERY_FAILED.to_string());
            }
        }
    }
    match journal.hunk_id.as_deref() {
        Some(hunk_id) => draft_proposals::set_hunk_pending(
            root,
            &journal.draft_id,
            &journal.proposal_id,
            hunk_id,
        ),
        None => draft_proposals::set_pending(root, &journal.draft_id, &journal.proposal_id),
    }
    .map_err(|_| ERR_HISTORY_RECOVERY_FAILED.to_string())?;
    let _ = root.delete_under(hist, JOURNAL_FILE, false);
    let _ = root.delete_under(hist, JOURNAL_PRIOR, false);
    Ok(())
}

/// DHS-FR-20: append the decision comment under the ids the journal minted.
///
/// The lock the conversation may since have taken is deliberately not consulted:
/// DCP-FR-15 checked it before the transaction began, and the transaction has
/// committed. A lock applied in the window between is a fact about what the
/// conversation will take **next**, not licence to leave a decision the author
/// already made unrecorded.
fn append_decision<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    // CMS-FR-01: the decision comment goes into the repository machine store
    // alone. The prompt, the entry, and the journal are the worktree's, and
    // this step writes none of them.
    store: &fs::RootFs,
    journal: &Journal,
) -> Result<(), String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let thread = draft_proposals::append_decision_comment(
        store,
        &journal.draft_id,
        &journal.thread_id,
        journal.comment_id.clone(),
        journal.event_id.clone(),
        journal.comment_body.clone(),
        &journal.comment_by,
    )?;
    // CMS-FR-51: the decision's own comment reaches the conversation's surface on
    // exactly the terms the proposal's did.
    comments::emit_discussion_changed(app, &thread);
    Ok(())
}

/// What a reconciliation did, for a caller that has to answer differently
/// because of it (DCP-FR-28).
///
/// The `RolledForward` case is the one that carries anything: an acceptance that
/// committed and was interrupted before its comment landed owes the conversation
/// exactly one line, and the author's retry is what completes it (DCR-FR-16). A
/// decision that finds the proposal already `accepted` for *that* reason answers
/// with the comment reconciliation just appended rather than with
/// `already_decided`, so the surface has a comment to name as the fresh turn's
/// trigger (DCR-FR-15) rather than a settled proposal and no way to tell the
/// agent about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reconciliation {
    /// No journal stood, or one stood in phase `complete` with nothing owed.
    Nothing,
    /// An operation that never committed was undone (DHS-FR-19).
    RolledBack,
    /// A committed operation's decision comment was appended (DHS-FR-20).
    RolledForward {
        proposal_id: String,
        comment_id: String,
    },
}

/// DHS-FR-18: read the journal and do exactly what the phase it finds requires.
///
/// Run before every operation that reads or writes a draft's history or its
/// proposals, and **idempotent**: running it twice does what running it once
/// did, and a reconciliation interrupted part-way is completed by the next run.
/// A draft carrying no journal reconciles in one absent-file read, so the
/// ordinary path costs nothing.
pub fn reconcile<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    // CMS-FR-01: the decision comment goes into the repository machine
    // store, while the prompt, the entry, and the journal stay in the worktree.
    store: &fs::RootFs,
    draft_id: &str,
) -> Result<Reconciliation, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    // DHS-FR-23: a journal on disk says where an operation got to and nothing
    // about whether the process that wrote it is still alive. Without this lock
    // an ordinary read — a second window merely opening the draft — would read
    // a live acceptance's `prepared` journal as an abandoned one and roll it
    // back underneath the transaction still writing it.
    let _guard = draft_proposals::draft_lock(draft_id);
    reconcile_locked(app, root, store, draft_id)
}

/// [`reconcile`] for a caller already holding the draft's lock (DHS-FR-23).
pub(crate) fn reconcile_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    // CMS-FR-01: the decision comment goes into the repository machine
    // store, while the prompt, the entry, and the journal stay in the worktree.
    store: &fs::RootFs,
    draft_id: &str,
) -> Result<Reconciliation, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let dir = draft_dir(root, draft_id)?;
    let hist = dir.join(drafts::HISTORY_DIR);
    let Some(journal) = read_journal(root, &hist)? else {
        return Ok(Reconciliation::Nothing);
    };
    match journal.phase {
        Phase::Prepared => {
            roll_back(root, &dir, &hist, &journal)?;
            log_recovery(app, "rollback", draft_id, &journal, "interrupted before commit");
            withdraw(app, draft_id, &journal);
            Ok(Reconciliation::RolledBack)
        }
        Phase::Committed => {
            append_decision(app, store, &journal)?;
            finish(root, &hist, &journal);
            log_recovery(app, "roll_forward", draft_id, &journal, "comment re-appended");
            Ok(Reconciliation::RolledForward {
                proposal_id: journal.proposal_id.clone(),
                comment_id: journal.comment_id.clone(),
            })
        }
        Phase::Complete => {
            let _ = root.delete_under(&hist, JOURNAL_FILE, false);
            let _ = root.delete_under(&hist, JOURNAL_PRIOR, false);
            Ok(Reconciliation::Nothing)
        }
    }
}

