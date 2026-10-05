//! The semantic turn a stream **update** asks for (GRB-FR-NNLS family).
//!
//! An update that Git cannot settle dispatches one semantic turn over the
//! conflicting paths, through the dispatch seam, in a throwaway checkout. The
//! task input and the instruction (`rebase.md`) belong to the update alone: a
//! stream merge is a graduation run and takes the merge turns of
//! `../ai/GRL-graduation-loop.md`.

use std::sync::Arc;

use tauri::Manager;

use super::artifact::{MergeArtifact, MergeConflict};
use super::*;
use crate::graduation::driver::{GraduationDispatch, LoopSettings, REBASE_PROMPT};
use crate::tools::agent_exec::protocol::{
    AgentTaskRequest, Cancellation, ExecutionControls, PROTOCOL_VERSION,
};
use crate::tools::agent_exec::runtime::CancellationToken;
use crate::tools::agent_exec::{
    AgentExecution, AgentExecutionError, AgentExecutionRequest, SupplementaryMount, TurnKind,
    SEMANTIC_REBASE_TARGET,
};

/// The version every semantic task carries.
const MERGE_INPUT_VERSION: u32 = 2;

/// GRB-FR-KMXT: one decision the author has already made about this merge.
///
/// The question travels with the answer: a turn that reads `"base"` alone has
/// nothing to attach it to, and the merge it is settling was re-planned from
/// branches that may have moved since the question was asked.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct MergeAuthorDecision {
    question: String,
    answer: String,
}

/// GRB contract surface: what one semantic turn is told.
///
/// The whole of it is structured input beside the instruction. Nothing about
/// the merge reaches the agent in an argument, an environment variable, or a
/// composed instruction.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct MergeTaskInput {
    merge_input_version: u32,
    attempt_id: String,
    stream_name: String,
    stream_branch: String,
    base_branch: String,
    merge_base_revision: String,
    /// The fixed container root the mirrors stand at.
    artifact_root_path: String,
    /// GRB-FR-SRVN: the whole of what the turn is asked.
    unresolved_questions: Vec<MergeConflict>,
    /// GRB-FR-KMXT: what the author has already settled. Empty on a merge no
    /// escalation has been answered for.
    author_decisions: Vec<MergeAuthorDecision>,
}

