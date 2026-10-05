//! Removing one linked worktree (`WTC-worktree-context.md` WTC-FR-RDVK,
//! WTC-FR-BHFE).
//!
//! Split out of `worktree.rs` so that file keeps the enumeration, the
//! re-rooting and the commands. This is the one place a worktree is removed:
//! `GTC-git.md` `delete_branch` composes it, and the stream removal of
//! `WKS-work-streams.md` shares its prune of the registration.

use super::*;
use crate::log_fields;
use crate::logging::{self, Domain};

/// WTC-FR-RDVK: the path names the repository's primary worktree.
pub const ERR_WORKTREE_IS_PRIMARY: &str = "worktree_is_primary";
/// WTC-FR-RDVK: the path names the worktree that roots the project.
pub const ERR_WORKTREE_IS_ACTIVE: &str = "worktree_is_active";
/// WTC-FR-RDVK: the path names a work stream's working copy.
pub const ERR_WORKTREE_BELONGS_TO_WORK_STREAM: &str = "worktree_belongs_to_work_stream";

/// WTC-FR-RDVK: prune the registration that names the worktree at `path`.
///
/// Pruning is what stops the repository listing a worktree whose directory has
/// gone. A registration matches by its name, when the caller knows it, or by the
/// path it records. Shared with the removal of a work stream's working copy
/// (WKS-FR-EIBC), so both leave the repository in the same state.
pub(crate) fn prune_worktree_registration(
    main: &Repository,
    path: &Path,
    registration: Option<&str>,
) -> Result<(), String> {
    let names: Vec<String> = main
        .worktrees()
        .map(|list| {
            list.iter()
                .filter_map(|n| n.ok().flatten().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let wanted = canonicalize_lenient(path);
    for candidate in names {
        let Ok(worktree) = main.find_worktree(&candidate) else {
            continue;
        };
        let matches_name = registration.is_some_and(|r| r == candidate);
        let matches_path = canonicalize_lenient(worktree.path()) == wanted;
        if matches_name || matches_path {
            let mut opts = git2::WorktreePruneOptions::new();
            opts.valid(true).working_tree(true);
            worktree.prune(Some(&mut opts)).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// WTC-FR-RDVK: remove the directory of one linked worktree and prune its
/// registration, through the filesystem access layer.
///
/// Refuses, before anything changes, the primary worktree
/// (`worktree_is_primary`), the active worktree (`worktree_is_active`), and a
/// work stream's working copy (`worktree_belongs_to_work_stream`). The
/// enumeration is read here, at the moment of the removal (WTC-FR-BHFE). A
/// worktree whose directory is gone has its registration pruned.
///
/// The directory is the author's own, not one the application owns, so it is
/// removed through an instance whose author root is its parent
/// (`delete_author_tree`, FSA-FR-OWVT). A symbolic link in the tree, such as an
/// installed dependency store, is unlinked and never followed, so its target is
/// untouched.
///
/// Not a Tauri command (WTC contract surface): `delete_branch` composes it.
pub fn remove_linked_worktree<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &Path,
    path: &Path,
) -> Result<(), String> {
    let result = remove_checked(app, root, path);
    match &result {
        Ok(()) => logging::log_info(
            app,
            &logging::BUFFER,
            &[Domain::Backend],
            "linked worktree removed",
            log_fields! { "path" => path.to_string_lossy() },
        ),
        Err(error) => logging::log_warn(
            app,
            &logging::BUFFER,
            &[Domain::Backend],
            "linked worktree removal refused or failed",
            log_fields! { "path" => path.to_string_lossy(), "reason" => error.clone() },
        ),
    }
    result
}

fn remove_checked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &Path,
    path: &Path,
) -> Result<(), String> {
    let repo = open_repo(root)?;
    let primary = primary_worktree_root(&repo).ok_or_else(|| ERR_NOT_A_REPO.to_string())?;
    let active = containing_worktree(&repo).ok_or_else(|| ERR_NOT_A_REPO.to_string())?;
    let target = canonicalize_lenient(path);

    // WTC-FR-BHFE: read now, never from an earlier listing.
    let entry = worktree_entries(&repo)?
        .into_iter()
        .find(|e| Path::new(&e.path) == target.as_path())
        .ok_or_else(|| ERR_NOT_A_WORKTREE.to_string())?;
    if entry.is_primary || target == primary {
        return Err(ERR_WORKTREE_IS_PRIMARY.to_string());
    }
    if target == active {
        return Err(ERR_WORKTREE_IS_ACTIVE.to_string());
    }
    if crate::streams::stream_at_path(app, &target).is_some() {
        return Err(ERR_WORKTREE_BELONGS_TO_WORK_STREAM.to_string());
    }

    if !entry.is_missing {
        remove_directory(&target)?;
    }
    let main = Repository::open(&primary).map_err(|_| ERR_NOT_A_REPO.to_string())?;
    prune_worktree_registration(&main, &target, None)
}

fn remove_directory(target: &Path) -> Result<(), String> {
    let parent = target
        .parent()
        .ok_or_else(|| ERR_NOT_A_WORKTREE.to_string())?;
    let access = crate::fs::FsAccess::builder()
        .author_root(parent)
        .build()
        .map_err(|e| format!("failed to remove the worktree: {e}"))?;
    access
        .delete_author_tree(target)
        .map_err(|e| format!("failed to remove the worktree: {e}"))
}
