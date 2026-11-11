//! The commit of a closed proposal (CEP-FR-FOJW, CEP-FR-XVXQ, CEP-FR-IYHP,
//! CEP-FR-OAIW).

use super::*;

/// The message of every commit this module makes (CEP-FR-FOJW).
pub fn commit_message(change: &str) -> String {
    format!("Accept edits to OpenSpec change {change}")
}

/// CEP-FR-FOJW / CEP-FR-IYHP: commit the files of a closed proposal, and every
/// file another proposal of the same change still owes.
///
/// It returns whether a commit landed. A proposal that is still pending, or
/// that closed with no accepted hunk (CEP-FR-OAIW), commits nothing. A commit
/// that fails marks the proposal `commit_owed`, and the files stay written.
pub(super) fn commit_close<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    site: Option<&ChangeSite>,
    proposal_id: &str,
) -> bool
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let Ok((record, dir)) = read_proposal(store, proposal_id) else {
        return false;
    };
    if record.state == ChangeProposalState::Pending || record.counts.accepted == 0 {
        return false;
    }
    // CEP-FR-IYHP: every file still owed by the change goes in the same commit.
    let owing: Vec<ChangeEditProposal> = read_change(store, &record.stream_id, &record.change)
        .into_iter()
        .filter(|p| p.id == record.id || p.commit_owed)
        .collect();
    let mut paths: Vec<String> = Vec::new();
    for proposal in &owing {
        for path in &proposal.written_paths {
            if !paths.contains(path) {
                paths.push(path.clone());
            }
        }
    }

    let outcome = match site {
        None => Err(ERR_UNKNOWN_STREAM.to_string()),
        Some(site) => {
            let rels: Result<Vec<String>, String> =
                paths.iter().map(|p| site.file_rel(p)).collect();
            rels.and_then(|rels| {
                // CEP-FR-XVXQ: through GTC, and never a push. A path another
                // writer also changed is committed as it stands on disk.
                match crate::git::commit_exact_paths(
                    &site.working_copy,
                    &commit_message(&record.change),
                    &rels,
                ) {
                    Ok(_) => Ok(true),
                    Err(e) if e == crate::git::ERR_NOTHING_TO_COMMIT => Ok(false),
                    Err(e) => Err(e),
                }
            })
        }
    };

    match outcome {
        Ok(landed) => {
            for mut proposal in owing {
                let was_owed = proposal.commit_owed;
                if was_owed {
                    proposal.commit_owed = false;
                    let _ = write_record(store, &dir, &proposal);
                }
                if was_owed || proposal.id == record.id {
                    announce(app, &proposal, ChangeProposalCause::Committed);
                }
            }
            logging::log_info(
                app,
                &BUFFER,
                &[Domain::Backend],
                "change edit proposal committed",
                log_fields! {
                    "streamId" => &record.stream_id,
                    "change" => &record.change,
                    "proposalId" => &record.id,
                    "files" => paths.len(),
                    "landed" => landed,
                },
            );
            landed
        }
        Err(reason) => {
            let mut owed = record.clone();
            owed.commit_owed = true;
            let _ = write_record(store, &dir, &owed);
            logging::log_warn(
                app,
                &BUFFER,
                &[Domain::Backend],
                "change edit proposal commit failed",
                log_fields! {
                    "streamId" => &record.stream_id,
                    "change" => &record.change,
                    "proposalId" => &record.id,
                    "files" => paths.len(),
                    "reason" => &reason,
                },
            );
            announce(app, &owed, ChangeProposalCause::Committed);
            false
        }
    }
}

/// CEP-FR-QVLO: a proposal that closed contributes one `proposal_decision`
/// event to its change's statistics, after the closing decision committed.
/// A proposal that is still pending contributes none.
pub(super) fn record_close_statistics<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    proposal_id: &str,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let Ok((record, _)) = read_proposal(store, proposal_id) else {
        return;
    };
    let decision = match record.state {
        ChangeProposalState::Pending => return,
        ChangeProposalState::Accepted => crate::statistics::Decision::Accepted,
        ChangeProposalState::Rejected => crate::statistics::Decision::Rejected,
    };
    crate::change_statistics::record_change_statistics_event(
        app,
        &record.stream_id,
        &record.change,
        crate::statistics::EventBody::ProposalDecision { proposal_id: record.id.clone(), decision },
    );
}