/// The field names one semantic turn's structured input carries.
///
/// Read off the type itself, so a prompt that names a field this input does not
/// carry is caught by the test that compares the two rather than by a turn that
/// cannot resolve it (GRL-FR-GADT).
#[cfg(test)]
pub(crate) fn merge_input_field_names() -> Vec<String> {
    let sample = MergeTaskInput {
        merge_input_version: MERGE_INPUT_VERSION,
        attempt_id: String::new(),
        stream_name: String::new(),
        stream_branch: String::new(),
        base_branch: String::new(),
        merge_base_revision: String::new(),
        artifact_root_path: String::new(),
        unresolved_questions: Vec::new(),
        author_decisions: Vec::new(),
    };
    match serde_json::to_value(&sample) {
        Ok(serde_json::Value::Object(map)) => map.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

/// What one semantic turn is asked about, before it is shaped as task input.
///
/// The seam an update asks its one semantic turn through (GRB-FR-CLRO).
pub(super) struct SemanticTurnInput {
    pub(super) stream_name: String,
    pub(super) stream_branch: String,
    pub(super) base_branch: String,
    pub(super) merge_base_revision: String,
    pub(super) conflicts: Vec<MergeConflict>,
    pub(super) decisions: Vec<StreamMergeDecision>,
}

/// GXD-FR-KXXB: one turn is one call through the dispatch seam, whichever
/// direction the reconciliation runs in.
pub(super) async fn execute_semantic_turn<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project_key: &str,
    attempt_id: &str,
    artifact: &MergeArtifact,
    asked: SemanticTurnInput,
    cancellation: CancellationToken,
    dispatch: &dyn GraduationDispatch<R>,
) -> Result<AgentExecution, AgentExecutionError> {
    let input = MergeTaskInput {
        merge_input_version: MERGE_INPUT_VERSION,
        attempt_id: attempt_id.to_string(),
        stream_name: asked.stream_name,
        stream_branch: asked.stream_branch,
        base_branch: asked.base_branch,
        merge_base_revision: asked.merge_base_revision,
        artifact_root_path: SEMANTIC_REBASE_TARGET.to_string(),
        unresolved_questions: asked.conflicts,
        // GRB-FR-KMXT: the question travels with the answer, so the turn reads
        // a decision rather than a value with nothing to attach it to.
        author_decisions: asked
            .decisions
            .iter()
            .map(|decision| MergeAuthorDecision {
                question: decision.question.clone(),
                answer: decision.answer.clone(),
            })
            .collect(),
    };
    execute(app, project_key, attempt_id, artifact, input, cancellation, dispatch).await
}

/// GRB-FR-LADU: the reason and the ordered question set one turn escalated
/// with, where it escalated with a set this application accepts.
pub(super) fn escalation_of(
    response: &crate::tools::agent_exec::protocol::AgentResponseEnvelope,
) -> Option<(String, Vec<crate::graduation::GraduationEscalationQuestion>)> {
    let escalation = response.escalation.as_ref()?;
    let questions = escalation
        .questions
        .iter()
        .enumerate()
        .map(|(index, question)| crate::graduation::GraduationEscalationQuestion {
            position: index as u32 + 1,
            question: question.question.clone(),
            options: question
                .options
                .iter()
                .map(|option| crate::graduation::GraduationProposedResponse {
                    answer: option.answer.clone(),
                    summary: option.summary.clone(),
                    description: option.description.clone(),
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    crate::graduation::validate_escalation_request(&escalation.reason, &questions).ok()?;
    Some((escalation.reason.clone(), questions))
}

/// GXD-FR-KXXB: one turn is one call through the dispatch seam.
async fn execute<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project_key: &str,
    attempt_id: &str,
    artifact: &MergeArtifact,
    input: MergeTaskInput,
    cancellation: CancellationToken,
    dispatch: &dyn GraduationDispatch<R>,
) -> Result<AgentExecution, AgentExecutionError> {
    let input_map = match serde_json::to_value(&input) {
        Ok(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    let request = AgentExecutionRequest {
        execution_directory: artifact.checkout.clone(),
        task: AgentTaskRequest {
            protocol_version: PROTOCOL_VERSION,
            instruction: REBASE_PROMPT.clone(),
            input: Some(input_map),
            // The turn answers with what it changed in the checkout, and the
            // checkout is what this application reads. No verdict shape applies.
            result_contract: None,
            resume: None,
            // GRL-FR-KWNP: the semantic rebase turn is a graduation dispatch
            // like any other, so its bounds are read from the project's
            // settings immediately before it is made.
            execution: ExecutionControls {
                cancellation: Cancellation::CallerControlled,
                timeout_ms: LoopSettings::read(app).execution_timeout_ms,
            },
        },
        // GRB-FR-TOOU: the mirrors, read-only, at the one container root the
        // executor allows. GRB-FR-VZHV: this kind is the masked one, so the
        // turn reaches no repository metadata of any kind.
        supplementary_mount: Some(SupplementaryMount::SemanticRebaseArtifact {
            host_path: artifact.mirrors(),
        }),
        turn_kind: TurnKind::SemanticRebase,
        // GRB-FR-TXVL: the update's own token, so `cancel_work_stream_update`
        // reaches the container this turn is running in.
        cancellation,
        // GXD-FR-IOZU: an update turn is agent work the author waits on, so what
        // it does reaches the same panel every other turn's work reaches.
        activity: update_activity_sink(app, attempt_id),
    };
    dispatch
        .dispatch_graduation_turn(app, project_key, request)
        .await
}

/// GXD-FR-IOZU: the activity sink one update turn reports through.
///
/// Bound to the attempt rather than to a run, because an update belongs to a
/// stream and to no run of it (GRB-FR-BQNF).
fn update_activity_sink<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    attempt_id: &str,
) -> Option<Arc<dyn crate::tools::agent_exec::AgentActivitySink>> {
    let store = app
        .try_state::<Arc<crate::agent_activity::ActivityStore>>()
        .map(|state| state.inner().clone())?;
    let fs = commands::store_fs(app).ok();
    Some(Arc::new(crate::agent_activity::RunActivitySink::new(
        app, attempt_id, store, fs,
    )))
}

