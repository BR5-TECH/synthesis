//! Committing what a turn wrote onto the stream's branch (GRD-FR-ARLT).
//!
//! The agent never commits: the repository is mounted read-only inside the
//! container (`../tools/EAC-execute-agent-cli.md` EAC-FR-FNFV), so the
//! application commits on the host after the turn ends. That is what makes
//! "run N+1 starts from run N's commit" auditable.

use super::*;

/// What a commit wrote.
pub struct Committed {
    pub revision: String,
    pub paths: Vec<String>,
}

/// GRL-FR-YKRI: one run's change set, as a tree and as the paths it names.
pub struct RunTree {
    /// The base commit's tree, updated at the paths this run changed.
    pub tree: git2::Oid,
    /// Those paths, sorted and unique.
    pub paths: Vec<String>,
}

/// GRD-FR-ARLT: the tree one run's work makes of its base commit.
///
/// **This is the one derivation of a run's change set.** The review turn stands
/// in this tree and the commit of a `ready` verdict holds it, so what the
/// reviewer judged and what landed cannot be two different things.
///
/// Scoped to the diff against the base rather than to everything in the tree,
/// so a path the base commit already holds unchanged is carried through rather
/// than rewritten. `None` means the working copy holds nothing the base does
/// not.
///
/// Nothing of the working copy's own index is written: the walk runs on a
/// scratch index of this function's own, which is what makes the derivation
/// safe to perform in the middle of a run.
pub fn run_tree(worktree: &Path, base_commit: &str) -> Result<Option<RunTree>, String> {
    let repo = git2::Repository::open(worktree).map_err(|e| e.to_string())?;
    let base_oid = git2::Oid::from_str(base_commit).map_err(|e| e.to_string())?;
    let base = repo.find_commit(base_oid).map_err(|e| e.to_string())?;
    let base_tree = base.tree().map_err(|e| e.to_string())?;

    // What the working copy holds against the base, ignore rules respected.
    let mut opts = git2::DiffOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_typechange(true)
        .include_ignored(false);
    let diff = repo
        .diff_tree_to_workdir_with_index(Some(&base_tree), Some(&mut opts))
        .map_err(|e| e.to_string())?;

    let mut paths: Vec<String> = Vec::new();
    for delta in diff.deltas() {
        for file in [delta.new_file(), delta.old_file()] {
            if let Some(path) = file.path() {
                paths.push(path.to_string_lossy().into_owned());
            }
        }
    }
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        return Ok(None);
    }

    // A scratch index the repository owns for this call alone. Git's own walk
    // is what settles a file's mode, a symbolic link and the repository's
    // ignore rules; reading those back by hand is how each of them gets lost.
    let mut scratch = git2::Index::new().map_err(|e| e.to_string())?;
    repo.set_index(&mut scratch).map_err(|e| e.to_string())?;
    scratch
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .map_err(|e| e.to_string())?;

    let mut update = git2::build::TreeUpdateBuilder::new();
    for path in &paths {
        match scratch.get_path(Path::new(path), 0) {
            Some(entry) => {
                update.upsert(path.as_str(), entry.id, crate::git::tree_mode(entry.mode));
            }
            // Absent from the walk means the path is gone. Removing one the
            // base tree never held is not a no-op in libgit2, so it is skipped.
            None => {
                if base_tree.get_path(Path::new(path)).is_ok() {
                    update.remove(path.as_str());
                }
            }
        }
    }
    let tree = update
        .create_updated(&repo, &base_tree)
        .map_err(|e| e.to_string())?;
    Ok(Some(RunTree { tree, paths }))
}

/// GRD-FR-ARLT: commit the paths that differ from `base_commit`.
///
/// The tree is the one [`run_tree`] derived, which is the tree the review turn
/// stood in.
pub fn commit_run_work(
    worktree: &Path,
    base_commit: &str,
    message: &str,
) -> Result<Option<Committed>, String> {
    let derived = run_tree(worktree, base_commit)?;
    let repo = git2::Repository::open(worktree).map_err(|e| e.to_string())?;
    let head = repo
        .head()
        .and_then(|h| h.peel_to_commit())
        .map_err(|e| e.to_string())?;
    let derived = match derived {
        Some(derived) => derived,
        // GRD-FR-ARLT: a working copy that holds the base again, after an
        // abandoned commit moved the branch, commits the base tree back.
        None => {
            let base_oid = git2::Oid::from_str(base_commit).map_err(|e| e.to_string())?;
            let base = repo.find_commit(base_oid).map_err(|e| e.to_string())?;
            RunTree {
                tree: base.tree_id(),
                paths: paths_between(&repo, head.tree_id(), base.tree_id())?,
            }
        }
    };
    // GRD-FR-ARLT / GRD-FR-SWOJ: work the branch head already holds, such as a
    // resumed turn that wrote nothing after its abandoned commit, creates no
    // commit.
    if head.tree_id() == derived.tree {
        return Ok(None);
    }
    // GTC-FR-19: the commit moves `HEAD` and resets the index, so it takes the
    // same hold. `run_tree` above needs none: it walks a scratch index of its
    // own and writes nothing of the worktree's.
    let _lock = crate::git::index_lock::IndexLock::try_acquire(&repo)?
        .ok_or_else(|| "the repository index is held by another write".to_string())?;
    let tree = repo.find_tree(derived.tree).map_err(|e| e.to_string())?;
    let signature = repo.signature().map_err(|e| e.to_string())?;
    let oid = repo
        .commit(Some("HEAD"), &signature, &signature, message, &tree, &[&head])
        .map_err(|e| e.to_string())?;

    // The index is brought to what was just committed. Left at the revision
    // before it, every committed path would read back to the author as an
    // uncommitted change of their own.
    let committed = repo.find_object(oid, None).map_err(|e| e.to_string())?;
    repo.reset(&committed, git2::ResetType::Mixed, None)
        .map_err(|e| e.to_string())?;
    Ok(Some(Committed {
        revision: oid.to_string(),
        paths: derived.paths,
    }))
}

