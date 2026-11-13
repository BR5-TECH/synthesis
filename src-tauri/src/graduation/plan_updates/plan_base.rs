//! The plan base of an apply run (OPU-FR-NQBK, OPU-FR-YNXV, OPU-FR-FSCV).
//!
//! The close commit of the plan proposal is the author's work. The run is
//! measured from a commit whose tree is its base commit's tree with the paths
//! of that close commit set as the commit holds them, so the change set, the
//! review checkout, the planning gate, and the run's own commit hold only
//! what the run did.

use std::path::Path;

/// OPU contract surface, storage: the private ref of a run's newest plan base.
pub const REF_PREFIX: &str = "refs/synthesis/plan-base/";

/// The ref of the run's plan base.
pub fn ref_name(run_id: &str) -> String {
    format!("{REF_PREFIX}{run_id}")
}

/// OPU-FR-YNXV, OPU-FR-FSCV: make the plan base of `base_commit` and the close
/// commit `close_commit`, and move the private ref of `run_id` to it. Answers
/// the revision. No checkout and no branch is written.
pub fn make(
    worktree: &Path,
    base_commit: &str,
    close_commit: &str,
    run_id: &str,
) -> Result<String, String> {
    let repo = git2::Repository::open(worktree).map_err(|e| e.to_string())?;
    let oid = |text: &str| git2::Oid::from_str(text).map_err(|e| e.to_string());
    let base = repo.find_commit(oid(base_commit)?).map_err(|e| e.to_string())?;
    let close = repo.find_commit(oid(close_commit)?).map_err(|e| e.to_string())?;
    let base_tree = base.tree().map_err(|e| e.to_string())?;
    let close_tree = close.tree().map_err(|e| e.to_string())?;
    let before = match close.parent(0) {
        Ok(parent) => Some(parent.tree().map_err(|e| e.to_string())?),
        Err(_) => None,
    };
    let diff = repo
        .diff_tree_to_tree(before.as_ref(), Some(&close_tree), None)
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
    let mut update = git2::build::TreeUpdateBuilder::new();
    for path in &paths {
        match close_tree.get_path(Path::new(path)) {
            Ok(entry) if entry.kind() == Some(git2::ObjectType::Blob) => {
                update.upsert(
                    path.as_str(),
                    entry.id(),
                    crate::git::tree_mode(entry.filemode() as u32),
                );
            }
            _ => {
                if base_tree.get_path(Path::new(path)).is_ok() {
                    update.remove(path.as_str());
                }
            }
        }
    }
    let tree_id = update.create_updated(&repo, &base_tree).map_err(|e| e.to_string())?;
    let tree = repo.find_tree(tree_id).map_err(|e| e.to_string())?;
    // OPU-FR-YTKN: the plan base is the application's own commit, as the
    // merge snapshot is, so a repository with no Git identity still gets one.
    let signature = repo
        .signature()
        .or_else(|_| git2::Signature::now("Synthesis", "synthesis@localhost"))
        .map_err(|e| e.to_string())?;
    let message = "Plan base of an apply run";
    let made = repo
        .commit(None, &signature, &signature, message, &tree, &[&base])
        .map_err(|e| e.to_string())?;
    repo.reference(&ref_name(run_id), made, true, "plan base").map_err(|e| e.to_string())?;
    Ok(made.to_string())
}

/// OPU-FR-FSCV: remove the private ref of `run_id`. Answers whether it stood.
pub fn remove_ref(worktree: &Path, run_id: &str) -> Result<bool, String> {
    let repo = git2::Repository::open(worktree).map_err(|e| e.to_string())?;
    let mut reference = match repo.find_reference(&ref_name(run_id)) {
        Ok(reference) => reference,
        Err(error) if error.code() == git2::ErrorCode::NotFound => return Ok(false),
        Err(error) => return Err(error.to_string()),
    };
    reference.delete().map_err(|e| e.to_string())?;
    Ok(true)
}
