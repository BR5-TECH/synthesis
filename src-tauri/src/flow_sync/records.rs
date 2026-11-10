//! The shapes of the contract surface of `FTS-flow-tracker-sync.md`, and its
//! typed errors.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::flow_validation::model::{SyncBase, TrackerRef};
use crate::flow_validation::ValidationReport;

pub const ERR_NO_PROJECT: &str = "no_project_open";
pub const ERR_FLOW_NOT_FOUND: &str = "flow_not_found";
pub const ERR_FLOW_INVALID: &str = "flow_invalid";
pub const ERR_SYNC_NOT_ENABLED: &str = "sync_not_enabled";
pub const ERR_SYNC_IN_PROGRESS: &str = "sync_in_progress";
pub const ERR_TRACKER_ITEM_NOT_FOUND: &str = "tracker_item_not_found";
pub const ERR_ALREADY_LINKED: &str = "already_linked";
pub const ERR_TRACKER_URL_REFUSED: &str = "tracker_url_refused";
pub const ERR_INVALID_SETTINGS: &str = "invalid_flow_tracker_settings";
pub const ERR_GITHUB_UNREACHABLE: &str = "github_unreachable";
pub const ERR_WRITE_FAILED: &str = "write_failed";

/// FTS-FR-WANV, FTS-FR-CJVS, FTS-FR-XITX: why an issue was not synced.
pub const SKIP_NO_KIND_FITS: &str = "no_kind_fits";
pub const SKIP_TYPE_UNAVAILABLE: &str = "issue_type_unavailable";
pub const SKIP_FOREIGN_REPOSITORY: &str = "foreign_repository";

/// FTS-FR-LVTU: what a sync or a resolve did.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    /// Tracker items created.
    pub created: usize,
    /// Tracker items updated.
    pub updated: usize,
    /// Flow items added by a pull or a restore.
    pub added: usize,
    /// Flow items changed by a pull.
    pub changed: usize,
    pub conflicts: Vec<SyncConflict>,
    pub skipped: Vec<SkippedIssue>,
    /// FTS-FR-ARHM: the conflict ids of a resolve that no longer matched.
    pub stale: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    Field,
    InvalidOnTracker,
    RemovedInFlow,
    RemovedOnTracker,
}

impl ConflictKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Field => "field",
            Self::InvalidOnTracker => "invalid_on_tracker",
            Self::RemovedInFlow => "removed_in_flow",
            Self::RemovedOnTracker => "removed_on_tracker",
        }
    }

    /// FTS-FR-PBQC, FTS-FR-SZGF, FTS-FR-BMWI, FTS-FR-VXMN: the choices each
    /// kind offers.
    pub fn choices(self) -> Vec<Choice> {
        match self {
            Self::Field => vec![Choice::KeepFlow, Choice::TakeTracker],
            Self::InvalidOnTracker => vec![Choice::KeepFlow, Choice::Unlink],
            Self::RemovedInFlow => vec![Choice::Unlink, Choice::Restore],
            Self::RemovedOnTracker => vec![Choice::RemoveItem, Choice::Unlink],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    KeepFlow,
    TakeTracker,
    Unlink,
    Restore,
    RemoveItem,
}

/// FTS-FR-VEEG: one decision the sync leaves to the author.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflict {
    pub id: String,
    pub kind: ConflictKind,
    pub item_id: String,
    /// The Flow's name of the item, or the tracker title where the item is
    /// gone from the Flow.
    pub item_name: String,
    /// `name`, `description`, `type`, `parent`, `milestone`, `due`, or `null`.
    pub field: Option<&'static str>,
    pub base: Value,
    pub flow: Value,
    pub tracker: Value,
    pub choices: Vec<Choice>,
    /// The tracker item's URL, where it exists.
    pub url: Option<String>,
}

/// FTS-FR-ARHM: the author's choice for one conflict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictResolution {
    pub conflict_id: String,
    pub choice: Choice,
}

