//! What an update reads and writes in Git (GRB-FR-HJZC, GRB-FR-RMKD).
//!
//! Everything here stands on **one pinned revision** of the base branch, which
//! the caller resolved once and validated once (WKS-FR-PWZC). Nothing in this
//! file looks the base branch up again, so a branch that moves while an update
//! runs changes nothing the update does.

use std::collections::BTreeMap;
use std::path::Path;

use super::update_record::StreamUpdateCommit;
use super::*;

/// WKS-FR-UBGX: how many commits of the stream's base branch a listing names.
pub(super) const MISSING_COMMITS_SHOWN: usize = 50;

/// WKS-FR-RJPD: how many commits the base branch holds that the stream does
/// not, with the first of them named.
///
/// Zero means the stream is up to date with its base, which is what disables
/// Update on the row.
pub(super) fn behind_base(
    main: &git2::Repository,
    stream: &WorkStream,
) -> (u32, String, Vec<StreamUpdateCommit>) {
    let Some(head) = branch_tip(main, &stream.branch) else {
        return (0, String::new(), Vec::new());
    };
    let Some(base) = branch_tip(main, &stream.base_branch) else {
        return (0, String::new(), Vec::new());
    };
    let behind = main
        .graph_ahead_behind(head, base)
        .map(|(_, behind)| behind as u32)
        .unwrap_or(0);
    (behind, base.to_string(), missing_commits(main, head, base))
}

/// The revision a local branch names right now.
pub(super) fn branch_tip(main: &git2::Repository, branch: &str) -> Option<git2::Oid> {
    main.find_branch(branch, git2::BranchType::Local)
        .ok()?
        .get()
        .peel_to_commit()
        .ok()
        .map(|commit| commit.id())
}

/// WKS-FR-UBGX: the commits `base` holds that `head` does not, newest first and
/// cut to the bound a listing carries.
pub(super) fn missing_commits(
    main: &git2::Repository,
    head: git2::Oid,
    base: git2::Oid,
) -> Vec<StreamUpdateCommit> {
    walk(main, base, Some(head))
        .into_iter()
        .take(MISSING_COMMITS_SHOWN)
        .filter_map(|oid| main.find_commit(oid).ok())
        .map(|commit| StreamUpdateCommit {
            revision: commit.id().to_string(),
            summary: commit.summary().ok().flatten().unwrap_or("").to_string(),
            author: commit.author().name().unwrap_or("").to_string(),
            committed_at: crate::notes::format_rfc3339_utc(commit.time().seconds()),
        })
        .collect()
}

/// The commits reachable from `from` and not from `hide`, newest first.
fn walk(main: &git2::Repository, from: git2::Oid, hide: Option<git2::Oid>) -> Vec<git2::Oid> {
    let Ok(mut walk) = main.revwalk() else {
        return Vec::new();
    };
    // A replay takes a commit only after its parent, and libgit2 sorts by commit
    // time unless it is told otherwise. Two commits in one second — routine for
    // a tool that commits programmatically — would otherwise replay a child
    // before its parent and conflict against a tree the parent never reached.
    if walk.set_sorting(git2::Sort::TOPOLOGICAL).is_err() {
        return Vec::new();
    }
    if walk.push(from).is_err() {
        return Vec::new();
    }
    if let Some(hide) = hide {
        let _ = walk.hide(hide);
    }
    walk.filter_map(Result::ok).collect()
}

/// The commits the stream branch holds that the pinned revision does not,
/// **oldest first**, which is the order a replay takes them in.
pub(super) fn stream_commits(
    main: &git2::Repository,
    head: git2::Oid,
    base: git2::Oid,
) -> Vec<git2::Oid> {
    let mut ordered = walk(main, head, Some(base));
    ordered.reverse();
    ordered
}

/// WKS-FR-KFVJ: whether the recorded base branch still points at the revision
/// the request pinned.
///
/// Read under the repository update guard and never again, so what the author
/// judged the commit list by is what the update runs on.
pub(super) fn pinned_tip_holds(
    main: &git2::Repository,
    branch: &str,
    pinned: &str,
) -> Result<(), String> {
    let actual = branch_tip(main, branch).map(|oid| oid.to_string());
    match actual {
        Some(actual) if actual == pinned => Ok(()),
        Some(actual) => Err(format!("{ERR_STALE_BASE_REVISION}: {actual}")),
        None => Err(ERR_STALE_BASE_REVISION.to_string()),
    }
}

