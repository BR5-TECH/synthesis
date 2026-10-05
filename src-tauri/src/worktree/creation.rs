//! Proposing and creating a worktree
//! (`WTC-worktree-context.md` WTC-FR-13, WTC-FR-14, WTC-FR-15, WTC-FR-QVNL).
//!
//! Split out of `worktree.rs` so that file holds the enumeration, the
//! re-rooting and the commands, and this one holds the two additive acts a new
//! checkout takes: the path proposed for it and its creation.

use super::*;

/// Replace every character that is awkward in a path segment. Applied to a
/// branch's leaf (`branch_leaf`), which can still hold characters a directory
/// name should not — a space, a colon, a backslash.
pub(super) fn sanitize_segment(branch: &str) -> String {
    branch
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => c,
            _ => '-',
        })
        .collect()
}

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
    branch
        .split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .next_back()
        .unwrap_or("")
}

/// WTC-FR-13: an absolute sibling of the repository's primary worktree, named
/// `<primary-basename>-<leaf>`. Read-only — creates nothing and does not check
/// the path for availability.
pub fn propose_path_for(active_root: &Path, branch: &str) -> Result<String, String> {
    let repo = open_repo(active_root)?;
    let primary = primary_worktree_root(&repo).ok_or_else(|| ERR_NOT_A_REPO.to_string())?;
    let base = project::basename(&primary.to_string_lossy());
    let suffix = sanitize_segment(branch_leaf(branch));
    let name = if suffix.is_empty() {
        base
    } else {
        format!("{base}-{suffix}")
    };
    // A primary worktree at the filesystem root has no parent; siblings of
    // `/` are its own children, and the result stays absolute either way.
    let parent = primary.parent().unwrap_or(primary.as_path());
    Ok(to_string_path(&parent.join(name)))
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

/// WTC-FR-14 / WTC-FR-15: create a worktree at `path` with `branch` checked
/// out, creating `branch` from the active worktree's `HEAD` when no branch of
/// that name exists. Returns the canonical path of the new worktree.
pub fn create_worktree_at(
    active_root: &Path,
    branch: &str,
    path: &str,
) -> Result<PathBuf, String> {
    create_worktree_seamed(active_root, branch, path, &NoCreateSeam)
}

pub(super) fn create_worktree_seamed(
    active_root: &Path,
    branch: &str,
    path: &str,
    seam: &dyn CreateSeam,
) -> Result<PathBuf, String> {
    let repo = open_repo(active_root)?;
    let branch = branch.trim();
    if branch.is_empty() {
        return Err(ERR_UNKNOWN_BRANCH.to_string());
    }
    let target = canonicalize_lenient(Path::new(path));
    if target.exists() {
        return Err(ERR_WORKTREE_PATH_EXISTS.to_string());
    }
    // A branch that already has a worktree cannot get a second one.
    if worktree_entries(&repo)?
        .iter()
        .any(|w| w.branch.as_deref() == Some(branch))
    {
        return Err(ERR_BRANCH_ALREADY_CHECKED_OUT.to_string());
    }

    // libgit2 registers worktrees on the primary repository.
    let primary = primary_worktree_root(&repo).ok_or_else(|| ERR_NOT_A_REPO.to_string())?;
    let main = Repository::open(&primary).map_err(|_| ERR_NOT_A_REPO.to_string())?;

    let mut created_branch = false;
    if main.find_branch(branch, BranchType::Local).is_err() {
        // WTC-FR-14: created from the active worktree's HEAD.
        let head = repo
            .head()
            .and_then(|h| h.peel_to_commit())
            .map_err(|e| format!("failed to resolve HEAD: {e}"))?;
        let head = main
            .find_commit(head.id())
            .map_err(|e| format!("failed to resolve HEAD: {e}"))?;
        main.branch(branch, &head, false)
            .map_err(|e| format!("failed to create branch: {e}"))?;
        created_branch = true;
    }

    let reference = main
        .find_branch(branch, BranchType::Local)
        .map_err(|_| ERR_UNKNOWN_BRANCH.to_string())?
        .into_reference();
    let mut opts = git2::WorktreeAddOptions::new();
    opts.reference(Some(&reference));
    let name = project::basename(&target.to_string_lossy());
    main.worktree(&name, &target, Some(&opts))
        .map_err(|e| format!("failed to create worktree: {e}"))?;
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
