//! Merging a work stream into the branch it was created from
//! (WKS-FR-GKPX, WKS-FR-RAOM, WKS-FR-UZHT).
//!
//! **Git settles what Git can settle.** The merge is computed in memory first,
//! so a conflict is known before any byte of either working copy is written and
//! a merge that conflicts leaves both trees exactly as they were. A conflict
//! Git cannot settle is reported rather than resolved here; a merge run
//! reconciles it, and this module dispatches no turn.

use std::path::Path;

use super::*;

/// What a merge found, before anything is written.
pub(super) struct Prepared {
    pub(super) index: git2::Index,
    pub(super) merged_paths: Vec<String>,
    pub(super) conflicted_paths: Vec<String>,
    pub(super) up_to_date: bool,
    pub(super) stream_oid: git2::Oid,
    pub(super) base_oid: git2::Oid,
}

/// WKS-FR-UZHT: both sides must be clean before anything is compared.
///
/// The author commits their own work under a message they wrote; this module
/// commits, stashes, resets and cleans nothing on their behalf.
pub(super) fn refuse_dirty(
    stream_worktree: &Path,
    base_worktree: Option<&Path>,
) -> Result<(), String> {
    // A working copy that cannot be opened reports no uncommitted path, which
    // would read as clean. A stream whose directory has gone is refused here
    // rather than merged from a tree nothing can see (WKS-FR-AXRD).
    if !stream_worktree.is_dir() {
        return Err(ERR_STREAM_MISSING.to_string());
    }
    // WKS-FR-JVLM: a merge writes the base worktree and not the stream's.
    let dirty = git::uncommitted_paths(stream_worktree, false);
    if !dirty.is_empty() {
        return Err(format!("{ERR_STREAM_DIRTY}: {}", dirty.join(", ")));
    }
    // A base branch no worktree holds has no tree to be dirty.
    if let Some(base_worktree) = base_worktree {
        let dirty = git::uncommitted_paths(base_worktree, true);
        if !dirty.is_empty() {
            return Err(format!("{ERR_BASE_DIRTY}: {}", dirty.join(", ")));
        }
    }
    Ok(())
}

/// The commit a local branch names, or the typed refusal where it names none.
pub(super) fn branch_tip(main: &git2::Repository, name: &str) -> Result<git2::Oid, String> {
    main.find_branch(name, git2::BranchType::Local)
        .map_err(|_| ERR_UNKNOWN_STREAM.to_string())?
        .get()
        .peel_to_commit()
        .map(|commit| commit.id())
        .map_err(|_| ERR_UNKNOWN_STREAM.to_string())
}

/// Compute the merge without writing anything.
///
/// The whole point of preparing separately is WKS-FR-RAOM: a caller learns what
/// conflicts before either working copy has been touched.
pub(super) fn prepare(main: &git2::Repository, stream: &WorkStream) -> Result<Prepared, String> {
    let resolve = |name: &str| -> Result<git2::Oid, String> {
        main.find_branch(name, git2::BranchType::Local)
            .map_err(|_| ERR_UNKNOWN_STREAM.to_string())?
            .get()
            .peel_to_commit()
            .map(|c| c.id())
            .map_err(|_| ERR_UNKNOWN_STREAM.to_string())
    };
    let stream_oid = resolve(&stream.branch)?;
    let base_oid = resolve(&stream.base_branch)?;

    // Nothing the base does not already hold is not a merge of nothing: it is
    // a distinct outcome, and the caller's next act is the one act that needs
    // no merge reported.
    let ahead = main
        .graph_ahead_behind(stream_oid, base_oid)
        .map(|(ahead, _)| ahead)
        .unwrap_or(0);
    if ahead == 0 {
        return Ok(Prepared {
            index: main.index().map_err(|e| e.to_string())?,
            merged_paths: Vec::new(),
            conflicted_paths: Vec::new(),
            up_to_date: true,
            stream_oid,
            base_oid,
        });
    }

    prepare_between(main, base_oid, stream_oid)
}

/// The same comparison, between two revisions the caller already holds.
///
/// An update stands on a **pinned** revision of the base branch rather than on
/// whatever that branch names now (per `WKS-work-streams.md` WKS-FR-PWZC), so it
/// reaches the comparison through this rather than through [`prepare`].
pub(super) fn prepare_between(
    main: &git2::Repository,
    base_oid: git2::Oid,
    stream_oid: git2::Oid,
) -> Result<Prepared, String> {
    let stream_commit = main.find_commit(stream_oid).map_err(|e| e.to_string())?;
    let base_commit = main.find_commit(base_oid).map_err(|e| e.to_string())?;
    let index = main
        .merge_commits(&base_commit, &stream_commit, None)
        .map_err(|e| e.to_string())?;

    let mut conflicted_paths: Vec<String> = Vec::new();
    if index.has_conflicts() {
        if let Ok(conflicts) = index.conflicts() {
            for conflict in conflicts.flatten() {
                let entry = conflict
                    .our
                    .as_ref()
                    .or(conflict.their.as_ref())
                    .or(conflict.ancestor.as_ref());
                if let Some(entry) = entry {
                    conflicted_paths.push(String::from_utf8_lossy(&entry.path).into_owned());
                }
            }
        }
        conflicted_paths.sort();
        conflicted_paths.dedup();
    }

    // The paths the merge would write: everything that differs between the base
    // tree and the merged result.
    let mut merged_paths: Vec<String> = Vec::new();
    if conflicted_paths.is_empty() {
        let base_tree = base_commit.tree().map_err(|e| e.to_string())?;
        let mut probe = index;
        let merged_tree_oid = probe.write_tree_to(main).map_err(|e| e.to_string())?;
        let merged_tree = main.find_tree(merged_tree_oid).map_err(|e| e.to_string())?;
        let diff = main
            .diff_tree_to_tree(Some(&base_tree), Some(&merged_tree), None)
            .map_err(|e| e.to_string())?;
        for delta in diff.deltas() {
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.to_string_lossy().into_owned());
            if let Some(path) = path {
                merged_paths.push(path);
            }
        }
        merged_paths.sort();
        merged_paths.dedup();
        return Ok(Prepared {
            index: probe,
            merged_paths,
            conflicted_paths,
            up_to_date: false,
            stream_oid,
            base_oid,
        });
    }

    Ok(Prepared {
        index: main.index().map_err(|e| e.to_string())?,
        merged_paths,
        conflicted_paths,
        up_to_date: false,
        stream_oid,
        base_oid,
    })
}

