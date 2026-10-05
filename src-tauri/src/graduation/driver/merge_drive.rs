//! The parts of the loop that belong to a merge run alone
//! (`GRD-graduation.md` GRD-FR-XHSE, GRD-FR-AQNW, GRD-FR-JSBE).
//!
//! A merge run drives the same loop as every run: a work turn, a review turn and
//! a bounded number of passes. What differs is where it stands and what a `ready`
//! review leads to. It stands in a worktree of its own, seeded from the merge
//! snapshot, and a `ready` review leads to the **apply** of the merge onto the
//! base branch, which is guarded by the pinned tips and by the repository update
//! guard. This module holds those differences so `drive.rs` reads as the loop.

use std::path::PathBuf;

use super::drive::Step;
use super::turns::note_blocked;
use super::*;
use crate::graduation::logs::{self, GraduationLogProducer, StructuredEvent};
use crate::graduation::{
    commit, log_merge_boundary, GraduationFailure, GraduationMergeResult, MergeBoundary,
    MergePublished, ReviewFinding,
};
use crate::logging::LogLevel;
use crate::streams::{MergeApplyFailure, StreamMergePublication, ERR_MERGE_BRANCH_MOVED};

/// GRD-FR-XHSE: whether either pinned tip of a merge run has left its branch.
pub(super) fn tips_moved<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &GraduationRun) -> bool {
    run.merge
        .as_ref()
        .is_some_and(|data| crate::streams::merge_tips_moved(app, data))
}

/// GRD-FR-XHSE: the check before a dispatch.
///
/// Answers `true` when the run was ended by it, so the caller stops. A run that
/// is not a merge run is never ended here.
pub(super) fn moved<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &mut GraduationRun) -> bool {
    if !run.is_merge() {
        return false;
    }
    if !tips_moved(app, run) {
        // GLG-FR-QNLC: the check that found both tips standing is a record too.
        log_merge_boundary(
            app,
            run,
            MergeBoundary {
                level: LogLevel::Info,
                domains: &[Domain::Ai, Domain::Backend],
                message: "graduation checked the pinned tips of a merge run",
                boundary: "tip_check",
                outcome: "ok",
                pass: Some(run.pass()),
                paths: None,
                extra: crate::logging::Fields::new(),
            },
        );
        return false;
    }
    fail_branch_moved(app, run, "dispatch");
    true
}

/// GRD-FR-XHSE: end the run on a moved tip. No branch and no worktree is
/// written, and nothing of the run's own is reclaimed until the loop has
/// returned.
pub(super) fn fail_branch_moved<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    boundary: &str,
) {
    // GLG-FR-QNLC: a refused tip check is a record of level `WARN`.
    log_merge_boundary(
        app,
        run,
        MergeBoundary {
            level: LogLevel::Warn,
            domains: &[Domain::Ai, Domain::Backend],
            message: "graduation ended a merge run: a pinned tip moved",
            boundary: "tip_check",
            outcome: ERR_MERGE_BRANCH_MOVED,
            pass: Some(run.pass()),
            paths: None,
            extra: log_fields! { "checked_at" => boundary },
        },
    );
    run.failure = Some(GraduationFailure {
        code: ERR_MERGE_BRANCH_MOVED.to_string(),
        message: "A branch this merge was pinned to has moved. Nothing was written. Start Merge again from the work stream."
            .to_string(),
        retryable: false,
    });
    let _ = transitions::enter_stage(
        app,
        run,
        observability::GraduationVisualStage::Done,
        observability::StageReason::Ended,
    );
    let _ = transitions::transition(app, run, GraduationRunState::Failed);
}

