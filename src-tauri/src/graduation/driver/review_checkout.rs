//! The throwaway checkout a review turn stands in (GRL-FR-YKRI).
//!
//! The review runs no check (GRL-FR-RTNC), but it can still write files. A
//! reviewer that shared the stream's working copy could change it, and that
//! change could not be told apart from the work it judges. So the turn is given
//! a checkout of its own that is removed when the turn ends.
//!
//! The checkout also receives the check folder of the working copy
//! (GRL-FR-AMQE), which is where the review reads the results of the checks the
//! work turn ran.
//!
//! What the checkout holds is the run's **change set**, standing on the run's
//! base commit as uncommitted work. That is the same tree the commit of a
//! `ready` verdict writes, so the reviewer judges what will land rather than
//! something near it.

use std::path::{Path, PathBuf};

use super::throwaway_checkout::Throwaway;
use super::*;

/// GRL-FR-YKRI / GRL-FR-BIQX: a checkout holding the run's change set, for one
/// review turn, at a path no earlier turn of the run used.
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
    let fs = crate::graduation::store_fs(app)?;
    let main = crate::streams::primary_repo_of(worktree)?;
    let throwaway = Throwaway::review(crate::graduation::store_base(app)?, &run.id);

    // GRL-FR-PQLM / GRL-FR-LYXG: what earlier turns of this run left is
    // removed first. What cannot go is reported, and the new checkout is made
    // all the same: it has a path of its own, so nothing left stands in its way.
    if let Err(reason) = throwaway.reclaim(&fs, &main) {
        logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation could not remove an earlier review checkout",
            log_fields! { "run_id" => run.id.clone(), "reason" => reason },
        );
    }

    // The tree the review reads, and the tree a `ready` verdict commits. One
    // derivation answers both, so they cannot drift.
    let tree = crate::graduation::commit::run_tree(worktree, base)?.map(|derived| derived.tree);

    let (checkout, target) = throwaway.make(&fs, &main, base_oid)?;

    // GRL-FR-YKRI: the change set is written over the base as uncommitted work.
    // The index is deliberately left at the base, so what the run changed reads
    // back as the uncommitted change the reviewer is asked to judge.
    if let Some(tree) = tree {
        let tree = checkout.find_tree(tree).map_err(|e| e.to_string())?;
        let mut builder = git2::build::CheckoutBuilder::new();
        builder.force().remove_untracked(true).update_index(false);
        checkout.checkout_tree(tree.as_object(), Some(&mut builder)).map_err(|e| e.to_string())?;
    }
    // GRL-FR-LXWU: the checkout shares the repository's `info/exclude`, so the
    // copied folder reads back as ignored rather than as an untracked change.
    if let Err(reason) = super::checks_folder::ensure_excluded(worktree).map(|added| {
        if added {
            logging::log_debug(
                app,
                &crate::logging::BUFFER,
                &[Domain::Backend],
                "graduation hid the check folder in the repository exclude file",
                log_fields! { "run_id" => run.id.clone() },
            );
        }
    }) {
        logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation could not hide the check folder",
            log_fields! { "run_id" => run.id.clone(), "reason" => reason },
        );
    }
    // GRL-FR-AMQE: the results of the work turn's checks go with the change
    // set. A copy that fails leaves the report missing, which the review reports
    // as a finding (GRL-FR-NTFD).
    let copied = match super::checks_folder::copy_into(worktree, &target) {
        Ok(copied) => {
            if copied.skipped > 0 {
                logging::log_warn(
                    app,
                    &crate::logging::BUFFER,
                    &[Domain::Backend],
                    "graduation left links out of the copied check folder",
                    log_fields! { "run_id" => run.id.clone(), "skipped" => copied.skipped as u64 },
                );
            }
            copied.found
        }
        Err(reason) => {
            logging::log_warn(
                app,
                &crate::logging::BUFFER,
                &[Domain::Backend],
                "graduation could not copy the check folder into the review checkout",
                log_fields! { "run_id" => run.id.clone(), "reason" => reason },
            );
            false
        }
    };
    logging::log_debug(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "graduation made a review checkout",
        log_fields! {
            "run_id" => run.id.clone(),
            "checkout" => target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            "check_folder" => copied,
        },
    );
    Ok(target)
}

/// GRL-FR-PQLM: remove every review checkout of the run, its registration and
/// the scratch branch.
///
/// Every step is idempotent, so this is also what a later turn calls before it
/// makes its own checkout.
pub(super) fn release<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
    worktree: &Path,
) -> Result<(), String> {
    let fs = crate::graduation::store_fs(app)?;
    let main = crate::streams::primary_repo_of(worktree)?;
    Throwaway::review(crate::graduation::store_base(app)?, &run.id).reclaim(&fs, &main)
}

/// GRL-FR-XJJU: reclaim what a review the application never finished left.
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
        let (Ok(fs), Ok(base)) =
            (crate::graduation::store_fs(app), crate::graduation::store_base(app))
        else {
            continue;
        };
        // GRL-FR-XJJU: a checkout of any name the run's turns used, the name
        // without a nonce included, and a registration as much as a directory.
        if !Throwaway::review(base, &run.id).stands(&fs, &main) {
            continue;
        }
        // Reported once the release has ended, so the log never says a
        // checkout went when it did not.
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
        } else {
            logging::log_warn(
                app,
                &crate::logging::BUFFER,
                &[Domain::Backend],
                "graduation reclaimed a review checkout the application never finished",
                log_fields! { "run_id" => run.id.clone() },
            );
        }
    }
}
