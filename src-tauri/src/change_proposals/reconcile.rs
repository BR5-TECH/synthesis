//! Settling an acceptance that did not finish (PCP-FR-14, applied by
//! CEP-FR-OPGW).

use super::*;
use crate::prompt_proposals::HunkState;

/// PCP-FR-14: settle an interrupted acceptance of `proposal_id`, reading the
/// file in the working copy against the digest the journal recorded. A caller
/// holds the change's lock.
///
/// A proposal with no journal reconciles in one absent-file read. Where the
/// stream cannot be reached, the journal stays and the call refuses, because
/// nothing can tell whether the write landed.
pub(super) fn reconcile_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    proposal_id: &str,
) -> Result<(), String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let (record, dir) = read_proposal(store, proposal_id)?;
    let journal_path = at(&dir, proposal_id, JOURNAL_EXT);
    if store.file_info(&journal_path).is_err() {
        return Ok(());
    }
    let journal =
        store.read_toml::<Journal>(&journal_path).map_err(|_| ERR_WRITE_FAILED.to_string())?;
    let site = sites.site(&record.stream_id, &record.change)?;
    let current =
        site.file_path(&journal.path).ok().and_then(|p| site.working_copy.sha256_file(p).ok());

    if current.as_deref() != Some(journal.candidate_sha256.as_str()) {
        // The write never landed, or something else has written the file
        // since: back to the prior row, and the file is left as it is.
        roll_back(store, &dir, &journal)?;
        log_recovery(app, "rollback", &record, "interrupted before commit");
        if let Ok((settled, _)) = read_proposal(store, proposal_id) {
            announce(app, &settled, ChangeProposalCause::Reconciled);
        }
        return Ok(());
    }

    // The commit point was passed: the row is accepted.
    let mut settled = record.clone();
    let accepted = settled.row(&journal.hunk_id).is_some_and(|r| r.state == HunkState::Accepted);
    if !accepted {
        if let Some(row) = settled.row_mut(&journal.hunk_id) {
            row.state = HunkState::Accepted;
            row.decided_at = Some(now_rfc3339());
        }
        if !settled.written_paths.contains(&journal.path) {
            settled.written_paths.push(journal.path.clone());
        }
        resettle(&mut settled);
        settled.comment_owed = journal.resolves;
        write_record(store, &dir, &settled)?;
    }
    if settled.comment_owed {
        let thread = locate_thread(store, &record.stream_id, &record.change, &journal.thread_id)?;
        if let Err(reason) = append_decision(app, store, &thread, &journal) {
            log_recovery(app, "comment_owed", &record, &reason);
            announce(app, &settled, ChangeProposalCause::Reconciled);
            return Err(ERR_ACCEPTANCE_INCOMPLETE.to_string());
        }
        settled = clear_owed(store, &dir, proposal_id)?;
    }
    finish(store, &dir, proposal_id);
    log_recovery(app, "roll_forward", &settled, "commit point passed");
    announce(app, &settled, ChangeProposalCause::Reconciled);
    if journal.resolves {
        commit_close(app, store, Some(&site), proposal_id);
        record_close_statistics(app, store, proposal_id);
    }
    Ok(())
}
