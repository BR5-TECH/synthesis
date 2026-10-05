//! Applying a merge run's result to the base branch
//! (`GRB-graduation-rebase.md`, `WKS-work-streams.md`).
//!
//! A merge run reconciles in a worktree of its own. Nothing of that work reaches
//! the base branch until a review has let it through, and then only here, under
//! the repository update guard, after both pinned tips are verified again. Every
//! refusal writes nothing to either branch and nothing to either working copy.

use tauri::Manager;

use super::commands::{project_root, store, store_fs};
use super::*;
use crate::graduation::GraduationMergeData;
use crate::log_fields;
use crate::logging::{self, Domain};

/// What stood in the way of an apply, each one leaving every tree as it was.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeApplyFailure {
    /// Either pinned tip is not the tip of its branch any more.
    BranchMoved,
    /// A conflict marker still stands in these unresolved paths.
    MarkersRemaining(Vec<String>),
    /// An update or another merge check holds the repository update guard.
    GuardHeld(String),
    /// A side holds uncommitted work, or the stream's working copy is gone. The
    /// typed refusal, with its paths, is the text.
    DirtySide(String),
    /// Git could not write the result. The typed text of the failure.
    Failed(String),
}

/// What an apply wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergeApplied {
    /// Project-relative paths the result changes against the base tip.
    pub merged_paths: Vec<String>,
    /// The merge commit, where the publication made one.
    pub commit: Option<String>,
}

/// Whether either pinned tip of a merge run has left its branch.
///
/// A branch that cannot be read is a branch that moved: nothing can be applied
/// to it. Read-only and writes nothing.
pub fn merge_tips_moved<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    data: &GraduationMergeData,
) -> bool {
    let Ok(root) = project_root(app) else {
        return true;
    };
    let Ok(main) = git::primary_repo(&root) else {
        return true;
    };
    tips_moved(&main, data)
}

fn tips_moved(main: &git2::Repository, data: &GraduationMergeData) -> bool {
    let pinned = |revision: &str| git2::Oid::from_str(revision).ok();
    let (Some(base), Some(stream)) = (pinned(&data.base_tip), pinned(&data.stream_tip)) else {
        return true;
    };
    let now_base = merge::branch_tip(main, &data.base_branch);
    let now_stream = merge::branch_tip(main, &data.stream_branch);
    !(now_base == Ok(base) && now_stream == Ok(stream))
}

/// Apply the tree a merge run settled to the base branch, on the publication the
/// run recorded.
///
/// The order is the order of cost and of reversibility: the guard first, then the
/// tips, then the marker check, then the clean check, and only then a write.
pub fn apply_merge_run<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    data: &GraduationMergeData,
    tree: git2::Oid,
) -> Result<MergeApplied, MergeApplyFailure> {
    let buffer = &crate::logging::BUFFER;
    let refuse = |failure: MergeApplyFailure, why: &str| {
        logging::log_warn(
            app,
            buffer,
            &[Domain::Backend],
            "work stream merge apply refused",
            log_fields! { "stream_id" => stream_id.to_string(), "why" => why.to_string() },
        );
        failure
    };
    let failed = |reason: String| MergeApplyFailure::Failed(reason);

    let root = project_root(app).map_err(failed)?;
    let main = git::primary_repo(&root).map_err(failed)?;
    let fs = store_fs(app).map_err(failed)?;
    let store = store(app).map_err(failed)?;
    let stream = store::read_stream(&fs, &store, stream_id)
        .ok_or_else(|| MergeApplyFailure::Failed(ERR_UNKNOWN_STREAM.to_string()))?;

    // WKS-FR-HLGN: the guard is held across the tip check and the write that
    // follows it, so the base branch cannot advance between them.
    let state = app.try_state::<StreamState>();
    let _hold = match state.as_ref() {
        Some(state) => match state.begin_repository_update(stream_id, Reconciliation::Merge) {
            Ok(hold) => Some(hold),
            Err(reason) => return Err(refuse(MergeApplyFailure::GuardHeld(reason), "guard_held")),
        },
        None => None,
    };

    if tips_moved(&main, data) {
        return Err(refuse(MergeApplyFailure::BranchMoved, "branch_moved"));
    }
    let (Ok(base_oid), Ok(stream_oid)) = (
        git2::Oid::from_str(&data.base_tip),
        git2::Oid::from_str(&data.stream_tip),
    ) else {
        return Err(refuse(MergeApplyFailure::BranchMoved, "unreadable_tip"));
    };

    // The review judged a tree it was handed; a marker the review missed in a
    // path Git could not merge is the one thing that must never reach the base
    // branch.
    let leftover = markers_in(&main, tree, &data.unresolved_paths);
    if !leftover.is_empty() {
        return Err(refuse(
            MergeApplyFailure::MarkersRemaining(leftover),
            "markers_remaining",
        ));
    }

    let base_repo = git::worktree_holding(&main, &data.base_branch);
    let base_worktree = base_repo
        .as_ref()
        .and_then(|repo| repo.workdir().map(std::path::PathBuf::from));
    if let Some(base_worktree) = base_worktree.as_deref() {
        crate::storage_floor::save::save_drafts_before_checkout(&fs, base_worktree)
            .map_err(|reason| refuse(MergeApplyFailure::Failed(reason), "draft_save_failed"))?;
    }
    // WKS-FR-UZHT: the base worktree is the author's own, and a forced checkout
    // over work they did while the run reconciled would lose it.
    if let Err(reason) = merge::refuse_dirty(&stream.worktree(), base_worktree.as_deref()) {
        return Err(refuse(MergeApplyFailure::DirtySide(reason), "dirty_side"));
    }

    let commit = merge::apply_tree(
        &main,
        base_repo.as_ref(),
        &data.base_branch,
        base_oid,
        stream_oid,
        tree,
        &data.publication,
    )
    .map_err(|reason| refuse(MergeApplyFailure::Failed(reason), "apply_failed"))?;
    let merged_paths = semantic::written_paths(&main, base_oid, tree);
    logging::log_info(
        app,
        buffer,
        &[Domain::Backend],
        "work stream merge applied",
        log_fields! {
            "stream_id" => stream_id.to_string(),
            "paths" => merged_paths.len() as i64,
            "committed" => commit.is_some(),
        },
    );
    Ok(MergeApplied {
        merged_paths,
        commit,
    })
}

/// The paths of `paths` that hold a conflict marker in `tree`.
fn markers_in(main: &git2::Repository, tree: git2::Oid, paths: &[String]) -> Vec<String> {
    let Ok(tree) = main.find_tree(tree) else {
        return Vec::new();
    };
    let mut left: Vec<String> = Vec::new();
    for path in paths {
        let Ok(entry) = tree.get_path(std::path::Path::new(path)) else {
            continue;
        };
        let Ok(blob) = main.find_blob(entry.id()) else {
            continue;
        };
        if semantic::holds_marker(blob.content()) {
            left.push(path.clone());
        }
    }
    left.sort();
    left.dedup();
    left
}
