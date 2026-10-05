//! The shapes GitHub publication is described by
//! (`../../specifications/core/GHP-github-publication.md`).

use serde::{Deserialize, Serialize};

/// GHP-FR-PVOA: the whole typed error vocabulary of this module.
///
/// A fixed vocabulary rather than a passed-through message: a remote URL or an
/// HTTP client's own error can carry an embedded credential, and GHP-FR-DHXK
/// keeps one out of everything this module returns.
pub const ERR_NO_PROJECT: &str = "no_project_open";
pub const ERR_DRAFT_NOT_FOUND: &str = "draft_not_found";
pub const ERR_DRAFT_ARCHIVED: &str = "draft_archived";
pub const ERR_DRAFT_LOCKED: &str = "draft_locked_by_graduation";
pub const ERR_LOCAL_ASSETS: &str = "local_assets_unsupported";
pub const ERR_NO_REMOTE: &str = "no_remote_configured";
pub const ERR_NO_GITHUB_REMOTE: &str = "no_github_remote";
pub const ERR_ISSUES_INACCESSIBLE: &str = "issues_inaccessible";
pub const ERR_ISSUES_DISABLED: &str = "issues_disabled";
pub const ERR_ISSUES_CREATE_FORBIDDEN: &str = "issues_create_forbidden";
pub const ERR_TOKEN_UNAVAILABLE: &str = "token_unavailable";
pub const ERR_ATTEMPT_IN_PROGRESS: &str = "attempt_in_progress";
pub const ERR_NO_ATTEMPT: &str = "no_attempt";
pub const ERR_GITHUB_UNREACHABLE: &str = "github_unreachable";
pub const ERR_ISSUE_CREATE_FAILED: &str = "issue_create_failed";
pub const ERR_ISSUE_UPDATE_FAILED: &str = "issue_update_failed";
pub const ERR_STORE_WRITE_FAILED: &str = "publication_store_write_failed";
/// GHP-FR-BKLT: a GitHub-shadow draft takes no publication.
pub const ERR_DRAFT_GITHUB_SHADOW: &str = crate::drafts::ERR_GITHUB_SHADOW;
/// GHP-FR-GHEA / GHP-FR-YSPJ: a metadata list that could not be read.
pub const ERR_PARENT_ISSUES_UNREADABLE: &str = "parent_issues_unreadable";
pub const ERR_ISSUE_TYPES_UNREADABLE: &str = "issue_types_unreadable";
pub const ERR_MILESTONES_UNREADABLE: &str = "milestones_unreadable";
/// GHP-FR-RMVQ / GHP-FR-PWJG: the saved or requested parent cannot be used.
pub const ERR_PARENT_ISSUE_UNAVAILABLE: &str = "parent_issue_unavailable";
/// GHP-FR-LCKZ: a requested root Type or milestone GitHub does not offer.
pub const ERR_ISSUE_TYPE_UNAVAILABLE: &str = "issue_type_unavailable";
pub const ERR_MILESTONE_UNAVAILABLE: &str = "milestone_unavailable";
/// GHP-FR-RMVQ / GHP-FR-AZPF: a choice the settings do not allow.
pub const ERR_INVALID_PUBLICATION_CHOICE: &str = "invalid_publication_choice";
/// GHP-FR-SWKU / GHP-FR-RCNL: the sub-issue relationship could not be applied.
pub const ERR_SUB_ISSUE_LINK_FAILED: &str = "sub_issue_link_failed";
/// GHP-FR-NQWX: settings the store refuses.
pub const ERR_INVALID_PUBLICATION_SETTINGS: &str = "invalid_publication_settings";

/// GHP-FR-YDAN: the repository the project's publication remote names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationRepository {
    pub remote_name: String,
    pub repository_owner: String,
    pub repository_name: String,
}

pub use crate::project_settings::{GithubPublicationSettings, MilestonePolicy};

/// GHP-FR-ATCH: root or sub-issue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationKind {
    Root,
    SubIssue,
}

/// GHP-FR-ATCH / GHP-FR-HSCH: what the author chose, resolved and saved before
/// the first GitHub mutation. A retry reuses it exactly (GHP-FR-IBXN).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationChoice {
    pub kind: PublicationKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_repository_owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_repository_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_issue_number: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub milestone_policy: Option<MilestonePolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub milestone_number: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub milestone_title: Option<String>,
}

impl PublicationChoice {
    /// GHP-FR-CDVT: what an attempt or record that holds no choice reads as.
    pub fn root() -> Self {
        Self {
            kind: PublicationKind::Root,
            parent_repository_owner: None,
            parent_repository_name: None,
            parent_issue_number: None,
            issue_type: None,
            milestone_policy: None,
            milestone_number: None,
            milestone_title: None,
        }
    }
}

