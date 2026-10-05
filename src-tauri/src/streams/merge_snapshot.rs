//! The merge snapshot a conflict merge is handed off with
//! (`GRB-graduation-rebase.md`, `WKS-work-streams.md`).
//!
//! Git merges the two pinned tips in memory. The result is captured as **one
//! commit object**: its tree holds the clean merge and, for every path Git could
//! not merge, a file with conflict markers; its two parents are the pinned base
//! tip and the pinned stream tip. A private ref keeps the commit reachable for
//! as long as the merge run lives. No branch moves and no working copy is
//! written, so a snapshot that is never used leaves both trees as they were.
//!
//! What a merge run reconciles is this tree. The run seeds its own worktree from
//! the commit, and the tree a `ready` review lets through is applied to the base
//! branch only after both tips are verified again.

use super::semantic::{
    change_of, changes_between, conflict_sides, ended, MARKER_ANCESTOR, MARKER_BASE,
    MARKER_DELETED_SIDE, MARKER_STREAM,
};
use super::*;

/// The namespace of the private refs that keep a snapshot reachable.
const SNAPSHOT_REF_PREFIX: &str = "refs/synthesis/merge/";

/// The private ref one run's snapshot is kept under.
pub fn snapshot_ref(run_id: &str) -> String {
    format!("{SNAPSHOT_REF_PREFIX}{run_id}")
}

/// What one capture holds.
pub(super) struct Snapshot {
    /// The snapshot commit.
    pub(super) commit: git2::Oid,
    /// The revision both branches last shared.
    pub(super) merge_base: git2::Oid,
    /// Every path the Git merge changes against the base tip, the Git-clean
    /// paths and the unresolved paths together.
    pub(super) changed_paths: Vec<String>,
    /// The paths Git could not merge.
    pub(super) unresolved_paths: Vec<String>,
    /// The same paths, with what each side did to each.
    pub(super) conflicts: Vec<StreamMergeConflict>,
}

/// One conflicting path, rendered as the file the run reconciles.
struct Rendered {
    path: String,
    blob: git2::Oid,
    size: usize,
    mode: u32,
}

/// Capture the merge of two pinned tips as a snapshot commit, and keep it under
/// the private ref of `run_id`.
pub(super) fn capture(
    main: &git2::Repository,
    base_oid: git2::Oid,
    stream_oid: git2::Oid,
    run_id: &str,
) -> Result<Snapshot, String> {
    let base_commit = main.find_commit(base_oid).map_err(|e| e.to_string())?;
    let stream_commit = main.find_commit(stream_oid).map_err(|e| e.to_string())?;
    let merge_base = main
        .merge_base(base_oid, stream_oid)
        .map_err(|e| e.to_string())?;
    let mut merged = main
        .merge_commits(&base_commit, &stream_commit, None)
        .map_err(|e| e.to_string())?;

    let sides = conflict_sides(&merged);
    refuse_unsupported(&merged)?;
    let mut rendered: Vec<Rendered> = Vec::new();
    if let Ok(conflicts) = merged.conflicts() {
        for conflict in conflicts.flatten() {
            if let Some(file) = render(main, &conflict)? {
                rendered.push(file);
            }
        }
    }

    // A conflicted index cannot be written as a tree, so the conflicting paths
    // come out and the rendered files go in.
    for side in &sides {
        merged
            .remove_path(std::path::Path::new(&side.path))
            .map_err(|e| e.to_string())?;
    }
    for file in &rendered {
        let entry = git2::IndexEntry {
            ctime: git2::IndexTime::new(0, 0),
            mtime: git2::IndexTime::new(0, 0),
            dev: 0,
            ino: 0,
            mode: file.mode,
            uid: 0,
            gid: 0,
            file_size: file.size as u32,
            id: file.blob,
            flags: 0,
            flags_extended: 0,
            path: file.path.clone().into_bytes(),
        };
        merged.add(&entry).map_err(|e| e.to_string())?;
    }
    let tree_oid = merged.write_tree_to(main).map_err(|e| e.to_string())?;
    let tree = main.find_tree(tree_oid).map_err(|e| e.to_string())?;

    let signature = main
        .signature()
        .or_else(|_| git2::Signature::now("Synthesis", "synthesis@localhost"))
        .map_err(|e| e.to_string())?;
    let commit = main
        .commit(
            None,
            &signature,
            &signature,
            "Merge snapshot: the Git merge with conflict markers where Git stopped",
            &tree,
            &[&base_commit, &stream_commit],
        )
        .map_err(|e| e.to_string())?;
    let base_changes = changes_between(main, merge_base, base_oid);
    let stream_changes = changes_between(main, merge_base, stream_oid);
    // A record that names only the path both sides moved away from is not a path
    // anybody has to settle: the renamed paths carry the conflict, and the old
    // path stands in no tree of the snapshot.
    let sides: Vec<_> = sides
        .into_iter()
        .filter(|side| side.ours.is_some() || side.theirs.is_some())
        .collect();
    let unresolved_paths: Vec<String> = sides.iter().map(|side| side.path.clone()).collect();
    let conflicts: Vec<StreamMergeConflict> = sides
        .iter()
        .map(|side| StreamMergeConflict {
            path: side.path.clone(),
            base_change: change_of(&base_changes, &side.path).as_str().to_string(),
            stream_change: change_of(&stream_changes, &side.path).as_str().to_string(),
        })
        .collect();

    let mut changed_paths = paths_differing(main, base_oid, tree_oid)?;
    changed_paths.extend(unresolved_paths.iter().cloned());
    changed_paths.sort();
    changed_paths.dedup();

    // Last, so a capture that failed above leaves no ref behind.
    main.reference(&snapshot_ref(run_id), commit, true, "merge snapshot")
        .map_err(|e| e.to_string())?;

    Ok(Snapshot {
        commit,
        merge_base,
        changed_paths,
        unresolved_paths,
        conflicts,
    })
}

