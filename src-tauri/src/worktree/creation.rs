//! Proposing and creating a worktree
//! (`WTC-worktree-context.md` WTC-FR-13, WTC-FR-14, WTC-FR-15, WTC-FR-QVNL).
//!
//! Split out of `worktree.rs` so that file holds the enumeration, the
//! re-rooting and the commands, and this one holds the two additive acts a new
//! checkout takes: the path proposed for it and its creation.

use super::*;

/// The part of a branch name a directory should be named after (WTC-FR-13):
/// its **final segment**. `feature/PROJ-12345/editor-scroll` yields
/// `editor-scroll`.
///
/// Branch names are routinely namespaced by team, ticket, or kind, and folding
/// that namespace into a directory name yields something long and unreadable
/// that distinguishes nothing the leaf does not. Empty when the branch is empty
/// or is only separators, so the caller can fall back rather than propose a
/// name ending in one.
pub(super) fn branch_leaf(branch: &str) -> &str {
    branch.split('/').map(str::trim).rfind(|segment| !segment.is_empty()).unwrap_or("")
}

/// WTC-FR-14: the branch a creation for `requested` checks out. A local branch
/// of exactly the trimmed name is used as it is. Any other name is normalized
/// (WTC-FR-XLXY), and a local branch of that name without regard to case is
/// used in its place. Empty when nothing of `requested` is left.
pub(super) fn resolve_branch_name(main: &Repository, requested: &str) -> String {
    let requested = requested.trim();
    let local = local_branch_names(main);
    if !requested.is_empty() && local.iter().any(|name| name == requested) {
        return requested.to_string();
    }
    let normalized = normalize_branch_name(requested);
    if normalized.is_empty() {
        return normalized;
    }
    // On a file system that ignores case, `feature/x` is the branch
    // `Feature/X`; matching here gives one result on every platform.
    local.into_iter().find(|name| name.eq_ignore_ascii_case(&normalized)).unwrap_or(normalized)
}

/// The names of the local branches, case included. `find_branch` does not
/// give that test: on a file system that ignores case it finds `main` for
/// `MAIN`.
fn local_branch_names(main: &Repository) -> Vec<String> {
    let Ok(branches) = main.branches(Some(BranchType::Local)) else {
        return Vec::new();
    };
    branches
        .filter_map(|b| b.ok().and_then(|(b, _)| b.name().ok().flatten().map(str::to_string)))
        .collect()
}

/// WTC-FR-13: an absolute sibling of the repository's primary worktree, named
/// `<primary-basename>-<leaf>` and normalized as a worktree name
/// (WTC-FR-AGQQ). Read-only — creates nothing and does not check the path for
/// availability.
pub fn propose_path_for(active_root: &Path, branch: &str) -> Result<String, String> {
    let repo = open_repo(active_root)?;
    let primary = primary_worktree_root(&repo).ok_or_else(|| ERR_NOT_A_REPO.to_string())?;
    let base = project::basename(&primary.to_string_lossy());
    let branch = resolve_branch_name(&repo, branch);
    let leaf = branch_leaf(&branch);
    let name = if leaf.is_empty() {
        normalize_worktree_name(&base)
    } else {
        normalize_worktree_name(&format!("{base}-{leaf}"))
    };
    // A basename with no character a worktree name keeps (and no leaf) must not
    // propose the parent directory itself.
    let name = if name.is_empty() { base } else { name };
    // A primary worktree at the filesystem root has no parent; siblings of
    // `/` are its own children, and the result stays absolute either way.
    let parent = primary.parent().unwrap_or(primary.as_path());
    Ok(to_string_path(&parent.join(name)))
}

