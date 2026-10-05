//! The path-scoped rollback (GTC-FR-23 .. GTC-FR-28).

use std::path::Path;

use git2::Repository;
use serde::Serialize;

use crate::changes::{self};

use super::*;

// ---------------------------------------------------------------------------
// Path-scoped rollback (GTC-FR-23 – GTC-FR-28)
// ---------------------------------------------------------------------------

/// Why one path of a rollback did not succeed (GTC-FR-25).
///
/// A typed kind rather than a message, because the caller renders it beside the
/// path and decides from it whether the artifact's in-memory buffer may be
/// discarded (`../ui/CHG-changes.md` CHG-FR-63).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathFailure {
    pub path: String,
    pub kind: String,
}

/// The path resolved outside the project's content root (GTC-FR-27).
pub const ROLLBACK_OUTSIDE_ROOT: &str = "path_outside_content_root";
/// The path names a directory rather than a file (GTC-FR-24).
pub const ROLLBACK_IS_DIRECTORY: &str = "is_directory";
/// The path could not be written or removed for want of permission.
pub const ROLLBACK_PERMISSION_DENIED: &str = "permission_denied";
/// The write or removal failed for any other reason.
pub const ROLLBACK_WRITE_FAILED: &str = "write_failed";
/// `HEAD` holds no such path and the working tree does not either.
pub const ROLLBACK_NOT_FOUND: &str = "not_found";

/// What became of one selected entry (GTC-FR-25).
///
/// `restored_paths` and `removed_paths` hold only what was **confirmed on
/// disk**; a path that failed appears in `failures` and in neither list, and no
/// path appears in both. A renamed entry carries two identities and reports each
/// in the list its own result earned (GTC-FR-26).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RollbackEntryOutcome {
    pub id: String,
    pub path: String,
    pub previous_path: Option<String>,
    pub outcome: String,
    pub restored_paths: Vec<String>,
    pub removed_paths: Vec<String>,
    pub failures: Vec<PathFailure>,
}

/// `"restored"` — something was written back to `HEAD` (GTC-FR-25).
pub const ROLLBACK_RESTORED: &str = "restored";
/// `"removed"` — every path this entry acted on was removed (GTC-FR-25).
pub const ROLLBACK_REMOVED: &str = "removed";
/// `"failed"` — at least one of the entry's paths failed (GTC-FR-25).
pub const ROLLBACK_FAILED: &str = "failed";

impl RollbackEntryOutcome {
    /// GTC-FR-25: the outcome follows from the three lists rather than being
    /// decided alongside them, so a result can never claim an outcome its own
    /// paths contradict.
    fn settle(mut self) -> Self {
        self.outcome = if !self.failures.is_empty() {
            ROLLBACK_FAILED
        } else if !self.restored_paths.is_empty() {
            ROLLBACK_RESTORED
        } else {
            ROLLBACK_REMOVED
        }
        .to_string();
        self
    }
}

/// GTC-FR-23: what a `rollback_paths` call reports, one entry per named path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RollbackOutcome {
    pub entries: Vec<RollbackEntryOutcome>,
}

/// One filesystem act a rollback performs on one path identity.
pub(crate) enum RollbackStep {
    Restore,
    Remove,
}

/// How one named path must be returned to `HEAD`.
pub(crate) enum RollbackPlan {
    /// Write `HEAD`'s blob back at this path and make the index agree.
    Restore,
    /// Take the path off disk; `HEAD` holds no such file.
    Remove,
    /// A rename: restore the pre-rename location, remove the current one.
    Rename { previous: String },
    /// Nothing to do — the working tree and `HEAD` already agree.
    AlreadyAtHead,
}

