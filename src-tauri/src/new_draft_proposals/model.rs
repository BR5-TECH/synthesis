//! The record shapes of a new-draft proposal (DCP contract surface).

use serde::{Deserialize, Serialize};

use crate::comments::{Participant, ThreadRef};
use crate::drafts::DraftRecord;

/// DCP contract surface: `NewDraftProposalState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NewDraftState {
    Pending,
    Accepted,
    Rejected,
    Withdrawn,
}

impl NewDraftState {
    pub fn as_str(self) -> &'static str {
        match self {
            NewDraftState::Pending => "pending",
            NewDraftState::Accepted => "accepted",
            NewDraftState::Rejected => "rejected",
            NewDraftState::Withdrawn => "withdrawn",
        }
    }
}

/// DCP contract surface: `NewDraftProposal`, stored as `<id>.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewDraftProposal {
    pub id: String,
    /// DCP-FR-CKWC: the decomposition this proposal belongs to.
    pub set_id: String,
    /// The original draft.
    pub draft_id: String,
    pub name: String,
    pub position: u32,
    pub total: u32,
    pub rationale: String,
    pub thread_id: String,
    pub comment_id: String,
    pub state: NewDraftState,
    /// 1 at recording; bumped by each `change` update (DCP-FR-GDTC).
    pub revision: u32,
    /// DCP-FR-AQDM: the draft an acceptance created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_draft_id: Option<String>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
    /// The agent participant that proposed it. Last: it is a table in TOML.
    pub agent: Participant,
}

/// DCP contract surface: `NewDraftDecisionOutcome`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewDraftDecisionOutcome {
    pub proposal: NewDraftProposal,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_draft: Option<DraftRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_id: Option<String>,
    /// AGC-FR-05: `draft_discussion` or `draft_comment`.
    pub origin_kind: &'static str,
}

/// What a caller hands `record_new_draft_proposal`.
pub struct NewDraftInput<'a> {
    pub draft_id: &'a str,
    pub name: &'a str,
    pub body: &'a str,
    pub position: i64,
    pub total: i64,
    pub rationale: &'a str,
    pub agent: &'a Participant,
    /// The conversation the comment is appended into.
    pub target: ThreadRef<'a>,
    pub thread_id: &'a str,
}

/// DCP-FR-GDTC: one update of the open proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewDraftUpdate {
    /// Each field that is `Some` replaces that field.
    Change {
        name: Option<String>,
        body: Option<String>,
        position: Option<i64>,
        total: Option<i64>,
    },
    Withdraw,
}
