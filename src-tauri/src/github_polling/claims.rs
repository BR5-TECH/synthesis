//! Claiming a ready task (GPP-FR-IFVC, GPP-FR-ILGG, GPP-FR-DHQM,
//! GPP-FR-XPUO, GPP-FR-CWGH, GPP-FR-BSLI, GPP-FR-IGER, GPP-FR-TOED,
//! GPP-FR-KSMZ, GPP-FR-RAQP, GPP-FR-CRWY).
//!
//! The order is the whole guarantee: re-fetch, then move the issue to
//! `In Progress`, then record the pending claim, then create the shadow draft.
//! A claim that fails before the status update has written nothing; one that
//! fails after it leaves a pending claim — on disk, or held in memory where the
//! disk refused it — that a retry completes without ever touching GitHub's
//! status again.

use super::client::{FetchedIssue, GithubProjects};
use super::eligibility::{self, PollFailure};
use super::records::*;
use crate::drafts::{GithubClaimState, GithubIssueLink};
use crate::fs::RootFs;
use crate::project_settings::{load_github_pending_claims_from, update_github_pending_claims};

/// What a claim runs against.
pub struct ClaimContext<'a> {
    pub root: &'a RootFs,
    pub secret: &'a str,
    pub repository: &'a RepositoryRef,
    pub project_id: &'a str,
}

/// The pending claim for an issue of the repository, from disk or from the
/// claims held in memory.
pub fn pending_claim(
    root: &RootFs,
    repository: &RepositoryRef,
    number: u64,
    held: &[GithubPendingClaim],
) -> Option<GithubPendingClaim> {
    let names = |claim: &GithubPendingClaim| claim.names(&repository.owner, &repository.name, number);
    load_github_pending_claims_from(root)
        .into_iter()
        .find(names)
        .or_else(|| held.iter().find(|c| names(c)).cloned())
}

/// GPP-FR-DHQM / GPP-FR-CWGH: write one claim in place of any claim of the
/// same issue, as one locked read-modify-write.
fn put_claim(root: &RootFs, claim: &GithubPendingClaim) -> Result<(), String> {
    update_github_pending_claims(root, |claims| {
        claims.retain(|c| {
            !c.names(&claim.repository_owner, &claim.repository_name, claim.issue_number)
        });
        claims.push(claim.clone());
    })
    .map_err(|_| ERR_PENDING_CLAIM_WRITE_FAILED.to_string())
}

/// GPP-FR-BOQX: the selected Project's item of a re-fetched issue, where the
/// issue is still eligible.
fn ready_item<'a>(
    ctx: &ClaimContext<'_>,
    issue: &'a FetchedIssue,
) -> Option<&'a super::client::IssueProjectItem> {
    let item = issue.project_items.iter().find(|item| item.project_id == ctx.project_id)?;
    eligibility::is_eligible(
        &issue.state,
        issue.issue_type.as_deref(),
        &issue.repository_owner,
        &issue.repository_name,
        ctx.repository,
        item.status.as_deref(),
    )
    .then_some(item)
}

/// GPP-FR-XPUO: the shadow draft that names the claim's issue, or a new one
/// linked to the Project the claim names.
fn shadow_for(
    root: &RootFs,
    claim: &GithubPendingClaim,
    issue: &FetchedIssue,
) -> Result<(GithubClaimResult, bool), String> {
    let (owner, name) = (&claim.repository_owner, &claim.repository_name);
    // A draft the pending claim already names, where it still exists.
    if let Some(id) = claim.draft_id.as_deref() {
        if let Ok(record) = crate::drafts::read_draft_record(root, id) {
            if record.github_issue.as_ref().is_some_and(|l| l.names(owner, name, claim.issue_number)) {
                return Ok((GithubClaimResult { draft_id: record.id, draft_name: record.name }, false));
            }
        }
    }
    if let Some(existing) = crate::drafts::find_github_shadow(root, owner, name, claim.issue_number) {
        return Ok((GithubClaimResult { draft_id: existing.id, draft_name: existing.name }, false));
    }
    let link = GithubIssueLink {
        repository_owner: owner.clone(),
        repository_name: name.clone(),
        issue_number: claim.issue_number,
        issue_url: issue.url.clone(),
        project_node_id: claim.project_node_id.clone(),
        claim_state: GithubClaimState::Claimed,
    };
    let created = crate::drafts::create_github_shadow_draft(root, &issue.title, &issue.body, link)
        .map_err(|_| ERR_SHADOW_CREATE_FAILED.to_string())?;
    Ok((
        GithubClaimResult { draft_id: created.draft.id, draft_name: created.draft.name },
        true,
    ))
}

/// What a successful claim or retry did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimOutcome {
    pub result: GithubClaimResult,
    /// Whether this call created the shadow draft, rather than reused one.
    pub created: bool,
    /// The pending claim as it stands now, the draft id included.
    pub claim: GithubPendingClaim,
    /// Whether that claim is on disk. Where it is not, the caller holds it in
    /// memory so the retry stays offered.
    pub saved: bool,
}

/// Why a claim failed, and the claim to hold in memory where the status
/// update succeeded and the disk refused the record (GPP-FR-IGER).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimFailure {
    pub code: String,
    pub project_title: Option<String>,
    pub unsaved_claim: Option<GithubPendingClaim>,
}