/// GRD-FR-KZPT: what a merge run needs before its first turn of a dispatch: its
/// pinned tips still standing, its worktree, and its base commit.
///
/// `None` where the run came to rest instead, having recorded why.
pub(super) fn prepare<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
) -> Option<PathBuf> {
    // GRD-FR-AQNW: a merge that reached the base branch and was recorded as
    // applied is a completed run, whatever stopped the loop before the state
    // moved. Applying it again would find its own result in the base worktree.
    if run.merge.as_ref().is_some_and(|data| data.result.is_some()) {
        let _ = transitions::enter_stage(
            app,
            run,
            observability::GraduationVisualStage::Done,
            observability::StageReason::Finished,
        );
        let _ = transitions::transition(app, run, GraduationRunState::Completed);
        return None;
    }
    if moved(app, run) {
        return None;
    }
    let worktree = match merge_workspace::ensure(app, run) {
        Ok(path) => path,
        Err(reason) => {
            // GXD-FR-TJRV: the phase the run stopped in stays the phase it
            // resumes at.
            let resume_part = run
                .checkpoint
                .resume_part
                .clone()
                .unwrap_or_else(|| phases::PART_WORK.to_string());
            log_merge_boundary(
                app,
                run,
                MergeBoundary {
                    level: LogLevel::Error,
                    domains: &[Domain::Backend],
                    message: "graduation could not prepare a merge worktree",
                    boundary: "dispatch",
                    outcome: "merge_worktree_failed",
                    pass: Some(run.pass()),
                    paths: None,
                    extra: log_fields! { "reason" => reason.clone() },
                },
            );
            note_blocked(app, run, "merge_worktree_failed");
            let _ = transitions::block(
                app,
                run,
                "merge_worktree_failed",
                reason,
                "Continue the run to prepare the merge worktree again.",
                &resume_part,
            );
            return None;
        }
    };
    // GRD-FR-YBUM / GRD-FR-KZPT: every change set of the run is measured from the
    // merge snapshot.
    if run.base_commit.is_none() {
        run.base_commit = run.merge.as_ref().map(|data| data.snapshot_commit.clone());
        let _ = crate::graduation::save_run(app, run);
    }
    Some(worktree)
}

/// GRD-FR-JSBE: the apply, resumed alone.
///
/// The run stands in the review stage while it applies, so the stage and the
/// state say so before anything is attempted.
pub(super) fn apply_again<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &PathBuf,
) -> Step {
    if moved(app, run) {
        return Step::Stop;
    }
    let _ = transitions::enter_stage(
        app,
        run,
        observability::GraduationVisualStage::Review,
        observability::StageReason::ReviewStarted,
    );
    if transitions::transition(app, run, GraduationRunState::Reviewing).is_err() {
        return Step::Stop;
    }
    run.checkpoint.resume_part = None;
    let _ = crate::graduation::save_run(app, run);
    finish_merge(app, run, worktree)
}

