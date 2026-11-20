//! The path-scoped commit (GTC-FR-19 / GTC-FR-20).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use git2::Repository;
use serde::Serialize;

use crate::changes::repo_set::{self, Member, RepoSet, GITLINK_MODE};
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
    repo.find_tree(oid).map_err(|e| format!("failed to read the empty tree: {e}"))
}

/// The tree entry mode for an index entry's raw mode. A submodule pointer is a
/// submodule entry (GTC-FR-GDNR), and anything that is not a symlink or an
/// executable blob is a plain blob.
pub(crate) fn tree_mode(raw: u32) -> git2::FileMode {
    match raw & 0o170000 {
        GITLINK_MODE => git2::FileMode::Commit,
        0o120000 => git2::FileMode::Link,
        _ if raw & 0o111 != 0 => git2::FileMode::BlobExecutable,
        _ => git2::FileMode::Blob,
    }
}

/// The changed paths and the `(new, previous)` pair of each renamed entry.
type ChangedNamedPaths = (Vec<String>, Vec<(String, String)>);

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
) -> Result<ChangedNamedPaths, String> {
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
        let new_path = delta.new_file().path().map(|p| p.to_string_lossy().into_owned());
        let old_path = delta.old_file().path().map(|p| p.to_string_lossy().into_owned());
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
    /// The commit of the main repository; the last commit made when the main
    /// repository holds none.
    pub commit_id: String,
    pub committed_paths: Vec<String>,
    /// GTC-FR-CHEC: one per commit made, in the order made.
    pub repositories: Vec<RepositoryCommit>,
}

/// GTC-FR-CHEC: the commit made in one repository of the set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryCommit {
    /// The project-relative root of the repository; `""` for the main one.
    pub repository: String,
    pub commit_id: String,
    pub committed_paths: Vec<String>,
}

/// GTC-FR-TJPJ: a commit failed after a commit in another repository was made.
pub const ERR_PARTIAL_COMMIT: &str = "partial_commit";

/// GTC-FR-TJPJ / GSM-FR-ZKEJ: the typed error, with the commits made and the
/// repository that failed as data, in the form of `worktree_identity_error`.
pub fn partial_commit_error(committed: &[RepositoryCommit], failed: &str, cause: &str) -> String {
    format!(
        "{ERR_PARTIAL_COMMIT}: {}",
        serde_json::json!({ "committed": committed, "failed": failed, "cause": cause })
    )
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

    // Every path arrives from the frontend, so each is resolved under the
    // content root before it is used: a `..` segment must not reach a file
    // outside the project (GTC-FR-18).
    for path in paths {
        crate::fs::resolve_under(root, path).map_err(|e| e.to_string())?;
    }

    // GTC-FR-HYHB: each path goes to the repository that owns it. A submodule
    // root path is the pointer of that submodule in its parent.
    let set = RepoSet::open(root)?;
    let mut named: Vec<Vec<String>> = vec![Vec::new(); set.members.len()];
    let mut pointers_named: BTreeSet<usize> = BTreeSet::new();
    for path in paths {
        // GSM-FR-GHFY: the exact commit of an application store or a work
        // stream runs no check per submodule, so it commits in the main
        // repository alone, as for a project with no submodule.
        if !pair_renames {
            named[0].push(set.members[0].to_repo(path));
            continue;
        }
        match set.index_of(path).filter(|i| *i != 0) {
            Some(sub) => {
                pointers_named.insert(sub);
            }
            None => {
                let owner = set.owner_index(path);
                named[owner].push(set.members[owner].to_repo(path));
                // CHG-FR-AYTR, GSM-FR-MPWK: a file of a submodule implies the
                // pointer of that submodule and of each parent submodule. A
                // retry after a partial commit then records the pointers that
                // the failed run did not record.
                let mut member = &set.members[owner];
                while !member.is_main() {
                    if let Some(i) = set.index_of(&member.path) {
                        pointers_named.insert(i);
                    }
                    match set.parent_of(member) {
                        Some(parent) => member = parent,
                        None => break,
                    }
                }
            }
        }
    }

    // GTC-FR-20 / GTC-FR-TJPJ: the whole list is validated before the first
    // commit, so a refusal writes nothing in any repository.
    let mut plans: Vec<ChangedNamedPaths> = Vec::with_capacity(set.members.len());
    for (member, specs) in set.members.iter().zip(&named) {
        plans.push(if specs.is_empty() {
            (Vec::new(), Vec::new())
        } else {
            changed_named_paths(&member.repo, &member.prefix, specs, pair_renames)?
        });
    }
    let pointer_moved = pointers_named.iter().any(|i| pointer_change(&set, *i).is_some());
    if plans.iter().all(|(changed, _)| changed.is_empty()) && !pointer_moved {
        return Err(ERR_NOTHING_TO_COMMIT.to_string());
    }

    // GSM-FR-KWWZ: the deepest repository first and the main one last, so the
    // parent commit records the new pointer of each submodule (GSM-FR-MPWK).
    let mut made: Vec<RepositoryCommit> = Vec::new();
    let mut committed_subs: BTreeSet<usize> = BTreeSet::new();
    for i in set.deepest_first() {
        let member = &set.members[i];
        let pointers: Vec<(String, git2::Oid)> = set
            .members
            .iter()
            .enumerate()
            .filter(|(c, child)| {
                child.parent.as_deref() == Some(member.path.as_str())
                    && (committed_subs.contains(c) || pointers_named.contains(c))
            })
            .filter_map(|(c, _)| pointer_change(&set, c))
            .collect();
        let (changed, previous_of) = &plans[i];
        if changed.is_empty() && pointers.is_empty() {
            continue;
        }
        match commit_in_member(root, member, message, changed, previous_of, &pointers) {
            Ok(commit) => {
                if !member.is_main() {
                    committed_subs.insert(i);
                }
                made.push(commit);
            }
            Err(e) if made.is_empty() => return Err(e),
            Err(e) => return Err(partial_commit_error(&made, &member.path, &e)),
        }
    }

    let commit_id = made
        .iter()
        .find(|c| c.repository.is_empty())
        .or(made.last())
        .map(|c| c.commit_id.clone())
        .unwrap_or_default();
    let committed_paths = made.iter().flat_map(|c| c.committed_paths.iter().cloned()).collect();
    Ok(CommitOutcome { commit_id, committed_paths, repositories: made })
}

