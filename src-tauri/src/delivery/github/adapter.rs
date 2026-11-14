//! GDP-FR-UPXR: the tracker adapter, the one boundary of all tracker access.
//!
//! An adapter is bound to one repository and to the credential it sends, so
//! the plan logic above it never holds a secret. Every value here is what the
//! tracker said, in the terms of this module. The read cache (GDP-FR-YOSF)
//! stores these values as they are, so they serialize. Every error is a fixed
//! code that carries no request text.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// The repository an adapter is bound to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackerRepository {
    pub host: String,
    pub owner: String,
    pub name: String,
}

impl TrackerRepository {
    /// Whether `owner/name` is this repository. GitHub names compare without
    /// case.
    pub fn is(&self, owner: &str, name: &str) -> bool {
        self.owner.eq_ignore_ascii_case(owner) && self.name.eq_ignore_ascii_case(name)
    }
}

/// An issue that a relation names: a parent or a blocker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueLink {
    pub owner: String,
    pub repo: String,
    pub number: u64,
    pub url: String,
    #[serde(default)]
    pub title: String,
    /// `open` or `closed`.
    #[serde(default)]
    pub state: String,
}

/// GitHub's reason for a closed issue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateReason {
    Completed,
    NotPlanned,
    Duplicate,
    Reopened,
}

/// One issue with the fields a plan reads (GDP-FR-NFWA, GDP-FR-QGFO).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerIssue {
    pub number: u64,
    pub url: String,
    /// The GitHub node ID, which a Project write names.
    #[serde(default)]
    pub node_id: String,
    /// The repository the issue is in.
    pub owner: String,
    pub repo: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// `open` or `closed`.
    pub state: String,
    #[serde(default)]
    pub state_reason: Option<StateReason>,
    #[serde(default)]
    pub issue_type: Option<String>,
    #[serde(default)]
    pub milestone: Option<u64>,
    #[serde(default)]
    pub parent: Option<IssueLink>,
    #[serde(default)]
    pub blocked_by: Vec<IssueLink>,
    pub updated_at: String,
}

/// One milestone of the repository (GDP-FR-MYLM).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerMilestone {
    pub number: u64,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// `open` or `closed`.
    pub state: String,
    /// A calendar date `YYYY-MM-DD`.
    #[serde(default)]
    pub due: Option<String>,
    pub updated_at: String,
}

/// What one Project item holds.
// The enum is the serde form of a cache record, held once per Project item
// in one list, so a box would save no memory that matters.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ItemContent {
    Issue(TrackerIssue),
    PullRequest,
    Draft,
}

/// One item of the Project, with its `Status` value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectItemRead {
    pub item_id: String,
    #[serde(default)]
    pub status: Option<String>,
    pub content: ItemContent,
}

/// One option of the Project's `Status` field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusChoice {
    pub id: String,
    pub name: String,
}

/// The Project's single-select `Status` field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusFieldRead {
    pub field_id: String,
    pub options: Vec<StatusChoice>,
}

impl StatusFieldRead {
    /// The field in the terms of GitHub polling, so its status resolution
    /// (GPP-FR-VXSB) applies.
    pub fn as_polling(&self) -> crate::github_polling::client::StatusField {
        crate::github_polling::client::StatusField {
            field_id: self.field_id.clone(),
            options: self
                .options
                .iter()
                .map(|o| crate::github_polling::client::StatusOption {
                    id: o.id.clone(),
                    name: o.name.clone(),
                })
                .collect(),
        }
    }
}

/// The selected Project as one read found it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRead {
    pub node_id: String,
    pub title: String,
    #[serde(default)]
    pub short_description: String,
    #[serde(default)]
    pub owner_login: String,
    #[serde(default)]
    pub number: u64,
    /// DSR-FR-EDHP, GDP-FR-PIAQ: the HTTPS address of the Project on GitHub.
    /// A cache of an earlier version has none and reads as empty.
    #[serde(default)]
    pub url: String,
    /// The options of the `Status` field, in the order of the field, or `None`
    /// where the Project has no single-select field named `Status`.
    #[serde(default)]
    pub status_field: Option<StatusFieldRead>,
    /// Every item, in Project order.
    pub items: Vec<ProjectItemRead>,
}

