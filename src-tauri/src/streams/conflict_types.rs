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
}
