//! The decision functions behind the hunk commands, called in-process
//! (`DCP-draft-change-proposals.md` DCP-FR-LAWR).
//!
//! The Tauri commands and `crate::tools::decide_draft_changes` call these
//! functions with the same arguments. So a decision by tool passes the same
//! checks, emits the same event, appends the same decision comment, and
//! contributes the same statistics line as the decision by chip
//! (DCP-FR-CIPW).

use super::*;

/// DCP-FR-11 / DCP-FR-14 / DCP-FR-LAWR: decide one hunk.
///
/// The graduation lock and the GitHub-shadow check come first, then the
/// decision engine, then the event of DCP-FR-16.
pub(crate) fn decide_hunk_in_process<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    repo_store: &fs::RootFs,
    proposal_id: &str,
    verdict: Verdict<'_>,
    by: &Participant,
) -> Result<DecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    // DRS-FR-19: a draft a graduation run holds takes no write by any route.
    if let Some((draft_id, _)) = find_proposal(root, proposal_id) {
        // DRS-FR-QPSC: an accepted change is a write of a GitHub-shadow
        // draft's prompt, refused with its own reason before the lock.
        if verdict.decision == Decision::Accept {
            drafts::require_not_github_shadow(root, &draft_id)?;
        }
        crate::graduation::require_unlocked_draft(app, &draft_id)?;
    }
    let operation =
        if verdict.decision == Decision::Accept { "accept_hunk" } else { "reject_hunk" };
    let Decided { proposal, comment_id, origin_kind, comment_failed } =
        decide(app, root, repo_store, proposal_id, verdict, by)
            .inspect_err(|e| log_refusal(app, operation, e))?;
    if let Some(reason) = comment_failed {
        log_refusal(app, "hunk_comment", &reason);
    }
    // DCP-FR-16: emitted when a change is decided, after any transaction it
    // delegated has committed.
    announce(app, &BUFFER, &proposal, ProposalChangeCause::Decided);
    Ok(DecisionOutcome { proposal, comment_id, origin_kind })
}

/// DCP-FR-PWSF / DCP-FR-LAWR: hold one change for discussion, or release it.
pub(crate) fn hold_hunk_in_process<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    proposal_id: &str,
    hunk_id: &str,
    discussing: bool,
) -> Result<DraftChangeProposal, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    // The draft is found before the lock, because the lock is per draft and
    // there is no way to name it otherwise. Everything the decision is *checked*
    // against is read again below, inside the lock.
    let (draft_id, _) = find_proposal(root, proposal_id).ok_or(ERR_PROPOSAL_NOT_FOUND)?;
    let _guard = draft_lock(&draft_id);
    // DCP-FR-NKTB: read again inside the lock. A decision that landed on
    // another change between the read above and this line has already settled
    // this proposal, and holding a settled change for discussion would put the
    // whole proposal back to `pending` after its acceptance had written the
    // prompt and recorded a version.
    let mut record =
        find_proposal(root, proposal_id).map(|(_, r)| r).ok_or(ERR_PROPOSAL_NOT_FOUND)?;
    // DCP-FR-QLMH: as in `decide`, a legacy record is given the row its one
    // change is named by.
    let dir = drafts::draft_proposals_dir(root, &draft_id)?;
    backfill_legacy_ledger(root, &dir, &mut record, &prompt_of(root, &draft_id));
    let row = record.ledger.iter().find(|row| row.id == hunk_id).ok_or(ERR_HUNK_NOT_FOUND)?;
    // DCP-FR-MKIM: a withdrawn change is not held either.
    if row.state == hunks::HunkState::Withdrawn {
        return Err(ERR_HUNK_WITHDRAWN.to_string());
    }
    if !row.state.is_undecided() {
        return Err(ERR_HUNK_ALREADY_DECIDED.to_string());
    }
    let next = if discussing { hunks::HunkState::Discussing } else { hunks::HunkState::Pending };
    let proposal = set_hunk_state(root, &draft_id, proposal_id, hunk_id, next)
        .inspect_err(|e| log_refusal(app, "hold_hunk", e))?;
    announce(app, &BUFFER, &proposal, ProposalChangeCause::Held);
    Ok(proposal)
}

/// DDC-FR-DSTU: one proposal as it stands now, with the ledger row of a
/// legacy record filled in (DCP-FR-QLMH). It writes nothing.
pub(crate) fn proposal_as_it_stands(
    root: &fs::RootFs,
    proposal_id: &str,
) -> Option<DraftChangeProposal> {
    let (draft_id, mut record) = find_proposal(root, proposal_id)?;
    if let Ok(dir) = drafts::draft_proposals_dir(root, &draft_id) {
        backfill_legacy_ledger(root, &dir, &mut record, &prompt_of(root, &draft_id));
    }
    Some(record)
}

/// DCP-FR-BMLX: whether one hunk resolves nowhere in the prompt as it stands.
///
/// A hunk that the document does not hold reads as not lost, so the caller
/// refuses it for the reason that applies to it.
pub(crate) fn hunk_is_lost(root: &fs::RootFs, proposal_id: &str, hunk_id: &str) -> bool {
    let Ok(read) = load_hunks_impl(root, proposal_id) else {
        return false;
    };
    read.hunks
        .iter()
        .position(|hunk| hunk.id == hunk_id)
        .and_then(|at| read.resolutions.get(at))
        .is_some_and(|placement| matches!(placement, HunkPlacement::Lost))
}
