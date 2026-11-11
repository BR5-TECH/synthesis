//! The typed errors and the records of a change edit proposal (CEP contract
//! surface).

use super::*;
use crate::prompt_proposals::{HunkCounts, HunkState};
use crate::text_hunks::hunks::{HunkDocument, ProposalHunk};

// ---------------------------------------------------------------------------
// Typed errors
// ---------------------------------------------------------------------------

/// CEP-FR-TFOY: no project is open.
pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";
/// CEP-FR-TFOY: the id names no proposal of the open project.
pub const ERR_PROPOSAL_NOT_FOUND: &str = "proposal_not_found";
/// CEP-FR-LYSS: the stream is not a live stream of the project.
pub const ERR_UNKNOWN_STREAM: &str = "unknown_stream";
/// CEP-FR-AYVS / CEP-FR-LYSS: `openspec/changes/<change>/` is not a folder.
pub const ERR_CHANGE_NOT_FOUND: &str = "change_not_found";
/// CEP-FR-WJKY: a hunk path leaves the change folder.
pub const ERR_PATH_OUTSIDE_CHANGE: &str = "path_outside_change";
/// CEP-FR-HRCX: a new-file hunk names a path that holds a file now.
pub const ERR_FILE_EXISTS: &str = "file_exists";
/// CEP-FR-ESQR: a graduation run of the stream works its working copy.
pub const ERR_STREAM_BUSY: &str = "stream_busy";
/// CEP-FR-ZOIG: the change already holds a pending proposal.
pub const ERR_PROPOSAL_PENDING: &str = "proposal_pending";
/// PCP-FR-18: a resolved proposal takes no further decision.
pub const ERR_ALREADY_DECIDED: &str = "already_decided";
/// PCP-FR-EDTW: the hunk set has moved on since the caller read it.
pub const ERR_CANDIDATE_STALE: &str = "candidate_stale";
/// PCP-FR-ZQAH: the text a hunk names is no longer in its file.
pub const ERR_ANCHOR_LOST: &str = "anchor_lost";
/// PCP-FR-ZQAH: the id names no hunk of this proposal.
pub const ERR_HUNK_NOT_FOUND: &str = "hunk_not_found";
/// PCP-FR-ZQAH / PCP-FR-RJHN: the hunk is decided already.
pub const ERR_HUNK_ALREADY_DECIDED: &str = "hunk_already_decided";
/// PCP-FR-18: the agent withdrew the hunk.
pub const ERR_HUNK_WITHDRAWN: &str = "hunk_withdrawn";
/// PCP-FR-14: a failure at or before the commit point. No file was written.
pub const ERR_WRITE_FAILED: &str = "write_failed";
/// CEP-FR-OIZT: a rework run of the change is not terminal.
pub const ERR_REWORK_RUNNING: &str = "rework_running";
/// PCP-FR-14: another acceptance of this proposal is in flight.
pub const ERR_ACCEPTANCE_IN_PROGRESS: &str = "acceptance_in_progress";
/// PCP-FR-14: the acceptance landed and its decision comment is still owed.
pub const ERR_ACCEPTANCE_INCOMPLETE: &str = "acceptance_incomplete";

/// AGC-FR-05's spellings for a change target.
pub const ORIGIN_CHANGE_DISCUSSION: &str = "change_discussion";
pub const ORIGIN_CHANGE_COMMENT: &str = "change_comment";

// ---------------------------------------------------------------------------
// The hunk set (CEP-FR-HXCC, CEP-FR-IISM)
// ---------------------------------------------------------------------------

/// The hunks of one file of the change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HunkSetFile {
    /// Change-relative path (CEP-FR-WJKY).
    pub path: String,
    /// CEP-FR-HRCX: the file did not exist when the hunk was proposed.
    #[serde(default)]
    pub new_file: bool,
    pub document: HunkDocument,
}

/// The `<proposal-id>.hunks` file: one hunk document per file, in proposal
/// order. Hunk ids are unique across the whole set.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HunkSet {
    #[serde(rename = "file", default)]
    pub files: Vec<HunkSetFile>,
}

