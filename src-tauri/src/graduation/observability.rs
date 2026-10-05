//! What a run tells the author about itself (`GOB-graduation-observability.md`).

use super::*;

/// GOB-FR-JAJU: the four stages, in order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraduationVisualStage {
    #[default]
    Queued,
    Working,
    Review,
    Done,
}

impl GraduationVisualStage {
    /// GRS-FR-KJVN: whether an id names one of the progress-bar stages.
    pub fn from_id(id: &str) -> bool {
        Self::ORDERED.iter().any(|stage| stage.as_str() == id)
    }

    /// GOB-FR-JAJU: the stages, in the order the author is shown them.
    pub const ORDERED: [Self; 4] = [Self::Queued, Self::Working, Self::Review, Self::Done];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Working => "working",
            Self::Review => "review",
            Self::Done => "done",
        }
    }
}

/// GOB-FR-ZMCA: the six conditions a stage stands in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraduationStageCondition {
    Active,
    #[default]
    Waiting,
    Paused,
    Blocked,
    Stopped,
    Complete,
}

impl GraduationStageCondition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Waiting => "waiting",
            Self::Paused => "paused",
            Self::Blocked => "blocked",
            Self::Stopped => "stopped",
            Self::Complete => "complete",
        }
    }
}

/// GOB-FR-HDZI: why a run moved from one stage to another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageReason {
    Enqueued,
    WorkStarted,
    ReviewStarted,
    /// GOB-FR-BTXN: a backward move.
    ReviewRevision,
    /// GOB-FR-BTXN: the other backward move — the author continued a run that
    /// had come to rest, so it goes back to the queue.
    BlockedRetry,
    Finished,
    Ended,
}

impl StageReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Enqueued => "enqueued",
            Self::WorkStarted => "work_started",
            Self::ReviewStarted => "review_started",
            Self::ReviewRevision => "review_revision",
            Self::BlockedRetry => "blocked_retry",
            Self::Finished => "finished",
            Self::Ended => "ended",
        }
    }
}

/// GOB-FR-VVNI: one append-only entry of the stage history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageTransition {
    pub from: GraduationVisualStage,
    pub to: GraduationVisualStage,
    /// The pass the move belongs to.
    pub pass: u32,
    /// RFC 3339 UTC.
    pub at: String,
    pub reason: StageReason,
}

/// GOB-FR-ABRE: what a pass is doing, or what it decided.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PassStatus {
    #[default]
    Working,
    Passed,
    Failed,
}

/// GOB-FR-XYCY: one pass, whole.
///
/// Nothing here is reduced to a code standing for it: the task the turn was
/// given, the review's own words, and the instruction the next turn was told
/// are each carried entire.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PassRecord {
    pub pass: u32,
    pub status: PassStatus,
    /// The whole instruction the work turn was given.
    pub task: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<ReviewOutcome>,
    /// The review's own words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(default)]
    pub findings: Vec<ReviewFinding>,
    /// What the following turn was told, whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_instruction: Option<String>,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
}

/// GOB-FR-XMDU: the whole record, versioned.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationObservability {
    /// GOB-FR-XMDU: the integer 1. A reader that does not recognise a version
    /// renders none of it rather than guessing at its shape.
    pub observability_version: u32,
    pub current_stage: GraduationVisualStage,
    pub stage_condition: GraduationStageCondition,
    #[serde(default)]
    pub stage_history: Vec<StageTransition>,
    #[serde(default)]
    pub passes: Vec<PassRecord>,
}

impl Default for GraduationObservability {
    fn default() -> Self {
        Self {
            observability_version: 1,
            current_stage: GraduationVisualStage::Queued,
            stage_condition: GraduationStageCondition::Waiting,
            stage_history: Vec::new(),
            passes: Vec::new(),
        }
    }
}

impl GraduationObservability {
    /// GOB-FR-HXUZ: the stage and the condition a state stands at.
    ///
    /// Persisted rather than inferred, so this is what writes them: a state
    /// that keeps the stage it stood at keeps it here too.
    pub fn apply_state(&mut self, state: GraduationRunState, paused: bool) {
        use GraduationRunState as S;
        use GraduationStageCondition as C;
        use GraduationVisualStage as V;
        let (stage, condition) = match state {
            S::Queued => (V::Queued, C::Waiting),
            S::Working => (V::Working, C::Active),
            S::Reviewing => (V::Review, C::Active),
            // These three keep the stage the run stood at and change only how
            // it stands there.
            S::AwaitingAuthor => (self.current_stage, C::Waiting),
            S::Blocked => (self.current_stage, C::Blocked),
            S::Interrupted => (
                self.current_stage,
                if paused { C::Paused } else { C::Stopped },
            ),
            S::Completed => (V::Done, C::Complete),
            S::Discarded | S::Failed => (V::Done, C::Stopped),
        };
        self.current_stage = stage;
        self.stage_condition = condition;
    }

    /// GOB-FR-VVNI / GOB-FR-VVNI: append a transition, and only where the stage
    /// actually changes.
    pub fn note_stage(
        &mut self,
        to: GraduationVisualStage,
        pass: u32,
        at: String,
        reason: StageReason,
    ) {
        if self.current_stage == to {
            return;
        }
        self.stage_history.push(StageTransition {
            from: self.current_stage,
            to,
            pass,
            at,
            reason,
        });
        self.current_stage = to;
    }

    /// GOB-FR-XYCY: open a pass record for the turn about to run.
    ///
    /// A pass that is already open is **kept**. A run that stopped and was
    /// continued, and one whose escalation the author answered, both dispatch a
    /// further work turn of the pass they were already in; opening a second
    /// record for it would report two passes where one was made, and would
    /// spend a bound the author never used (GRL-FR-ARPX, GRL-FR-ISIL).
    pub fn open_pass(&mut self, pass: u32, task: String, at: String) {
        if self
            .passes
            .iter()
            .any(|record| record.pass == pass && record.ended_at.is_none())
        {
            return;
        }
        self.begin_pass(pass, task, at);
    }

    /// Append a pass record, whatever is already there.
    fn begin_pass(&mut self, pass: u32, task: String, at: String) {
        self.passes.push(PassRecord {
            pass,
            status: PassStatus::Working,
            task,
            verdict: None,
            rationale: None,
            findings: Vec::new(),
            next_instruction: None,
            started_at: at,
            ended_at: None,
        });
    }

    /// GOB-FR-ABRE / GOB-FR-DKCJ: record what the review decided about the pass
    /// that is open, findings whole.
    pub fn settle_pass(
        &mut self,
        verdict: &ReviewVerdict,
        next_instruction: Option<String>,
        at: String,
    ) {
        let Some(record) = self.passes.last_mut() else {
            return;
        };
        record.status = match verdict.verdict {
            ReviewOutcome::Ready => PassStatus::Passed,
            ReviewOutcome::Revise => PassStatus::Failed,
        };
        record.verdict = Some(verdict.verdict);
        record.rationale = Some(verdict.rationale.clone());
        record.findings = verdict.findings.clone();
        record.next_instruction = next_instruction;
        record.ended_at = Some(at);
    }

    /// The pass the record's own history stands at, counted from one.
    ///
    /// This reads what was recorded. What the loop counts against its bound is
    /// `GraduationRun::pass`, which reads the checkpoint (GXD-FR-GMDI).
    pub fn pass(&self) -> u32 {
        self.passes.last().map(|record| record.pass).unwrap_or(1)
    }
}
