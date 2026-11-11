//! OpenSpec plan updates (`OPU-openspec-plan-updates.md`).
//!
//! A **plan update** is how an apply run asks the author to change the plan of
//! its change. The apply agent writes the planning files it proposes into the
//! scratch folder and answers `escalation_required` with `kind:
//! "plan_update"`. The application turns the scratch folder into hunks and
//! records them, with the reason and the questions, in a new discussion of the
//! change. The author answers and decides there, and Continue takes the run on
//! from the plan the author accepted.
//!
//! This module holds the data of a plan update and the operations outside the
//! loop: the gates the change edit proposal store and the question set store
//! ask, the hand-over of answers, Continue, and the plan base. The loop's own
//! part is in `driver/plan_turns.rs` and `driver/plan_record.rs`.

use super::*;

mod continuing;
mod gates;
pub mod plan_base;
pub mod scratch;

pub use continuing::*;
pub use gates::*;

/// OPU contract surface: the rest of a recorded plan update.
pub const REST_PLAN_UPDATE: &str = "plan_update";
/// OPU contract surface: the rest of a plan correction turn that gave no
/// valid plan update.
pub const REST_PLAN_FINDINGS: &str = "plan_findings";
/// OPU contract surface: the rest of a plan update correction turn that gave
/// no valid plan update.
pub const REST_PLAN_UPDATE_INVALID: &str = "plan_update_invalid";

/// OPU-FR-UOYJ: the origin of a plan update that a work turn gave.
pub const ORIGIN_WORK: &str = "work";
/// OPU-FR-UOYJ: the origin of a plan update that a plan correction turn gave.
pub const ORIGIN_REVIEW: &str = "review";

/// OPU-FR-THOZ: Continue while the questions of the plan update stand.
pub const ERR_PLAN_QUESTIONS_PENDING: &str = crate::change_proposals::ERR_PLAN_QUESTIONS_PENDING;
/// OPU-FR-THOZ: Continue while the plan proposal is pending or owes its
/// commit.
pub const ERR_PROPOSAL_PENDING: &str = crate::change_proposals::ERR_PROPOSAL_PENDING;
/// OPU-FR-PXDY: carries the read's code after a colon.
pub const ERR_OPENSPEC_VALIDATION_UNAVAILABLE: &str = "openspec_validation_unavailable";
/// OPU-FR-TPYT, OPU-FR-YTKN.
pub const ERR_WRITE_FAILED: &str = crate::change_proposals::ERR_WRITE_FAILED;

/// OPU-FR-XSRD: the `refusals.md` section of a plan update correction turn.
pub const REFUSAL_PLAN_UPDATE_INVALID: &str = "plan_update_invalid";
/// OPU-FR-DTYE: the `refusals.md` section of a plan correction turn.
pub const REFUSAL_PLAN_CORRECTION: &str = "plan_correction";

/// OPU contract surface, fixed text: the comment of a `plan_findings` rest.
pub fn plan_findings_comment(change: &str) -> String {
    format!(
        "The review of the apply run found that the plan of \"{change}\" is wrong. Ask an agent here to propose the edits, then use Continue."
    )
}

/// OPU-FR-XLVF: why a plan update is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanRefusalCause {
    /// An entry is not a regular file, or its path is not below the change
    /// folder.
    PlanPathOutsideChange,
    /// A file is not UTF-8 text.
    PlanFileNotText,
    /// A file is empty where the working copy holds that path not empty.
    PlanDeletesFile,
    /// No hunk and no question.
    PlanUpdateEmpty,
}

impl PlanRefusalCause {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PlanPathOutsideChange => "plan_path_outside_change",
            Self::PlanFileNotText => "plan_file_not_text",
            Self::PlanDeletesFile => "plan_deletes_file",
            Self::PlanUpdateEmpty => "plan_update_empty",
        }
    }
}

/// GXD-FR-GMDI: what the checkpoint of an apply run holds about its plan
/// updates. Empty for every other run. It holds no byte of a project file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanCheckpoint {
    /// OPU-FR-BKZX: a recording that has not completed, with the id of each
    /// step that has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording: Option<PlanRecording>,
    /// OPU-FR-XSRD, OPU-FR-FDCW: the correction turn dispatched next, saved
    /// before its dispatch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correction: Option<PlanCorrection>,
    /// OPU-FR-DTYE: the session of the run's last work turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_work_session: Option<GraduationResumeRef>,
    /// OPU-FR-WPSR: the session of the turn that gave the plan update of the
    /// rest, which the turn after Continue resumes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest_session: Option<GraduationResumeRef>,
    /// OPU-FR-WPSR, OPU-FR-TZBD, OPU-FR-VKBO: what the turn that Continue
    /// dispatches carries. Cleared when that turn ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_turn: Option<PlanNextTurn>,
}

