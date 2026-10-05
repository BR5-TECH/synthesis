//! Creating, measuring and reclaiming a stream's branch and working copy
//! (WKS-FR-KDXF, WKS-FR-VTEY, WKS-FR-EIBC).
//!
//! libgit2 registers worktrees on the primary repository, so every operation
//! here opens that repository rather than whichever worktree the author is
//! standing in.

use std::path::{Path, PathBuf};

use super::*;

/// What a partly-finished creation made, so a failure can reclaim it
/// (WKS-FR-VTEY).
#[derive(Default)]
pub(super) struct Created {
    pub(super) branch: Option<String>,
    pub(super) worktree: Option<PathBuf>,
    /// The name Git registered the worktree under.
    pub(super) registration: Option<String>,
}

/// The repository that owns `path`, and its primary worktree.
///
/// Both are needed together: the primary is where a branch and a worktree
/// registration must be made, whatever worktree the caller is in.
pub(super) fn primary_repo(path: &Path) -> Result<git2::Repository, String> {
    let repo = git2::Repository::discover(path).map_err(|_| ERR_NOT_A_GIT_REPOSITORY.to_string())?;
    let primary =
        crate::worktree::primary_worktree_root(&repo).ok_or(ERR_NOT_A_GIT_REPOSITORY.to_string())?;
    git2::Repository::open(&primary).map_err(|_| ERR_NOT_A_GIT_REPOSITORY.to_string())
}

/// WKS-FR-PWNR: the branch checked out in the worktree at `path`.
///
/// `None` where that worktree's `HEAD` is not on a branch, which is what makes
/// `base_branch` required rather than defaulted.
pub(super) fn current_branch(path: &Path) -> Option<String> {
    let repo = git2::Repository::discover(path).ok()?;
    let head = repo.head().ok()?;
    if !head.is_branch() {
        return None;
    }
    head.shorthand().ok().map(str::to_string)
}

/// The revision a branch names, or a typed failure where it names none.
pub(super) fn branch_revision(main: &git2::Repository, branch: &str) -> Result<String, String> {
    let found = main
        .find_branch(branch, git2::BranchType::Local)
        .map_err(|_| ERR_BASE_BRANCH_REQUIRED.to_string())?;
    let commit = found
        .get()
        .peel_to_commit()
        .map_err(|_| ERR_BASE_BRANCH_REQUIRED.to_string())?;
    Ok(commit.id().to_string())
}

/// WKS-FR-KDXF: create the branch and the linked worktree, as one unit.
///
/// The caller reclaims through [`reclaim`] on every failure path, so nothing
/// here is left behind by a creation that did not finish (WKS-FR-VTEY).
pub(super) fn create_stream_checkout(
    fs: &fsa::FsAccess,
    main: &git2::Repository,
    stream_id: &str,
    branch: &str,
    base_revision: &str,
    target: &Path,
    created: &mut Created,
) -> Result<(), String> {
    let oid = git2::Oid::from_str(base_revision).map_err(|_| ERR_STREAM_CREATION_FAILED.to_string())?;
    let base_commit = main
        .find_commit(oid)
        .map_err(|_| ERR_STREAM_CREATION_FAILED.to_string())?;
    main.branch(branch, &base_commit, false)
        .map_err(|_| ERR_STREAM_CREATION_FAILED.to_string())?;
    created.branch = Some(branch.to_string());

    if let Some(parent) = target.parent() {
        // The typed refusal is what the surface switches on, so a filesystem
        // error is mapped rather than passed through as its own text.
        store::ensure_dir(fs, parent).map_err(|_| ERR_STREAM_CREATION_FAILED.to_string())?;
    }
    let reference = main
        .find_branch(branch, git2::BranchType::Local)
        .map_err(|_| ERR_STREAM_CREATION_FAILED.to_string())?
        .into_reference();
    let mut opts = git2::WorktreeAddOptions::new();
    opts.reference(Some(&reference));
    // Recorded **before** the call, because a worktree-add is not atomic: it
    // writes the admin registration and may create the target directory before
    // it fails. Reclaiming is idempotent for a name and a path that were never
    // made, so recording early costs nothing and recording late strands both.
    created.worktree = Some(target.to_path_buf());
    created.registration = Some(stream_id.to_string());
    main.worktree(stream_id, target, Some(&opts))
        .map_err(|_| ERR_STREAM_CREATION_FAILED.to_string())?;

    // WKS-FR-BSLO: the worktree records the store it shares as a path relative
    // to its own Git directory. An agent turn reads the repository through a
    // container that mounts that store somewhere of the executor's choosing,
    // where an absolute host path names nothing.
    crate::worktree::normalize_worktree_commondir(main, stream_id)
        .map_err(|_| ERR_STREAM_CREATION_FAILED.to_string())?;
    Ok(())
}

/// WKS-FR-VTEY / WKS-FR-EIBC: remove a branch and a working copy this module
/// made.
///
/// Returns the names it could not reclaim, so a cleanup that itself fails can
/// report them rather than leaving the author to find them later.
pub(super) fn reclaim(
    fs: &fsa::FsAccess,
    main: &git2::Repository,
    created: Created,
) -> Vec<String> {
    let mut stranded: Vec<String> = Vec::new();
    if let Some(path) = &created.worktree {
        if remove_worktree(fs, main, path, created.registration.as_deref()).is_err() {
            stranded.push(path.to_string_lossy().into_owned());
        }
    }
    if let Some(branch) = &created.branch {
        if delete_branch(main, branch).is_err() {
            stranded.push(branch.clone());
        }
    }
    stranded
}

