//! Reconciling a decision that did not finish, and deciding a proposal.

use super::*;

/// What a reconciliation did, for a caller that has to answer from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reconciliation {
    /// No journal stood — the proposal is settled already.
    Nothing,
    /// A transaction that never committed was undone (PCP-FR-14).
    RolledBack,
    /// A committed acceptance was settled; the comment carried where one was
    /// appended by this run or was already there.
    Settled {
        proposal_id: String,
        comment_id: Option<String>,
    },
}

/// PCP-FR-14: settle an interrupted transaction, reading the artifact's own
/// bytes against the two checksums the journal recorded.
///
/// Idempotent: running it twice does what running it once did, and a
/// reconciliation interrupted part-way is completed by the next run. A proposal
/// with no journal reconciles in one absent-file read, so the ordinary path
/// costs nothing.
pub fn reconcile<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
    proposal_id: &str,
) -> Result<Reconciliation, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let Ok(record) = read_proposal(root, proposal_id) else {
        return Ok(Reconciliation::Nothing);
    };
    let _guard = artifact_lock(&record.artifact_id);
    reconcile_locked(app, root, store, proposal_id)
}

/// [`reconcile`] for a caller already holding the artifact's lock.
pub(super) fn reconcile_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
    proposal_id: &str,
) -> Result<Reconciliation, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let dir = proposals_dir(root)?;
    let journal_path = at(&dir, proposal_id, JOURNAL_EXT);
    if root.file_info(&journal_path).is_err() {
        return Ok(Reconciliation::Nothing);
    }
    // A journal that will not parse is an error rather than an absence: acting
    // as though no operation stood would let a second acceptance run over an
    // artifact whose first one may have written half of itself.
    let journal = root
        .read_toml::<Journal>(&journal_path)
        .map_err(|_| ERR_WRITE_FAILED.to_string())?;
    let record = read_proposal(root, proposal_id)?;

    // What to do is read from the artifact's own bytes.
    let current = fs::resolve_under(root.path(), &journal.artifact_id)
        .ok()
        .and_then(|p| root.sha256_file(p).ok());

    if current.as_deref() == Some(journal.candidate_sha256.as_str()) {
        // The commit point was passed.
        //
        // Read first, and deliberately so, for the one case where the two
        // checksums coincide — an author who edited the candidate to what the
        // artifact already held. The file then reads the same whichever side of
        // the commit point the operation stopped on, so there is nothing to tell
        // apart: settling `accepted` is truthful either way, the author having
        // activated Accept (a journal exists for no other reason) and the file
        // holding exactly the bytes they accepted.
        let mut settled = record.clone();
        if settled.state != PromptProposalState::Accepted {
            settled.state = PromptProposalState::Accepted;
            settled.decided_at = Some(settled.decided_at.clone().unwrap_or_else(now_rfc3339));
            settled.comment_owed = true;
            root.write_toml_atomic(at(&dir, proposal_id, RECORD_EXT), &settled)
                .map_err(|_| ERR_WRITE_FAILED.to_string())?;
        }
        let comment_id;
        if settled.comment_owed {
            let (thread_artifact, discussion) = locate_thread(root, store, &journal.thread_id)
                .map_err(|_| ERR_WRITE_FAILED.to_string())?;
            match append_decision(app, store, &journal, &thread_artifact, discussion) {
                Ok(()) => {
                    settled = clear_owed(root, &dir, proposal_id)?;
                    comment_id = Some(journal.comment_id.clone());
                }
                Err(reason) => {
                    // The journal stays exactly where it was, so the next
                    // reconciliation finds the same debt and the same means of
                    // paying it.
                    log_recovery(app, "comment_owed", &record, &reason);
                    announce(app, &BUFFER, &settled);
                    return Ok(Reconciliation::Settled {
                        proposal_id: proposal_id.to_string(),
                        comment_id: None,
                    });
                }
            }
        } else {
            comment_id = Some(journal.comment_id.clone());
        }
        finish(root, &dir, proposal_id);
        log_recovery(app, "roll_forward", &settled, "commit point passed");
        announce(app, &BUFFER, &settled);
        return Ok(Reconciliation::Settled {
            proposal_id: proposal_id.to_string(),
            comment_id,
        });
    }

    if current.as_deref() != Some(journal.prior_sha256.as_str()) {
        // PCP-FR-14: something outside this transaction has written that file,
        // so the file is left exactly as it is — nothing this module has is a
        // better answer than the bytes an author or another process put there.
        logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "prompt change acceptance left an unrecognised file alone",
            log_fields! {
                "artifactId" => &journal.artifact_id,
                "proposalId" => proposal_id,
                "path" => &journal.path,
            },
        );
    }

    // The artifact write never landed — or the file has moved on and the safe
    // settlement is the same. Back to `pending`, so it is offered again rather
    // than claimed as accepted on evidence that no longer exists.
    roll_back(root, &dir, &record).map_err(|_| ERR_WRITE_FAILED.to_string())?;
    log_recovery(app, "rollback", &record, "interrupted before commit");
    Ok(Reconciliation::RolledBack)
}