/// GSM-FR-GHFY: the repositories a commit of `paths` can write, as
/// `(project-relative root, checkout)`, so the main branch write check runs on
/// each before the first commit. The owner of each path and every parent above
/// it are written, because each parent records the new pointer (GSM-FR-MPWK).
/// The main repository's checkout is the content root.
pub fn commit_targets(root: &Path, paths: &[String]) -> Result<Vec<(String, PathBuf)>, String> {
    let set = RepoSet::open(root)?;
    let mut written: BTreeSet<usize> = BTreeSet::new();
    for path in paths {
        let mut member = match set.submodule_at(path) {
            Some(sub) => set.parent_of(sub),
            None => Some(set.owner(path)),
        };
        while let Some(current) = member {
            if let Some(i) = set.index_of(&current.path) {
                written.insert(i);
            }
            member = set.parent_of(current);
        }
    }
    Ok(written
        .into_iter()
        .filter_map(|i| {
            let member = &set.members[i];
            let checkout = if member.is_main() {
                Some(root.to_path_buf())
            } else {
                member.repo.workdir().map(Path::to_path_buf)
            };
            checkout.map(|c| (member.path.clone(), c))
        })
        .collect())
}

/// GSM-FR-FCOF: the pointer of the submodule at index `sub` as its parent
/// must record it — `(path in the parent, commit at the submodule HEAD)` —
/// when the parent `HEAD` records another commit for it.
fn pointer_change(set: &RepoSet, sub: usize) -> Option<(String, git2::Oid)> {
    let member = &set.members[sub];
    let current = member.head_id()?;
    let parent = set.parent_of(member)?;
    let spec = member.path_in_parent.clone()?;
    let recorded =
        repo_set::head_tree(&parent.repo).and_then(|t| repo_set::recorded_commit(&t, &spec));
    (recorded != Some(current)).then_some((spec, current))
}