/// WKS-FR-OKVB: both sides must be clean, and the base branch must have a
/// checkout.
///
/// An update writes only the stream's own working copy, but it reads the base
/// branch's worktree to be sure the author has nothing standing in it that a
/// later act of theirs would reconcile against a revision that has moved.
pub(super) fn refuse_dirty_for_update(
    stream_worktree: &Path,
    base_worktree: Option<&Path>,
) -> Result<(), String> {
    if !stream_worktree.is_dir() {
        return Err(ERR_STREAM_MISSING.to_string());
    }
    // WKS-FR-JVLM: an update writes the stream's working copy and not the
    // base worktree.
    let dirty = git::uncommitted_paths(stream_worktree, true);
    if !dirty.is_empty() {
        return Err(format!("{ERR_STREAM_DIRTY}: {}", dirty.join(", ")));
    }
    let base_worktree = base_worktree.ok_or_else(|| ERR_BASE_NOT_CHECKED_OUT.to_string())?;
    let dirty = git::uncommitted_paths(base_worktree, false);
    if !dirty.is_empty() {
        return Err(format!("{ERR_BASE_DIRTY}: {}", dirty.join(", ")));
    }
    Ok(())
}

/// What a replay of the stream commits onto the pinned revision produced.
pub(super) struct Replay {
    /// The rewritten commit the stream branch would stand on.
    pub(super) tip: git2::Oid,
    /// The tree that tip holds.
    pub(super) tree: git2::Oid,
    /// GRB-FR-WGPS: every path any step of the replay could not settle, once.
    pub(super) unresolved: Vec<String>,
}

/// What one reconciliation pass settled, by path.
///
/// `None` is a path the pass settled by letting a deletion stand. A replay reads
/// this at every step that could not settle that path on its own, so **one**
/// decision reaches every commit it applies to (GRB-FR-WGPS).
pub(super) type Resolutions = BTreeMap<String, Option<(git2::Oid, u32)>>;

/// GRB-FR-RMKD: replay the stream commits onto the pinned revision.
///
/// The commits are written as objects and **no reference moves**, so a replay
/// that is abandoned leaves the stream branch exactly where it was. A step Git
/// cannot settle takes the settled content for that path where `resolutions`
/// holds one, and otherwise records the path and takes the replayed commit's own
/// side, so the replay reaches its end and reports the whole unsettled set at
/// once rather than stopping at the first conflict (GRB-FR-WGPS).
///
/// A replay that reports any unsettled path is **never landed**: its caller
/// reconciles that set in one pass and replays again with what the pass settled,
/// so no commit an author reads carries a side this function chose.
pub(super) fn replay(
    main: &git2::Repository,
    head: git2::Oid,
    pinned: git2::Oid,
    resolutions: &Resolutions,
) -> Result<Replay, String> {
    let mut unresolved: BTreeMap<String, ()> = BTreeMap::new();
    let mut onto = pinned;
    for oid in stream_commits(main, head, pinned) {
        let commit = main.find_commit(oid).map_err(|e| e.to_string())?;
        let onto_commit = main.find_commit(onto).map_err(|e| e.to_string())?;
        let commit_tree = commit.tree().map_err(|e| e.to_string())?;
        let onto_tree = onto_commit.tree().map_err(|e| e.to_string())?;
        let parent_tree = match commit.parent(0) {
            Ok(parent) => parent.tree().map_err(|e| e.to_string())?,
            // A root commit has nothing behind it, so the empty tree is what
            // both sides started from.
            Err(_) => empty_tree(main)?,
        };
        let mut index = main
            .merge_trees(&parent_tree, &onto_tree, &commit_tree, None)
            .map_err(|e| e.to_string())?;
        if index.has_conflicts() {
            for path in resolve_conflicts(&mut index, &commit_tree, resolutions)? {
                unresolved.insert(path, ());
            }
        }
        let tree_oid = index.write_tree_to(main).map_err(|e| e.to_string())?;
        let tree = main.find_tree(tree_oid).map_err(|e| e.to_string())?;
        onto = main
            .commit(
                None,
                &commit.author(),
                &commit.committer(),
                commit.message().unwrap_or_default(),
                &tree,
                &[&onto_commit],
            )
            .map_err(|e| e.to_string())?;
    }
    let tip_commit = main.find_commit(onto).map_err(|e| e.to_string())?;
    Ok(Replay {
        tip: onto,
        tree: tip_commit.tree().map_err(|e| e.to_string())?.id(),
        unresolved: unresolved.into_keys().collect(),
    })
}

