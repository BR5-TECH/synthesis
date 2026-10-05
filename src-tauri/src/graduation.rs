//! Graduation (`GRD-graduation.md`).
//!
//! The backend module that turns a finished draft prompt into committed work. A
//! draft is refined in conversation until its author judges the prompt ready;
//! **graduation** is what happens next — the prompt is captured, an execution
//! agent is driven against a work stream's working copy until it reports the
//! work finished, a fresh agent session reviews what it wrote, and the
//! application commits the result onto the stream's branch.
//!
//! The application enforces no shape on the work itself: what a change must
//! contain is what the project's own instructions and skills say, read by the
//! agent in the repository it stands in (GRL-FR-DAIB).
//!
//! ## Why a durable queued run rather than a modal
//!
//! Authoring the next prompt, reading an already-committed change, and starting
//! another run all continue while one is working (GRD-FR-QJHM). That is only
//! true if the work outlives the surface that started it, so everything about a
//! run is on disk before it is useful (GSU-FR-RJRF) and a relaunch finds an
//! interrupted run to continue rather than nothing at all.
//!
//! ## What this module does not do
//!
//! It says nothing to a model and takes no round — that is
//! `graduation::driver` (`../ai/GRL-graduation-loop.md`). It launches no
//! container — that is `crate::tools::agent_exec`
//! (`../tools/EAC-execute-agent-cli.md`), which this module supplies no vendor,
//! model, effort, credential, or container argument to (GXD-FR-KXXB). It owns
//! no branch and no working copy — those are `crate::streams`'
//! (`WKS-work-streams.md`) — and it merges nothing into a base branch, which is
//! `GRB-graduation-rebase.md`'s and happens at the author's request alone.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use crate::fs::{self as fsa};
use crate::log_fields;
use crate::logging::{self, Domain, BUFFER};
use crate::project::ProjectState;

pub mod driver;
pub mod logs;
pub mod observability;
pub mod statistics;

mod arrangement;
mod commands;
mod commit;
mod direct;
mod errors;
mod events;
mod merge_run;
mod queue;
mod record;
mod run_progress;
mod runs;
mod scheduler;
mod standing;
mod state;
mod store;
mod transitions;


pub use arrangement::*;
pub use commands::*;
pub use commit::*;
pub use direct::*;
pub use errors::*;
pub use events::{
    GRADUATION_LOG_RECORDS_APPENDED, GRADUATION_QUEUE_CHANGED, GRADUATION_RUN_CHANGED,
};
pub use observability::{
    GraduationObservability, GraduationStageCondition, GraduationVisualStage, PassRecord,
    PassStatus, StageReason, StageTransition,
};
pub use queue::{
    draft_graduation, draft_graduation_stands, latest_run_for, load_queue, merge_runs_by_stream,
    next_eligible,
    position_in_queue, project_queue, queue_of, queued_counts_by_stream, require_no_running_graduation,
    require_unlocked_draft,
    streams_with_runs,
};
pub use record::*;
pub use scheduler::*;
pub use runs::{
    load_run, record_checkpoint, record_escalation, save_run, validate_escalation_request,
};
pub use logs::{GraduationLogIndexes, GraduationLogStream};
pub use merge_run::{
    enqueue_merge_run, merge_title, reclaim_ended_merge_run, GraduationMergeData, GraduationMergeResult, MergePublished,
    MergeRunRequest, MERGE_PASS_BUDGET,
};
pub(crate) use merge_run::{log_merge_boundary, MergeBoundary};
pub use state::GraduationState;
pub use store::{
    new_run_id, read_queue_index, store_base, store_fs, write_queue_index, QueueIndex,
    QueueIndexEntry, StoreBase,
};
pub use run_progress::{end_with_loop as end_run_progress, PROGRESS_KIND_GRADUATION};
/// The sync of a run's progress operation, for a test that holds a stale copy.
#[cfg(test)]
pub(crate) fn run_progress_sync_for_test<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
) {
    run_progress::sync(app, run);
}
pub use transitions::{block, enter_stage, interrupt, transition};
// ---------------------------------------------------------------------------
// Lifecycle (GRD-FR-TWMA)
// ---------------------------------------------------------------------------

