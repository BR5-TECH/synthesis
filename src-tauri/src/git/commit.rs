//! The path-scoped commit (GTC-FR-19 / GTC-FR-20).

use std::path::Path;

use git2::Repository;
use serde::Serialize;

use crate::changes::{self};

use super::*;

// ---------------------------------------------------------------------------
// Path-scoped commit (GTC-FR-19 / GTC-FR-20)
// ---------------------------------------------------------------------------

/// The message was empty or held nothing but whitespace (GTC-FR-20).
pub const ERR_EMPTY_COMMIT_MESSAGE: &str = "empty_commit_message";
/// No path was named, so there is no commit to make (GTC-FR-20).
pub const ERR_NO_PATHS_SELECTED: &str = "no_paths_selected";
/// Every named path is already identical to `HEAD` (GTC-FR-20).
pub const ERR_NOTHING_TO_COMMIT: &str = "nothing_to_commit";

/// `HEAD`'s tree, or the repository's empty tree on an unborn branch, so the
/// first commit builds against the same shape every later one does.
pub(crate) fn base_tree_or_empty(repo: &Repository) -> Result<git2::Tree<'_>, String> {
    if let Some(tree) = head_tree(repo) {
        return Ok(tree);
    }
    let oid = repo
        .treebuilder(None)
        .and_then(|builder| builder.write())
        .map_err(|e| format!("failed to create the empty tree: {e}"))?;
    repo.find_tree(oid)
        .map_err(|e| format!("failed to read the empty tree: {e}"))
}

/// The tree entry mode for an index entry's raw mode. Anything that is not a
/// symlink or an executable blob is a plain blob — a gitlink cannot reach here,
/// because `diff_options()` ignores submodules.
pub(crate) fn tree_mode(raw: u32) -> git2::FileMode {
    match raw & 0o170000 {
        0o120000 => git2::FileMode::Link,
        _ if raw & 0o111 != 0 => git2::FileMode::BlobExecutable,
        _ => git2::FileMode::Blob,
    }
}

/// GTC-FR-19: which named paths differ from `HEAD`, and where a renamed one
/// came from.
///
/// Returns `(changed, previous_of)`: the repository-relative paths among
/// `specs` that the working tree reports as differing from `HEAD`, and the
/// pre-rename location of each renamed entry. Computed from a diff rather than
/// from the index so it can answer *before* anything is written — GTC-FR-20
/// requires a refusal to leave the index untouched.
pub(crate) fn changed_named_paths(
    repo: &Repository,
    prefix: &str,
    specs: &[String],
    pair_renames: bool,
) -> Result<(Vec<String>, Vec<(String, String)>), String> {
    // Deliberately NOT narrowed to `specs`: a pathspec filters deltas before
    // rename detection runs, so scoping the diff to the ticked path alone drops
    // the deletion half of a rename and the pair never forms — the commit would
    // then hold the file at both locations. The whole project's change set is
    // computed and the named paths are selected out of it below, which is also
    // the set the panel that ticked them was rendering.
    let mut opts = changes::diff_options();
    changes::scope_to_project(&mut opts, prefix);
    let tree = head_tree(repo);
    let mut diff = repo
        .diff_tree_to_workdir_with_index(tree.as_ref(), Some(&mut opts))
        .map_err(|e| format!("failed to diff working tree: {e}"))?;
    // A rename has to be paired, or its old location is never removed from the
    // tree and the commit records the file at both paths as two files. A caller
    // that names both locations itself asks for no pairing (see
    // [`commit_exact_paths`]).
    if pair_renames {
        changes::find_renames(&mut diff)?;
    }

    let mut changed = Vec::new();
    let mut previous_of = Vec::new();
    for delta in diff.deltas() {
        let new_path = delta
            .new_file()
            .path()
            .map(|p| p.to_string_lossy().into_owned());
        let old_path = delta
            .old_file()
            .path()
            .map(|p| p.to_string_lossy().into_owned());
        // A deletion has no new-side path; the entry is still named by its old
        // one, which is the path the caller ticked.
        let named = new_path.clone().or_else(|| old_path.clone());
        let Some(named) = named else { continue };
        if !specs.iter().any(|spec| spec == &named) {
            continue;
        }
        if changes::map_status(delta.status()).is_none() {
            continue;
        }
        changed.push(named.clone());
        if let (Some(old), Some(new)) = (old_path, new_path) {
            if old != new {
                previous_of.push((new, old));
            }
        }
    }
    changed.sort();
    changed.dedup();
    Ok((changed, previous_of))
}

