//! The typed errors and the records of a prompt change proposal.

use super::*;

// ---------------------------------------------------------------------------
// Typed errors (PCP contract surface)
// ---------------------------------------------------------------------------

/// PCP-FR-26: no project is open, so nothing here can be resolved.
pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";
/// PCP-FR-07 / PCP-FR-19: the id names no file the worktree holds.
pub const ERR_ARTIFACT_NOT_FOUND: &str = "artifact_not_found";
/// PCP-FR-07 / PCP-FR-19: the file's resolved type is not exactly `prompt`.
pub const ERR_NOT_A_PROMPT_ARTIFACT: &str = "not_a_prompt_artifact";
/// PCP-FR-26: the id names no proposal.
pub const ERR_PROPOSAL_NOT_FOUND: &str = "proposal_not_found";
/// PCP-FR-18: decided once, and a second decision finds it settled.
pub const ERR_ALREADY_DECIDED: &str = "already_decided";
/// PCP-FR-24: the stored candidate has moved on since the caller read it.
pub const ERR_CANDIDATE_STALE: &str = "candidate_stale";
/// PCP-FR-14: a failure at or before the commit point. The artifact was never
/// written and the proposal is still `pending`.
pub const ERR_WRITE_FAILED: &str = "write_failed";
/// PCP-FR-14: another acceptance of this proposal is in flight.
pub const ERR_ACCEPTANCE_IN_PROGRESS: &str = "acceptance_in_progress";
/// PCP-FR-14: the acceptance **landed** and its decision comment is still owed.
///
/// Not a failure of the acceptance and never rendered as one (PCR-FR-15): the
/// artifact holds the candidate and the record reads `accepted`. What is
/// outstanding is the line the conversation is owed.
pub const ERR_ACCEPTANCE_INCOMPLETE: &str = "acceptance_incomplete";

/// AGC-FR-05's spellings, which a dispatched turn's origin is named by.
pub const ORIGIN_ARTIFACT_DISCUSSION: &str = "artifact_discussion";
pub const ORIGIN_ARTIFACT_COMMENT: &str = "artifact_comment";

/// PCP-FR-07: the refusals `record_prompt_proposal` reports, which
/// `crate::tools::propose_prompt_changes` renders as its own.
///
/// A typed enum rather than the string errors the Tauri commands return, because
/// the tool has to tell them apart to pick a refusal message and a retryable
/// flag, and matching on prose would break the moment a sentence was edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordRefusal {
    /// PCP-FR-04: the artifact already carries an undecided proposal.
    ProposalPending,
    /// PCP-FR-07: `path` names no file the project holds.
    ArtifactNotFound,
    /// PCP-FR-07: the file's resolved artifact type is not `prompt`.
    NotAPromptArtifact,
    /// PCP-FR-07: the proposed text is what the artifact already holds.
    NoChange,
    /// CMS-FR-17: the conversation takes no further comments.
    ThreadLocked,
    /// Anything else — a write that failed, a thread that is not there.
    NotRecorded,
}

// ---------------------------------------------------------------------------
// Record shapes (PCP contract surface)
// ---------------------------------------------------------------------------

/// PCP-FR-04: where a proposal stands. `pending` is the only state that occupies
/// the artifact's one slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptProposalState {
    Pending,
    Accepted,
    Rejected,
}

impl PromptProposalState {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            PromptProposalState::Pending => "pending",
            PromptProposalState::Accepted => "accepted",
            PromptProposalState::Rejected => "rejected",
        }
    }
}

impl Default for PromptProposalState {
    fn default() -> Self {
        PromptProposalState::Rejected
    }
}

/// Read a record's state, taking anything this enum does not admit as
/// `rejected`.
///
/// The same reasoning `crate::draft_proposals` applies: a record whose state
/// will not parse fails the whole record, and a proposal that silently read as
/// `pending` would occupy the artifact's one slot forever (PCP-FR-04) with no
/// way to clear it. `rejected` is the state that is safe to be wrong about — it
/// writes nothing, holds nothing, and the agent can simply propose again.
pub(super) fn state_or_rejected<'de, D>(de: D) -> Result<PromptProposalState, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(de)?;
    Ok(match raw.as_str() {
        "pending" => PromptProposalState::Pending,
        "accepted" => PromptProposalState::Accepted,
        _ => PromptProposalState::Rejected,
    })
}