/// Remove the working copy and prune the registration that names it.
///
/// Pruning is what stops the repository listing a worktree whose directory has
/// gone; without it `git worktree list` keeps naming it.
pub(super) fn remove_worktree(
    fs: &fsa::FsAccess,
    main: &git2::Repository,
    path: &Path,
    registration: Option<&str>,
) -> Result<(), String> {
    if path.exists() {
        // FSA-FR-ZUCF: a linked worktree is the application's own directory,
        // and a checkout the project's own install has run in holds a
        // dependency store of symbolic links. The strict removal refuses over
        // one and leaves the worktree standing for ever, so this reclaims it.
        fs.delete_owned_tree(path).map_err(|e| e.to_string())?;
    }
    // WTC-FR-RDVK: the one prune of a registration, shared with the removal of
    // a linked worktree by `delete_branch`.
    crate::worktree::prune_worktree_registration(main, path, registration)
}

/// Delete a local branch. Already gone is the state the caller wanted.
pub(super) fn delete_branch(main: &git2::Repository, branch: &str) -> Result<(), String> {
    match main.find_branch(branch, git2::BranchType::Local) {
        Ok(mut found) => found.delete().map_err(|e| e.to_string()),
        Err(_) => Ok(()),
    }
}

/// The worktree that has `branch` checked out, if any worktree does.
///
/// A merge writes into the tree that holds the base branch, and that is not
/// always the primary one: a stream's base is whatever the author was standing
/// on when they created it. Writing into the primary regardless would rewrite
/// an unrelated checkout's files under a `HEAD` that names another branch.
pub(super) fn worktree_holding(
    main: &git2::Repository,
    branch: &str,
) -> Option<git2::Repository> {
    let wanted = format!("refs/heads/{branch}");
    let holds = |repo: &git2::Repository| -> bool {
        repo.head()
            .ok()
            .and_then(|h| h.name().map(str::to_string).ok())
            .is_some_and(|name| name == wanted)
    };
    if holds(main) {
        return git2::Repository::open(main.workdir()?).ok();
    }
    let names: Vec<String> = main
        .worktrees()
        .map(|list| {
            list.iter()
                .filter_map(|n| n.ok().flatten().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    for name in names {
        let Ok(worktree) = main.find_worktree(&name) else {
            continue;
        };
        let Ok(repo) = git2::Repository::open(worktree.path()) else {
            continue;
        };
        if holds(&repo) {
            return Some(repo);
        }
    }
    None
}

/// WKS-FR-EIBC: how many commits the stream branch holds that its base does
/// not.
///
/// Zero means a deletion strands nothing, which is what makes the unmerged
/// refusal answerable without reading either tree.
pub(super) fn ahead_of_base(main: &git2::Repository, stream: &WorkStream) -> u32 {
    let resolve = |name: &str| -> Option<git2::Oid> {
        main.find_branch(name, git2::BranchType::Local)
            .ok()?
            .get()
            .peel_to_commit()
            .ok()
            .map(|c| c.id())
    };
    let (Some(head), Some(base)) = (resolve(&stream.branch), resolve(&stream.base_branch)) else {
        return 0;
    };
    main.graph_ahead_behind(head, base)
        .map(|(ahead, _)| ahead as u32)
        .unwrap_or(0)
}

/// WKS-FR-UZHT / WKS-FR-JVLM / `GRB-graduation-rebase.md` GRB-FR-QIHE: the
/// uncommitted paths a worktree holds.
///
/// The complete set, so a refusal can name all of it rather than the first
/// path it found. Application-owned storage (`PST-project-storage.md`
/// PST-FR-XKVD) is read on the terms of WKS-FR-JVLM:
///
/// - an untracked path of it is never uncommitted work, a forced checkout
///   leaving untracked files where they are;
/// - on a side the operation does not check out (`checked_out` false), no path
///   of it is, because nothing will overwrite it;
/// - on the side it checks out, a tracked path of it that still differs from
///   `HEAD` after the save of PST-FR-RONA is, because the checkout would revert
///   it. The drafts root's own Git files are not: the application ensures them
///   again on its next draft write.
pub(crate) fn uncommitted_paths(worktree: &Path, checked_out: bool) -> Vec<String> {
    let Ok(repo) = git2::Repository::open(worktree) else {
        return Vec::new();
    };
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let Ok(statuses) = repo.statuses(Some(&mut opts)) else {
        return Vec::new();
    };
    let mut paths: Vec<String> = statuses
        .iter()
        .filter(|entry| !entry.status().is_empty())
        .filter(|entry| {
            let Ok(path) = entry.path() else {
                return true;
            };
            if !crate::storage_floor::is_application_storage_in_repo(path) {
                return true;
            }
            checked_out
                && !entry.status().is_wt_new()
                && !crate::storage_floor::save::split_at_drafts(path)
                    .is_some_and(|(_, rel)| crate::storage_floor::save::is_root_git_file(&rel))
        })
        .filter_map(|entry| entry.path().ok().map(str::to_string))
        .collect();
    paths.sort();
    paths.dedup();
    paths
}