/// GTC-FR-19 / GTC-FR-GDNR: one commit in `member` holding exactly `changed`
/// (repository-relative), the previous location of each rename in
/// `previous_of`, and the submodule `pointers`.
fn commit_in_member(
    root: &crate::fs::RootFs,
    member: &Member,
    message: &str,
    changed: &[String],
    previous_of: &[(String, String)],
    pointers: &[(String, git2::Oid)],
) -> Result<RepositoryCommit, String> {
    let repo = &member.repo;
    // The index is advanced only for what is actually going into the commit.
    // `add_path` reads the working-directory content through the repository's
    // filters, so a checkout under `core.autocrlf` commits the blob Git would
    // have committed rather than the raw bytes on disk.
    let mut index = repo.index().map_err(|e| format!("failed to open the index: {e}"))?;
    // Every `spec` is repository-relative, so presence on disk is decided
    // against the repository's working directory rather than against the
    // project's content root, which may sit below it.
    let workdir = repo
        .workdir()
        .ok_or_else(|| "the repository has no working directory".to_string())?
        .to_path_buf();
    for spec in changed {
        let relative = Path::new(spec);
        // `symlink_metadata`, not `exists`: a symlink whose target is gone is
        // still a file the commit records, not a deletion.
        // Only a file that is really gone is a deletion. Any other failure —
        // a path the filesystem gate does not cover above all — is reported,
        // so a file that could not be read is never committed as deleted.
        match root.file_info(workdir.join(spec)) {
            Ok(_) => {
                index.add_path(relative).map_err(|e| format!("failed to stage {spec}: {e}"))?;
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
    for (new, old) in previous_of {
        if changed.iter().any(|spec| spec == new) && !changed.iter().any(|spec| spec == old) {
            let _ = index.remove_path(Path::new(old));
        }
    }
    // GTC-FR-GDNR: a pointer is recorded as a submodule entry, never as a file.
    for (spec, commit) in pointers {
        index
            .add(&gitlink_entry(spec, *commit))
            .map_err(|e| format!("failed to stage {spec}: {e}"))?;
    }
    index.write().map_err(|e| format!("failed to write the index: {e}"))?;

    // The tree is HEAD's, updated at the named paths only.
    let base = base_tree_or_empty(repo)?;
    let mut update = git2::build::TreeUpdateBuilder::new();
    let mut touched: Vec<String> = changed.to_vec();
    touched.extend(
        previous_of
            .iter()
            .filter(|(new, old)| {
                changed.iter().any(|spec| spec == new) && !changed.iter().any(|spec| spec == old)
            })
            .map(|(_, old)| old.clone()),
    );
    touched.extend(pointers.iter().map(|(spec, _)| spec.clone()));
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
        .create_updated(repo, &base)
        .map_err(|e| format!("failed to build the commit tree: {e}"))?;
    let tree =
        repo.find_tree(tree_oid).map_err(|e| format!("failed to read the commit tree: {e}"))?;

    // GTC-FR-03: the authoring identity is the user's Git configuration; this
    // module does not own it and does not invent one.
    let signature =
        repo.signature().map_err(|e| format!("no Git authoring identity is configured: {e}"))?;
    let parents: Vec<git2::Commit> = match repo.head().ok().and_then(|h| h.peel_to_commit().ok()) {
        Some(parent) => vec![parent],
        None => Vec::new(),
    };
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    let oid = repo
        .commit(Some("HEAD"), &signature, &signature, message, &tree, &parent_refs)
        .map_err(|e| format!("failed to create the commit: {e}"))?;
    let committed_paths = touched.iter().filter_map(|spec| member.to_project(spec)).collect();
    Ok(RepositoryCommit {
        repository: member.path.clone(),
        commit_id: oid.to_string(),
        committed_paths,
    })
}

/// The index entry of a submodule pointer at `spec`.
fn gitlink_entry(spec: &str, commit: git2::Oid) -> git2::IndexEntry {
    git2::IndexEntry {
        ctime: git2::IndexTime::new(0, 0),
        mtime: git2::IndexTime::new(0, 0),
        dev: 0,
        ino: 0,
        mode: GITLINK_MODE,
        uid: 0,
        gid: 0,
        file_size: 0,
        id: commit,
        flags: 0,
        flags_extended: 0,
        path: spec.as_bytes().to_vec(),
    }
}