/// Settle every path this step could not settle, and report the ones no
/// reconciliation had already answered.
///
/// A conflicted index cannot be written as a tree, so each conflicting path is
/// replaced: with the content one reconciliation pass settled where it has one,
/// and otherwise with the replayed commit's own side. Only the second kind is
/// reported, because only it is still a question (GRB-FR-WGPS).
fn resolve_conflicts(
    index: &mut git2::Index,
    commit_tree: &git2::Tree,
    resolutions: &Resolutions,
) -> Result<Vec<String>, String> {
    let mut conflicting: Vec<String> = Vec::new();
    if let Ok(conflicts) = index.conflicts() {
        for conflict in conflicts.flatten() {
            let entry = conflict
                .our
                .as_ref()
                .or(conflict.their.as_ref())
                .or(conflict.ancestor.as_ref());
            let Some(entry) = entry else { continue };
            conflicting.push(String::from_utf8_lossy(&entry.path).into_owned());
        }
    }
    conflicting.sort();
    conflicting.dedup();

    let mut unresolved: Vec<String> = Vec::new();
    for path in &conflicting {
        let as_path = std::path::Path::new(path);
        index.remove_path(as_path).map_err(|e| e.to_string())?;
        let settled = resolutions.get(path);
        if settled.is_none() {
            unresolved.push(path.clone());
        }
        // A settled deletion writes no entry at all, which is what letting the
        // deletion stand means.
        // The **replayed commit's own** mode wherever it holds the path: a
        // reconciliation settles content and never an executable bit, so a
        // script or a hook keeps the mode the commit gave it. Only where the
        // commit does not hold the path at all does the settled mode stand.
        let commit_side = commit_tree.get_path(as_path).ok();
        let (oid, mode) = match (settled, commit_side) {
            (Some(None), _) => continue,
            (Some(Some((oid, _))), Some(entry)) => (*oid, entry.filemode() as u32),
            (Some(Some((oid, mode))), None) => (*oid, *mode),
            (None, Some(entry)) => (entry.id(), entry.filemode() as u32),
            (None, None) => continue,
        };
        let index_entry = git2::IndexEntry {
            ctime: git2::IndexTime::new(0, 0),
            mtime: git2::IndexTime::new(0, 0),
            dev: 0,
            ino: 0,
            mode,
            uid: 0,
            gid: 0,
            file_size: 0,
            id: oid,
            flags: 0,
            flags_extended: 0,
            path: path.clone().into_bytes(),
        };
        index.add(&index_entry).map_err(|e| e.to_string())?;
    }
    Ok(unresolved)
}

/// GRB-FR-WGPS: what one reconciliation settled for the paths a replay could
/// not, read out of the tree that pass produced.
///
/// A path the tree does not hold was settled by letting its deletion stand.
fn resolutions_from(
    main: &git2::Repository,
    tree_oid: git2::Oid,
    paths: &[String],
    into: &mut Resolutions,
) {
    let Ok(tree) = main.find_tree(tree_oid) else {
        return;
    };
    for path in paths {
        let entry = tree.get_path(std::path::Path::new(path)).ok();
        // The mode travels with the content: a settled path that is executable
        // or a symbolic link is not a plain file, and a rebase that rewrote it
        // as one would break the tree it landed.
        into.insert(
            path.clone(),
            entry.map(|entry| (entry.id(), entry.filemode() as u32)),
        );
    }
}

/// How many times a replay is taken again with what one pass settled.
///
/// A replay carrying an answer reaches a different tree at each step, so a step
/// may reach a path no earlier replay ever asked about. Each pass answers every
/// path the pass before it reported, so the answered set grows by at least one
/// path a pass and the loop ends. The bound is a floor under that argument
/// rather than the mechanism.
const REPLAY_PASSES_MAX: usize = 8;

/// GRB-FR-WGPS: replay the stream onto the pinned revision, answering every
/// path it cannot settle from `settled_tree`.
///
/// The **one** reconciliation pass a rebase spends is what produced
/// `settled_tree` — either a semantic turn's own result or the deterministic
/// merge of the two tips. This asks that one result for each path a replay step
/// could not settle, however many steps ask about it, and takes the replay again
/// until nothing is left unanswered. A replay that still reports a path answers
/// the typed refusal rather than landing a side this module chose.
pub(super) fn replay_settled_by(
    main: &git2::Repository,
    head: git2::Oid,
    pinned: git2::Oid,
    settled_tree: git2::Oid,
    unresolved: &[String],
) -> Result<Replay, String> {
    let mut resolutions: Resolutions = BTreeMap::new();
    resolutions_from(main, settled_tree, unresolved, &mut resolutions);
    for _ in 0..REPLAY_PASSES_MAX {
        let landed = replay(main, head, pinned, &resolutions)?;
        if landed.unresolved.is_empty() {
            return Ok(landed);
        }
        resolutions_from(main, settled_tree, &landed.unresolved, &mut resolutions);
    }
    Err(REPLAY_UNSETTLED.to_string())
}

