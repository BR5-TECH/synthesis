//! The throwaway checkout a review turn stands in (GRL-FR-YKRI).
//!
//! The review must run the project's own build and test commands, and those
//! commands write files. A reviewer that shared the stream's working copy could
//! not be told apart from one that tampered with it, so the turn is given a
//! checkout of its own that is removed when the turn ends.
//!
//! What the checkout holds is the run's **change set**, standing on the run's
//! base commit as uncommitted work. That is the same tree the commit of a
//! `ready` verdict writes, so the reviewer judges what will land rather than
//! something near it.

use std::path::{Path, PathBuf};

use super::*;

/// The branch a review checkout is made on, for one run.
///
/// A worktree needs a reference to be created from. A scratch branch of this
/// module's own is what keeps the review off the stream's branch, so nothing
/// the turn does can move the stream.
fn scratch_branch(run_id: &str) -> String {
    format!("synthesis/review/{run_id}")
}

/// The registration Git holds the review checkout under.
fn registration(run_id: &str) -> String {
    format!("{run_id}-rv")
}

/// GRL-FR-YKRI: a checkout holding the run's change set, for one review turn.
///
/// It stands detached at the run's base commit and the change set is written
/// over it, so `git diff HEAD` inside the checkout names exactly what this run
/// changed. Nothing of the stream is written — not even its index.
pub(crate) fn create<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
    worktree: &Path,
) -> Result<PathBuf, String> {
    let base = run
        .base_commit
        .as_deref()
        .ok_or_else(|| "the run has no base commit to review against".to_string())?;
    let base_oid = git2::Oid::from_str(base).map_err(|e| e.to_string())?;
    let target = crate::graduation::store_base(app)?.review_checkout(&run.id);
    let fs = crate::graduation::store_fs(app)?;
    let main = crate::streams::primary_repo_of(worktree)?;

    // An earlier turn of this run stood here. Reclaim whatever it left before
    // asking for the name again, or Git refuses the registration.
    release(app, run, worktree)?;
    if let Some(parent) = target.parent() {
        if !parent.is_dir() {
            fs.create_dir(parent).map_err(|e| e.to_string())?;
        }
    }

    // The tree the review reads, and the tree a `ready` verdict commits. One
    // derivation answers both, so they cannot drift.
    let tree = crate::graduation::commit::run_tree(worktree, base)?.map(|derived| derived.tree);

    let base_commit = main.find_commit(base_oid).map_err(|e| e.to_string())?;
    let branch = scratch_branch(&run.id);
    let name = registration(&run.id);
    main.branch(&branch, &base_commit, true)
        .map_err(|e| e.to_string())?;
    let reference = main
        .find_branch(&branch, git2::BranchType::Local)
        .map_err(|e| e.to_string())?
        .into_reference();
    let mut opts = git2::WorktreeAddOptions::new();
    opts.reference(Some(&reference));
    main.worktree(&name, &target, Some(&opts))
        .map_err(|e| e.to_string())?;
    crate::worktree::normalize_worktree_commondir(&main, &name).map_err(|e| e.to_string())?;

    let checkout = git2::Repository::open(&target).map_err(|e| e.to_string())?;
    // Detached: the review reads one revision and owns no branch, so nothing it
    // writes can move the scratch branch either.
    checkout
        .set_head_detached(base_oid)
        .map_err(|e| e.to_string())?;
    let mut builder = git2::build::CheckoutBuilder::new();
    builder.force();
    checkout
        .checkout_head(Some(&mut builder))
        .map_err(|e| e.to_string())?;

    // GRL-FR-YKRI: the change set is written over the base as uncommitted work.
    // The index is deliberately left at the base, so what the run changed reads
    // back as the uncommitted change the reviewer is asked to judge.
    if let Some(tree) = tree {
        let tree = checkout.find_tree(tree).map_err(|e| e.to_string())?;
        let mut builder = git2::build::CheckoutBuilder::new();
        builder.force().remove_untracked(true).update_index(false);
        checkout
            .checkout_tree(tree.as_object(), Some(&mut builder))
            .map_err(|e| e.to_string())?;
    }
    Ok(target)
}

/// Remove the checkout, its registration and its scratch branch.
///
/// Every step is idempotent, so this is also what a later turn calls before it
/// asks for the name again.
pub(super) fn release<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
    worktree: &Path,
) -> Result<(), String> {
    let target = crate::graduation::store_base(app)?.review_checkout(&run.id);
    let fs = crate::graduation::store_fs(app)?;
    let main = crate::streams::primary_repo_of(worktree)?;
    crate::streams::reclaim_linked_worktree(
        &fs,
        &main,
        &target,
        &registration(&run.id),
        Some(&scratch_branch(&run.id)),
    )
}

/// GRL-FR-YKRI: reclaim what a review the application never finished left.
///
/// A graceful end of a review turn reclaims its own checkout, but a process
/// killed while one is running cannot. What it strands is a registration, a
/// branch and a whole checkout that no later turn would ever look at again, so
/// they are removed at the first read of the project's queue, on the terms
/// `WKS-work-streams.md` WKS-FR-LWEI releases a stream the application stopped
/// while holding.
pub fn sweep_stranded<R: tauri::Runtime>(app: &tauri::AppHandle<R>, queue: &GraduationQueue) {
    let Some(state) = app.try_state::<GraduationState>() else {
        return;
    };
    for run in &queue.runs {
        // A review this process is running is the live one. So is a review
        // another process is running: `is_driving` knows only about this one, so
        // the run's own persisted state is what keeps a second window from
        // reclaiming a checkout out from under a turn that is using it.
        if state.is_driving(&run.id) || run.state == GraduationRunState::Reviewing {
            continue;
        }
        let Some(worktree) = crate::graduation::target_worktree(app, run) else {
            continue;
        };
        let Ok(main) = crate::streams::primary_repo_of(&worktree) else {
            continue;
        };
        if !crate::streams::registers_worktree(&main, &registration(&run.id)) {
            continue;
        }
        logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation reclaimed a review checkout the application never finished",
            log_fields! { "run_id" => run.id.clone() },
        );
        if let Err(reason) = release(app, run, &worktree) {
            logging::log_warn(
                app,
                &crate::logging::BUFFER,
                &[Domain::Backend],
                "graduation could not reclaim a stranded review checkout",
                log_fields! {
                    "run_id" => run.id.clone(),
                    "reason" => reason,
                },
            );
        }
    }
}