/// WKS-FR-GKPX: the conflicts a text turn cannot reconcile are refused at the
/// check, so no run is made for them and nothing is written.
///
/// A conflict that involves a symbolic link or a submodule has no text to mark.
/// A path that is a file on one side and a directory on the other cannot hold
/// both in one tree, and writing a marker file over it would drop the other
/// side's paths without a word. The author settles these with Git.
///
/// The handoff asks this before the image preflight, so a conflict that no run
/// could settle is told as such and not as a machine that cannot execute an
/// agent. `capture` asks it again, because it is the only place that builds a
/// snapshot.
pub(super) fn refuse_unsupported_between(
    main: &git2::Repository,
    base_oid: git2::Oid,
    stream_oid: git2::Oid,
) -> Result<(), String> {
    let base_commit = main.find_commit(base_oid).map_err(|e| e.to_string())?;
    let stream_commit = main.find_commit(stream_oid).map_err(|e| e.to_string())?;
    let merged = main
        .merge_commits(&base_commit, &stream_commit, None)
        .map_err(|e| e.to_string())?;
    refuse_unsupported(&merged)
}

fn refuse_unsupported(merged: &git2::Index) -> Result<(), String> {
    let Ok(conflicts) = merged.conflicts() else {
        return Ok(());
    };
    for conflict in conflicts.flatten() {
        let entries = [
            conflict.ancestor.as_ref(),
            conflict.our.as_ref(),
            conflict.their.as_ref(),
        ];
        if entries
            .into_iter()
            .flatten()
            .any(|entry| matches!(entry.mode, 0o120000 | 0o160000))
        {
            return Err(ERR_UNSUPPORTED_CONFLICT.to_string());
        }
        let Some(entry) = conflict.our.as_ref().or(conflict.their.as_ref()) else {
            continue;
        };
        let path = String::from_utf8_lossy(&entry.path).into_owned();
        let under = format!("{path}/");
        let holds_directory = merged.iter().any(|other| {
            other.flags & 0x3000 == 0 && String::from_utf8_lossy(&other.path).starts_with(&under)
        });
        let parent_is_file = path.match_indices('/').any(|(at, _)| {
            merged
                .get_path(std::path::Path::new(&path[..at]), 0)
                .is_some()
        });
        if holds_directory || parent_is_file {
            return Err(ERR_UNSUPPORTED_CONFLICT.to_string());
        }
    }
    Ok(())
}

/// Remove the private ref of one run's snapshot. A ref that is not there is
/// the state the caller wanted.
pub(crate) fn release_ref(main: &git2::Repository, run_id: &str) -> Result<(), String> {
    match main.find_reference(&snapshot_ref(run_id)) {
        Ok(mut reference) => reference.delete().map_err(|e| e.to_string()),
        Err(_) => Ok(()),
    }
}

/// Whether the private ref of one run's snapshot stands.
pub(crate) fn has_ref(main: &git2::Repository, run_id: &str) -> bool {
    main.find_reference(&snapshot_ref(run_id)).is_ok()
}

