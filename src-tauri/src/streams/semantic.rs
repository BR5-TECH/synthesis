//! The one turn a merge Git could not settle asks for (GRB-FR-NNLS).
//!
//! The turn stands in a **throwaway checkout of its own**, holding the
//! deterministic merge with conflict markers where Git stopped. Nothing it
//! writes reaches the stream branch, the base branch, or either working copy
//! until the application has read what it left and applied it, so a turn that
//! escalates or fails leaves every tree byte-identical (GRB-FR-UHFE).

use std::collections::BTreeMap;

use super::artifact::{ConflictSides, MergeArtifact, MergeConflict, SideChange};
use super::*;

/// What a conflict marker names each side. The instruction a turn reads names
/// them identically.
pub(super) const MARKER_BASE: &str = "the base branch";
pub(super) const MARKER_STREAM: &str = "the stream";
pub(super) const MARKER_ANCESTOR: &str = "what both started from";
/// What a marked deletion says stood on the side that removed the path.
pub(super) const MARKER_DELETED_SIDE: &str = "this side";

/// The branch a throwaway checkout stands on, so the worktree registration has
/// a reference of its own and never borrows the stream's.
fn scratch_branch(attempt_id: &str) -> String {
    format!("synthesis/merge/{attempt_id}")
}

/// What one staged attempt holds.
pub(super) struct Staged {
    /// The paths the turn is asked about, in path order.
    pub(super) conflicts: Vec<MergeConflict>,
    /// The revision both branches last shared.
    pub(super) merge_base: String,
    /// The deterministic merge, as a tree, with every conflicting path left
    /// out of it. What the turn settles is put back into this rather than a
    /// tree being rebuilt from the checkout's working copy — a working copy
    /// holds no record of which paths are tracked, so rebuilding from it drops
    /// every tracked path an ignore rule matches and takes in whatever else the
    /// turn happened to write.
    pub(super) merged_tree: git2::Oid,
    /// The two revisions the merged tree in the checkout was built from.
    ///
    /// A turn may run for hours, and either branch can move while it does. What
    /// the turn corrected answers **these** two revisions and no others, so an
    /// attempt that finds either has moved is abandoned rather than applied
    /// (GRB-FR-UHFE).
    pub(super) base_oid: git2::Oid,
    pub(super) stream_oid: git2::Oid,
}

