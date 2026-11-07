//! The decision ledger and the review history of a run (`GXD-FR-NWSL`, `GXD-FR-WRVH`).

use super::*;

impl GraduationEscalationOrigin {
    /// GXD-FR-NWSL: the turn kind a ledger entry names for an escalation that
    /// this origin raised. A merge run's work and review turns carry the
    /// `merge_` prefix (GXD-FR-HSQV).
    pub fn decision_turn(self, merge_run: bool) -> &'static str {
        match (self, merge_run) {
            (Self::Work, false) => "work",
            (Self::Work, true) => "merge_work",
            (Self::Review, false) => "review",
            (Self::Review, true) => "merge_review",
            (Self::SemanticMerge, _) => "semantic_rebase",
        }
    }
}

/// GXD-FR-NWSL: one entry of a run's decision ledger — one question the author
/// answered.
///
/// An entry is never replaced, edited, merged or removed (GXD-FR-HRDM). The
/// question travels with the answer, because an answer such as `"base"` has
/// nothing to attach it to once the turn that asked has ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationDecision {
    pub question: String,
    pub answer: String,
    /// The summary of the chosen proposed response, or empty for the author's
    /// own words.
    #[serde(default)]
    pub summary: String,
    /// The author's note beside the answer, where there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// `work`, `review`, `merge_work`, `merge_review` or `semantic_rebase`.
    pub turn: String,
    /// The pass the turn asked in.
    pub pass: u32,
}

impl GraduationDecision {
    /// The entry one answered question adds to the ledger.
    pub fn from_answer(
        question: &GraduationEscalationQuestion,
        answer: &GraduationEscalationAnswer,
        turn: &str,
        pass: u32,
    ) -> Self {
        Self {
            question: question.question.clone(),
            answer: answer.answer.clone(),
            summary: answer.summary.clone(),
            note: answer.note.clone(),
            turn: turn.to_string(),
            pass,
        }
    }
}

/// GXD-FR-WRVH: one finding of the run's review history, with the pass of the
/// review that found it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewHistoryEntry {
    pub pass: u32,
    pub finding: ReviewFinding,
}

impl GraduationCheckpoint {
    /// GXD-FR-QPFT: the history a work turn is given — the findings of the
    /// passes before the latest review. The latest findings travel as the
    /// turn's `loop_instruction`, so they are left out here.
    pub fn earlier_findings(&self) -> Vec<&ReviewHistoryEntry> {
        let latest = self.review_history.iter().map(|entry| entry.pass).max().unwrap_or(0);
        self.review_history.iter().filter(|entry| entry.pass < latest).collect()
    }
}