/// The paths the base tip's tree and a tree differ at.
fn paths_differing(
    main: &git2::Repository,
    base_oid: git2::Oid,
    tree_oid: git2::Oid,
) -> Result<Vec<String>, String> {
    let base_tree = main
        .find_commit(base_oid)
        .and_then(|commit| commit.tree())
        .map_err(|e| e.to_string())?;
    let tree = main.find_tree(tree_oid).map_err(|e| e.to_string())?;
    let diff = main
        .diff_tree_to_tree(Some(&base_tree), Some(&tree), None)
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

/// The file a conflicting path becomes in the snapshot.
///
/// A path both sides hold is written with diff3 markers, naming the sides by
/// role as `merge-work.md` does. A path one side deleted is written as a marked
/// file too, because libgit2 writes no marker for one and a turn that changed
/// nothing could not then be told from a settled path (GRB-FR-SRVN). A path
/// either side holds as a binary file keeps the base side's content: an agent
/// edits text, and the path stays unresolved until the review says otherwise.
fn render(
    main: &git2::Repository,
    conflict: &git2::IndexConflict,
) -> Result<Option<Rendered>, String> {
    // A record that holds only the ancestor names the path both sides moved away
    // from (a rename against a delete or a rename). Neither tree of the snapshot
    // holds that path, so no file is written for it (GRB-FR-SRVN).
    if conflict.our.is_none() && conflict.their.is_none() {
        return Ok(None);
    }
    let entry = conflict
        .our
        .as_ref()
        .or(conflict.their.as_ref())
        .or(conflict.ancestor.as_ref());
    let Some(entry) = entry else {
        return Ok(None);
    };
    let path = String::from_utf8_lossy(&entry.path).into_owned();
    let read = |entry: Option<&git2::IndexEntry>| -> Option<Vec<u8>> {
        entry
            .and_then(|entry| main.find_blob(entry.id).ok())
            .map(|blob| blob.content().to_vec())
    };
    let ancestor = read(conflict.ancestor.as_ref());
    let ours = read(conflict.our.as_ref());
    let theirs = read(conflict.their.as_ref());

    let content: Vec<u8> = match (&ours, &theirs) {
        (Some(ours_bytes), Some(theirs_bytes)) => {
            let binary = |bytes: &[u8]| bytes.contains(&0);
            if binary(ours_bytes) || binary(theirs_bytes) || ancestor.as_deref().is_some_and(binary)
            {
                ours_bytes.clone()
            } else {
                let empty: Vec<u8> = Vec::new();
                let ancestor_bytes = ancestor.as_ref().unwrap_or(&empty);
                let mut ancestor_input = git2::MergeFileInput::new();
                ancestor_input.content(ancestor_bytes).path(path.as_str());
                let mut ours_input = git2::MergeFileInput::new();
                ours_input.content(ours_bytes).path(path.as_str());
                let mut theirs_input = git2::MergeFileInput::new();
                theirs_input.content(theirs_bytes).path(path.as_str());
                let mut options = git2::MergeFileOptions::new();
                options
                    .style_diff3(true)
                    .ancestor_label(MARKER_ANCESTOR)
                    .our_label(MARKER_BASE)
                    .their_label(MARKER_STREAM);
                let merged = git2::merge_file(
                    &ancestor_input,
                    &ours_input,
                    &theirs_input,
                    Some(&mut options),
                )
                .map_err(|e| e.to_string())?;
                merged.content().to_vec()
            }
        }
        // One side deleted the path and the other side changed a binary file:
        // there is no text to mark, so the surviving bytes stand as they are and
        // the path stays unresolved for the turn to keep or to delete.
        (Some(only), None) | (None, Some(only)) if only.contains(&0) => only.clone(),
        _ => {
            let body = |bytes: &Option<Vec<u8>>| -> String {
                match bytes {
                    Some(bytes) => String::from_utf8_lossy(bytes).into_owned(),
                    None => format!("(this path was deleted on {MARKER_DELETED_SIDE})\n"),
                }
            };
            format!(
                "<<<<<<< {MARKER_BASE}\n{}||||||| {MARKER_ANCESTOR}\n{}=======\n{}>>>>>>> {MARKER_STREAM}\n",
                ended(&body(&ours)),
                ended(&body(&ancestor)),
                ended(&body(&theirs)),
            )
            .into_bytes()
        }
    };

    let executable = [conflict.our.as_ref(), conflict.their.as_ref()]
        .into_iter()
        .flatten()
        .any(|entry| entry.mode == 0o100755);
    let blob = main.blob(&content).map_err(|e| e.to_string())?;
    Ok(Some(Rendered {
        path,
        blob,
        size: content.len(),
        mode: if executable { 0o100755 } else { 0o100644 },
    }))
}
