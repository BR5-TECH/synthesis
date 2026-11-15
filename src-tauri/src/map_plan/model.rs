//! The shapes of a plan proposal (`MPR-map-plan-proposals.md`, Contract surface).
//! Field names on the wire are camelCase.

use serde::{Deserialize, Serialize};

use crate::comments::Participant;

/// MPR-FR-QUYY: the four states of a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowState {
    Waiting,
    Accepted,
    Rejected,
    Failed,
}

/// MPR-FR-YLKL: why a row cannot be accepted yet, or for good.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowBlock {
    pub row_key: String,
    /// `waiting`, `rejected`, or `failed`.
    pub reason: RowState,
}

/// MPR contract: the typed error of a row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowError {
    pub code: String,
    pub message: String,
}

/// MPR contract: what an accepted row produced.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_number: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_changes: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MilestoneRef {
    pub number: u64,
    pub title: String,
}

/// MPR-FR-YLKL: the parent of a new issue: a plan issue, or an earlier row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParentRef {
    Issue(u64),
    Row(String),
}

/// MPR-FR-YCHW: the fields of a new issue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateIssueFields {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub issue_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub milestone: Option<MilestoneRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<ParentRef>,
    #[serde(default)]
    pub labels: Vec<String>,
}

/// MPR-FR-GKOL: one changed field of a plan issue, as it was and as it would be.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowChange {
    /// `title`, `body`, `type`, `milestone`, `parent`, `labels`, `close`, `status`.
    pub field: String,
    pub before: serde_json::Value,
    pub after: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed: Option<Vec<String>>,
    /// A close only: `completed` or `not_planned`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// MPR-FR-FBSH, MPR-FR-RTGH: the fields of a change to a plan issue.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifyIssueFields {
    pub number: u64,
    pub title: String,
    pub base_updated_at: String,
    pub changes: Vec<RowChange>,
}

/// MPR-FR-XPXP: the fields of a new draft.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewDraftFields {
    pub name: String,
    pub body: String,
}

/// The fields of a row, by kind. On the wire the kind is the `kind` of the row
/// and the fields are its `fields`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "fields", rename_all = "snake_case")]
pub enum RowFields {
    CreateIssue(CreateIssueFields),
    ModifyIssue(ModifyIssueFields),
    NewDraft(NewDraftFields),
}

/// MPR-FR-TQVZ: one row of a proposal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapPlanRow {
    pub key: String,
    #[serde(flatten)]
    pub fields: RowFields,
    pub state: RowState,
    /// Computed at each read. Never stored.
    #[serde(default)]
    pub blocked_by: Option<RowBlock>,
    #[serde(default)]
    pub error: Option<RowError>,
    #[serde(default)]
    pub result: Option<RowResult>,
}

/// MPR-FR-TQVZ: one batch of rows that belongs to one comment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapPlanProposal {
    pub id: String,
    pub discussion_id: String,
    pub comment_id: String,
    pub created_at: String,
    pub rows: Vec<MapPlanRow>,
}

/// MPR contract: a typed error of a plan proposal operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapPlanError {
    pub code: String,
    pub message: String,
}

/// PPT-FR-UZEK, PPT-FR-CXFG: the one comment that the recording appends beside
/// the proposal, in the same step (MPR-FR-RVRN).
#[derive(Clone, Debug)]
pub struct ProposalComment {
    pub body: String,
    pub author: Participant,
    pub spoken_original: Option<String>,
}