/// WTC-FR-14: the creation, split out of the command for the same reason as
/// [`activate_worktree_at`].
pub(crate) fn create_worktree_in<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project: &ProjectState,
    watcher: &ProjectWatcher,
    store: &GlobalSettingsStore,
    branch: &str,
    path: &str,
) -> Result<WorktreeContext, String> {
    let root = project.require_root()?;
    // WTC-FR-FBJQ: creating a worktree makes it the content root.
    crate::graduation::require_no_dispatched_direct_run(app)?;
    let offset = root_offset(&open_repo(&root)?, &root);
    let created = match create_worktree_at(&root, branch, path) {
        Ok(created) => created,
        Err(reason) => {
            crate::logging::log_warn(
                app,
                &crate::logging::BUFFER,
                &[crate::logging::Domain::Backend],
                "worktree creation refused or failed",
                crate::log_fields! { "requested_branch" => branch, "path" => path, "reason" => reason.clone() },
            );
            return Err(reason);
        }
    };
    let context = reroot(app, project, watcher, store, &content_root_in(&created, &offset))
        .inspect_err(|reason| {
            crate::logging::log_error(
                app,
                &crate::logging::BUFFER,
                &[crate::logging::Domain::Backend],
                "worktree created but the project was not re-rooted on it",
                crate::log_fields! { "requested_branch" => branch, "path" => created.to_string_lossy(), "reason" => reason.clone() },
            );
        })?;
    // WTC-FR-14: the branch can differ from the one requested (WTC-FR-XLXY).
    let created_path = created.to_string_lossy();
    let checked_out = context
        .worktrees
        .iter()
        .find(|w| w.path == created_path)
        .and_then(|w| w.branch.clone())
        .unwrap_or_default();
    crate::logging::log_info(
        app,
        &crate::logging::BUFFER,
        &[crate::logging::Domain::Backend],
        "worktree created",
        crate::log_fields! { "requested_branch" => branch, "branch" => checked_out, "path" => created_path },
    );
    Ok(context)
}

/// WTC-FR-QVNL: where a test acts between the worktree being registered and the
/// record that finishes it, so the reclaim of a creation that cannot be finished
/// is exercised rather than argued from.
pub(crate) trait CreateSeam {
    fn after_worktree(&self) {}
}

/// The production seam: the creation runs straight through.
pub(crate) struct NoCreateSeam;
impl CreateSeam for NoCreateSeam {}

/// WTC-FR-14 / WTC-FR-15: create a worktree at `path` with the branch that
/// `branch` resolves to (`resolve_branch_name`) checked out, creating that
/// branch from the active worktree's `HEAD` when it does not exist. Returns the
/// canonical path of the new worktree.
pub fn create_worktree_at(active_root: &Path, branch: &str, path: &str) -> Result<PathBuf, String> {
    create_worktree_seamed(active_root, branch, path, &NoCreateSeam)
}