/// FTS-FR-LVTU: one issue the sync did not sync.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedIssue {
    /// The issue number, or `null` for an item whose issue was not created.
    pub number: Option<u64>,
    pub title: String,
    /// The tracker item's URL, or `null` where it has none.
    pub url: Option<String>,
    pub reason: &'static str,
    /// The Flow item the skip is about, where there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
}

/// FTS-FR-TPWE: one tracker item, with its values on the terms of
/// FTS-FR-PNMA. The values have the shape of a link's base, so the caller can
/// store them as the base of a new link (`../ui/FLO-flow.md` FLO-FR-TYEZ).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TrackerItemSnapshot {
    #[serde(rename = "ref")]
    pub reference: TrackerRef,
    #[serde(flatten)]
    pub values: SyncBase,
}

/// The typed error of every command of this module. `code` is the stable
/// value a caller branches on, and `message` is the text it shows.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowSyncError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<ValidationReport>,
    /// FTS-FR-AIDI: the URL of each tracker item the call created.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub created_urls: Vec<String>,
}

impl FlowSyncError {
    /// The error of `code`, with the text that states it.
    pub fn of(code: impl Into<String>) -> Self {
        let code = code.into();
        let message = message_for(&code);
        Self { code, message, report: None, created_urls: Vec::new() }
    }

    pub fn invalid(report: ValidationReport) -> Self {
        Self { report: Some(report), ..Self::of(ERR_FLOW_INVALID) }
    }

    pub fn with_created(mut self, urls: Vec<String>) -> Self {
        self.created_urls = urls;
        self
    }
}

/// The displayable text of a code. A refusal of the publication repository
/// or the token resolution keeps its code and takes the text of its module.
pub fn message_for(code: &str) -> String {
    let text = match code {
        ERR_NO_PROJECT => "No project is open.",
        ERR_FLOW_NOT_FOUND => "This Flow could not be found in the project.",
        ERR_FLOW_INVALID => "This Flow is not a valid delivery plan.",
        ERR_SYNC_NOT_ENABLED => "This Flow is not connected to a tracker.",
        ERR_SYNC_IN_PROGRESS => "A sync of this Flow is already running.",
        ERR_TRACKER_ITEM_NOT_FOUND => {
            "The publication repository has no issue or milestone with this number."
        }
        ERR_ALREADY_LINKED => "An item of this Flow is already linked to this tracker item.",
        ERR_TRACKER_URL_REFUSED => {
            "This address is not an issue or a milestone of the publication repository."
        }
        ERR_INVALID_SETTINGS => "Each issue Type must have a name.",
        ERR_GITHUB_UNREACHABLE => "GitHub could not be reached, or it refused the request.",
        ERR_WRITE_FAILED => {
            "The tracker was updated, but the Flow could not be written. The next sync completes it."
        }
        crate::project_settings::ERR_MALFORMED_PROJECT_CONFIG => {
            "The project settings file could not be read."
        }
        crate::github_tokens::ERR_TOKEN_MISSING => "No GitHub token is stored for this host.",
        crate::github_tokens::ERR_SELECTION_REQUIRED => {
            "Select the GitHub token this project uses in the project settings."
        }
        crate::github_tokens::ERR_UNKNOWN_TOKEN => "The GitHub token of this project is gone.",
        crate::github_tokens::ERR_KEYCHAIN_UNAVAILABLE => {
            "The GitHub token could not be read from the system keychain."
        }
        crate::github_tokens::ERR_HOST_MISMATCH => {
            "The project's GitHub token belongs to another host than the publication repository."
        }
        other => return crate::github_publication::remotes::refusal_reason(other),
    };
    text.to_string()
}

/// A reference as the Flow stores it.
pub fn issue_ref(host: &str, owner: &str, repo: &str, number: u64, url: &str) -> TrackerRef {
    TrackerRef {
        kind: crate::flow_validation::model::RefKind::Issue,
        host: host.to_string(),
        owner: owner.to_string(),
        repo: repo.to_string(),
        number,
        url: url.to_string(),
    }
}