/// Classify each named path against the project's change set.
///
/// The diff is computed over the whole project rather than narrowed to `specs`
/// for the reason `changed_named_paths` gives: a pathspec filters deltas before
/// rename detection runs, so scoping it would drop the deletion half of a rename
/// and the pair would never form — the rollback would then restore the new path
/// and leave the old one missing.
pub(crate) fn plan_rollback(
    repo: &Repository,
    prefix: &str,
    specs: &[String],
) -> Result<Vec<RollbackPlan>, String> {
    let mut opts = changes::diff_options();
    changes::scope_to_project(&mut opts, prefix);
    let tree = head_tree(repo);
    let mut diff = repo
        .diff_tree_to_workdir_with_index(tree.as_ref(), Some(&mut opts))
        .map_err(|e| format!("failed to diff working tree: {e}"))?;
    changes::find_renames(&mut diff)?;

    let mut plan_of: std::collections::HashMap<String, RollbackPlan> =
        std::collections::HashMap::new();
    for delta in diff.deltas() {
        let new_path = delta
            .new_file()
            .path()
            .map(|p| p.to_string_lossy().into_owned());
        let old_path = delta
            .old_file()
            .path()
            .map(|p| p.to_string_lossy().into_owned());
        let named = new_path.clone().or_else(|| old_path.clone());
        let Some(named) = named else { continue };
        let Some(status) = changes::map_status(delta.status()) else {
            continue;
        };
        let plan = match status {
            // `HEAD` has no version of either, so returning to `HEAD` is the
            // file ceasing to exist (GTC-FR-23).
            changes::ChangeStatus::Untracked | changes::ChangeStatus::Added => RollbackPlan::Remove,
            changes::ChangeStatus::Renamed => match (&old_path, &new_path) {
                (Some(old), Some(new)) if old != new => RollbackPlan::Rename {
                    previous: old.clone(),
                },
                // Rename detection paired it, then reported one path; treat it
                // as the ordinary modification it now looks like rather than
                // inventing a second identity.
                _ => RollbackPlan::Restore,
            },
            // A deletion and a modification are the same act undone: put
            // `HEAD`'s content back at the path.
            changes::ChangeStatus::Deleted | changes::ChangeStatus::Modified => {
                RollbackPlan::Restore
            }
        };
        plan_of.insert(named, plan);
    }

    // GTC-FR-23: a path the working tree and `HEAD` already agree on is not an
    // error — it simply needs nothing done to it.
    Ok(specs
        .iter()
        .map(|spec| plan_of.remove(spec).unwrap_or(RollbackPlan::AlreadyAtHead))
        .collect())
}

/// Read `HEAD`'s blob for `spec`, or `None` when `HEAD` holds no such path.
pub(crate) fn head_blob(repo: &Repository, spec: &str) -> Result<Option<(Vec<u8>, u32)>, String> {
    let Some(tree) = head_tree(repo) else {
        return Ok(None);
    };
    let entry = match tree.get_path(Path::new(spec)) {
        Ok(entry) => entry,
        Err(_) => return Ok(None),
    };
    let object = entry
        .to_object(repo)
        .map_err(|e| format!("failed to read {spec} from HEAD: {e}"))?;
    let blob = object
        .as_blob()
        .ok_or_else(|| format!("{spec} is not a file in HEAD"))?;
    Ok(Some((blob.content().to_vec(), entry.filemode() as u32)))
}

/// Classify a filesystem failure into the typed kind the caller renders.
///
/// Matched on the variant rather than sniffed from the message, so a reworded
/// error cannot silently reclassify a permission failure as an ordinary one.
pub(crate) fn failure_kind(error: &crate::fs::FsError) -> &'static str {
    use crate::fs::FsError;
    match error {
        FsError::NotFound { .. } => ROLLBACK_NOT_FOUND,
        FsError::PathEscape { .. } | FsError::EscapesAllowedRoots { .. } => ROLLBACK_OUTSIDE_ROOT,
        FsError::NotEmpty { .. } | FsError::NotADirectory { .. } => ROLLBACK_IS_DIRECTORY,
        FsError::Io(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            ROLLBACK_PERMISSION_DENIED
        }
        _ => ROLLBACK_WRITE_FAILED,
    }
}