/// The persisted record, serialised as TOML into `<proposal-id>.toml`.
///
/// `agent` stands last: `serde`'s TOML serialiser emits fields in declaration
/// order and refuses a scalar written after a table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptChangeProposal {
    pub id: String,
    /// The stable, path-derived node key of the target (ASC-FR-13).
    pub artifact_id: String,
    /// Project-relative path of the prompt artifact.
    pub path: String,
    /// What the agent wrote, which is also the body of its comment.
    pub rationale: String,
    pub thread_id: String,
    /// The comment carrying the `prompt_proposal` attachment (CMS-FR-66).
    pub comment_id: String,
    #[serde(default, deserialize_with = "state_or_rejected")]
    pub state: PromptProposalState,
    /// PCP-FR-22: the checksum of the candidate **as the recording wrote it**,
    /// so `candidate_edited` describes the text rather than the history of
    /// getting to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_checksum: Option<String>,
    /// PCP-FR-22: true while the candidate differs from the agent's own.
    #[serde(default)]
    pub candidate_edited: bool,
    /// PCP-FR-14: true between an acceptance landing and its decision comment
    /// being appended. The decision is settled either way; only the
    /// conversation is owed one.
    #[serde(default)]
    pub comment_owed: bool,
    pub created_at: String,
    /// RFC 3339 UTC; absent while pending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
    /// PCP-FR-05: the agent participant that proposed it — the same shape a
    /// comment's author carries, so a surface renders the two alike.
    pub agent: Participant,
}

/// PCP contract surface: what a decision hands back.
///
/// The proposal as it now stands, plus the identity of the comment the decision
/// was recorded as and the origin kind of the conversation it landed in — which
/// together are exactly what the surface needs to dispatch the decision's own
/// turns (PCR-FR-14), and nothing more. Who those reach is the surface's own
/// rule, read from the conversation's comments (per
/// `../../specifications/ui/CMT-comments.md` CTA-FR-LCFU, CTA-FR-SSUZ): this module
/// names no recipient and knows of none.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptDecisionOutcome {
    pub proposal: PromptChangeProposal,
    /// Absent when the comment could not be appended — no turn then has
    /// anything to answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_id: Option<String>,
    /// `artifact_discussion` or `artifact_comment`, as AGC-FR-05 spells them.
    pub origin_kind: &'static str,
}

/// PCP-FR-11: the candidate's text, for the surface about to render a
/// comparison of it — together with the checksum that is the baseline its next
/// save is checked against (PCP-FR-24).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptProposalContent {
    pub content: String,
    pub checksum: String,
}

/// PCP-FR-22: what a candidate save reports — the checksum of the bytes it
/// wrote, which becomes the caller's next baseline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptCandidateSaved {
    pub checksum: String,
}

/// PCP-FR-14: the standing acceptance operation.
///
/// It carries everything a reconciliation needs to finish or undo the operation
/// **without its caller**, because the caller may be a process that no longer
/// exists. The decision comment's body, its identity, and both of its pre-minted
/// ids are here rather than being recomposed: a body recomposed after a crash
/// could differ from the one that was owed, and an id minted a second time would
/// fold as a second comment (CMS-FR-06).
///
/// There is deliberately no phase field. What to do is read from the artifact's
/// own bytes against the two checksums recorded here (PCP-FR-14), which is a
/// fact about the file rather than a claim a crashed process left behind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Journal {
    pub(super) operation_id: String,
    pub(super) proposal_id: String,
    pub(super) artifact_id: String,
    /// The artifact's project-relative path at the moment the operation began.
    pub(super) path: String,
    /// The digest of the bytes `<proposal-id>.prior` holds.
    pub(super) prior_sha256: String,
    /// The digest of the candidate **as the write path will persist it**, which
    /// is what tells a reconciliation whether the commit point was passed.
    pub(super) candidate_sha256: String,
    pub(super) comment_id: String,
    pub(super) event_id: String,
    pub(super) thread_id: String,
    pub(super) comment_body: String,
    pub(super) created_at: String,
    /// The acting human the decision comment is stamped with.
    pub(super) comment_by: Participant,
}
