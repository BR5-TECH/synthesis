//! The polling view, built from memory and disk alone (GPP-FR-UBDE,
//! GPP-FR-QCAM, GPP-FR-IGER, and the "get github polling state" read).

use super::eligibility::exclude_claimed;
use super::records::*;
use super::session::{SessionKey, SessionSlot};
use crate::drafts::{DraftStatus, DraftSummary, GithubIssueLink};

/// GPP-FR-UBDE: the shadow-draft rows of a drafts listing whose statuses and
/// graduation runs the caller already resolved.
pub fn shadow_rows(drafts: &[DraftSummary]) -> Vec<GithubShadowDraftRow> {
    drafts
        .iter()
        .filter_map(|draft| {
            let link = draft.github_issue.as_ref()?;
            Some(GithubShadowDraftRow {
                draft_id: draft.id.clone(),
                name: draft.name.clone(),
                // DRS-FR-EZDB: `graduated` while a committed run stands, and
                // `github_shadow` otherwise.
                status: match draft.status {
                    DraftStatus::Graduated => DraftStatus::Graduated,
                    _ => DraftStatus::GithubShadow,
                },
                repository_host: link.repository_host.clone(),
                repository_owner: link.repository_owner.clone(),
                repository_name: link.repository_name.clone(),
                issue_number: link.issue_number,
                issue_url: link.issue_url.clone(),
                project_node_id: link.project_node_id.clone(),
                locked: draft.graduation.as_ref().is_some_and(|g| g.locked),
            })
        })
        .collect()
}

/// The whole view. The snapshot rows exclude every issue a shadow draft or a
/// pending claim names now, not only when the poll ran (GPP-FR-QCAM).
pub fn build_view(
    slot: &SessionSlot,
    key: &SessionKey,
    settings: GithubPollingSettings,
    pending_claims: Vec<GithubPendingClaim>,
    shadows: Vec<GithubShadowDraftRow>,
) -> GithubPollingView {
    let configuration = slot.configuration_for(key, &settings);
    let session = slot.session_for(key);
    let links: Vec<GithubIssueLink> = shadows
        .iter()
        .map(|row| GithubIssueLink {
            repository_host: row.repository_host.clone(),
            repository_owner: row.repository_owner.clone(),
            repository_name: row.repository_name.clone(),
            issue_number: row.issue_number,
            issue_url: row.issue_url.clone(),
            project_node_id: row.project_node_id.clone(),
            claim_state: crate::drafts::GithubClaimState::Claimed,
        })
        .collect();
    let tasks = session
        .map(|s| exclude_claimed(s.tasks.clone(), &links, &pending_claims))
        .unwrap_or_default();
    GithubPollingView {
        settings,
        configuration,
        repository: session.and_then(|s| s.repository.clone()),
        polling: session.is_some_and(|s| s.in_flight.is_some()),
        tasks,
        stale: session.is_some_and(|s| s.stale),
        last_error_code: session.and_then(|s| s.last_error_code.clone()),
        last_error: session.and_then(|s| s.last_error.clone()),
        last_success_at: session.and_then(|s| s.last_success_at.clone()),
        pending_claims,
        shadows,
    }
}