/// GTC-FR-23 – GTC-FR-28: return exactly `paths` to the state `HEAD` holds for
/// them in the active worktree, reporting each path on its own.
///
/// Per-path rather than atomic (GTC-FR-25): each named path is attempted
/// independently, a failure at one neither aborts the run nor undoes a path
/// already restored, and the result never reports as successful a path that was
/// not confirmed on disk — which is the property the caller relies on to decide
/// whose in-memory edits it may discard.
/// Returns the per-path outcome together with the index-flush failure, if any.
/// The two are separate because a failed flush must not discard the outcome —
/// see the comment at the flush itself.
pub fn rollback_named_paths(
    root: &crate::fs::RootFs,
    paths: &[String],
) -> Result<(RollbackOutcome, Option<String>), String> {
    // GTC-FR-27: validated before anything is written.
    if paths.is_empty() {
        return Err(ERR_NO_PATHS_SELECTED.to_string());
    }

    let repo = changes::open_repo(root)?;
    let prefix = changes::project_prefix(&repo, root);
    let workdir = repo
        .workdir()
        .ok_or_else(|| "the repository has no working directory".to_string())?
        .to_path_buf();

    // GTC-FR-27: a path escaping the content root is that path's own typed
    // failure rather than a refusal of the whole call, so the valid paths beside
    // it are still rolled back.
    let mut escaped = vec![false; paths.len()];
    let mut specs = Vec::with_capacity(paths.len());
    for (i, path) in paths.iter().enumerate() {
        match crate::fs::resolve_under(root, path) {
            Ok(_) => specs.push(repo_pathspec(&prefix, path)),
            Err(_) => {
                escaped[i] = true;
                // A placeholder keeps `specs` index-aligned with `paths`; it is
                // never used, the entry having already failed.
                specs.push(String::new());
            }
        }
    }

    let plans = plan_rollback(&repo, &prefix, &specs)?;
    let mut index = repo
        .index()
        .map_err(|e| format!("failed to open the index: {e}"))?;
    let mut index_dirty = false;
    let mut entries = Vec::with_capacity(paths.len());

    for (i, path) in paths.iter().enumerate() {
        let mut entry = RollbackEntryOutcome {
            // ASC-FR-13: the entry's stable key is its project-relative path,
            // which is what the panel keyed its checkbox by.
            id: path.clone(),
            path: path.clone(),
            previous_path: None,
            outcome: String::new(),
            restored_paths: Vec::new(),
            removed_paths: Vec::new(),
            failures: Vec::new(),
        };
        if escaped[i] {
            entry.failures.push(PathFailure {
                path: path.clone(),
                kind: ROLLBACK_OUTSIDE_ROOT.to_string(),
            });
            entries.push(entry.settle());
            continue;
        }

        let spec = &specs[i];
        // A directory is refused before anything beneath it is touched
        // (GTC-FR-24).
        if let Ok(info) = root.file_info(workdir.join(spec)) {
            if info.kind == crate::fs::EntryKind::Dir {
                entry.failures.push(PathFailure {
                    path: path.clone(),
                    kind: ROLLBACK_IS_DIRECTORY.to_string(),
                });
                entries.push(entry.settle());
                continue;
            }
        }

        // Each identity is applied on its own and reported under the name the
        // caller keyed it by, so a rename half that fails leaves the half that
        // succeeded standing (GTC-FR-26).
        let mut steps: Vec<(RollbackStep, &str, String)> = Vec::new();
        match &plans[i] {
            RollbackPlan::Restore => steps.push((RollbackStep::Restore, spec, path.clone())),
            RollbackPlan::Remove => steps.push((RollbackStep::Remove, spec, path.clone())),
            RollbackPlan::Rename { previous } => {
                // GTC-FR-26: `HEAD` holds the file only under the name it was
                // committed with, so the previous location is restored and the
                // current one removed.
                // A previous location outside the project prefix is reported by
                // its repository-relative spelling rather than dropped: the
                // caller must be told both identities of a rename it is about to
                // act on, even one it holds no session for.
                let reported_previous =
                    project_relative(&prefix, previous).unwrap_or_else(|| previous.clone());
                entry.previous_path = Some(reported_previous.clone());
                steps.push((RollbackStep::Restore, previous.as_str(), reported_previous));
                steps.push((RollbackStep::Remove, spec, path.clone()));
            }
            RollbackPlan::AlreadyAtHead => {
                // The path is not in the change set, which is two situations
                // worth separating. If the file is there, or `HEAD` holds a
                // version of it, the working tree already says what `HEAD` says
                // and nothing needs doing (GTC-FR-23). If it is in neither, the
                // caller named something that does not exist — and reporting
                // that as restored would tell it to discard an in-memory buffer
                // on the strength of a path nothing was done to (GTC-FR-25).
                let on_disk = root.file_info(workdir.join(spec)).is_ok();
                let in_head = matches!(head_blob(&repo, spec), Ok(Some(_)));
                if on_disk || in_head {
                    entry.restored_paths.push(path.clone());
                } else {
                    entry.failures.push(PathFailure {
                        path: path.clone(),
                        kind: ROLLBACK_NOT_FOUND.to_string(),
                    });
                }
            }
        }

        for (step, step_spec, reported) in steps {
            match step {
                RollbackStep::Restore => {
                    match restore_one(&repo, root, &workdir, step_spec, &mut index) {
                        Ok(true) => {
                            index_dirty = true;
                            entry.restored_paths.push(reported);
                        }
                        // `HEAD` holds nothing there, so there is nothing to
                        // restore and the caller must not be told there was.
                        Ok(false) => entry.failures.push(PathFailure {
                            path: reported,
                            kind: ROLLBACK_NOT_FOUND.to_string(),
                        }),
                        Err(kind) => entry.failures.push(PathFailure {
                            path: reported,
                            kind: kind.to_string(),
                        }),
                    }
                }
                RollbackStep::Remove => match remove_one(root, &workdir, step_spec, &mut index) {
                    Ok(()) => {
                        index_dirty = true;
                        entry.removed_paths.push(reported);
                    }
                    Err(kind) => entry.failures.push(PathFailure {
                        path: reported,
                        kind: kind.to_string(),
                    }),
                },
            }
        }
        entries.push(entry.settle());
    }

    // The index flush is deliberately NOT allowed to discard `entries`.
    //
    // Every path above has already been written or removed on disk, and
    // `restored_paths` / `removed_paths` mean exactly "confirmed on disk"
    // (GTC-FR-25). Propagating a flush failure with `?` would throw that record
    // away and hand the caller a bare error — which it reads as "nothing
    // happened", leaving every affected artifact's dirty buffer in place to be
    // autosaved back over the content this call just restored. That would
    // reintroduce the very change the author asked to discard, which is worse
    // than the stale index a failed flush leaves behind.
    //
    // So the outcome stands and the flush failure is reported as its own thing.
    // The consequence of a failed flush is cosmetic by comparison: the working
    // tree is correct, and the paths read as staged until the next index write.
    let index_error = if index_dirty {
        index.write().err().map(|e| e.to_string())
    } else {
        None
    };
    Ok((RollbackOutcome { entries }, index_error))
}