/// The values of a new issue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewIssue {
    pub title: String,
    pub body: String,
    pub issue_type: Option<String>,
    pub milestone: Option<u64>,
}

/// The fields an issue update writes. A `None` field is not written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IssueChange {
    pub title: Option<String>,
    pub body: Option<String>,
    pub issue_type: Option<String>,
}

/// The values of a new milestone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewMilestone {
    pub title: String,
    pub description: String,
    pub due: Option<String>,
}

/// The fields a milestone update writes. A `None` field is not written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MilestoneChange {
    pub title: Option<String>,
    pub description: Option<String>,
    pub due: Option<Option<String>>,
}

/// The reason an issue is closed with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseWith {
    NotPlanned,
}

/// GDP-FR-UPXR: every tracker operation of the plan. `github` is the one
/// adapter; a test uses a fake that makes no network request.
pub trait TrackerAdapter: Send + Sync {
    /// The bound repository.
    fn repository(&self) -> &TrackerRepository;

    /// The Project with its `Status` field and every item, page by page.
    fn read_project(&self, project_id: &str) -> Result<ProjectRead, String>;

    /// Every milestone of the repository, open and closed.
    fn read_milestones(&self) -> Result<Vec<TrackerMilestone>, String>;

    /// One issue of the repository, or `None` where none exists or the number
    /// is a pull request.
    fn read_issue(&self, number: u64) -> Result<Option<TrackerIssue>, String>;

    /// One milestone of the repository, or `None` where none exists.
    fn read_milestone(&self, number: u64) -> Result<Option<TrackerMilestone>, String>;

    /// The issue Types the repository offers. Empty where it offers none.
    fn list_issue_types(&self) -> Result<Vec<String>, String>;

    fn create_issue(&self, issue: &NewIssue) -> Result<TrackerIssue, String>;

    fn update_issue(&self, number: u64, change: &IssueChange) -> Result<(), String>;

    /// Make `parent` the parent of `number`, or remove the parent `current`
    /// where `parent` is `None`.
    fn set_parent(
        &self,
        number: u64,
        current: Option<&IssueLink>,
        parent: Option<u64>,
    ) -> Result<(), String>;

    fn set_milestone(&self, number: u64, milestone: Option<u64>) -> Result<(), String>;

    fn add_blocked_by(&self, number: u64, blocker: u64) -> Result<(), String>;

    fn remove_blocked_by(&self, number: u64, blocker: u64) -> Result<(), String>;

    /// Add the issue with the node ID `issue_node_id` to the Project, and
    /// return the ID of the new Project item.
    fn add_project_item(&self, project_id: &str, issue_node_id: &str) -> Result<String, String>;

    fn remove_project_item(&self, project_id: &str, item_id: &str) -> Result<(), String>;

    fn create_milestone(&self, milestone: &NewMilestone) -> Result<TrackerMilestone, String>;

    fn update_milestone(&self, number: u64, change: &MilestoneChange) -> Result<(), String>;

    fn close_milestone(&self, number: u64) -> Result<(), String>;

    fn close_issue(&self, number: u64, reason: CloseWith) -> Result<(), String>;
}

/// Makes the adapter of one repository with one secret.
pub trait TrackerConnector: Send + Sync {
    fn connect(&self, repository: &TrackerRepository, secret: &str) -> Arc<dyn TrackerAdapter>;
}

/// The managed seam, so a test drives the plan against a fake adapter.
pub struct TrackerSeam(pub Arc<dyn TrackerConnector>);

impl Default for TrackerSeam {
    fn default() -> Self {
        TrackerSeam(Arc::new(super::http::HttpConnector))
    }
}
