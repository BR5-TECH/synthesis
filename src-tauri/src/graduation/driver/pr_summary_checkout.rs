//! The disposable checkout a PR summary turn stands in (GRD-FR-TYVT).
//!
//! The turn reads the change of a head branch, and the project's own skills may
//! run commands that write files while it does. So the turn is given a checkout
//! of its own at `short_data_dir()/g/<run-id>/ps-<nonce>/`, detached at the
//! pinned head tip, at a path no earlier turn of the run used (GRD-FR-XKGC). It
//! changes no branch and no worktree of the author, and it is removed when the
//! turn ends and when the run is terminal.

use std::path::PathBuf;

use super::throwaway_checkout::Throwaway;
use super::*;

/// The pinned head tip of a PR summary run, as an object id.
fn pinned_tip(run: &GraduationRun) -> Result<git2::Oid, String> {
    let tip = run
        .pr_summary
        .as_ref()
        .and_then(|data| data.head_tip.as_deref())
        .ok_or_else(|| "the run has no pinned head tip".to_string())?;
    git2::Oid::from_str(tip).map_err(|e| e.to_string())
}

/// GRD-FR-TYVT / GRD-FR-XKGC: a fresh checkout detached at the pinned head tip.
///
/// What an earlier dispatch of the run left is removed first, so each turn
/// starts from the pinned tip and nothing else. What cannot go is reported, and
/// the new checkout is made all the same (GRL-FR-LYXG).
pub(crate) fn create<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
) -> Result<PathBuf, String> {
    let tip = pinned_tip(run)?;
    let fs = crate::graduation::store_fs(app)?;
    let main = crate::graduation::pr_summary::project_repo(app)?;
    let throwaway = Throwaway::pr_summary(crate::graduation::store_base(app)?, &run.id);
    if let Err(reason) = throwaway.reclaim(&fs, &main) {
        logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation could not remove an earlier checkout of a PR summary run",
            log_fields! { "run_id" => run.id.clone(), "reason" => reason },
        );
    }
    let (_, target) = throwaway.make(&fs, &main, tip)?;
    logging::log_debug(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "graduation created the checkout of a PR summary run",
        log_fields! {
            "run_id" => run.id.clone(),
            "head_tip" => tip.to_string(),
            "checkout" => target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        },
    );
    Ok(target)
}

/// GRD-FR-TYVT / GRD-FR-XKGC: remove every checkout of the run, its
/// registration and its scratch branch.
///
/// Every step is idempotent, so a release that stopped half way is finished by
/// the next one.
pub(crate) fn release<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
) -> Result<(), String> {
    let fs = crate::graduation::store_fs(app)?;
    let main = crate::graduation::pr_summary::project_repo(app)?;
    Throwaway::pr_summary(crate::graduation::store_base(app)?, &run.id).reclaim(&fs, &main)
}

/// Whether anything of a checkout still stands, so a queue read that finds an
/// ended run reclaims it once and then costs it nothing.
pub(crate) fn stands<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &GraduationRun) -> bool {
    let (Ok(base), Ok(fs), Ok(main)) = (
        crate::graduation::store_base(app),
        crate::graduation::store_fs(app),
        crate::graduation::pr_summary::project_repo(app),
    ) else {
        return false;
    };
    Throwaway::pr_summary(base, &run.id).stands(&fs, &main)
}