/// GTC-FR-19: what a successful `commit_paths` reports.
///
/// The commit's id, and **every path the commit recorded** — which is not the
/// set the caller submitted: a rename is recorded at both its locations, and a
/// named path the commit found nothing to record for is absent. The caller acts
/// on this rather than on what it asked for (`../ui/TAB-tabs.md` TAB-FR-22).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitOutcome {
    pub commit_id: String,
    pub committed_paths: Vec<String>,
}

/// GTC-FR-19: create one commit holding exactly `paths`, against the active
/// worktree rooted at `root`.
///
/// The commit tree is built from `HEAD`'s tree with only the named paths
/// applied, rather than from the index — which is what keeps a path the author
/// did not tick out of the commit *even when it was already staged*. The index
/// is then advanced for the named paths alone, so they read as clean afterwards
/// while every other staged path stays staged and uncommitted.
pub fn commit_named_paths(
    root: &crate::fs::RootFs,
    message: &str,
    paths: &[String],
    expected_worktree: Option<&str>,
) -> Result<CommitOutcome, String> {
    commit_paths_with(root, message, paths, expected_worktree, true)
}

/// `PST-project-storage.md` PST-FR-TYNC: [`commit_named_paths`] for a caller
/// that names **every** path it wants recorded, each side of a move included.
///
/// No rename is paired. Git pairs a deleted file with any untracked file of
/// similar content, which may be another draft or a file of the author's, and a
/// paired rename would carry that other path into the commit or leave the named
/// deletion out of it. Each named path is recorded on its own terms: present on
/// disk is an addition or a modification, absent is a deletion.
pub fn commit_exact_paths(
    root: &crate::fs::RootFs,
    message: &str,
    paths: &[String],
) -> Result<CommitOutcome, String> {
    commit_paths_with(root, message, paths, None, false)
}