/// Build the throwaway checkout and the artifact for one attempt.
///
/// The checkout is brought to the merged index rather than to either branch,
/// so what the turn stands in is the deterministic merge with markers where
/// Git could not settle a path.
pub(super) fn stage(
    fs: &fsa::FsAccess,
    main: &git2::Repository,
    prepared: &mut merge::Prepared,
    artifact: &MergeArtifact,
) -> Result<Staged, String> {
    let merge_base = main
        .merge_base(prepared.base_oid, prepared.stream_oid)
        .map_err(|e| e.to_string())?;

    store::ensure_dir(fs, &artifact.root)?;
    let base_commit = main.find_commit(prepared.base_oid).map_err(|e| e.to_string())?;
    let branch = scratch_branch(&artifact.attempt_id);
    main.branch(&branch, &base_commit, true)
        .map_err(|e| e.to_string())?;
    let reference = main
        .find_branch(&branch, git2::BranchType::Local)
        .map_err(|e| e.to_string())?
        .into_reference();
    let mut opts = git2::WorktreeAddOptions::new();
    opts.reference(Some(&reference));
    main.worktree(&artifact.attempt_id, &artifact.checkout, Some(&mut opts))
        .map_err(|e| e.to_string())?;
    crate::worktree::normalize_worktree_commondir(main, &artifact.attempt_id)
        .map_err(|e| e.to_string())?;

    // The merge is computed again through the checkout's own handle. libgit2
    // refuses an index that belongs to another repository, and the result is
    // the same deterministic merge: what it costs is two tree walks beside a
    // turn that costs a container.
    let checkout = git2::Repository::open(&artifact.checkout).map_err(|e| e.to_string())?;
    let ours = checkout
        .find_commit(prepared.base_oid)
        .map_err(|e| e.to_string())?;
    let theirs = checkout
        .find_commit(prepared.stream_oid)
        .map_err(|e| e.to_string())?;
    let mut merged = checkout
        .merge_commits(&ours, &theirs, None)
        .map_err(|e| e.to_string())?;
    // A path libgit2 could not merge is written with conflict markers, so the
    // content both sides started from stands inside the file the turn corrects.
    // The labels are the two branches by role rather than by name: the
    // instruction the turn reads names them the same way, so a marker reads as
    // the thing the question already told it about.
    let mut builder = git2::build::CheckoutBuilder::new();
    builder
        .force()
        .conflict_style_diff3(true)
        .our_label(MARKER_BASE)
        .their_label(MARKER_STREAM)
        .ancestor_label(MARKER_ANCESTOR);
    checkout
        .checkout_index(Some(&mut merged), Some(&mut builder))
        .map_err(|e| e.to_string())?;

    let sides = conflict_sides(&merged);
    // GRB-FR-SRVN: a deletion on one side of a path the other side changed is a
    // conflict, and libgit2 writes **no marker** for one — it writes the
    // surviving side verbatim. A turn that changed nothing would then leave a
    // file this application could not tell from a settled one, so the two sides
    // are written as a marked file here, exactly as a text conflict is.
    for side in &sides {
        if side.ours.is_some() && side.theirs.is_some() {
            continue;
        }
        write_deletion_conflict(fs, &checkout, artifact, side)?;
    }
    let base_changes = changes_between(main, merge_base, prepared.base_oid);
    let stream_changes = changes_between(main, merge_base, prepared.stream_oid);

    let conflicts: Vec<MergeConflict> = sides
        .iter()
        .map(|side| MergeConflict {
            path: side.path.clone(),
            base_change: change_of(&base_changes, &side.path).as_str().to_string(),
            stream_change: change_of(&stream_changes, &side.path).as_str().to_string(),
            merged_file: format!(
                "{}/{}/{}",
                crate::tools::agent_exec::SEMANTIC_REBASE_TARGET,
                artifact::MIRROR_MERGED,
                side.path
            ),
        })
        .collect();

    // GRB-FR-AAVK: every other path the merge wrote, named once with what each
    // side did to it.
    let mut other: Vec<(String, SideChange, SideChange)> = Vec::new();
    let mut named: Vec<&String> = base_changes.keys().chain(stream_changes.keys()).collect();
    named.sort();
    named.dedup();
    for path in named {
        if sides.iter().any(|side| &side.path == path) {
            continue;
        }
        other.push((
            path.clone(),
            change_of(&base_changes, path),
            change_of(&stream_changes, path),
        ));
    }

    artifact::generate(fs, main, artifact, &sides, &other)?;
    Ok(Staged {
        merged_tree: merged_without_conflicts(&checkout, &mut merged)?,
        conflicts,
        merge_base: merge_base.to_string(),
        base_oid: prepared.base_oid,
        stream_oid: prepared.stream_oid,
    })
}