/// GHP-FR-BWNI: the choice a caller sends.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PublicationChoiceInput {
    pub parent_issue_number: Option<u64>,
    pub issue_type: Option<String>,
    pub milestone_number: Option<u64>,
}

/// GHP-FR-DZLB: one metadata list and how its read ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataList<T> {
    pub state: MetadataState,
    pub items: Vec<T>,
    pub error_code: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataState {
    Loaded,
    Failed,
}

impl<T> MetadataList<T> {
    pub fn loaded(items: Vec<T>) -> Self {
        Self { state: MetadataState::Loaded, items, error_code: None, error: None }
    }

    pub fn failed(code: &str) -> Self {
        Self {
            state: MetadataState::Failed,
            items: Vec::new(),
            error_code: Some(code.to_string()),
            error: Some(metadata_error_text(code).to_string()),
        }
    }
}

/// GHP-FR-DZLB: displayable text for a failed metadata read. A fixed text
/// rather than a passed-through message (GHP-FR-DHXK).
pub fn metadata_error_text(code: &str) -> &'static str {
    match code {
        ERR_PARENT_ISSUES_UNREADABLE => "The open issues of this repository could not be read.",
        ERR_ISSUE_TYPES_UNREADABLE => "The issue Types of this repository could not be read.",
        ERR_MILESTONES_UNREADABLE => "The milestones of this repository could not be read.",
        _ => "GitHub could not be reached.",
    }
}

/// GHP-FR-FTMC: one issue offered as a parent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationParentIssue {
    pub number: u64,
    pub title: String,
    pub issue_type: String,
    pub url: String,
    pub milestone: Option<PublicationMilestone>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationIssueType {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationMilestone {
    pub number: u64,
    pub title: String,
}

/// GHP-FR-ETJD: the configured sub-issue Type and how it resolved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubIssueTypeResolution {
    pub name: String,
    /// The Type in GitHub's own spelling, or `None` where it is unavailable.
    pub resolved: Option<String>,
}

/// GHP-FR-MDLD: everything the publication chooser renders.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationMetadata {
    pub repository_owner: String,
    pub repository_name: String,
    pub settings: GithubPublicationSettings,
    pub parents: MetadataList<PublicationParentIssue>,
    pub issue_types: MetadataList<PublicationIssueType>,
    pub milestones: MetadataList<PublicationMilestone>,
    pub sub_issue_type: SubIssueTypeResolution,
}

/// GHP-FR-JAWD: one successful GitHub publication.
///
/// Append-only (GHP-FR-UZMX): nothing rewrites, reorders, or removes a record
/// once it is in the store, so an issue an earlier record names stays a valid
/// reference however many later publications the draft has.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationRecord {
    /// Always `github` today. Carried rather than assumed so a second provider
    /// does not have to migrate every stored record.
    pub provider: String,
    pub repository_owner: String,
    pub repository_name: String,
    pub issue_number: u64,
    pub issue_url: String,
    /// RFC 3339 UTC.
    pub published_at: String,
    /// The attempt marker the issue body carries (GHP-FR-FQIZ).
    pub marker: String,
    /// GHP-FR-HSCH: absent in a record written before the choice existed,
    /// which reads as a root issue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice: Option<PublicationChoice>,
}

/// DRS-FR-VDQR: an attempt's state. Both values are **non-terminal** — a
/// completed or abandoned attempt is cleared rather than parked in a state
/// (DRS-FR-JOEV).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptState {
    Open,
    AwaitingChoice,
}

/// GHP-FR-RUYT: what is written to disk **before** the first GitHub request, so
/// a create whose response was lost is still recoverable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationAttempt {
    pub marker: String,
    pub remote_name: String,
    /// Canonicalized (GHP-FR-BXTU), so it never carries an embedded credential.
    pub remote_url: String,
    pub repository_owner: String,
    pub repository_name: String,
    pub state: AttemptState,
    pub started_at: String,
    pub updated_at: String,
    /// GHP-FR-ATCH: absent in an attempt written before the choice existed
    /// (GHP-FR-CDVT).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice: Option<PublicationChoice>,
}

impl PublicationAttempt {
    /// GHP-FR-CDVT: the saved choice, or a root choice where none is saved.
    pub fn effective_choice(&self) -> PublicationChoice {
        self.choice.clone().unwrap_or_else(PublicationChoice::root)
    }
}

