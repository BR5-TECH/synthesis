//! Reconciling an acceptance that did not finish, and the sweep (PCP-FR-14,
//! PCP-FR-20).

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
    Settled { proposal_id: String, comment_id: Option<String> },
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
    let journal =
        root.read_toml::<Journal>(&journal_path).map_err(|_| ERR_WRITE_FAILED.to_string())?;
    let record = read_proposal(root, proposal_id)?;

    // What to do is read from the artifact's own bytes.
    let current = fs::resolve_under(root.path(), &journal.artifact_id)
        .ok()
        .and_then(|p| root.sha256_file(p).ok());

    if current.as_deref() == Some(journal.candidate_sha256.as_str()) {
        // The commit point was passed.
        //
        // Read first, and deliberately so, for the one case where the two
        // checksums coincide — a hunk whose acceptance leaves the file as it
        // was. The file then reads the same whichever side of the commit point
        // the operation stopped on, so settling `accepted` is truthful either
        // way: the author activated Accept (a journal exists for no other
        // reason) and the file holds exactly the bytes they accepted.
        let hunk_id = journal.hunk();
        let mut settled = record.clone();
        let row_accepted = settled
            .ledger
            .iter()
            .find(|row| row.id == hunk_id)
            .is_some_and(|row| row.state == HunkState::Accepted);
        if !row_accepted {
            if let Some(row) = settled.ledger.iter_mut().find(|row| row.id == hunk_id) {
                row.state = HunkState::Accepted;
                row.decided_at = Some(now_rfc3339());
            }
            ledger::resettle(&mut settled);
            settled.comment_owed = journal.resolves;
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
            comment_id = journal.resolves.then(|| journal.comment_id.clone());
        }
        finish(root, &dir, proposal_id);
        log_recovery(app, "roll_forward", &settled, "commit point passed");
        announce(app, &BUFFER, &settled);
        return Ok(Reconciliation::Settled { proposal_id: proposal_id.to_string(), comment_id });
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
    roll_back(root, &dir, &journal).map_err(|_| ERR_WRITE_FAILED.to_string())?;
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
) where
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

/// PCP-FR-20's sweep alone: the record and hunks of a **decided** proposal
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
            let _ = root.delete_under(&dir, format!("{}.{HUNKS_EXT}", record.id), false);
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