fn commit_paths_with(
    root: &crate::fs::RootFs,
    message: &str,
    paths: &[String],
    expected_worktree: Option<&str>,
    pair_renames: bool,
) -> Result<CommitOutcome, String> {
    // GTC-FR-20 / GTC-FR-31: the identity guard runs **ahead of every other
    // check**, because a commit aimed at one checkout must not be corrected into
    // another — a message this call would have refused as empty is not the
    // author's problem when the checkout they were shown has moved.
    require_worktree_identity(root.path(), expected_worktree)?;
    // GTC-FR-20: validated before anything is written, so a refusal leaves the
    // index, the refs, and the working tree byte-identical.
    if message.trim().is_empty() {
        return Err(ERR_EMPTY_COMMIT_MESSAGE.to_string());
    }
    if paths.is_empty() {
        return Err(ERR_NO_PATHS_SELECTED.to_string());
    }

    let repo = changes::open_repo(root)?;
    let prefix = changes::project_prefix(&repo, root);

    // Every path arrives from the frontend, so each is resolved under the
    // content root before it is used: a `..` segment must not reach a file
    // outside the project (GTC-FR-18).
    let mut specs = Vec::with_capacity(paths.len());
    for path in paths {
        crate::fs::resolve_under(root, path).map_err(|e| e.to_string())?;
        specs.push(repo_pathspec(&prefix, path));
    }

    let (changed, previous_of) = changed_named_paths(&repo, &prefix, &specs, pair_renames)?;
    if changed.is_empty() {
        return Err(ERR_NOTHING_TO_COMMIT.to_string());
    }

    // The index is advanced only for what is actually going into the commit.
    // `add_path` reads the working-directory content through the repository's
    // filters, so a checkout under `core.autocrlf` commits the blob Git would
    // have committed rather than the raw bytes on disk.
    let mut index = repo
        .index()
        .map_err(|e| format!("failed to open the index: {e}"))?;
    // Every `spec` is repository-relative, so presence on disk is decided
    // against the repository's working directory rather than against the
    // project's content root, which may sit below it.
    let workdir = repo
        .workdir()
        .ok_or_else(|| "the repository has no working directory".to_string())?
        .to_path_buf();
    for spec in &changed {
        let relative = Path::new(spec);
        // `symlink_metadata`, not `exists`: a symlink whose target is gone is
        // still a file the commit records, not a deletion.
        // Only a file that is really gone is a deletion. Any other failure —
        // a path the filesystem gate does not cover above all — is reported,
        // so a file that could not be read is never committed as deleted.
        match root.file_info(workdir.join(spec)) {
            Ok(_) => {
                index
                    .add_path(relative)
                    .map_err(|e| format!("failed to stage {spec}: {e}"))?;
            }
            Err(crate::fs::FsError::NotFound { .. }) => {
                index
                    .remove_path(relative)
                    .map_err(|e| format!("failed to record the deletion of {spec}: {e}"))?;
            }
            Err(e) => return Err(format!("failed to stage {spec}: {e}")),
        }
    }
    // A rename is recorded at both locations (GTC-FR-19); without the removal
    // the commit holds the file twice.
    //
    // A path that is itself among the changed set is never removed, however
    // much it is some other entry's previous location. A rotation (`old` moved
    // to `new` while `third` took `old`'s place) is the shape to watch: dropping
    // `old` for the first rename would discard what the second put there, and
    // the commit would be missing a file the caller explicitly named. libgit2
    // does not currently pair a rename whose old path is reoccupied — it reports
    // that path as a modification instead, so the two halves cannot both appear
    // — but the guard costs nothing and the invariant is the one that matters:
    // a named path is never removed on another entry's behalf.
    for (new, old) in &previous_of {
        if changed.iter().any(|spec| spec == new) && !changed.iter().any(|spec| spec == old) {
            let _ = index.remove_path(Path::new(old));
        }
    }
    index
        .write()
        .map_err(|e| format!("failed to write the index: {e}"))?;

    // The tree is HEAD's, updated at the named paths only.
    let base = base_tree_or_empty(&repo)?;
    let mut update = git2::build::TreeUpdateBuilder::new();
    let mut touched: Vec<String> = changed.clone();
    touched.extend(
        previous_of
            .iter()
            .filter(|(new, old)| {
                changed.iter().any(|spec| spec == new)
                    && !changed.iter().any(|spec| spec == old)
            })
            .map(|(_, old)| old.clone()),
    );
    touched.sort();
    touched.dedup();
    for spec in &touched {
        match index.get_path(Path::new(spec), 0) {
            Some(entry) => {
                update.upsert(spec.as_str(), entry.id, tree_mode(entry.mode));
            }
            // Absent from the index means the path is gone: a deletion, or a
            // rename's old location. Removing a path the base tree never held
            // is not a no-op in libgit2, so it is skipped.
            None => {
                if base.get_path(Path::new(spec)).is_ok() {
                    update.remove(spec.as_str());
                }
            }
        }
    }
    let tree_oid = update
        .create_updated(&repo, &base)
        .map_err(|e| format!("failed to build the commit tree: {e}"))?;
    let tree = repo
        .find_tree(tree_oid)
        .map_err(|e| format!("failed to read the commit tree: {e}"))?;

    // GTC-FR-03: the authoring identity is the user's Git configuration; this
    // module does not own it and does not invent one.
    let signature = repo
        .signature()
        .map_err(|e| format!("no Git authoring identity is configured: {e}"))?;
    let parents: Vec<git2::Commit> = match repo.head().ok().and_then(|h| h.peel_to_commit().ok()) {
        Some(parent) => vec![parent],
        None => Vec::new(),
    };
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    let oid = repo
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_refs,
        )
        .map_err(|e| format!("failed to create the commit: {e}"))?;
    // GTC-FR-19: report every path the commit **recorded** — the named set that
    // actually changed, plus the previous location of each rename — rather than
    // the set the caller submitted. The strip closes Diff tabs from this
    // (`../ui/TAB-tabs.md` TAB-FR-22), and re-deriving "what landed" from "what
    // was asked for" would miss a rename's old location and would close a tab on
    // a path the commit turned out to hold nothing for.
    //
    // Reported project-relative, the way every path crossing this boundary is:
    // `touched` is repository-relative, and a project nested below the
    // repository root would otherwise hand the frontend paths it cannot match
    // against anything it holds.
    let committed_paths = touched
        .iter()
        .filter_map(|spec| project_relative(&prefix, spec))
        .collect();
    Ok(CommitOutcome {
        commit_id: oid.to_string(),
        committed_paths,
    })
}

/// The inverse of [`repo_pathspec`]: a repository-relative spec back to the
/// project-relative path the frontend names files by.
///
/// A spec outside the project prefix is dropped rather than reported: it names
/// a file the project does not contain, so no surface downstream could act on
/// it.
pub(crate) fn project_relative(prefix: &str, spec: &str) -> Option<String> {
    if prefix.is_empty() {
        return Some(spec.to_string());
    }
    spec.strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('/'))
        .map(str::to_string)
}