/// GRD-FR-AQNW: a `ready` review leads to the apply of the merge.
///
/// The tree applied is the tree the review stood in: the one derivation of the
/// run's change set over the merge snapshot (GRL-FR-XNQU).
pub(super) fn finish_merge<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &PathBuf,
) -> Step {
    let Some(data) = run.merge.clone() else {
        return Step::Stop;
    };
    // A discard or a pause that landed between the verdict and the apply stands:
    // the store answers, not the copy in hand.
    if crate::graduation::load_run(app, &run.id).is_ok_and(|stored| {
        stored.state.is_terminal() || stored.state == GraduationRunState::Interrupted
    }) {
        return Step::Stop;
    }
    let tree = match derived_tree(worktree, &data.snapshot_commit) {
        Ok(tree) => tree,
        Err(reason) => return block_apply(app, run, "merge_apply_failed", reason),
    };

    match crate::streams::apply_merge_run(app, &run.stream_id, &data, tree) {
        Ok(applied) => {
            run.merge = run.merge.take().map(|mut merge| {
                merge.result = Some(GraduationMergeResult {
                    published: match merge.publication {
                        StreamMergePublication::Uncommitted => MergePublished::Uncommitted,
                        StreamMergePublication::Commit { .. } => MergePublished::Commit,
                    },
                    commit: applied.commit.clone(),
                    merged_paths: applied.merged_paths.clone(),
                });
                merge
            });
            // A merge commit is recorded in the run's `commits`.
            if let Some(commit) = applied.commit.clone() {
                run.commits.push(commit);
            }
            run.checkpoint.resume_part = None;
            transitions::clear_block_count(run);
            // The result is durable before anything that can fail is attempted:
            // the merge has reached the base branch, and a loop that stopped now
            // completes the run rather than applying the merge a second time.
            let _ = crate::graduation::save_run(app, run);
            let _ = transitions::enter_stage(
                app,
                run,
                observability::GraduationVisualStage::Done,
                observability::StageReason::Finished,
            );
            let _ = transitions::transition(app, run, GraduationRunState::Completed);
            log_merge_boundary(
                app,
                run,
                MergeBoundary {
                    level: LogLevel::Info,
                    domains: &[Domain::Ai, Domain::Backend],
                    message: "graduation applied a merge run",
                    boundary: "apply",
                    outcome: "applied",
                    pass: None,
                    paths: Some(applied.merged_paths.len()),
                    extra: log_fields! { "committed" => applied.commit.is_some() },
                },
            );
            // GRS-FR-WNRC: a record that cannot be written leaves the run as the
            // state machine already has it, which is completed.
            let applied_record = StructuredEvent::new(GraduationLogProducer::Commit, "merge_applied")
                .with("paths", applied.merged_paths.len() as u64)
                .with("committed", applied.commit.is_some());
            let _ = logs::emit(app, run, applied_record);
            Step::Stop
        }
        Err(MergeApplyFailure::BranchMoved) => {
            fail_branch_moved(app, run, "apply");
            Step::Stop
        }
        // GRL-FR-NDHW: the review let through a result that still holds a marker.
        // The verdict is replaced by one that asks for the paths to be settled,
        // and the loop treats it as any revision of a merge review.
        Err(MergeApplyFailure::MarkersRemaining(paths)) => {
            let verdict = ReviewVerdict {
                verdict: ReviewOutcome::Revise,
                rationale: "A conflict marker still stands in a path Git could not merge, so the merge was not applied."
                    .to_string(),
                findings: vec![ReviewFinding {
                    severity: ReviewSeverity::Critical,
                    description: format!(
                        "A conflict marker still stands in {}.",
                        paths.join(", ")
                    ),
                    affected_files: paths,
                    correction: "Settle each of these paths so that one version stands and no conflict marker remains."
                        .to_string(),
                }],
            };
            let _ = logs::emit(
                app,
                run,
                StructuredEvent::new(GraduationLogProducer::ReviewTurn, "merge review replaced")
                    .at_level(logs::GraduationLogLevel::Warn)
                    .with("findings", 1u64),
            );
            super::drive::revise_or_rest(app, run, &verdict)
        }
        Err(MergeApplyFailure::GuardHeld(reason)) => {
            block_apply(app, run, "merge_guard_held", reason)
        }
        Err(MergeApplyFailure::DirtySide(reason)) => {
            block_apply(app, run, "merge_dirty_side", reason)
        }
        Err(MergeApplyFailure::Failed(reason)) => {
            block_apply(app, run, "merge_apply_failed", reason)
        }
    }
}

/// GRD-FR-JSBE: rest the run `blocked` at the apply. Nothing was written.
fn block_apply<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    code: &str,
    reason: String,
) -> Step {
    note_blocked(app, run, code);
    // GLG-FR-QNLC: an apply that could not proceed is a record of the apply, and
    // one that failed to write is an error. The reason is a typed refusal that
    // can name paths, and never names content.
    log_merge_boundary(
        app,
        run,
        MergeBoundary {
            level: if code == "merge_apply_failed" {
                LogLevel::Error
            } else {
                LogLevel::Warn
            },
            domains: &[Domain::Ai, Domain::Backend],
            message: "graduation could not apply a merge run",
            boundary: "apply",
            outcome: code,
            pass: None,
            paths: None,
            extra: log_fields! { "reason" => reason.clone() },
        },
    );
    let clears_by = match code {
        "merge_dirty_side" => {
            "Commit or discard the uncommitted work in the named worktree, then continue the merge."
        }
        "merge_guard_held" => "Wait for the update or merge that is running, then continue the merge.",
        _ => "Resolve what stopped the apply, then continue the merge.",
    };
    let _ = transitions::block(app, run, code, reason, clears_by, phases::PART_APPLY);
    Step::Stop
}

/// The tree the merge worktree settled: the snapshot's tree with the run's
/// change set over it.
fn derived_tree(worktree: &PathBuf, snapshot_commit: &str) -> Result<git2::Oid, String> {
    match commit::run_tree(worktree, snapshot_commit)? {
        Some(derived) => Ok(derived.tree),
        None => {
            let repo = git2::Repository::open(worktree).map_err(|e| e.to_string())?;
            let oid = git2::Oid::from_str(snapshot_commit).map_err(|e| e.to_string())?;
            repo.find_commit(oid)
                .map(|commit| commit.tree_id())
                .map_err(|e| e.to_string())
        }
    }
}
