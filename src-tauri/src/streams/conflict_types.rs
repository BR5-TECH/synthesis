//! The two shapes a reconciliation records about what it was asked and what the
//! author answered (`WKS-work-streams.md`, `GRB-graduation-rebase.md`).
//!
//! A stream update keeps them on its update record. A merge run keeps its own
//! conflicts on the run's merge data.

use super::*;

/// GRB-FR-SRVN: one path a reconciliation could not settle, as a record keeps it.
///
/// `MergeConflict::merged_file` names a path inside an attempt's mount, and the
/// attempt is reclaimed when it completes. A record therefore keeps the path and
/// the two side changes, and no container path at all.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamMergeConflict {
    /// Project-relative.
    pub path: String,
    pub base_change: String,
    pub stream_change: String,
}

/// GRB-FR-KMXT: one decision the author has already made about a reconciliation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamMergeDecision {
    /// The recorded position of the question this answers.
    pub position: u32,
    /// The question, whole. A turn reads a decision rather than a bare value
    /// with nothing to attach it to.
    pub question: String,
    /// What the author answered.
    pub answer: String,
    /// The proposed response's summary, or empty for the author's own words.
    #[serde(default)]
    pub summary: String,
    /// WKS-FR-CBXW: the author's note beside a chosen response, where one was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// GXD-FR-NWSL: the turn that asked, always `semantic_rebase` for an update.
    #[serde(default = "semantic_rebase_turn")]
    pub turn: String,
    /// GXD-FR-NWSL: an update has one pass.
    #[serde(default = "first_pass")]
    pub pass: u32,
}

/// The turn every ledger entry of an update names.
pub const SEMANTIC_REBASE_TURN: &str = "semantic_rebase";

fn semantic_rebase_turn() -> String {
    SEMANTIC_REBASE_TURN.to_string()
}

fn first_pass() -> u32 {
    1
}
