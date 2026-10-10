//! The shapes GitHub polling is described by, and its typed errors
//! (`../../specifications/core/GPP-github-polling.md` contract surface).

use serde::Serialize;

pub use crate::project_settings::{GithubPendingClaim, GithubPollingSettings};

/// GPP-FR-SUFH: the typed error vocabulary of this module. The GHP remote
/// refusals (GHP-FR-ZRFP) pass through unchanged.
pub const ERR_NO_PROJECT: &str = "no_project_open";
pub const ERR_UNCONFIGURED: &str = "polling_unconfigured";
pub const ERR_CONFIGURATION_INVALID: &str = "polling_configuration_invalid";
pub const ERR_INVALID_INTERVAL: &str = "invalid_interval";
pub const ERR_PROJECT_UNAVAILABLE: &str = "project_unavailable";
pub const ERR_STATUS_FIELD_MISSING: &str = "status_field_missing";
pub const ERR_READY_OPTION_MISSING: &str = "ready_option_missing";
pub const ERR_IN_PROGRESS_OPTION_MISSING: &str = "in_progress_option_missing";
pub const ERR_GITHUB_UNREACHABLE: &str = "github_unreachable";
pub const ERR_REQUEST_FAILED: &str = "github_request_failed";
pub const ERR_TASK_NOT_READY: &str = "task_not_ready";
pub const ERR_CLAIM_PENDING: &str = "claim_pending";
pub const ERR_CLAIM_IN_PROGRESS: &str = "claim_in_progress";
pub const ERR_NO_PENDING_CLAIM: &str = "no_pending_claim";
pub const ERR_STATUS_UPDATE_FAILED: &str = "status_update_failed";
pub const ERR_PENDING_CLAIM_WRITE_FAILED: &str = "pending_claim_write_failed";
pub const ERR_SHADOW_CREATE_FAILED: &str = "shadow_draft_create_failed";
pub const ERR_ISSUE_NOT_LISTED: &str = "issue_not_listed";

/// GPP-FR-DEZO: the exact names the selected Project must hold.
pub const STATUS_FIELD: &str = "Status";
pub const READY: &str = "Ready";
pub const IN_PROGRESS: &str = "In Progress";
/// GPP-FR-BOQX: the GitHub issue type an eligible issue carries.
pub const TASK_TYPE: &str = "Task";

/// GPP-FR-TYOV: the event every surface follows. Kebab-case, because Tauri
/// rejects a space in an event name at `emit`.
pub const GITHUB_POLLING_CHANGED: &str = "github-polling-changed";

/// GPP-FR-EHRC: whether a code is one of the four configuration errors.
pub fn is_configuration_error(code: &str) -> bool {
    matches!(
        code,
        ERR_PROJECT_UNAVAILABLE
            | ERR_STATUS_FIELD_MISSING
            | ERR_READY_OPTION_MISSING
            | ERR_IN_PROGRESS_OPTION_MISSING
    )
}

/// GPP-FR-EHRC / GPP-FR-PUXT: the displayable text of a typed error.
pub fn error_text(code: &str) -> String {
    let text = match code {
        ERR_PROJECT_UNAVAILABLE => {
            "The selected GitHub Project cannot be found or the token cannot read it. Select \
             another Project, or give the token access to it."
        }
        ERR_STATUS_FIELD_MISSING => {
            "The selected GitHub Project has no single-select field named \"Status\". Add it in \
             the Project settings."
        }
        ERR_READY_OPTION_MISSING => {
            "The \"Status\" field of the selected GitHub Project has no option named \"Ready\". \
             Add it in the Project settings."
        }
        ERR_IN_PROGRESS_OPTION_MISSING => {
            "The \"Status\" field of the selected GitHub Project has no option named \"In \
             Progress\". Add it in the Project settings."
        }
        ERR_UNCONFIGURED => "No GitHub Project is selected for polling.",
        ERR_CONFIGURATION_INVALID => {
            "The GitHub Project configuration is not valid. Correct it in Project settings."
        }
        ERR_INVALID_INTERVAL => "The polling interval must be 1, 5, 15, 30, or 60 minutes.",
        ERR_GITHUB_UNREACHABLE => "GitHub cannot be reached.",
        ERR_REQUEST_FAILED => "GitHub refused the request.",
        ERR_TASK_NOT_READY => "This task is no longer ready on GitHub.",
        ERR_CLAIM_PENDING => "This task is already claimed on this machine.",
        ERR_CLAIM_IN_PROGRESS => "A claim of this task is already running.",
        ERR_NO_PENDING_CLAIM => "This task has no pending claim.",
        ERR_STATUS_UPDATE_FAILED => "The task status on GitHub could not be set to In Progress.",
        ERR_PENDING_CLAIM_WRITE_FAILED => "The claim could not be recorded on this machine.",
        ERR_SHADOW_CREATE_FAILED => "The draft for this task could not be created.",
        ERR_ISSUE_NOT_LISTED => "This issue is not a listed task of the polling repository.",
        ERR_NO_PROJECT => "No project is open.",
        other => {
            return crate::github_publication::remotes::refusal_reason(other);
        }
    };
    text.to_string()
}

