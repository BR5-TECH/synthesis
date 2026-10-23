//! Applying a pull run's result to the local branch (`GRD-graduation.md`
//! GRD-FR-PAPX, GRD-FR-PRBZ).
//!
//! A pull run applies on the terms of a merge run, with three differences: it
//! verifies its local branch alone, it lands through the safe landing of
//! `crate::git::land_on_branch` rather than a forced checkout, and for a rebase
//! it replays the local commits onto the pinned upstream tip, answering each
//! conflicting step from the tree the run settled.

use tauri::Manager;

use super::commands::project_root;
use super::merge_apply::{markers_in, tips_moved};
use super::update_git::{self, Resolutions};
use super::*;
use crate::git::{self as gtc, PullStrategy, Upstream};
use crate::graduation::GraduationMergeData;
use crate::log_fields;
use crate::logging::{self, Domain};

/// Apply the tree a pull run settled to its local branch.
///
/// The order is the order of a merge run's apply: the guard, the tip, the
/// marker check, the target, and only then a write.
pub fn apply_pull_run<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    data: &GraduationMergeData,
    tree: git2::Oid,
) -> Result<MergeApplied, MergeApplyFailure> {
    let Some(subject) = data.pull.as_ref() else {
        return Err(MergeApplyFailure::Failed("not a pull run".to_string()));
    };
    let refuse = |failure: MergeApplyFailure, why: &str| {
        logging::log_warn(
            app,
            &logging::BUFFER,
            &[Domain::Backend],
            "pull run apply refused",
            log_fields! { "branch" => subject.branch.clone(), "why" => why.to_string() },
        );
        failure
    };
    let failed = |reason: String| MergeApplyFailure::Failed(reason);
    let root = project_root(app).map_err(failed)?;
    let main = git::primary_repo(&root).map_err(failed)?;

    let state = app.try_state::<StreamState>();
    let _hold = match state.as_ref() {
        Some(state) => match state.begin_repository_update(
            &format!("pull:{}", subject.worktree_path),
            Reconciliation::Pull,
        ) {
            Ok(hold) => Some(hold),
            Err(reason) => return Err(refuse(MergeApplyFailure::GuardHeld(reason), "guard_held")),
        },
        None => None,
    };

    if tips_moved(&main, data) {
        return Err(refuse(MergeApplyFailure::BranchMoved, "branch_moved"));
    }
    let (Ok(local), Ok(remote)) =
        (git2::Oid::from_str(&data.base_tip), git2::Oid::from_str(&data.stream_tip))
    else {
        return Err(refuse(MergeApplyFailure::BranchMoved, "unreadable_tip"));
    };
    let leftover = markers_in(&main, tree, &data.unresolved_paths);
    if !leftover.is_empty() {
        return Err(refuse(MergeApplyFailure::MarkersRemaining(leftover), "markers_remaining"));
    }

    // GRD-FR-PAPX: the worktree the run was handed must still hold the branch.
    let holder = git2::Repository::open(&subject.worktree_path)
        .ok()
        .filter(|repo| gtc::current_branch(repo).as_deref() == Some(subject.branch.as_str()));
    let Some(holder) = holder else {
        return Err(refuse(MergeApplyFailure::TargetChanged, "target_changed"));
    };

    let up = Upstream {
        branch: subject.branch.clone(),
        upstream: subject.upstream.clone(),
        local,
        remote,
    };
    let tip = match subject.strategy {
        PullStrategy::Merge => {
            super::pull_reconcile::merge_commit(&main, &up, tree).map_err(failed)?
        }
        PullStrategy::Rebase => match rebased_tip(&main, &up, tree) {
            Ok(tip) => tip,
            Err(failure) => return Err(refuse(failure, "rebase_unsettled")),
        },
    };
    let landed = gtc::land_on_branch(&holder, &subject.branch, local, tip).map_err(|reason| {
        if reason.starts_with(gtc::ERR_PULL_WOULD_OVERWRITE) {
            refuse(MergeApplyFailure::DirtySide(reason), "dirty_side")
        } else {
            refuse(MergeApplyFailure::Failed(reason), "apply_failed")
        }
    })?;
    let merged_paths = semantic::written_paths(&main, local, tree);
    gtc::announce_changes(app, landed.len());
    logging::log_info(
        app,
        &logging::BUFFER,
        &[Domain::Backend],
        "pull run applied",
        log_fields! {
            "strategy" => subject.strategy.as_str(),
            "paths" => merged_paths.len() as i64,
            "landedPaths" => landed.len() as i64,
        },
    );
    Ok(MergeApplied { merged_paths, commit: Some(tip.to_string()) })
}

/// GRD-FR-PRBZ: the local commits replayed onto the upstream tip, each
/// conflicting step answered from `settled`. A chain whose tree does not hold
/// `settled` is refused rather than landed.
fn rebased_tip(
    main: &git2::Repository,
    up: &Upstream,
    settled: git2::Oid,
) -> Result<git2::Oid, MergeApplyFailure> {
    let failed = |reason: String| MergeApplyFailure::Failed(reason);
    let replay =
        update_git::replay(main, up.local, up.remote, &Resolutions::new()).map_err(failed)?;
    let landed =
        update_git::replay_settled_by(main, up.local, up.remote, settled, &replay.unresolved)
            .map_err(|reason| {
                if reason == update_git::REPLAY_UNSETTLED {
                    MergeApplyFailure::Unsettled(replay.unresolved.clone())
                } else {
                    MergeApplyFailure::Failed(reason)
                }
            })?;
    let dropped = update_git::content_differences(main, landed.tree, settled);
    if !dropped.is_empty() {
        return Err(MergeApplyFailure::Unsettled(dropped));
    }
    Ok(landed.tip)
}
