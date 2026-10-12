//! Removing one linked worktree (`WTC-worktree-context.md` WTC-FR-RDVK,
//! WTC-FR-BHFE).
//!
//! Split out of `worktree.rs` so that file keeps the enumeration, the
//! re-rooting and the commands. This is the one place a worktree is removed:
//! `GTC-git.md` `delete_branch` composes it, and the stream removal of
//! `WKS-work-streams.md` shares its prune of the registration.

use super::*;
use crate::log_fields;
use crate::logging::{self, Domain};

/// WTC-FR-RDVK: the path names the repository's primary worktree.
pub const ERR_WORKTREE_IS_PRIMARY: &str = "worktree_is_primary";
/// WTC-FR-RDVK: the path names the worktree that roots the project.
pub const ERR_WORKTREE_IS_ACTIVE: &str = "worktree_is_active";
/// WTC-FR-RDVK: the path names a work stream's working copy.
pub const ERR_WORKTREE_BELONGS_TO_WORK_STREAM: &str = "worktree_belongs_to_work_stream";

/// GTC-FR-IVPE: the lock on a linked worktree registration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeLock {
    /// The reason Git records, trimmed. Absent when Git records none or a
    /// blank one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The registrations that name the worktree at `path`, by the name the caller
/// knows or by the path the registration records.
fn registrations_at(
    main: &Repository,
    path: &Path,
    registration: Option<&str>,
) -> Vec<git2::Worktree> {
    let names: Vec<String> = main
        .worktrees()
        .map(|list| list.iter().filter_map(|n| n.ok().flatten().map(str::to_string)).collect())
        .unwrap_or_default();
    let wanted = canonicalize_lenient(path);
    names
        .into_iter()
        .filter_map(|candidate| {
            let worktree = main.find_worktree(&candidate).ok()?;
            let matches_name = registration.is_some_and(|r| r == candidate);
            let matches_path = canonicalize_lenient(worktree.path()) == wanted;
            (matches_name || matches_path).then_some(worktree)
        })
        .collect()
}

/// The largest `locked` file read. A lock reason is one line.
const MAX_LOCK_FILE_BYTES: u64 = 4096;

/// GTC-FR-IVPE: the lock on the registration of the worktree at `path`, or
/// `None` when it is not locked. Also read when the directory is gone.
///
/// The `locked` file is read here and not through `git2::Worktree::is_locked`,
/// which panics on a reason that is not UTF-8. Another tool writes that file,
/// so its bytes are decoded lossily and the read is bounded. The read goes
/// through an instance rooted at Git's administrative directory alone, on the
/// terms of `normalize_worktree_commondir`. A read that fails for another
/// reason than a missing file is logged and reads as not locked: the lock is
/// information for the author, and the deletion does not depend on it.
pub(crate) fn worktree_lock_at<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    repo: &Repository,
    path: &Path,
) -> Option<WorktreeLock> {
    registrations_at(repo, path, None).iter().find_map(|worktree| {
        let admin = linked_admin_dir(repo, worktree.name().ok().flatten()?);
        let read = crate::fs::FsAccess::builder()
            .allow_root(&admin)
            .build()
            .map_err(|e| e.to_string())
            .and_then(|fs| {
                match fs.read_regular_bytes(admin.join("locked"), MAX_LOCK_FILE_BYTES) {
                    Err(crate::fs::FsError::NotFound { .. }) => Ok(None),
                    other => other.map(Some).map_err(|e| e.to_string()),
                }
            });
        let bytes = match read {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return None,
            Err(reason) => {
                // GTC-FR-JUZP: the failure only, never the content of the file.
                logging::log_warn(
                    app,
                    &logging::BUFFER,
                    &[Domain::Backend],
                    "worktree lock read failed",
                    log_fields! { "path" => path.to_string_lossy(), "reason" => reason },
                );
                return None;
            }
        };
        let reason = String::from_utf8_lossy(&bytes).trim().to_string();
        Some(WorktreeLock { reason: (!reason.is_empty()).then_some(reason) })
    })
}