impl HunkSet {
    /// The file and the hunk that `hunk_id` names.
    pub fn find(&self, hunk_id: &str) -> Option<(&HunkSetFile, &ProposalHunk)> {
        self.files.iter().find_map(|file| file.document.find(hunk_id).map(|hunk| (file, hunk)))
    }

    /// Every hunk in proposal order, each with its file.
    pub fn hunks(&self) -> impl Iterator<Item = (&HunkSetFile, &ProposalHunk)> {
        self.files.iter().flat_map(|file| file.document.hunks.iter().map(move |hunk| (file, hunk)))
    }

    /// The TOML text of the set. A set that does not serialise is an error,
    /// so an empty file is never written in its place.
    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string(self).map_err(|e| e.to_string())
    }

    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// PCP-FR-EDTW: the baseline an author's hunk edit is checked against.
    pub fn checksum(&self) -> String {
        fs::sha256_bytes(self.to_toml().unwrap_or_default().as_bytes())
    }
}

// ---------------------------------------------------------------------------
// Record shapes (CEP contract surface)
// ---------------------------------------------------------------------------

/// CEP-FR-ZOIG: where a proposal stands, derived as PCP-FR-HKLD derives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeProposalState {
    Pending,
    Accepted,
    #[default]
    Rejected,
}

impl ChangeProposalState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
        }
    }
}

/// A state that does not parse reads as `rejected`, for the reason of
/// `prompt_proposals::model::state_or_rejected`: a record that silently read
/// as `pending` would hold the change's one slot for good.
fn state_or_rejected<'de, D>(de: D) -> Result<ChangeProposalState, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(de)?;
    Ok(match raw.as_str() {
        "pending" => ChangeProposalState::Pending,
        "accepted" => ChangeProposalState::Accepted,
        _ => ChangeProposalState::Rejected,
    })
}

/// CEP contract surface `ChangeEditProposal`, also the persisted record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEditProposal {
    pub id: String,
    pub stream_id: String,
    pub change: String,
    /// What the agent wrote, which is also the body of its comment.
    pub rationale: String,
    pub thread_id: String,
    /// The comment that carries the `change_proposal` attachment.
    pub comment_id: String,
    #[serde(default, deserialize_with = "state_or_rejected")]
    pub state: ChangeProposalState,
    /// PCP-FR-EDTW: true while any hunk's text differs from the agent's own.
    #[serde(default)]
    pub candidate_edited: bool,
    /// PCP-FR-14: an acceptance landed and its decision comment is owed.
    #[serde(default)]
    pub comment_owed: bool,
    /// CEP-FR-IYHP: a commit of this proposal's files failed.
    #[serde(default)]
    pub commit_owed: bool,
    #[serde(default)]
    pub hunk_count: u32,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
    #[serde(default)]
    pub counts: HunkCounts,
    /// CEP-FR-FOJW: the change-relative paths the accepted hunks wrote, in the
    /// order they were first written.
    #[serde(default)]
    pub written_paths: Vec<String>,
    /// The per-hunk ledger, in proposal order, with the path on each row.
    #[serde(default, rename = "hunks")]
    pub ledger: Vec<ChangeHunkRow>,
    /// PCP-FR-EDTW: the checksum of each hunk's `after` as the agent wrote it.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub origin_checksums: std::collections::BTreeMap<String, String>,
    /// The agent participant that proposed it.
    pub agent: Participant,
}

impl ChangeEditProposal {
    pub fn row(&self, hunk_id: &str) -> Option<&ChangeHunkRow> {
        self.ledger.iter().find(|row| row.id == hunk_id)
    }

    pub fn row_mut(&mut self, hunk_id: &str) -> Option<&mut ChangeHunkRow> {
        self.ledger.iter_mut().find(|row| row.id == hunk_id)
    }

    /// Whether any row stands in `state`.
    pub fn any(&self, state: HunkState) -> bool {
        self.ledger.iter().any(|row| row.state == state)
    }
}