/// What [`replay_settled_by`] answers when its passes ran out.
///
/// Its own value rather than a typed refusal, because the caller decides what to
/// do about it: the deterministic half asks a semantic turn instead, and a turn
/// that has already run leaves the attempt unsettled.
pub(super) const REPLAY_UNSETTLED: &str = "replay_unsettled";

/// The empty tree, which is what a root commit's parent holds.
fn empty_tree(main: &git2::Repository) -> Result<git2::Tree<'_>, String> {
    let builder = main.treebuilder(None).map_err(|e| e.to_string())?;
    let oid = builder.write().map_err(|e| e.to_string())?;
    main.find_tree(oid).map_err(|e| e.to_string())
}

/// GRB-FR-HJZC: put the pinned revision into the stream branch as one merge
/// commit holding `tree`.
///
/// The stream's own working copy is brought to the tree **before** the branch
/// moves, so a checkout that fails leaves the branch where it was and the
/// update reports a refusal that really did nothing.
pub(super) fn commit_merge_into_stream(
    main: &git2::Repository,
    stream: &WorkStream,
    head: git2::Oid,
    pinned: git2::Oid,
    tree_oid: git2::Oid,
) -> Result<(), String> {
    let message = format!(
        "Merge {} into {}",
        stream.base_branch, stream.branch
    );
    let parents = [head, pinned];
    write_stream_tip(main, stream, tree_oid, &parents, &message)
}

/// GRB-FR-RMKD: move the stream branch to a replayed tip and bring its working
/// copy to it.
///
/// A stream that held nothing of its own is **fast-forwarded** to the pinned
/// revision rather than given a copy of it: a rewritten duplicate would leave
/// the stream one commit ahead of a base branch holding the same tree for ever.
pub(super) fn apply_replay(
    main: &git2::Repository,
    stream: &WorkStream,
    pinned: git2::Oid,
    tip: git2::Oid,
    tree_oid: git2::Oid,
) -> Result<(), String> {
    let stream_repo = open_stream_worktree(main, stream)?;
    if tip == pinned {
        // A fast-forward lands the pinned commit itself, so the tree it lands
        // must be that commit's own. Anything else would leave the reconciled
        // content standing in the working copy under a commit that does not
        // hold it, and the update would report work it never committed.
        let pinned_tree = main
            .find_commit(pinned)
            .and_then(|commit| commit.tree())
            .map(|tree| tree.id())
            .map_err(|e| e.to_string())?;
        if pinned_tree != tree_oid {
            return Err(REPLAY_UNSETTLED.to_string());
        }
        return checkout_and_move(main, &stream_repo, stream, pinned, tree_oid);
    }
    let tip_commit = main.find_commit(tip).map_err(|e| e.to_string())?;
    // The reconciled tree may differ from the one the replay left, so the tip
    // is rewritten onto it rather than reused as it stands.
    let parents: Vec<git2::Oid> = tip_commit.parent_ids().collect();
    let message = tip_commit.message().unwrap_or_default().to_string();
    let tree = main.find_tree(tree_oid).map_err(|e| e.to_string())?;
    let parent_commits: Vec<git2::Commit> = parents
        .iter()
        .filter_map(|oid| main.find_commit(*oid).ok())
        .collect();
    // A parent this repository cannot open would silently make a commit with
    // less history than the replay built, so it is a refusal rather than a
    // truncation.
    if parent_commits.len() != parents.len() {
        return Err(ERR_UNKNOWN_STREAM.to_string());
    }
    let refs: Vec<&git2::Commit> = parent_commits.iter().collect();
    let author = tip_commit.author();
    let committer = tip_commit.committer();
    let new_tip = main
        .commit(None, &author, &committer, &message, &tree, &refs)
        .map_err(|e| e.to_string())?;
    checkout_and_move(main, &stream_repo, stream, new_tip, tree_oid)
}

/// WKS-FR-OKVB: refuse before anything is computed where the stream's own
/// working copy is not one this update may write.
///
/// Taken at the same point as the clean checks, so a stream standing on some
/// other branch is answered at once rather than after a replay and an agent
/// turn.
pub(super) fn refuse_unwritable_stream(
    main: &git2::Repository,
    stream: &WorkStream,
) -> Result<(), String> {
    open_stream_worktree(main, stream).map(|_| ())
}