/// PCP-FR-14: reconcile every journal the folder holds, and PCP-FR-20's sweep.
///
/// Runs at project open and on every worktree change, before any surface has
/// been served an artifact.
pub fn on_content_root_mounted<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
)
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let Ok(dir) = proposals_dir(root) else {
        return;
    };
    let Ok(entries) = root.list_dir(&dir) else {
        return;
    };
    for entry in entries {
        let Some(id) = entry.name.strip_suffix(&format!(".{JOURNAL_EXT}")) else {
            continue;
        };
        if !is_valid_proposal_id(id) {
            continue;
        }
        let _ = reconcile(app, root, store, id);
    }
    sweep_on_content_root_mounted(app, root);
}

/// PCP-FR-20's sweep alone: the record and candidate of a **decided** proposal
/// whose artifact the project no longer holds.
///
/// Separated from the reconciliation because it reads the **worktree** and
/// nothing else. A store that could not be resolved costs a decision comment
/// (per `RMS-repository-machine-storage.md` RMS-FR-WGQS) and must not also cost
/// the housekeeping the worktree alone can do.
pub fn sweep_on_content_root_mounted<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let Ok(dir) = proposals_dir(root) else {
        return;
    };
    // A pending proposal is never swept, whatever its target has become,
    // because it is still the author's to decline.
    for record in read_all(root) {
        if record.state == PromptProposalState::Pending {
            continue;
        }
        if require_prompt_artifact(root, &record.artifact_id).is_err()
            && fs::resolve_under(root.path(), &record.artifact_id)
                .ok()
                .and_then(|p| root.file_info(p).ok())
                .is_none()
        {
            let _ = root.delete_under(&dir, format!("{}.{RECORD_EXT}", record.id), false);
            let _ = root.delete_under(&dir, format!("{}.{CONTENT_EXT}", record.id), false);
            logging::log_info(
                app,
                &BUFFER,
                &[Domain::Backend],
                "prompt change proposal swept",
                log_fields! {
                    "artifactId" => &record.artifact_id,
                    "proposalId" => &record.id,
                    "path" => &record.path,
                },
            );
        }
    }
}