/// GRD-FR-TWMA: a project change and an application shutdown stop every running
/// turn.
///
/// **A change of active worktree stops nothing**: a run works in its stream's
/// working copy, which the author's active worktree is not.
pub fn interrupt_running<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    reason: GraduationInterruptionReason,
) {
    let Some(state) = app.try_state::<GraduationState>() else {
        return;
    };
    let Some(queue) = project_queue(app) else {
        state.cancel_all(reason);
        return;
    };
    state.cancel_all(reason);
    for run in queue.runs {
        // GRD-FR-TWMA: a stop reaches a running turn. A `blocked` run holds its
        // stream but runs no turn, so it rests as it stands (GRD-FR-XVUD).
        if !matches!(
            run.state,
            GraduationRunState::Working | GraduationRunState::Reviewing
        ) {
            continue;
        }
        let Ok(mut run) = load_run(app, &run.id) else {
            continue;
        };
        // GXD-FR-TJRV: a run stopped in the review resumes at the review.
        if run.state == GraduationRunState::Reviewing {
            run.checkpoint.resume_part = Some(driver::phases::PART_REVIEW.to_string());
        }
        // Nothing the turn wrote is undone: what it left in the stream is
        // committed as an abandoned turn when the run is next continued
        // (GRD-FR-SWOJ).
        let _ = transitions::interrupt(app, &mut run, reason, "The application stopped the run.");
    }
}

/// GRD-FR-EGWS / WKS-FR-RQVM: a stream a merge or an update held is free again,
/// so the earliest eligible run waiting on it starts.
///
/// A run whose claim that reconciliation refused stays queued, and nothing but
/// this call dispatches it once the stream is free.
pub fn advance_stream_queue<R: tauri::Runtime>(app: &tauri::AppHandle<R>, stream_id: &str) {
    logging::log_debug(
        app,
        &BUFFER,
        &[Domain::Backend],
        "graduation offered a freed stream's queue a dispatch",
        log_fields! { "stream_id" => stream_id.to_string() },
    );
    scheduler::dispatch_pending(app);
}

/// GRD-FR-XVUD: on the first read of a project's queue after launch, a run the
/// store holds as working with no loop behind it is marked abandoned.
///
/// The stream it held is released with it, so a stream nothing will ever release
/// is not left busy for good (WKS-FR-LWEI).
pub fn sweep_abandoned_runs<R: tauri::Runtime>(app: &tauri::AppHandle<R>, queue: &GraduationQueue) {
    let Some(state) = app.try_state::<GraduationState>() else {
        return;
    };
    // GRL-FR-YKRI: a review checkout the application never finished is reclaimed
    // here too, for the same reason and at the same moment.
    driver::review_checkout::sweep_stranded(app, queue);
    // GRD-FR-PFMD: a merge run that ended has what it owns removed. A removal
    // that fails is logged and does not stop the read.
    for run in &queue.runs {
        if run.is_merge() && run.state.is_terminal() && driver::merge_workspace::stands(app, run) {
            reclaim_ended_merge_run(app, run);
        }
    }
    for summary in &queue.runs {
        // GRD-FR-XVUD: `working` and `reviewing` alone. A `blocked` run also
        // holds its stream, but it is resting on a condition the author clears
        // rather than on a loop that has gone, so a relaunch leaves it blocked.
        if !matches!(
            summary.state,
            GraduationRunState::Working | GraduationRunState::Reviewing
        ) || state.is_driving(&summary.id)
        {
            continue;
        }
        let Ok(mut run) = load_run(app, &summary.id) else {
            continue;
        };
        logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation found a run the application had stopped without stopping",
            log_fields! {
                "run_id" => run.id.clone(),
                "stream_id" => run.stream_id.clone(),
            },
        );
        // GRS-FR-IOHF: the record was saved at the last turn boundary, and the
        // files hold what the stopped turn wrote after it.
        logs::reconcile_saved(app, &mut run);
        let _ = transitions::interrupt(
            app,
            &mut run,
            GraduationInterruptionReason::ExecutionAbandoned,
            "The application stopped while this run was working.",
        );
    }
}

// The module's own tests. Declared last so the production half of this file
// is the whole of it, which is what the filesystem-access guard sweeps.
#[cfg(test)]
mod tests;