/// Write `HEAD`'s content back at `spec` and make the index agree, discarding
/// the staged and unstaged change together (GTC-FR-23).
///
/// `Ok(false)` means `HEAD` holds no such path.
pub(crate) fn restore_one(
    repo: &Repository,
    root: &crate::fs::RootFs,
    workdir: &Path,
    spec: &str,
    index: &mut git2::Index,
) -> Result<bool, &'static str> {
    // The file mode `HEAD` records is deliberately not applied: every write here
    // goes through `FsAccess` (FSA-FR-19), which exposes no way to set one, and
    // reaching past it to `std::fs::set_permissions` is the raw filesystem call
    // this module is forbidden. A rolled-back file therefore takes the mode an
    // atomic write gives it; restoring the recorded mode needs an `FsAccess`
    // primitive that does not exist yet.
    let Some((content, _mode)) = head_blob(repo, spec).map_err(|_| ROLLBACK_WRITE_FAILED)? else {
        return Ok(false);
    };
    let absolute = workdir.join(spec);
    root.write_bytes_atomic(&absolute, &content)
        .map_err(|e| failure_kind(&e))?;
    // Staging `HEAD`'s own content is what discards a *staged* edit: the index
    // entry, the working tree, and `HEAD` all agree afterwards, so the path
    // reads clean rather than staged-and-reverted.
    index
        .add_path(Path::new(spec))
        .map_err(|_| ROLLBACK_WRITE_FAILED)?;
    Ok(true)
}

/// Take `spec` off disk and out of the index (GTC-FR-23, GTC-FR-24).
pub(crate) fn remove_one(
    root: &crate::fs::RootFs,
    workdir: &Path,
    spec: &str,
    index: &mut git2::Index,
) -> Result<(), &'static str> {
    let absolute = workdir.join(spec);
    // GTC-FR-24: the removal goes through the shared `FsAccess` instance — which
    // judges it against the allowlist like any other write — and names one file,
    // so a parent directory the removal empties is left exactly where it is.
    match root.access().delete_path(&absolute, false) {
        Ok(()) => {}
        // Already gone is the outcome the caller asked for.
        Err(e) if matches!(e, crate::fs::FsError::NotFound { .. }) => {}
        Err(e) => return Err(failure_kind(&e)),
    }
    // A staged addition is also unstaged, or the path would still be in the
    // index with `HEAD` holding nothing for it.
    let _ = index.remove_path(Path::new(spec));
    Ok(())
}