impl PlanCheckpoint {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// OPU-FR-QXUE, OPU-FR-MCSG: a recording in progress.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRecording {
    /// `plan_update` or `plan_findings`: the rest the recording ends in.
    pub rest: String,
    /// `work` or `review`.
    pub origin: String,
    /// The agent's reason; empty for `plan_findings`.
    #[serde(default)]
    pub reason: String,
    /// The questions as the agent gave them.
    #[serde(default)]
    pub questions: Vec<GraduationEscalationQuestion>,
    /// Origin `review`: every finding of the verdict.
    #[serde(default)]
    pub findings: Vec<ReviewFinding>,
    /// The session of the turn that gave the plan update.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<GraduationResumeRef>,
    /// Step 1: the plan thread, once it is open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    /// Step 2: the question set, once it is reserved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_id: Option<String>,
    /// Step 3: the plan proposal, once it is recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal_id: Option<String>,
    /// Step 3 is done, also where it recorded nothing.
    #[serde(default)]
    pub proposal_done: bool,
}

/// OPU-FR-XSRD, OPU-FR-DTYE: the two correction turns of a plan update.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanCorrectionStep {
    /// OPU-FR-XSRD: after a refused plan update.
    UpdateInvalid,
    /// OPU-FR-DTYE: after a verdict that holds a `plan` finding.
    Plan,
}

/// OPU-FR-XSRD, OPU-FR-DTYE: a saved correction turn.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanCorrection {
    pub step: PlanCorrectionStep,
    /// `update_invalid`: the cause of the refusal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<PlanRefusalCause>,
    /// `plan`: every finding of the verdict, in order.
    #[serde(default)]
    pub findings: Vec<ReviewFinding>,
    /// The session the turn resumes, where there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<GraduationResumeRef>,
}

/// OPU-FR-WPSR, OPU-FR-SKTV: what the turn after Continue carries.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanNextTurn {
    /// `escalation_answer` or `resume`.
    pub purpose: String,
    /// OPU-FR-SKTV: the plan update context, where Continue took decisions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_update: Option<PlanUpdateTaskContext>,
}

/// OPU contract surface `PlanUpdateTaskContext`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanUpdateTaskContext {
    /// `work` or `review`.
    pub origin: String,
    /// The agent's reason; absent after a `plan_findings` rest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub hunks: Vec<PlanHunkOutcome>,
}

/// OPU-FR-SKTV: one hunk of the plan proposal, with no file text.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanHunkOutcome {
    pub path: String,
    /// 1-based, among the hunks of its path.
    pub position: u32,
    /// `accepted`, `edited`, or `rejected`.
    pub outcome: String,
}

impl GraduationRest {
    /// OPU-FR-RMZN, OPU-FR-QKLX, OPU-FR-CLYW: whether this is a plan rest.
    pub fn is_plan_rest(&self) -> bool {
        matches!(
            self.reason.as_str(),
            REST_PLAN_UPDATE | REST_PLAN_FINDINGS | REST_PLAN_UPDATE_INVALID
        )
    }

    /// OPU-FR-CLYW: the rest of a refused plan update correction.
    pub fn plan_update_invalid(at: String, cause: PlanRefusalCause) -> Self {
        Self {
            reason: REST_PLAN_UPDATE_INVALID.to_string(),
            at,
            cause: Some(GraduationRestCause::Plan(cause)),
            ..Self::default()
        }
    }

    /// OPU-FR-UOYJ, OPU-FR-MCSG: the rest a completed recording writes.
    pub fn of_recording(at: String, recording: &PlanRecording) -> Self {
        let update = recording.rest == REST_PLAN_UPDATE;
        let review = recording.origin == ORIGIN_REVIEW;
        Self {
            reason: recording.rest.clone(),
            at,
            origin: Some(recording.origin.clone()),
            thread_id: recording.thread_id.clone(),
            proposal_id: recording.proposal_id.clone(),
            set_id: recording.set_id.clone(),
            questions: update.then(|| recording.questions.clone()),
            plan_reason: update.then(|| recording.reason.clone()),
            findings: review.then(|| recording.findings.clone()),
            ..Self::default()
        }
    }
}

/// OPU-FR-RFFH: the answer of a turn that may not give a plan update, with a
/// plan update refused as `escalation_shape`. The answer correction of
/// GRL-FR-QKDC then follows on the terms of every refused answer. Every other
/// answer is returned as it came.
pub fn refuse_plan_update(
    execution: crate::tools::agent_exec::AgentExecution,
) -> crate::tools::agent_exec::AgentExecution {
    use crate::tools::agent_exec::ProcessOutcome;
    let response = execution.response.as_ref();
    let plan = execution.process_outcome == ProcessOutcome::Completed
        && response.and_then(|r| r.escalation.as_ref()).is_some_and(|e| e.is_plan_update());
    if !plan {
        return execution;
    }
    let session = execution.session.clone().or_else(|| response.and_then(|r| r.session.clone()));
    crate::tools::agent_exec::AgentExecution {
        process_outcome: ProcessOutcome::InvalidStructuredOutput,
        response: None,
        refusal: Some(crate::tools::agent_exec::protocol::EnvelopeInvalid::EscalationShape),
        session,
        ..execution
    }
}

#[cfg(test)]
mod tests;