/// One decision, whichever way it went (PCP-FR-12, PCP-FR-15, PCP-FR-16).
pub(super) fn decide<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
    proposal_id: &str,
    decision: Decision,
    feedback: Option<&str>,
    by: &Participant,
    tracker: &ContentTracker,
) -> Result<PromptDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let held = read_proposal(root, proposal_id)?;
    // PCP-FR-14: checked before the artifact lock is taken, so a decision made
    // while another acceptance of this proposal is in flight is refused rather
    // than queued behind it and then answered `already_decided`.
    if is_accepting(proposal_id) {
        return Err(ERR_ACCEPTANCE_IN_PROGRESS.to_string());
    }
    // PCP-FR-18 / PCP-FR-14: held for the whole decision — the already-decided
    // check and the writes that follow it are one act, and the reconciliation
    // between them must not race an acceptance running in another window.
    let _guard = artifact_lock(&held.artifact_id);
    // PCP-FR-14: reconciled before the proposal is reported in any state, so
    // neither decision ever answers from one a journal has not yet settled. What
    // the reconciliation *did* is deliberately not read here: an acceptance it
    // settled is a decision already made, and this call is a second one.
    reconcile_locked(app, root, store, proposal_id)?;
    let record = read_proposal(root, proposal_id)?;

    // PCP-FR-18: decided once, and a second decision — of **either** kind —
    // finds it settled rather than overwriting what the first decided.
    //
    // There is deliberately no exception here for an acceptance whose comment
    // was owed and which the reconciliation above has just completed. Answering
    // such a call with that acceptance's own outcome would let a **decline**
    // report success carrying the *accept* comment: the author would believe
    // they had declined, the agent would be told the change was accepted, and
    // the artifact would keep the accepted text, with nothing anywhere reporting
    // an error. Completing an owed comment is `complete_prompt_change_decision`'s
    // and is reached from the one control that offers it (PCR-FR-15).
    if record.state != PromptProposalState::Pending {
        return Err(ERR_ALREADY_DECIDED.to_string());
    }

    // PCP-FR-12 / PCP-FR-19: checked for both decisions where it is a fact about
    // the target rather than about the write — an acceptance refuses a target
    // that is gone or no longer a prompt, and a decline clears such a proposal.
    if decision == Decision::Accept {
        require_prompt_artifact(root, &record.artifact_id)?;
    }

    let (thread_artifact, discussion) = locate_thread(root, store, &record.thread_id)?;
    // PCP-FR-16: the two conditions that would make the append impossible are
    // checked **before anything is written**. The identity has already been
    // resolved by the command; the lock is this.
    if comments::thread_at(
        store,
        thread_ref(&thread_artifact, discussion),
        &record.thread_id,
    )?
    .locked
    {
        return Err(comments::ERR_DISCUSSION_LOCKED.to_string());
    }

    if decision == Decision::Accept {
        return accept_locked(
            app,
            root,
            store,
            &record,
            &thread_artifact,
            discussion,
            feedback,
            by,
            tracker,
        );
    }

    // PCP-FR-15: a decline moves the record to `rejected` and **writes nothing
    // into the project** — no artifact write, no journal begun — and keeps the
    // `.content` file, so a declined proposal can still be read back afterwards
    // as the candidate the author declined.
    //
    // The record moves before the comment is appended. Each can fail, and this
    // ordering makes both failures recoverable: a record that will not write
    // leaves a proposal still pending with nothing said, which the author's next
    // click repairs; a comment that will not append leaves the decision made and
    // the conversation one line short. The reverse order has a state neither
    // recovers from — a conversation saying the proposal was declined while the
    // record still reads pending.
    let dir = ensure_folder(root)?;
    let rejected = PromptChangeProposal {
        state: PromptProposalState::Rejected,
        decided_at: Some(now_rfc3339()),
        comment_owed: false,
        ..record.clone()
    };
    root.write_toml_atomic(at(&dir, proposal_id, RECORD_EXT), &rejected)
        .map_err(|e| e.to_string())?;

    // Minted here so the caller can name it as the fresh turn's trigger
    // (PCR-FR-14) without re-folding the thread to find what was just appended.
    let comment_id = new_note_id();
    let appended = comments::add_comment_with_id(
        store,
        thread_ref(&thread_artifact, discussion),
        &record.thread_id,
        comment_id.clone(),
        decision_body(decision, &record.path, record.candidate_edited, feedback),
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

    announce(app, &BUFFER, &rejected);
    Ok(PromptDecisionOutcome {
        proposal: rejected,
        comment_id,
        origin_kind: origin_kind_of(discussion),
    })
}