/// WTC-FR-RDVK, WTC-FR-KELA: prune the registration that names the worktree at
/// `path`, also when it is locked.
///
/// Pruning is what stops the repository listing a worktree whose directory has
/// gone. A registration matches by its name, when the caller knows it, or by the
/// path it records. Shared with the removal of a work stream's working copy
/// (WKS-FR-EIBC), so both leave the repository in the same state. A lock, such
/// as the one an agent tool puts on the worktree it works in, does not stop a
/// removal the author confirmed: the lock goes with the registration.
pub(crate) fn prune_worktree_registration(
    main: &Repository,
    path: &Path,
    registration: Option<&str>,
) -> Result<(), String> {
    for worktree in registrations_at(main, path, registration) {
        let mut opts = git2::WorktreePruneOptions::new();
        opts.valid(true).locked(true).working_tree(true);
        worktree.prune(Some(&mut opts)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// WTC-FR-RDVK: remove the directory of one linked worktree and prune its
/// registration, through the filesystem access layer.
///
/// Refuses, before anything changes, the primary worktree
/// (`worktree_is_primary`), the active worktree (`worktree_is_active`), and a
/// work stream's working copy (`worktree_belongs_to_work_stream`). The
/// enumeration is read here, at the moment of the removal (WTC-FR-BHFE). A
/// worktree whose directory is gone has its registration pruned.
///
/// The directory is the author's own, not one the application owns, so it is
/// removed through an instance whose author root is its parent
/// (`delete_author_tree`, FSA-FR-OWVT). A symbolic link in the tree, such as an
/// installed dependency store, is unlinked and never followed, so its target is
/// untouched.
///
/// Not a Tauri command (WTC contract surface): `delete_branch` composes it.
pub fn remove_linked_worktree<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &Path,
    path: &Path,
) -> Result<(), String> {
    let result = remove_checked(app, root, path);
    match &result {
        Ok(()) => logging::log_info(
            app,
            &logging::BUFFER,
            &[Domain::Backend],
            "linked worktree removed",
            log_fields! { "path" => path.to_string_lossy() },
        ),
        Err(error) => logging::log_warn(
            app,
            &logging::BUFFER,
            &[Domain::Backend],
            "linked worktree removal refused or failed",
            log_fields! { "path" => path.to_string_lossy(), "reason" => error.clone() },
        ),
    }
    result
}

fn remove_checked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &Path,
    path: &Path,
) -> Result<(), String> {
    let repo = open_repo(root)?;
    let primary = primary_worktree_root(&repo).ok_or_else(|| ERR_NOT_A_REPO.to_string())?;
    let active = containing_worktree(&repo).ok_or_else(|| ERR_NOT_A_REPO.to_string())?;
    let target = canonicalize_lenient(path);

    // WTC-FR-BHFE: read now, never from an earlier listing.
    let entry = worktree_entries(&repo)?
        .into_iter()
        .find(|e| Path::new(&e.path) == target.as_path())
        .ok_or_else(|| ERR_NOT_A_WORKTREE.to_string())?;
    if entry.is_primary || target == primary {
        return Err(ERR_WORKTREE_IS_PRIMARY.to_string());
    }
    if target == active {
        return Err(ERR_WORKTREE_IS_ACTIVE.to_string());
    }
    if crate::streams::stream_at_path(app, &target).is_some() {
        return Err(ERR_WORKTREE_BELONGS_TO_WORK_STREAM.to_string());
    }

    if !entry.is_missing {
        remove_directory(&target)?;
    }
    let main = Repository::open(&primary).map_err(|_| ERR_NOT_A_REPO.to_string())?;
    prune_worktree_registration(&main, &target, None)
}

fn remove_directory(target: &Path) -> Result<(), String> {
    let parent = target.parent().ok_or_else(|| ERR_NOT_A_WORKTREE.to_string())?;
    let access = crate::fs::FsAccess::builder()
        .author_root(parent)
        .build()
        .map_err(|e| format!("failed to remove the worktree: {e}"))?;
    access.delete_author_tree(target).map_err(|e| format!("failed to remove the worktree: {e}"))
}
