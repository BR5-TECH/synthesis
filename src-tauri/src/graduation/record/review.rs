//! The review verdict of a run (`../ai/GRL-graduation-loop.md` GRL-FR-VIAT).

use super::*;

/// GRL-FR-VIAT: how serious a finding is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewSeverity {
    Critical,
    Major,
    Minor,
}

impl ReviewSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::Major => "major",
            Self::Minor => "minor",
        }
    }
}

/// GRL-FR-VIAT: one problem, stated once.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFinding {
    pub severity: ReviewSeverity,
    pub description: String,
    /// Project-relative; may name a path that does not exist yet.
    #[serde(default)]
    pub affected_files: Vec<String>,
    /// Directly actionable by the next work turn.
    pub correction: String,
    /// OPU-FR-EPXL: `work` or `plan`, and `work` where absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<FindingKind>,
}

/// OPU-FR-EPXL: whether a finding is about the work or about the plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    Work,
    Plan,
}

impl ReviewFinding {
    /// OPU-FR-EPXL: a finding of the kind `plan`.
    pub fn is_plan(&self) -> bool {
        self.kind == Some(FindingKind::Plan)
    }
}

/// GRL-FR-VIAT: what a review turn answers with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewVerdict {
    /// `ready` or `revise`, and nothing else.
    pub verdict: ReviewOutcome,
    /// Never blank.
    pub rationale: String,
    /// Empty for `ready`; at least one for `revise`.
    #[serde(default)]
    pub findings: Vec<ReviewFinding>,
}

/// GRL-FR-VIAT: the two verdicts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewOutcome {
    Ready,
    Revise,
}
