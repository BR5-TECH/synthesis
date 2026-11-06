//! The disposable checkout a PR summary turn stands in (GRD-FR-TYVT).
//!
//! The turn reads the change of a head branch, and the project's own skills may
//! run commands that write files while it does. So the turn is given a checkout
//! of its own at `short_data_dir()/g/<run-id>/ps/`, detached at the pinned head
//! tip. It changes no branch and no worktree of the author, and it is removed
//! when the run is terminal.

use std::path::PathBuf;

use super::*;

/// The scratch branch the registration needs a reference of its own for, so
/// it never borrows a live branch.
fn scratch_branch(run_id: &str) -> String {
    format!("synthesis/pr-summary/{run_id}")
}

/// The registration Git holds the checkout under.
fn registration(run_id: &str) -> String {
    format!("{run_id}-ps")
}

/// The pinned head tip of a PR summary run, as an object id.
fn pinned_tip(run: &GraduationRun) -> Result<git2::Oid, String> {
    let tip = run
        .pr_summary
        .as_ref()
        .and_then(|data| data.head_tip.as_deref())
        .ok_or_else(|| "the run has no pinned head tip".to_string())?;
    git2::Oid::from_str(tip).map_err(|e| e.to_string())
}

/// GRD-FR-TYVT: a fresh checkout detached at the pinned head tip.
///
/// What an earlier dispatch of the run left is removed first, so each turn
/// starts from the pinned tip and nothing else.
pub(crate) fn create<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
) -> Result<PathBuf, String> {
    let tip = pinned_tip(run)?;
    let target = crate::graduation::store_base(app)?.pr_summary_checkout(&run.id);
    let fs = crate::graduation::store_fs(app)?;
    let main = crate::graduation::pr_summary::project_repo(app)?;
    release(app, run)?;
    if let Some(parent) = target.parent() {
        if !crate::graduation::is_directory(&fs, parent) {
            fs.create_dir(parent).map_err(|e| e.to_string())?;
        }
    }
    let commit = main.find_commit(tip).map_err(|e| e.to_string())?;
    let branch = scratch_branch(&run.id);
    main.branch(&branch, &commit, true).map_err(|e| e.to_string())?;
    let reference = main
        .find_branch(&branch, git2::BranchType::Local)
        .map_err(|e| e.to_string())?
        .into_reference();
    let mut opts = git2::WorktreeAddOptions::new();
    opts.reference(Some(&reference));
    main.worktree(&registration(&run.id), &target, Some(&opts)).map_err(|e| e.to_string())?;
    crate::worktree::normalize_worktree_commondir(&main, &registration(&run.id))
        .map_err(|e| e.to_string())?;

    // Detached: the turn reads one revision and owns no branch.
    let checkout = git2::Repository::open(&target).map_err(|e| e.to_string())?;
    checkout.set_head_detached(tip).map_err(|e| e.to_string())?;
    let mut builder = git2::build::CheckoutBuilder::new();
    builder.force();
    checkout.checkout_head(Some(&mut builder)).map_err(|e| e.to_string())?;
    logging::log_debug(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "graduation created the checkout of a PR summary run",
        log_fields! { "run_id" => run.id.clone(), "head_tip" => tip.to_string() },
    );
    Ok(target)
}

/// GRD-FR-TYVT: remove the checkout, its registration and its scratch branch.
///
/// Every step is idempotent, so a release that stopped half way is finished by
/// the next one.
pub(crate) fn release<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
) -> Result<(), String> {
    let target = crate::graduation::store_base(app)?.pr_summary_checkout(&run.id);
    let fs = crate::graduation::store_fs(app)?;
    let main = crate::graduation::pr_summary::project_repo(app)?;
    crate::streams::reclaim_linked_worktree(
        &fs,
        &main,
        &target,
        &registration(&run.id),
        Some(&scratch_branch(&run.id)),
    )
}

/// Whether anything of the checkout still stands, so a queue read that finds an
/// ended run reclaims it once and then costs it nothing.
pub(crate) fn stands<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &GraduationRun) -> bool {
    let Ok(base) = crate::graduation::store_base(app) else {
        return false;
    };
    let checkout = base.pr_summary_checkout(&run.id);
    if crate::graduation::store_fs(app)
        .is_ok_and(|fs| crate::graduation::is_directory(&fs, &checkout))
    {
        return true;
    }
    crate::graduation::pr_summary::project_repo(app)
        .is_ok_and(|main| crate::streams::registers_worktree(&main, &registration(&run.id)))
}