/// WKS-FR-GKPX: write a prepared merge into the base branch.
///
/// Called only for a preparation that found no conflict, so what it writes is
/// a tree Git settled on its own. `base` is the repository handle of the
/// worktree that actually holds the base branch, which need not be the
/// primary one. The answer is the merge commit, where the publication made one.
pub(super) fn apply(
    main: &git2::Repository,
    base: Option<&git2::Repository>,
    stream: &WorkStream,
    prepared: &mut Prepared,
    publication: &StreamMergePublication,
) -> Result<Option<String>, String> {
    let merged_tree_oid = prepared.index.write_tree_to(main).map_err(|e| e.to_string())?;
    apply_tree(
        main,
        base,
        &stream.base_branch,
        prepared.base_oid,
        prepared.stream_oid,
        merged_tree_oid,
        publication,
    )
}

/// The same, for a tree the caller already holds and the two revisions it was
/// built from.
///
/// A merge run applies the tree its worktree settled, which is not a tree the
/// index of a clean merge holds. The two revisions are the pinned tips the
/// caller verified under the repository update guard, and the commit a
/// `commit` publication makes has both as its parents.
pub(super) fn apply_tree(
    main: &git2::Repository,
    base: Option<&git2::Repository>,
    base_branch: &str,
    base_oid: git2::Oid,
    stream_oid: git2::Oid,
    merged_tree_oid: git2::Oid,
    publication: &StreamMergePublication,
) -> Result<Option<String>, String> {
    match publication {
        // Apply into the base branch's own worktree and stop: no commit, no
        // ref moved. With no worktree holding that branch there is nothing to
        // apply into, and leaving the result nowhere would report a merge that
        // did not happen.
        StreamMergePublication::Uncommitted => {
            let base = base.ok_or_else(|| ERR_BASE_NOT_CHECKED_OUT.to_string())?;
            let tree = base.find_tree(merged_tree_oid).map_err(|e| e.to_string())?;
            // Forced, and the index deliberately left alone: the base
            // worktree was refused unless it was clean (WKS-FR-UZHT), so there
            // is nothing of the author's to overwrite, and an index that still
            // names the old revision is what makes the result read back as the
            // unstaged change the caller asked for. A safe checkout would skip
            // every path the merge changed, because a moved `HEAD` makes an
            // unchanged working file look like the author's own edit.
            // The index is deliberately NOT updated: libgit2 updates it by
            // default, and an updated index would make the result read back as
            // a staged change when the request asked for an unstaged one.
            let mut opts = git2::build::CheckoutBuilder::new();
            opts.force().remove_untracked(false).update_index(false);
            base.checkout_tree(tree.as_object(), Some(&mut opts))
                .map_err(|e| e.to_string())?;
            Ok(None)
        }
        StreamMergePublication::Commit { message } => {
            let base_commit = main.find_commit(base_oid).map_err(|e| e.to_string())?;
            let stream_commit = main.find_commit(stream_oid).map_err(|e| e.to_string())?;
            let merged_tree = main.find_tree(merged_tree_oid).map_err(|e| e.to_string())?;
            let signature = main.signature().map_err(|e| e.to_string())?;

            // The working copy is brought to the merge **before** the ref
            // moves. A checkout that fails then leaves the branch where it was,
            // so the merge reports a refusal and really did nothing; moving the
            // ref first would leave a branch advanced past a working copy that
            // never received it, and the retry behind it would find nothing to
            // merge and call the half-landed merge complete.
            //
            // The index is brought along, so the commit that follows records
            // the merge rather than the author being shown every merged path as
            // an uncommitted change of their own.
            if let Some(base) = base {
                // GRD-FR-JSBE: an apply that fails leaves no part of the result in
                // the working copy. The index is written once, unchanged, before
                // the checkout, so an index lock that another process holds
                // refuses the apply before any file is changed.
                let mut index = base.index().map_err(|e| e.to_string())?;
                index.read(true).map_err(|e| e.to_string())?;
                index.write().map_err(|e| e.to_string())?;
                let tree = base.find_tree(merged_tree_oid).map_err(|e| e.to_string())?;
                let mut opts = git2::build::CheckoutBuilder::new();
                opts.force().update_index(true);
                base.checkout_tree(tree.as_object(), Some(&mut opts))
                    .map_err(|e| e.to_string())?;
            }
            let reference = format!("refs/heads/{base_branch}");
            let oid = main
                .commit(
                    Some(&reference),
                    &signature,
                    &signature,
                    message,
                    &merged_tree,
                    &[&base_commit, &stream_commit],
                )
                .map_err(|e| e.to_string())?;
            Ok(Some(oid.to_string()))
        }
    }
}
