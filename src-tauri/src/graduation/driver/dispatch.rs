//! One turn's dispatch through the seam (`GXD-graduation-execution.md`
//! GXD-FR-KXXB, GXD-FR-IOZU).

use super::turns::*;
use super::*;
use crate::graduation::logs::{self, GraduationLogProducer};
use std::path::Path;

/// One turn to dispatch: where it runs, which part of the loop it is, its
/// input, and the timeout read for this dispatch.
pub(super) struct TurnRequest<'a> {
    pub execution_directory: &'a Path,
    pub part: &'a str,
    pub input: task_input::GraduationTaskInput,
    pub bound: settings::TurnBound,
    /// GRL-FR-QKDC: the session a correction turn resumes, in place of the
    /// one the run's checkpoint names.
    pub resume: Option<SessionRef>,
}

/// GXD-FR-KXXB: one turn is one call through the dispatch seam.
pub(super) async fn execute<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    turn: TurnRequest<'_>,
    cancellation: &CancellationToken,
    dispatch: &dyn seams::GraduationDispatch<R>,
) -> Result<AgentExecution, AgentExecutionError> {
    let TurnRequest { execution_directory, part, input, bound, resume } = turn;
    let task = AgentTaskRequest {
        protocol_version: PROTOCOL_VERSION,
        // GRL-FR-LCSB: a correction turn announces no milestones.
        instruction: if input.correction.is_some() {
            prompts::without_milestones(&prompts::instruction_text_for_run(run, part))
        } else {
            prompts::instruction_text_for_run(run, part)
        },
        input: Some(input.to_map()),
        // GRL-FR-VIAT / GRL-FR-UNKD / GRL-FR-IJTT: every turn of the loop
        // names the shape its own answer must take. The work part is a
        // `merge_work` turn in a merge run.
        result_contract: match part {
            phases::PART_REVIEW => Some(ResultContract::ReviewVerdict),
            phases::PART_PR_SUMMARY => Some(ResultContract::PullRequestSummary),
            phases::PART_WORK => Some(ResultContract::WorkResult),
            _ => None,
        },
        resume: resume.or_else(|| resume_for(run, part)),
        // GXD-FR-MTVR: the controls are composed from the settings read for
        // this dispatch, and nothing of an earlier turn's controls is reused.
        execution: ExecutionControls {
            cancellation: Cancellation::CallerControlled,
            timeout_ms: bound.limit_ms,
        },
    };
    let request = AgentExecutionRequest {
        // GXD-FR-SHNW: the run's queue — its stream, or the pinned worktree of
        // a direct run outside a stream — is the scope whose VM it shares.
        sandbox_scope: Some(crate::tools::agent_exec::sandbox::SandboxScope {
            queue_key: run.queue_key(),
        }),
        execution_directory: execution_directory.to_path_buf(),
        task,
        supplementary_mount: None,
        turn_kind: task_input::turn_kind_for_run(run, part),
        cancellation: cancellation.clone(),
        // GXD-FR-IOZU: what the agent does reaches the run it is doing it for.
        // A turn with no sink is a run the author watches an empty panel for.
        activity: activity_sink(app, run),
        // GXD-FR-IOZU / GLG-FR-QKVI: the record of what the agent did, which
        // the executor must deliver durably and acknowledged.
        durable_output: Some(durable_sink(app, run, part)),
    };
    // GXD-FR-CYIW / GRD-FR-NHRY: the interval is the turn's own execution, from
    // the moment it is dispatched to the moment it returns. Held on this stack,
    // so it can span no queue wait and no author wait.
    let recorder = crate::graduation::statistics::OperationRecorder::begin(
        crate::graduation::statistics::bucket_for_part(part),
    );
    // GLG-FR-QNLC: the dispatch of each merge turn is a record of the run.
    if run.is_merge() {
        crate::graduation::log_merge_boundary(
            app,
            run,
            crate::graduation::MergeBoundary {
                level: crate::logging::LogLevel::Info,
                domains: &[Domain::Ai, Domain::Backend],
                message: "graduation dispatched a merge turn",
                boundary: "dispatch",
                outcome: "dispatched",
                pass: Some(run.pass()),
                paths: None,
                extra: log_fields! { "turn_kind" => request.turn_kind.as_str() },
            },
        );
    }
    let started = std::time::Instant::now();
    let outcome = dispatch.dispatch_graduation_turn(app, &run.project_key, request).await;
    // GRD-FR-UTTX: the turn's execution time counts against the run's total.
    time_budget::charge(run, started, &outcome, bound);
    recorder.finish(app, run);
    // OPU-FR-RFFH: a turn that may not give a plan update has one refused.
    if super::plan_turns::admits_plan_update(run, part) {
        return outcome;
    }
    outcome.map(crate::graduation::plan_updates::refuse_plan_update)
}

/// GXD-FR-IOZU: the live activity sink one turn of this run reports through.
///
/// `None` where the application holds no activity store, which costs the live
/// panel and nothing else: no turn is refused over it.
fn activity_sink<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
) -> Option<Arc<dyn crate::tools::agent_exec::AgentActivitySink>> {
    // AGV: the live panel, which is bounded and evicted.
    let store = app
        .try_state::<Arc<crate::agent_activity::ActivityStore>>()
        .map(|state| state.inner().clone())?;
    let fs = crate::graduation::store_fs(app).ok();
    Some(Arc::new(crate::agent_activity::RunActivitySink::new(app, &run.id, store, fs)))
}

/// GXD-FR-IOZU / GLG-FR-QKVI: the durable activity sink one turn of this run
/// reports through.
///
/// GRS-FR-JQOO: the record is retained for the lifetime of the run record and
/// is what the author reads a finished run back from.
fn durable_sink<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
    part: &str,
) -> Arc<dyn crate::tools::agent_exec::DurableOutputSink> {
    Arc::new(logs::GraduationActivitySink::new(app, run, source_producer(part)))
}

/// GRS-FR-KJVN: which producer a turn's activity is attributed to.
fn source_producer(part: &str) -> GraduationLogProducer {
    match part {
        phases::PART_REVIEW => GraduationLogProducer::ReviewTurn,
        phases::PART_SEMANTIC_MERGE => GraduationLogProducer::SemanticMergeTurn,
        _ => GraduationLogProducer::WorkTurn,
    }
}
