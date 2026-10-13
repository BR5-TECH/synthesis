//! What a poll keeps, decided without a network (GPP-FR-DEZO, GPP-FR-VXSB,
//! GPP-FR-EHRC, GPP-FR-YNOT, GPP-FR-BOQX, GPP-FR-XSKT, GPP-FR-QCAM).
//!
//! Every function here is pure over what the client returned and what the
//! disk holds, so the rules a poll applies are testable against a fake client
//! and a temporary worktree.

use super::client::{GithubProjects, ProjectItem, ProjectShape, StatusOption};
use super::records::*;
use crate::drafts::GithubIssueLink;

/// GPP-FR-VXSB: the two status names of the polling settings. `None` is an
/// unset name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatusNames {
    pub ready: Option<String>,
    pub in_progress: Option<String>,
}

impl StatusNames {
    pub fn of(settings: &GithubPollingSettings) -> Self {
        Self {
            ready: settings.ready_status.clone(),
            in_progress: settings.in_progress_status.clone(),
        }
    }
}

/// GPP-FR-DEZO: a configuration that passed validation, with the ids a claim
/// needs to move an item to the In Progress status, and the option names the
/// two statuses resolved to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidConfiguration {
    pub title: String,
    pub field_id: String,
    pub ready_option_id: String,
    pub ready_name: String,
    pub in_progress_option_id: String,
    pub in_progress_name: String,
}

/// Why a poll or a claim failed, with the Project title where the failure
/// came after the Project resolved, and the missing status name of a
/// missing-option error (GPP-FR-YNOT).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PollFailure {
    pub code: String,
    pub project_title: Option<String>,
    pub missing_status: Option<String>,
}

impl PollFailure {
    pub fn code(code: impl Into<String>) -> Self {
        Self { code: code.into(), project_title: None, missing_status: None }
    }
}

/// GPP-FR-VXSB: the option a status name resolves to. A stored name matches
/// exactly. An unset name matches the first option whose name equals the
/// default case-insensitively, other than `other` — the option the other
/// status holds — so the two statuses never resolve to one option
/// (GPP-FR-DBPI).
pub fn resolve_status<'a>(
    options: &'a [StatusOption],
    stored: Option<&str>,
    default: &str,
    other: Option<&str>,
) -> Option<&'a StatusOption> {
    match stored {
        Some(name) => options.iter().find(|o| o.name == name),
        None => options.iter().find(|o| {
            o.name.to_lowercase() == default.to_lowercase() && Some(o.name.as_str()) != other
        }),
    }
}

/// GPP-FR-DEZO / GPP-FR-EHRC / GPP-FR-YNOT: check the Project's `Status`
/// field against the two status names.
pub fn validate_shape(
    shape: &ProjectShape,
    statuses: &StatusNames,
) -> Result<ValidConfiguration, PollFailure> {
    let fail = |code: &str, missing: Option<String>| PollFailure {
        code: code.to_string(),
        project_title: Some(shape.title.clone()),
        missing_status: missing,
    };
    let Some(field) = shape.status_field.as_ref() else {
        return Err(fail(ERR_STATUS_FIELD_MISSING, None));
    };
    let resolve = |stored: &Option<String>, default: &str, other: Option<&str>, code: &str| {
        resolve_status(&field.options, stored.as_deref(), default, other)
            .ok_or_else(|| fail(code, Some(stored.clone().unwrap_or_else(|| default.to_string()))))
    };
    let ready =
        resolve(&statuses.ready, READY, statuses.in_progress.as_deref(), ERR_READY_OPTION_MISSING)?;
    let in_progress = resolve(
        &statuses.in_progress,
        IN_PROGRESS,
        Some(ready.name.as_str()),
        ERR_IN_PROGRESS_OPTION_MISSING,
    )?;
    Ok(ValidConfiguration {
        title: shape.title.clone(),
        field_id: field.field_id.clone(),
        ready_option_id: ready.id.clone(),
        ready_name: ready.name.clone(),
        in_progress_option_id: in_progress.id.clone(),
        in_progress_name: in_progress.name.clone(),
    })
}

/// GPP-FR-DEZO: read the Project and validate it.
pub fn read_configuration(
    client: &dyn GithubProjects,
    secret: &str,
    project_id: &str,
    statuses: &StatusNames,
) -> Result<ValidConfiguration, PollFailure> {
    let shape = client.project_shape(secret, project_id).map_err(PollFailure::code)?;
    validate_shape(&shape, statuses)
}

/// GPP-FR-NNSJ: the options of the Project's `Status` field, in the order of
/// the field.
pub fn status_options(
    client: &dyn GithubProjects,
    secret: &str,
    project_id: &str,
) -> Result<Vec<GithubProjectStatusOption>, String> {
    let shape = client.project_shape(secret, project_id)?;
    let field = shape.status_field.ok_or_else(|| ERR_STATUS_FIELD_MISSING.to_string())?;
    Ok(field
        .options
        .into_iter()
        .map(|o| GithubProjectStatusOption { id: o.id, name: o.name })
        .collect())
}

/// GPP-FR-BOQX: the eligibility rule. Open, of issue type `Task`, in the
/// polling repository, and with the `Status` value that the Ready status
/// resolved to. No label is read.
pub fn is_eligible(
    state: &str,
    issue_type: Option<&str>,
    owner: &str,
    name: &str,
    repository: &RepositoryRef,
    status: Option<&str>,
    ready_name: &str,
) -> bool {
    state.eq_ignore_ascii_case("OPEN")
        && issue_type == Some(TASK_TYPE)
        && owner.eq_ignore_ascii_case(&repository.owner)
        && name.eq_ignore_ascii_case(&repository.name)
        && status == Some(ready_name)
}