/// The deterministic merge with what the turn settled folded into it.
///
/// **Built from the merged index, not from the working copy.** A working copy
/// holds no record of which paths a repository tracks, so a tree rebuilt from
/// one drops every tracked path an ignore rule matches — deleting it from the
/// author's branch — and takes in whatever else the turn happened to write. The
/// merge already computed every path but the conflicting ones; those, and only
/// those, are read back out of the checkout.
pub(super) fn harvest(
    fs: &fsa::FsAccess,
    artifact: &MergeArtifact,
    merged_tree: git2::Oid,
    conflicts: &[MergeConflict],
) -> Result<git2::Oid, String> {
    let checkout = git2::Repository::open(&artifact.checkout).map_err(|e| e.to_string())?;
    let tree = checkout.find_tree(merged_tree).map_err(|e| e.to_string())?;
    let mut index = git2::Index::new().map_err(|e| e.to_string())?;
    index.read_tree(&tree).map_err(|e| e.to_string())?;

    for conflict in conflicts {
        let path = std::path::Path::new(&conflict.path);
        match fs.read_bytes(artifact.checkout.join(&conflict.path)) {
            Ok(bytes) => {
                let oid = checkout.blob(&bytes).map_err(|e| e.to_string())?;
                let entry = git2::IndexEntry {
                    ctime: git2::IndexTime::new(0, 0),
                    mtime: git2::IndexTime::new(0, 0),
                    dev: 0,
                    ino: 0,
                    // A regular file. The merge carries no mode of its own for a
                    // path neither side could settle, and an executable bit is
                    // not what a semantic turn is asked about. A replay puts the
                    // replayed commit's own mode back (per `update_git.rs`).
                    mode: 0o100644,
                    uid: 0,
                    gid: 0,
                    file_size: bytes.len() as u32,
                    id: oid,
                    flags: 0,
                    flags_extended: 0,
                    path: conflict.path.clone().into_bytes(),
                };
                index.add(&entry).map_err(|e| e.to_string())?;
            }
            // The turn settled the question by letting the deletion stand.
            Err(_) => {
                let _ = index.remove_path(path);
            }
        }
    }
    index.write_tree_to(&checkout).map_err(|e| e.to_string())
}

/// The merged index as a tree, with every conflicting path removed from it.
///
/// A conflicted index cannot be written as a tree at all, so the conflicting
/// paths come out and the turn's answers go back in afterwards.
fn merged_without_conflicts(
    checkout: &git2::Repository,
    merged: &mut git2::Index,
) -> Result<git2::Oid, String> {
    let paths: Vec<Vec<u8>> = merged
        .conflicts()
        .map(|conflicts| {
            conflicts
                .flatten()
                .filter_map(|conflict| {
                    conflict
                        .our
                        .as_ref()
                        .or(conflict.their.as_ref())
                        .or(conflict.ancestor.as_ref())
                        .map(|entry| entry.path.clone())
                })
                .collect()
        })
        .unwrap_or_default();
    for path in paths {
        let path = String::from_utf8_lossy(&path).into_owned();
        merged
            .remove_path(std::path::Path::new(&path))
            .map_err(|e| e.to_string())?;
    }
    merged.write_tree_to(checkout).map_err(|e| e.to_string())
}

/// GRB-FR-SRVN: the two sides of a path one side deleted, written as a marked
/// file so the turn has the same thing to settle a text conflict gives it.
fn write_deletion_conflict(
    fs: &fsa::FsAccess,
    checkout: &git2::Repository,
    artifact: &MergeArtifact,
    side: &ConflictSides,
) -> Result<(), String> {
    let body = |oid: Option<git2::Oid>| -> String {
        match oid.and_then(|oid| checkout.find_blob(oid).ok()) {
            Some(blob) => String::from_utf8_lossy(blob.content()).into_owned(),
            None => format!("(this path was deleted on {MARKER_DELETED_SIDE})\n"),
        }
    };
    let text = format!(
        "<<<<<<< {MARKER_BASE}\n{}||||||| {MARKER_ANCESTOR}\n{}=======\n{}>>>>>>> {MARKER_STREAM}\n",
        ended(&body(side.ours)),
        ended(&body(side.ancestor)),
        ended(&body(side.theirs)),
    );
    let target = artifact.checkout.join(&side.path);
    if let Some(parent) = target.parent() {
        store::ensure_dir(fs, parent)?;
    }
    fs.write_text_atomic(&target, &text).map_err(|e| e.to_string())
}

/// A section of a conflict marker ends on its own line.
pub(super) fn ended(text: &str) -> String {
    if text.is_empty() || text.ends_with('\n') {
        text.to_string()
    } else {
        format!("{text}\n")
    }
}

/// Whether the turn left any conflict marker standing, anywhere.
///
/// **Every path the merged tree changed** is read, not only the paths a
/// question named. The turn's execution directory is the whole checkout, so a
/// marker it wrote into a path nobody asked it about would otherwise reach the
/// author's history unread — and a marker in the author's history is the one
/// thing this must never do.
pub(super) fn unsettled(
    fs: &fsa::FsAccess,
    artifact: &MergeArtifact,
    paths: &[String],
) -> Vec<String> {
    let mut left: Vec<String> = Vec::new();
    for path in paths {
        let Ok(bytes) = fs.read_bytes(artifact.checkout.join(path)) else {
            continue;
        };
        if holds_marker(&bytes) {
            left.push(path.clone());
        }
    }
    left.sort();
    left.dedup();
    left
}

