//! The worktree a merge run reconciles its merge in (GRD-FR-KZPT).
//!
//! A work turn of a merge run stands here and nowhere else. The worktree is a
//! linked worktree of the project's repository, seeded from the run's merge
//! snapshot, so the snapshot's tree stands in it with conflict markers where Git
//! stopped. The repository is mounted read-only in the turn's container, and the
//! application derives what the turn changed from this worktree on the host.
//! Neither live branch and neither live worktree is written.
//!
//! The worktree outlives a rest: a run that waits for the author keeps what its
//! passes wrote, so Continue resumes the saved work. It is removed when the run
//! ends.

use std::path::PathBuf;

use super::*;

/// The scratch branch the worktree's registration needs a reference of its own
/// for, so it never borrows a live branch.
fn scratch_branch(run_id: &str) -> String {
    format!("synthesis/merge-run/{run_id}")
}

/// The registration Git holds the worktree under.
fn registration(run_id: &str) -> String {
    format!("{run_id}-mw")
}

/// The project's repository, opened at its primary worktree.
fn primary<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<git2::Repository, String> {
    let root = app
        .try_state::<crate::project::ProjectState>()
        .and_then(|state| state.root())
        .ok_or_else(|| crate::graduation::ERR_NO_PROJECT_OPEN.to_string())?;
    crate::streams::primary_repo_of(&root)
}

/// GRD-FR-KZPT: the merge worktree of a run, created from the snapshot when it
/// does not stand yet.
///
/// A worktree that stands is kept as it is: it holds the work of the earlier
/// passes. A worktree the registration names but whose directory is gone is
/// created again from the snapshot, and the loss is logged.
pub(crate) fn ensure<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
) -> Result<PathBuf, String> {
    let data = run
        .merge
        .as_ref()
        .ok_or_else(|| "the run is not a merge run".to_string())?;
    let target = crate::graduation::store_base(app)?.merge_worktree(&run.id);
    let fs = crate::graduation::store_fs(app)?;
    let main = primary(app)?;
    let name = registration(&run.id);

    if target.is_dir() && crate::streams::registers_worktree(&main, &name) {
        return Ok(target);
    }
    if crate::streams::registers_worktree(&main, &name) || target.is_dir() {
        logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation created a merge worktree again from its snapshot",
            log_fields! { "run_id" => run.id.clone() },
        );
        crate::streams::reclaim_linked_worktree(
            &fs,
            &main,
            &target,
            &name,
            Some(&scratch_branch(&run.id)),
        )?;
    }
    if let Some(parent) = target.parent() {
        if !parent.is_dir() {
            fs.create_dir(parent).map_err(|e| e.to_string())?;
        }
    }

    let snapshot = git2::Oid::from_str(&data.snapshot_commit).map_err(|e| e.to_string())?;
    let commit = main.find_commit(snapshot).map_err(|e| e.to_string())?;
    let branch = scratch_branch(&run.id);
    main.branch(&branch, &commit, true).map_err(|e| e.to_string())?;
    let reference = main
        .find_branch(&branch, git2::BranchType::Local)
        .map_err(|e| e.to_string())?
        .into_reference();
    let mut opts = git2::WorktreeAddOptions::new();
    opts.reference(Some(&reference));
    main.worktree(&name, &target, Some(&opts))
        .map_err(|e| e.to_string())?;
    crate::worktree::normalize_worktree_commondir(&main, &name).map_err(|e| e.to_string())?;

    // Detached: the worktree reads one revision and owns no branch, so nothing a
    // turn writes can move the scratch branch either.
    let checkout = git2::Repository::open(&target).map_err(|e| e.to_string())?;
    checkout.set_head_detached(snapshot).map_err(|e| e.to_string())?;
    let mut builder = git2::build::CheckoutBuilder::new();
    builder.force();
    checkout
        .checkout_head(Some(&mut builder))
        .map_err(|e| e.to_string())?;
    logging::log_info(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "graduation created a merge worktree",
        log_fields! { "run_id" => run.id.clone() },
    );
    Ok(target)
}

/// GRD-FR-KZPT: remove the merge worktree, its registration, its scratch branch
/// and the private ref that keeps the snapshot reachable.
///
/// Every step is idempotent, so a release that stopped half way is finished by
/// the next one. The ref goes first: what a crash leaves behind is then a
/// directory the first queue read finds, and never a ref nobody looks for.
pub(crate) fn release<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
) -> Result<(), String> {
    let target = crate::graduation::store_base(app)?.merge_worktree(&run.id);
    let fs = crate::graduation::store_fs(app)?;
    let main = primary(app)?;
    crate::streams::release_snapshot_ref(&main, &run.id)?;
    crate::streams::reclaim_linked_worktree(
        &fs,
        &main,
        &target,
        &registration(&run.id),
        Some(&scratch_branch(&run.id)),
    )
}

/// Whether anything of a merge run's own still stands, so a queue read that
/// finds an ended run reclaims it once and then costs it nothing.
pub(crate) fn stands<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &GraduationRun) -> bool {
    let Ok(base) = crate::graduation::store_base(app) else {
        return false;
    };
    if base.merge_worktree(&run.id).is_dir() {
        return true;
    }
    let Ok(main) = primary(app) else {
        return false;
    };
    crate::streams::registers_worktree(&main, &registration(&run.id))
        || crate::streams::has_snapshot_ref(&main, &run.id)
}
