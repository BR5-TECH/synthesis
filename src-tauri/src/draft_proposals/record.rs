//! The record a proposal is held as, and the shapes the frontend reads it in
//! (`DCP-draft-change-proposals.md` contract surface).
//!
//! A vocabulary rather than behaviour: nothing here does anything, and that is
//! why it is here. Every field carries the requirement it answers, because a
//! record is committed content that travels through Git (DCP-FR-02) — a reader
//! of it may be a build years away from the one that wrote it.

use serde::{Deserialize, Serialize};

use crate::comments::Participant;

use super::{anchors, hunks};

/// DCP-FR-04: where a proposal stands. `pending` is the only state that occupies
/// the draft's one slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalState {
    Pending,
    Accepted,
    Rejected,
}

impl ProposalState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ProposalState::Pending => "pending",
            ProposalState::Accepted => "accepted",
            ProposalState::Rejected => "rejected",
        }
    }
}

/// Read a record's state, taking anything this enum does not admit as
/// `rejected`.
///
/// The same shape `crate::drafts`' `status_or_active` has, and the opposite
/// default, for the same kind of reason. A record whose state will not parse
/// fails the whole record, and a proposal that cannot be read is one the author
/// can neither accept nor decline — but unlike a draft, a proposal that silently
/// reads as `pending` would occupy the draft's one slot forever (DCP-FR-04) and
/// block every future proposal with no way to clear it. `rejected` is the state
/// that is safe to be wrong about: it writes nothing, holds nothing, and the
/// agent can simply propose again.
fn state_or_rejected<'de, D>(de: D) -> Result<ProposalState, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(de)?;
    Ok(match raw.as_str() {
        "pending" => ProposalState::Pending,
        "accepted" => ProposalState::Accepted,
        _ => ProposalState::Rejected,
    })
}

/// The persisted record, serialised as TOML into `<proposal-id>.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftChangeProposal {
    pub id: String,
    pub draft_id: String,
    /// Draft-relative path of the file this would replace.
    pub path: String,
    /// DCP-FR-05: the agent participant that proposed it — the same shape a
    /// comment's author carries, so a surface renders the two alike.
    pub agent: Participant,
    /// What the agent wrote, which is also the body of its comment.
    pub rationale: String,
    pub thread_id: String,
    /// The comment carrying the `proposal` attachment that refers to this
    /// record (CMS-FR-60).
    pub comment_id: String,
    #[serde(default, deserialize_with = "state_or_rejected")]
    pub state: ProposalState,
    /// DCP-FR-25: the checksum of the candidate **as the recording wrote it**.
    ///
    /// Held so `candidate_edited` can describe the text rather than the history
    /// of getting to it: an author who edits a candidate and then puts it back
    /// letter for letter has an unedited candidate again, and without the
    /// agent's own bytes to compare against there would be no way to say so.
    ///
    /// Defaulted for a record written by an older build, which read as "no
    /// origin recorded" — the flag then stays wherever it already was rather
    /// than being asserted against a checksum nobody stamped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_checksum: Option<String>,
    /// DCP-FR-25: true while any hunk's text differs from the agent's own.
    #[serde(default)]
    pub candidate_edited: bool,
    /// DCP-FR-QLMH: a proposal held as a whole document, with no `.hunks` file
    /// beside it. Such a proposal arrives through Git long after this module
    /// stopped writing them, so it is read rather than migrated.
    #[serde(default)]
    pub legacy: bool,
    /// DCP-FR-HRQN: the prompt as the agent read it, at record time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_sha256: Option<String>,
    #[serde(default)]
    pub hunk_count: u32,
    /// DCP-FR-PWSF: derived from the ledger on every write, so no reader has to
    /// recompute it and none can disagree with it.
    #[serde(default)]
    pub counts: hunks::HunkCounts,
    /// DCP-FR-01: the per-hunk decision ledger, in proposal order. It lives in
    /// the record rather than beside it because a decision must be one atomic
    /// write.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ledger: Vec<hunks::HunkLedgerRow>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
}

impl Default for ProposalState {
    fn default() -> Self {
        ProposalState::Rejected
    }
}

/// DCR-FR-15: what a decision hands back.
///
/// The proposal as it now stands, plus the identity of the comment the decision
/// was recorded as and the origin kind of the conversation it landed in — which
/// together are exactly what the surface needs to dispatch the decision's own
/// turns, and nothing more. Who those reach is the surface's own rule, read from
/// the conversation's comments (per `../../specifications/ui/DCR-draft-change-review.md`
/// DCR-FR-15 and `../../specifications/ui/CMT-comments.md` CTA-FR-LCFU,
/// CTA-FR-SSUZ): this module names no recipient and knows of none.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionOutcome {
    pub proposal: DraftChangeProposal,
    /// Absent when the conversation would not take the comment — the decision
    /// still stands, and there is then nothing for a turn to answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_id: Option<String>,
    /// `draft_discussion` or `draft_comment`, as `AGC-agent-conversations.md`
    /// spells them (AGC-FR-05).
    pub origin_kind: &'static str,
}

/// DCP-FR-10: the candidate's text, for the surface about to render a diff of
/// it — together with the checksum that is the baseline its next save is
/// checked against (DCP-FR-27).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalContent {
    pub content: String,
    pub checksum: String,
}

/// DCP-FR-10: one hunk's placement in the prompt as it stands, for the surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HunkPlacement {
    Resolved { start: usize, end: usize },
    Lost,
}

impl From<anchors::Resolution> for HunkPlacement {
    fn from(r: anchors::Resolution) -> Self {
        match r {
            anchors::Resolution::Resolved { start, end } => HunkPlacement::Resolved { start, end },
            anchors::Resolution::Lost => HunkPlacement::Lost,
        }
    }
}

/// DCP-FR-10: the proposal's hunks in proposal order, each with its placement
/// against the prompt as it stands, and the baseline the next hunk edit is
/// checked against (DCP-FR-27).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalHunks {
    pub hunks: Vec<hunks::ProposalHunk>,
    pub resolutions: Vec<HunkPlacement>,
    pub checksum: String,
    /// DCP-FR-QLMH: the surface states that a legacy proposal's text is not
    /// editable, so it has to be told which kind it is holding.
    pub legacy: bool,
}

/// DCP-FR-25: what a candidate save reports — the checksum of the bytes it
/// wrote, which becomes the caller's next baseline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateSaved {
    pub checksum: String,
}