pub(super) fn create_worktree_seamed(
    active_root: &Path,
    branch: &str,
    path: &str,
    seam: &dyn CreateSeam,
) -> Result<PathBuf, String> {
    let repo = open_repo(active_root)?;
    if branch.trim().is_empty() {
        return Err(ERR_UNKNOWN_BRANCH.to_string());
    }
    let target = canonicalize_lenient(Path::new(path));
    if target.exists() {
        return Err(ERR_WORKTREE_PATH_EXISTS.to_string());
    }

    // libgit2 registers worktrees on the primary repository.
    let primary = primary_worktree_root(&repo).ok_or_else(|| ERR_NOT_A_REPO.to_string())?;
    let main = Repository::open(&primary).map_err(|_| ERR_NOT_A_REPO.to_string())?;

    // WTC-FR-XLXY: a name with no character a branch can keep is no branch.
    let branch = resolve_branch_name(&main, branch);
    if branch.is_empty() {
        return Err(ERR_UNKNOWN_BRANCH.to_string());
    }
    let branch = branch.as_str();
    // WTC-FR-15: a branch that already has a worktree cannot get a second one.
    // Case is ignored here: on a file system that ignores case, `feature/x`
    // opens the branch `Feature/X`.
    let checked_out =
        |w: &WorktreeEntry| w.branch.as_deref().is_some_and(|b| b.eq_ignore_ascii_case(branch));
    if worktree_entries(&repo)?.iter().any(checked_out) {
        return Err(ERR_BRANCH_ALREADY_CHECKED_OUT.to_string());
    }

    let mut created_branch = false;
    if main.find_branch(branch, BranchType::Local).is_err() {
        // WTC-FR-14: created from the active worktree's HEAD.
        let head = repo
            .head()
            .and_then(|h| h.peel_to_commit())
            .map_err(|e| format!("failed to resolve HEAD: {e}"))?;
        let head =
            main.find_commit(head.id()).map_err(|e| format!("failed to resolve HEAD: {e}"))?;
        main.branch(branch, &head, false).map_err(|e| format!("failed to create branch: {e}"))?;
        created_branch = true;
    }

    let reference = main
        .find_branch(branch, BranchType::Local)
        .map_err(|_| ERR_UNKNOWN_BRANCH.to_string())?
        .into_reference();
    let mut opts = git2::WorktreeAddOptions::new();
    opts.reference(Some(&reference));
    let name = registration_name(&target, branch);
    if let Err(e) = main.worktree(&name, &target, Some(&opts)) {
        // WTC-FR-PKRW: a refused registration leaves no branch it made. Only
        // the branch: `name` can be another worktree's registration, which is
        // not this creation's to prune.
        if created_branch {
            if let Ok(mut found) = main.find_branch(branch, BranchType::Local) {
                let _ = found.delete();
            }
        }
        return Err(format!("failed to create worktree: {e}"));
    }
    seam.after_worktree();
    // WTC-FR-QVNL: the record the worktree resolves its store through is part
    // of creating it, so a creation that cannot write it reclaims the worktree
    // and the branch it made rather than leaving one a container cannot read.
    if let Err(reason) = normalize_worktree_commondir(&main, &name) {
        reclaim_partial_worktree(&main, &name, &target, branch, created_branch);
        return Err(format!("failed to create worktree: {reason}"));
    }
    Ok(canonicalize_lenient(&target))
}

/// WTC-FR-AGQQ: the name Git registers the worktree at `target` under — the
/// normalized basename of `target`, or the normalized branch when that
/// basename has no character a worktree name can keep.
pub(super) fn registration_name(target: &Path, branch: &str) -> String {
    let name = normalize_worktree_name(&project::basename(&target.to_string_lossy()));
    if name.is_empty() {
        normalize_worktree_name(branch)
    } else {
        name
    }
}

/// WTC-FR-QVNL: remove what a creation made before it failed.
///
/// Best-effort throughout: the caller is already reporting a failure, and a
/// reclaim that cannot finish must not replace that report with its own.
pub(super) fn reclaim_partial_worktree(
    main: &Repository,
    name: &str,
    target: &Path,
    branch: &str,
    created_branch: bool,
) {
    // The working copy first, and by this module rather than by Git. A worktree
    // whose administrative directory is not in order is one libgit2 declines to
    // prune, and that is exactly the worktree this reclaim is called for — so a
    // prune alone would leave the directory standing.
    if let Some(parent) = target.parent() {
        // Strict on purpose: the target is a worktree the **user** chose, not
        // a directory this application owns, so `delete_owned_tree` refuses it
        // by construction and the symlink refusal of FSA-FR-17 stands.
        if let Ok(fs) = crate::fs::FsAccess::builder().allow_root(parent).build() {
            let _ = fs.delete_path(target, true);
        }
    }
    if let Ok(worktree) = main.find_worktree(name) {
        let mut opts = git2::WorktreePruneOptions::new();
        opts.valid(true).working_tree(true);
        let _ = worktree.prune(Some(&mut opts));
    }
    if created_branch {
        if let Ok(mut found) = main.find_branch(branch, BranchType::Local) {
            let _ = found.delete();
        }
    }
}