impl From<PollFailure> for ClaimFailure {
    fn from(failure: PollFailure) -> Self {
        Self { code: failure.code, project_title: failure.project_title, unsaved_claim: None }
    }
}

impl ClaimFailure {
    fn code(code: impl Into<String>) -> Self {
        Self { code: code.into(), project_title: None, unsaved_claim: None }
    }
}

/// GPP-FR-IFVC through GPP-FR-CWGH: claim one ready task.
pub fn claim(
    ctx: &ClaimContext<'_>,
    client: &dyn GithubProjects,
    number: u64,
    now: &str,
    held: &[GithubPendingClaim],
) -> Result<ClaimOutcome, ClaimFailure> {
    // GPP-FR-RAQP: a claim already recorded on this machine is completed by a
    // retry, never by a second claim.
    if pending_claim(ctx.root, ctx.repository, number, held).is_some() {
        return Err(ClaimFailure::code(ERR_CLAIM_PENDING));
    }
    let configuration = eligibility::read_configuration(client, ctx.secret, ctx.project_id)?;
    // GPP-FR-IFVC: the issue as GitHub holds it now, before anything changes.
    let issue = client
        .fetch_issue(ctx.secret, &ctx.repository.owner, &ctx.repository.name, number)
        .map_err(ClaimFailure::code)?
        .ok_or_else(|| ClaimFailure::code(ERR_TASK_NOT_READY))?;
    let Some(item) = ready_item(ctx, &issue) else {
        return Err(ClaimFailure::code(ERR_TASK_NOT_READY));
    };
    // GPP-FR-ILGG: the status update comes before any local write.
    client
        .set_item_status(
            ctx.secret,
            ctx.project_id,
            &item.item_id,
            &configuration.field_id,
            &configuration.in_progress_option_id,
        )
        .map_err(|e| ClaimFailure::code(crate::tls::keep_tls(e, ERR_STATUS_UPDATE_FAILED)))?;
    // GPP-FR-DHQM: the pending claim comes before the draft. Where the disk
    // refuses it, the claim goes back to the caller to hold in memory.
    let mut pending = GithubPendingClaim {
        repository_owner: ctx.repository.owner.clone(),
        repository_name: ctx.repository.name.clone(),
        issue_number: issue.number,
        issue_url: issue.url.clone(),
        project_node_id: ctx.project_id.to_string(),
        draft_id: None,
        claimed_at: now.to_string(),
    };
    if let Err(code) = put_claim(ctx.root, &pending) {
        return Err(ClaimFailure { code, project_title: None, unsaved_claim: Some(pending) });
    }
    // GPP-FR-XPUO / GPP-FR-CWGH: the shadow draft, then its id in the claim.
    let (result, created) = shadow_for(ctx.root, &pending, &issue).map_err(ClaimFailure::code)?;
    pending.draft_id = Some(result.draft_id.clone());
    let saved = put_claim(ctx.root, &pending).is_ok();
    Ok(ClaimOutcome { result, created, claim: pending, saved })
}

/// GPP-FR-TOED / GPP-FR-KSMZ: complete the local steps of a pending claim.
///
/// It reads no polling settings: the shadow link names the Project the claim
/// recorded, so a retry works after the selection was cleared or changed. The
/// GitHub status is never changed here.
pub fn retry(
    root: &RootFs,
    secret: &str,
    repository: &RepositoryRef,
    client: &dyn GithubProjects,
    number: u64,
    held: &[GithubPendingClaim],
) -> Result<ClaimOutcome, String> {
    let on_disk = load_github_pending_claims_from(root)
        .into_iter()
        .find(|c| c.names(&repository.owner, &repository.name, number));
    let from_disk = on_disk.is_some();
    let Some(mut pending) = on_disk.or_else(|| pending_claim(root, repository, number, held)) else {
        return Err(ERR_NO_PENDING_CLAIM.to_string());
    };
    // GPP-FR-TOED: the latest title and body. A re-fetch that fails keeps the
    // pending claim (GPP-FR-KSMZ).
    let issue = client
        .fetch_issue(secret, &repository.owner, &repository.name, number)?
        .ok_or_else(|| ERR_REQUEST_FAILED.to_string())?;
    let (result, created) = shadow_for(root, &pending, &issue)?;
    let changed = pending.draft_id.as_deref() != Some(result.draft_id.as_str());
    pending.draft_id = Some(result.draft_id.clone());
    // A claim held in memory is written now; one on disk only where it moved.
    let saved = match (from_disk, changed) {
        (true, false) => true,
        _ => put_claim(root, &pending).is_ok(),
    };
    Ok(ClaimOutcome { result, created, claim: pending, saved })
}

/// GPP-FR-BSLI: remove the pending claim of one issue from disk.
pub fn acknowledge(root: &RootFs, repository: &RepositoryRef, number: u64) -> Result<(), String> {
    let mut found = false;
    update_github_pending_claims(root, |claims| {
        let before = claims.len();
        claims.retain(|c| !c.names(&repository.owner, &repository.name, number));
        found = claims.len() != before;
    })
    .map_err(|_| ERR_PENDING_CLAIM_WRITE_FAILED.to_string())?;
    match found {
        true => Ok(()),
        false => Err(ERR_NO_PENDING_CLAIM.to_string()),
    }
}