/// GPP-FR-WYRP: one GitHub Project the token can read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubProjectOption {
    pub node_id: String,
    pub title: String,
    pub owner_login: String,
    pub number: u64,
}

/// The four configuration states (GPP contract surface).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigurationState {
    Unset,
    Unchecked,
    Valid,
    Invalid,
}

/// GPP-FR-DEZO / GPP-FR-EHRC: the configuration as the view reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubPollingConfiguration {
    pub state: ConfigurationState,
    pub error_code: Option<String>,
    pub error: Option<String>,
    pub project_title: Option<String>,
}

impl GithubPollingConfiguration {
    pub fn unset() -> Self {
        Self { state: ConfigurationState::Unset, error_code: None, error: None, project_title: None }
    }

    pub fn unchecked() -> Self {
        Self { state: ConfigurationState::Unchecked, ..Self::unset() }
    }

    pub fn valid(title: &str) -> Self {
        Self {
            state: ConfigurationState::Valid,
            error_code: None,
            error: None,
            project_title: Some(title.to_string()),
        }
    }

    pub fn invalid(code: &str, title: Option<String>) -> Self {
        Self {
            state: ConfigurationState::Invalid,
            error_code: Some(code.to_string()),
            error: Some(error_text(code)),
            project_title: title,
        }
    }
}

/// GPP-FR-XSKT: one eligible issue of the last successful poll.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubReadyTask {
    pub repository_host: String,
    pub repository_owner: String,
    pub repository_name: String,
    pub issue_number: u64,
    pub title: String,
    pub url: String,
    pub status: String,
}

/// GPP-FR-UBDE: one GitHub-shadow draft of the active worktree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubShadowDraftRow {
    pub draft_id: String,
    pub name: String,
    pub status: crate::drafts::DraftStatus,
    pub repository_host: String,
    pub repository_owner: String,
    pub repository_name: String,
    pub issue_number: u64,
    pub issue_url: String,
    pub project_node_id: String,
    pub locked: bool,
}

/// GPP-FR-RGNM: the polling repository.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RepositoryRef {
    pub host: String,
    pub owner: String,
    pub name: String,
}

/// The whole polling state a surface renders (GPP contract surface).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubPollingView {
    pub settings: GithubPollingSettings,
    pub configuration: GithubPollingConfiguration,
    pub repository: Option<RepositoryRef>,
    pub polling: bool,
    pub tasks: Vec<GithubReadyTask>,
    pub stale: bool,
    pub last_error_code: Option<String>,
    pub last_error: Option<String>,
    pub last_success_at: Option<String>,
    pub pending_claims: Vec<GithubPendingClaim>,
    pub shadows: Vec<GithubShadowDraftRow>,
}

/// GPP-FR-CWGH: what a claim and a retry return.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubClaimResult {
    pub draft_id: String,
    pub draft_name: String,
}

/// GPP-FR-TYOV: one new issue in the event payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewIssue {
    pub issue_number: u64,
    pub title: String,
}

/// GPP-FR-TYOV: the event payload.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubPollingChanged {
    pub new_issues: Vec<NewIssue>,
}

/// The identity of one issue: repository host, owner and name (lowercase), and
/// the issue number (GPP-FR-QCAM, GPP-FR-YROY).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IssueKey {
    pub host: String,
    pub owner: String,
    pub name: String,
    pub number: u64,
}

impl IssueKey {
    pub fn new(host: &str, owner: &str, name: &str, number: u64) -> Self {
        Self {
            host: host.to_ascii_lowercase(),
            owner: owner.to_ascii_lowercase(),
            name: name.to_ascii_lowercase(),
            number,
        }
    }

    pub fn of_task(task: &GithubReadyTask) -> Self {
        Self::new(
            &task.repository_host,
            &task.repository_owner,
            &task.repository_name,
            task.issue_number,
        )
    }
}