/// GPP-FR-XSKT: the eligible issues among the Project's items.
pub fn eligible_tasks(
    items: &[ProjectItem],
    repository: &RepositoryRef,
    ready_name: &str,
) -> Vec<GithubReadyTask> {
    items
        .iter()
        .filter_map(|item| {
            let issue = item.issue.as_ref()?;
            let eligible = is_eligible(
                &issue.state,
                issue.issue_type.as_deref(),
                &issue.repository_owner,
                &issue.repository_name,
                repository,
                item.status.as_deref(),
                ready_name,
            );
            eligible.then(|| GithubReadyTask {
                repository_host: repository.host.clone(),
                repository_owner: issue.repository_owner.clone(),
                repository_name: issue.repository_name.clone(),
                issue_number: issue.number,
                title: issue.title.clone(),
                url: issue.url.clone(),
                status: item.status.clone().unwrap_or_default(),
            })
        })
        .collect()
}

/// GPP-FR-QCAM: drop every issue a GitHub-shadow draft or a pending claim
/// already names.
pub fn exclude_claimed(
    tasks: Vec<GithubReadyTask>,
    shadows: &[GithubIssueLink],
    pending: &[GithubPendingClaim],
) -> Vec<GithubReadyTask> {
    tasks
        .into_iter()
        .filter(|task| {
            let (host, owner, name, number) = (
                &task.repository_host,
                &task.repository_owner,
                &task.repository_name,
                task.issue_number,
            );
            !shadows.iter().any(|link| link.names(host, owner, name, number))
                && !pending.iter().any(|claim| claim.names(host, owner, name, number))
        })
        .collect()
}

/// What one successful poll found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PollSuccess {
    pub repository: RepositoryRef,
    pub project_title: String,
    /// Eligible, and not named by a shadow draft or a pending claim.
    pub tasks: Vec<GithubReadyTask>,
    /// The item page cap stopped the read before the last page.
    pub truncated: bool,
}

/// GPP-FR-XSKT / GPP-FR-QCAM: one poll against GitHub and the worktree.
pub fn poll_once(
    root: &crate::fs::RootFs,
    client: &dyn GithubProjects,
    secret: &str,
    repository: &RepositoryRef,
    project_id: &str,
    statuses: &StatusNames,
) -> Result<PollSuccess, PollFailure> {
    let configuration = read_configuration(client, secret, project_id, statuses)?;
    let read = client.project_items(secret, project_id).map_err(|code| PollFailure {
        project_title: Some(configuration.title.clone()),
        ..PollFailure::code(code)
    })?;
    let shadows: Vec<GithubIssueLink> = crate::drafts::github_shadow_drafts(root)
        .into_iter()
        .filter_map(|draft| draft.github_issue)
        .collect();
    let pending = crate::project_settings::load_github_pending_claims_from(root);
    Ok(PollSuccess {
        repository: repository.clone(),
        project_title: configuration.title,
        tasks: exclude_claimed(
            eligible_tasks(&read.items, repository, &configuration.ready_name),
            &shadows,
            &pending,
        ),
        truncated: read.truncated,
    })
}

/// GPP-FR-WOLE: the URL of one listed issue, or `issue_not_listed`.
///
/// The issue must be held by the snapshot rows or by a pending claim, and its
/// URL must be an HTTPS issue address of the polling repository on its host.
pub fn listed_issue_url(
    number: u64,
    repository: &RepositoryRef,
    tasks: &[GithubReadyTask],
    pending: &[GithubPendingClaim],
) -> Result<String, String> {
    let from_tasks = tasks
        .iter()
        .find(|t| {
            t.issue_number == number
                && t.repository_host.eq_ignore_ascii_case(&repository.host)
                && t.repository_owner.eq_ignore_ascii_case(&repository.owner)
                && t.repository_name.eq_ignore_ascii_case(&repository.name)
        })
        .map(|t| t.url.clone());
    let from_claims = || {
        pending
            .iter()
            .find(|c| c.names(&repository.host, &repository.owner, &repository.name, number))
            .map(|c| c.issue_url.clone())
    };
    let url = from_tasks.or_else(from_claims).ok_or(ERR_ISSUE_NOT_LISTED)?;
    match is_repository_issue_url(&url, repository) {
        true => Ok(url),
        false => Err(ERR_ISSUE_NOT_LISTED.to_string()),
    }
}

/// GPP-FR-WOLE: an HTTPS issue address of the polling repository on its host.
pub fn is_repository_issue_url(url: &str, repository: &RepositoryRef) -> bool {
    if !crate::github_publication::flow::is_openable_issue_url(url, &repository.host) {
        return false;
    }
    let Some(path) = url.strip_prefix("https://").and_then(|rest| rest.split_once('/')) else {
        return false;
    };
    let mut segments = path.1.split('/');
    let (Some(owner), Some(name), Some("issues"), Some(number), None) =
        (segments.next(), segments.next(), segments.next(), segments.next(), segments.next())
    else {
        return false;
    };
    owner.eq_ignore_ascii_case(&repository.owner)
        && name.eq_ignore_ascii_case(&repository.name)
        && !number.is_empty()
        && number.bytes().all(|b| b.is_ascii_digit())
}