/// A conflict marker at the start of a line, which is what Git writes.
pub(super) fn holds_marker(bytes: &[u8]) -> bool {
    bytes
        .split(|b| *b == b'\n')
        .any(|line| line.starts_with(b"<<<<<<< ") || line.starts_with(b">>>>>>> "))
}

/// Remove everything one attempt made, whatever it decided.
pub(super) fn release(fs: &fsa::FsAccess, main: &git2::Repository, artifact: &MergeArtifact) {
    let _ = git::remove_worktree(fs, main, &artifact.checkout, Some(&artifact.attempt_id));
    let _ = git::delete_branch(main, &scratch_branch(&artifact.attempt_id));
    artifact::release(fs, artifact);
}

/// The three sides of every path the merge could not settle.
pub(super) fn conflict_sides(index: &git2::Index) -> Vec<ConflictSides> {
    let mut sides: Vec<ConflictSides> = Vec::new();
    let Ok(conflicts) = index.conflicts() else {
        return sides;
    };
    for conflict in conflicts.flatten() {
        let path = conflict
            .our
            .as_ref()
            .or(conflict.their.as_ref())
            .or(conflict.ancestor.as_ref())
            .map(|entry| String::from_utf8_lossy(&entry.path).into_owned());
        let Some(path) = path else { continue };
        sides.push(ConflictSides {
            path,
            ancestor: conflict.ancestor.as_ref().map(|e| e.id),
            ours: conflict.our.as_ref().map(|e| e.id),
            theirs: conflict.their.as_ref().map(|e| e.id),
        });
    }
    sides.sort_by(|a, b| a.path.cmp(&b.path));
    sides
}

/// What one side did to each path it touched since the merge base.
pub(super) fn changes_between(
    main: &git2::Repository,
    from: git2::Oid,
    to: git2::Oid,
) -> BTreeMap<String, SideChange> {
    let mut changes: BTreeMap<String, SideChange> = BTreeMap::new();
    let trees = main
        .find_commit(from)
        .and_then(|c| c.tree())
        .and_then(|from| main.find_commit(to).and_then(|c| c.tree()).map(|to| (from, to)));
    let Ok((from_tree, to_tree)) = trees else {
        return changes;
    };
    let Ok(diff) = main.diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None) else {
        return changes;
    };
    for delta in diff.deltas() {
        let path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().into_owned());
        let Some(path) = path else { continue };
        let change = match delta.status() {
            git2::Delta::Added | git2::Delta::Copied => SideChange::Created,
            git2::Delta::Deleted => SideChange::Deleted,
            git2::Delta::Unmodified => SideChange::Unchanged,
            _ => SideChange::Updated,
        };
        changes.insert(path, change);
    }
    changes
}

pub(super) fn change_of(changes: &BTreeMap<String, SideChange>, path: &str) -> SideChange {
    changes.get(path).copied().unwrap_or(SideChange::Unchanged)
}

/// The paths a merged tree writes onto the base branch.
pub(super) fn written_paths(
    main: &git2::Repository,
    base_oid: git2::Oid,
    merged_tree: git2::Oid,
) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    let trees = main
        .find_commit(base_oid)
        .and_then(|c| c.tree())
        .and_then(|base| main.find_tree(merged_tree).map(|merged| (base, merged)));
    let Ok((base_tree, merged)) = trees else {
        return paths;
    };
    let Ok(diff) = main.diff_tree_to_tree(Some(&base_tree), Some(&merged), None) else {
        return paths;
    };
    for delta in diff.deltas() {
        if let Some(path) = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().into_owned())
        {
            paths.push(path);
        }
    }
    paths.sort();
    paths.dedup();
    paths
}