/// The worktree that really holds the stream's branch.
///
/// Resolved rather than assumed, on the terms `merge.rs` resolves the base
/// branch's: a forced checkout into a worktree standing on some other branch
/// would write that branch's files under a ref this update then moves.
fn open_stream_worktree(
    main: &git2::Repository,
    stream: &WorkStream,
) -> Result<git2::Repository, String> {
    if !stream.worktree().is_dir() {
        return Err(ERR_STREAM_MISSING.to_string());
    }
    let holder = git::worktree_holding(main, &stream.branch)
        .ok_or_else(|| ERR_STREAM_MISSING.to_string())?;
    let holds_the_stream = holder
        .workdir()
        .map(crate::changes::canonicalize_lenient)
        .is_some_and(|path| path == crate::changes::canonicalize_lenient(&stream.worktree()));
    if !holds_the_stream {
        return Err(ERR_STREAM_MISSING.to_string());
    }
    Ok(holder)
}

/// Write one commit onto the stream branch and bring its working copy to it.
fn write_stream_tip(
    main: &git2::Repository,
    stream: &WorkStream,
    tree_oid: git2::Oid,
    parents: &[git2::Oid],
    message: &str,
) -> Result<(), String> {
    let stream_repo = open_stream_worktree(main, stream)?;
    let tree = main.find_tree(tree_oid).map_err(|e| e.to_string())?;
    let signature = main.signature().map_err(|e| e.to_string())?;
    let parent_commits: Vec<git2::Commit> = parents
        .iter()
        .filter_map(|oid| main.find_commit(*oid).ok())
        .collect();
    if parent_commits.len() != parents.len() {
        return Err(ERR_UNKNOWN_STREAM.to_string());
    }
    let refs: Vec<&git2::Commit> = parent_commits.iter().collect();
    let new_tip = main
        .commit(None, &signature, &signature, message, &tree, &refs)
        .map_err(|e| e.to_string())?;
    checkout_and_move(main, &stream_repo, stream, new_tip, tree_oid)
}

/// Bring the stream's working copy to `tree_oid`, then move its branch to
/// `new_tip`.
///
/// The order matters: a checkout that fails leaves the branch where it was, so
/// the update really did nothing. Moving the ref first would leave a branch
/// advanced past a working copy that never received it.
fn checkout_and_move(
    main: &git2::Repository,
    stream_repo: &git2::Repository,
    stream: &WorkStream,
    new_tip: git2::Oid,
    tree_oid: git2::Oid,
) -> Result<(), String> {
    let tree = stream_repo.find_tree(tree_oid).map_err(|e| e.to_string())?;
    let mut opts = git2::build::CheckoutBuilder::new();
    // The stream's working copy was refused unless it was clean (WKS-FR-OKVB),
    // so there is nothing of the author's to overwrite. The index is brought
    // along, so the result reads back as the committed state it is rather than
    // as an uncommitted change of the author's own.
    opts.force().update_index(true);
    stream_repo
        .checkout_tree(tree.as_object(), Some(&mut opts))
        .map_err(|e| e.to_string())?;
    let reference = format!("refs/heads/{}", stream.branch);
    main.reference(&reference, new_tip, true, "synthesis: work stream update")
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// The paths at which two trees hold different content.
///
/// Blob identity alone: a replay puts the replayed commit's own mode back on a
/// settled path, so two trees that agree on every byte may still differ as
/// objects. What matters is that no content one pass settled was dropped.
pub(super) fn content_differences(
    main: &git2::Repository,
    left: git2::Oid,
    right: git2::Oid,
) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    let trees = main
        .find_tree(left)
        .and_then(|left| main.find_tree(right).map(|right| (left, right)));
    let Ok((left, right)) = trees else {
        return vec![String::new()];
    };
    let Ok(diff) = main.diff_tree_to_tree(Some(&left), Some(&right), None) else {
        return vec![String::new()];
    };
    for delta in diff.deltas() {
        if delta.old_file().id() == delta.new_file().id() {
            continue;
        }
        if let Some(path) = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|path| path.to_string_lossy().into_owned())
        {
            paths.push(path);
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

/// The paths an update writes into the stream's working copy.
pub(super) fn written_paths(
    main: &git2::Repository,
    head: git2::Oid,
    result_tree: git2::Oid,
) -> Vec<String> {
    semantic::written_paths(main, head, result_tree)
}