/// DRS-FR-EJBM: the draft-owned publication store, held in `publication.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PublicationStore {
    /// Append-only, oldest first.
    pub publication: Vec<PublicationRecord>,
    /// At most one, always non-terminal (DRS-FR-JOEV).
    pub attempt: Option<PublicationAttempt>,
}

/// GHP-FR-MZPR / GHP-FR-LTAC: why a configured remote can or cannot receive an
/// issue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteEligibility {
    Eligible,
    NotGithub,
    IssuesInaccessible,
    IssuesDisabled,
    IssuesCreateForbidden,
    TokenUnavailable,
}

impl RemoteEligibility {
    /// The typed error a refusal on this ground returns (GHP-FR-ZRFP).
    pub fn error_code(self) -> &'static str {
        match self {
            RemoteEligibility::Eligible => "",
            RemoteEligibility::NotGithub => ERR_NO_GITHUB_REMOTE,
            RemoteEligibility::IssuesInaccessible => ERR_ISSUES_INACCESSIBLE,
            RemoteEligibility::IssuesDisabled => ERR_ISSUES_DISABLED,
            RemoteEligibility::IssuesCreateForbidden => ERR_ISSUES_CREATE_FORBIDDEN,
            RemoteEligibility::TokenUnavailable => ERR_TOKEN_UNAVAILABLE,
        }
    }

    /// GHP-FR-MZPR: the exact reason, as text a surface renders unchanged.
    pub fn reason(self) -> Option<&'static str> {
        match self {
            RemoteEligibility::Eligible => None,
            RemoteEligibility::NotGithub => Some("Not a GitHub repository."),
            RemoteEligibility::IssuesInaccessible => {
                Some("The token cannot read this repository.")
            }
            // GHP-FR-TKBW: the repository refuses the issue, not the token, so the
            // text must send the author to the repository rather than to their token.
            RemoteEligibility::IssuesDisabled => Some(
                "This repository does not accept new issues. Check that Issues are turned \
                 on and that the repository is neither archived nor disabled.",
            ),
            RemoteEligibility::IssuesCreateForbidden => {
                Some("The token cannot create issues in this repository.")
            }
            RemoteEligibility::TokenUnavailable => {
                Some("No GitHub token is available for this project.")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteKind {
    Github,
    Other,
}

/// One configured Git remote, classified (GHP-FR-WKDE).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationRemote {
    pub name: String,
    pub url: String,
    pub kind: RemoteKind,
    pub repository_owner: Option<String>,
    pub repository_name: Option<String>,
    pub eligibility: RemoteEligibility,
    pub reason: Option<String>,
}

/// GHP-FR-NDSB: how the reported selection was reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionOrigin {
    Automatic,
    Persisted,
    AttemptOnly,
    None,
}

/// GHP-FR-WKDE: the whole enumeration, plus which remote the next attempt would
/// use and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationRemoteResolution {
    pub remotes: Vec<PublicationRemote>,
    pub selection: Option<String>,
    pub origin: SelectionOrigin,
    pub persisted_choice: Option<PersistedChoice>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedChoice {
    pub name: String,
    pub url: String,
}

/// GHP-FR-CWTG: whether the action is offered, and the exact reason where it is
/// not.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationEligibility {
    pub publishable: bool,
    pub reason_code: Option<String>,
    pub reason: Option<String>,
    /// GHP-FR-AKUM: the offending image references; empty otherwise.
    pub local_assets: Vec<String>,
}

impl PublicationEligibility {
    pub fn publishable() -> Self {
        Self { publishable: true, reason_code: None, reason: None, local_assets: Vec::new() }
    }

    pub fn refused(code: &str, reason: impl Into<String>) -> Self {
        Self {
            publishable: false,
            reason_code: Some(code.to_string()),
            reason: Some(reason.into()),
            local_assets: Vec::new(),
        }
    }
}

/// GHP-FR-CWTG: everything a surface renders about one draft's publication.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftPublicationView {
    /// The newest record, or `None`.
    pub current: Option<PublicationRecord>,
    /// Every record, **newest first** — the order both surfaces render.
    pub history: Vec<PublicationRecord>,
    pub attempt: Option<PublicationAttempt>,
    pub eligibility: PublicationEligibility,
}

/// GHP-FR-JAWD / GHP-FR-HRUN: what a publication attempt answers with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PublicationOutcome {
    Published {
        record: PublicationRecord,
    },
    RecoveryRequired {
        issue_number: u64,
        issue_url: String,
        marker: String,
        /// GHP-FR-HRUN: which of `title`, `body`, `parent`, `type`, and
        /// `milestone` differ.
        mismatches: Vec<String>,
    },
}

/// GHP-FR-YPGL: the two answers to a recovery choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryChoice {
    UpdateExisting,
    PublishNew,
}