/// CEP contract surface `ChangeDecisionOutcome`.
///
/// `proposal` and `committed` are the contract. `resolved`, `comment_id`, and
/// `origin_kind` are what the surface needs to dispatch the decision's own
/// turns, as `PromptDecisionOutcome` carries them (PCR-FR-14).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeDecisionOutcome {
    pub proposal: ChangeEditProposal,
    /// True where this decision closed the proposal and its commit landed.
    pub committed: bool,
    /// PCP-FR-RSLV: this decision left no hunk undecided.
    pub resolved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_id: Option<String>,
    /// `change_discussion` or `change_comment`.
    pub origin_kind: &'static str,
}

/// CEP contract surface `ChangeEditHunk`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEditHunk {
    #[serde(flatten)]
    pub hunk: ProposalHunk,
    /// Change-relative path (CEP-FR-WJKY).
    pub path: String,
    /// CEP-FR-HRCX.
    pub new_file: bool,
    pub state: HunkState,
}

/// One hunk's placement in its file as the file stands (PCP-FR-LDHK).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HunkPlacement {
    Resolved { start: usize, end: usize },
    Lost,
}

/// CEP-FR-HUFE: what "load change edit proposal hunks" returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeProposalHunks {
    pub hunks: Vec<ChangeEditHunk>,
    pub resolutions: Vec<HunkPlacement>,
    pub checksum: String,
}

/// PCP-FR-EDTW: what a hunk edit reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeCandidateSaved {
    pub checksum: String,
}

/// PCP-FR-14: the standing acceptance, with everything a reconciliation needs
/// to finish or undo it without its caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Journal {
    pub(super) operation_id: String,
    pub(super) proposal_id: String,
    pub(super) hunk_id: String,
    /// Change-relative path of the file the hunk writes.
    pub(super) path: String,
    /// The digest of the file before the write, or `None` for a new file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) prior_sha256: Option<String>,
    /// The digest of the text as the write lands it.
    pub(super) candidate_sha256: String,
    pub(super) prior_state: HunkState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) prior_decided_at: Option<String>,
    /// Whether the record listed the path as written before this acceptance.
    #[serde(default)]
    pub(super) path_was_written: bool,
    pub(super) resolves: bool,
    pub(super) comment_id: String,
    pub(super) event_id: String,
    pub(super) thread_id: String,
    pub(super) comment_body: String,
    pub(super) comment_by: Participant,
}

/// CEP-FR-QDJH / PCE-FR-FEAK: the refusals `record_change_proposal` reports.
/// Every refusal about one hunk carries its one-based place and the length of
/// the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordRefusal {
    ProposalPending,
    /// CEP-FR-OIZT.
    ReworkRunning,
    UnknownStream,
    ChangeNotFound,
    PathOutsideChange {
        at: usize,
        of: usize,
    },
    NoChange,
    NoHunks,
    HunkAnchorLost {
        at: usize,
        of: usize,
    },
    HunkAmbiguous {
        at: usize,
        of: usize,
    },
    HunkOverlap {
        at: usize,
        of: usize,
    },
    HunkContextTooLong {
        at: usize,
        of: usize,
        side: crate::text_hunks::hunks::ContextSide,
    },
    ThreadResolved,
    NotRecorded,
}

impl RecordRefusal {
    /// A stable code for the log record (CEP-FR-MZFF).
    pub fn code(&self) -> &'static str {
        match self {
            Self::ProposalPending => ERR_PROPOSAL_PENDING,
            Self::ReworkRunning => ERR_REWORK_RUNNING,
            Self::UnknownStream => ERR_UNKNOWN_STREAM,
            Self::ChangeNotFound => ERR_CHANGE_NOT_FOUND,
            Self::PathOutsideChange { .. } => ERR_PATH_OUTSIDE_CHANGE,
            Self::NoChange => "no_change",
            Self::NoHunks => "no_hunks",
            Self::HunkAnchorLost { .. } => "hunk_anchor_lost",
            Self::HunkAmbiguous { .. } => "hunk_ambiguous",
            Self::HunkOverlap { .. } => "hunk_overlap",
            Self::HunkContextTooLong { .. } => "hunk_context_too_long",
            Self::ThreadResolved => comments::ERR_DISCUSSION_RESOLVED,
            Self::NotRecorded => ERR_WRITE_FAILED,
        }
    }
}