/// GRD-FR-WQTN: rewrite the message of the branch head, where the head is the
/// named revision and its message opens with `title`, and answer the revision
/// the rewrite made.
///
/// The tree and the parents are kept, so the branch holds the same work under
/// the message the run completes with.
pub fn retitle_head_commit(
    worktree: &Path,
    revision: &str,
    title: &str,
    message: &str,
) -> Result<Option<String>, String> {
    let repo = git2::Repository::open(worktree).map_err(|e| e.to_string())?;
    let head = repo
        .head()
        .and_then(|h| h.peel_to_commit())
        .map_err(|e| e.to_string())?;
    if head.id().to_string() != revision || !head.message().unwrap_or_default().starts_with(title) {
        return Ok(None);
    }
    // GTC-FR-19: the rewrite moves `HEAD`, so it takes the index hold.
    let _lock = crate::git::index_lock::IndexLock::try_acquire(&repo)?
        .ok_or_else(|| "the repository index is held by another write".to_string())?;
    let rewritten = head
        .amend(Some("HEAD"), None, None, None, Some(message), None)
        .map_err(|e| e.to_string())?;
    Ok(Some(rewritten.to_string()))
}

/// The paths two trees of one repository differ at, sorted and unique.
fn paths_between(repo: &git2::Repository, from: git2::Oid, to: git2::Oid) -> Result<Vec<String>, String> {
    let from = repo.find_tree(from).map_err(|e| e.to_string())?;
    let to = repo.find_tree(to).map_err(|e| e.to_string())?;
    let diff = repo
        .diff_tree_to_tree(Some(&from), Some(&to), None)
        .map_err(|e| e.to_string())?;
    let mut paths: Vec<String> = Vec::new();
    for delta in diff.deltas() {
        for file in [delta.new_file(), delta.old_file()] {
            if let Some(path) = file.path() {
                paths.push(path.to_string_lossy().into_owned());
            }
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// GRD-FR-HQPD / GRD-FR-RJFC: commit what the author left standing in the
/// stream, as the run's starting point, under the message the run carries.
///
/// The author chose this when they enqueued the run, so the commit is theirs
/// rather than the agent's.
pub fn commit_author_work(worktree: &Path, message: &str) -> Result<Option<String>, String> {
    let repo = git2::Repository::open(worktree).map_err(|e| e.to_string())?;
    // GTC-FR-19: this writes the worktree's own index, so it takes the hold the
    // draft-event commit and an outside `git` are excluded by.
    let _lock = crate::git::index_lock::IndexLock::try_acquire(&repo)?
        .ok_or_else(|| "the repository index is held by another write".to_string())?;
    let mut status = git2::StatusOptions::new();
    status
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let dirty = repo
        .statuses(Some(&mut status))
        .map(|s| s.iter().any(|e| !e.status().is_empty()))
        .unwrap_or(false);
    if !dirty {
        return Ok(None);
    }
    let mut index = repo.index().map_err(|e| e.to_string())?;
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .map_err(|e| e.to_string())?;
    index.write().map_err(|e| e.to_string())?;
    let tree_oid = index.write_tree().map_err(|e| e.to_string())?;
    let tree = repo.find_tree(tree_oid).map_err(|e| e.to_string())?;
    let head = repo
        .head()
        .and_then(|h| h.peel_to_commit())
        .map_err(|e| e.to_string())?;
    let signature = repo.signature().map_err(|e| e.to_string())?;
    let oid = repo
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &[&head],
        )
        .map_err(|e| e.to_string())?;
    Ok(Some(oid.to_string()))
}

/// GRD-FR-BLCR: revert the commits one run made, as new commits.
///
/// Rewrites no history: what a run did stays in the log, and the revert stands
/// beside it.
pub fn revert_run_commits(worktree: &Path, revisions: &[String]) -> Result<usize, String> {
    if revisions.is_empty() {
        return Err(ERR_RUN_NOT_REVERTABLE.to_string());
    }
    let repo = git2::Repository::open(worktree).map_err(|e| e.to_string())?;
    let mut reverted = 0usize;
    // Newest first, so each revert applies against the tree the one before it
    // left behind.
    for revision in revisions.iter().rev() {
        let oid = git2::Oid::from_str(revision).map_err(|e| e.to_string())?;
        let commit = repo.find_commit(oid).map_err(|e| e.to_string())?;
        repo.revert(&commit, None).map_err(|e| e.to_string())?;
        let mut index = repo.index().map_err(|e| e.to_string())?;
        if index.has_conflicts() {
            repo.cleanup_state().ok();
            return Err(format!("revert of {revision} conflicts"));
        }
        let tree_oid = index.write_tree().map_err(|e| e.to_string())?;
        let tree = repo.find_tree(tree_oid).map_err(|e| e.to_string())?;
        let head = repo
            .head()
            .and_then(|h| h.peel_to_commit())
            .map_err(|e| e.to_string())?;
        let signature = repo.signature().map_err(|e| e.to_string())?;
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            &format!("Revert {}", &revision[..revision.len().min(12)]),
            &tree,
            &[&head],
        )
        .map_err(|e| e.to_string())?;
        repo.cleanup_state().ok();
        reverted += 1;
    }
    Ok(reverted)
}
