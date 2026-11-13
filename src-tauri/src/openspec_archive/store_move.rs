//! The move of a change's records in the repository machine store
//! (`OAR-openspec-archive.md` OAR-FR-MSOM, OAR-FR-LUSO, OAR-FR-KSZI;
//! `RMS-repository-machine-storage.md`).
//!
//! The records of a change stand in `changes/<stream-id>/<change>/`: the
//! proposals, the action requests, the new-change proposals, the split
//! record, the conversation logs, and the statistics log. An archive moves the
//! whole folder to `changes/<stream-id>/archive/<archivedAs>/`, where no
//! operation reads it (OAR-FR-BMBR), so a later change of the same name starts
//! with no record.

use std::path::{Path, PathBuf};

use super::*;
use crate::repository_store::CHANGES_DIRNAME;

/// The folder name of the archive below a stream's change records.
pub const ARCHIVE_DIRNAME: &str = "archive";

/// OAR-FR-LUSO: the target of the move, relative to the store root:
/// `changes/<stream-id>/archive/<archivedAs>`, or the same with the lowest
/// free suffix `-2`, `-3`, and so on where that folder exists.
pub fn store_target(store: &RootFs, stream_id: &str, archived_as: &str) -> PathBuf {
    let base = Path::new(CHANGES_DIRNAME).join(stream_id).join(ARCHIVE_DIRNAME);
    let free = |name: &str| store.file_info(store.path().join(base.join(name))).is_err();
    if free(archived_as) {
        return base.join(archived_as);
    }
    let mut n = 2u32;
    loop {
        let name = format!("{archived_as}-{n}");
        if free(&name) {
            return base.join(name);
        }
        n += 1;
    }
}

/// The kind of a move that failed, as a fixed word for a log record. No path
/// reaches it.
fn reason_of(error: &crate::fs::FsError) -> &'static str {
    use crate::fs::FsError;
    match error {
        FsError::AlreadyExists { .. } => "already_exists",
        FsError::NotFound { .. } => "not_found",
        FsError::SymlinkRefused { .. } => "symlink_refused",
        FsError::Io(e) if e.kind() == std::io::ErrorKind::PermissionDenied => "permission_denied",
        _ => "move_failed",
    }
}

/// OAR-FR-MSOM, OAR-FR-LUSO, OAR-FR-KSZI: move the store folder of `change`
/// to its archive target. A folder that does not exist moves nothing. A move
/// that fails emits a `WARN` record with `store_move_failed` and leaves the
/// folder where it is. It answers the target, where the folder moved.
pub fn move_change_records<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    change: &str,
    archived_as: &str,
) -> Option<PathBuf> {
    use tauri::Manager;
    let started = Instant::now();
    let warn = |reason: &str| {
        logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "openspec archive could not move the change records",
            log_fields! {
                "stream_id" => stream_id.to_string(),
                "change" => change.to_string(),
                "archivedAs" => archived_as.to_string(),
                "code" => CODE_STORE_MOVE_FAILED,
                "reason" => reason.to_string(),
            },
        );
    };
    if !crate::comments::is_change_owner(stream_id, change) {
        warn("invalid_owner");
        return None;
    }
    let store = match app
        .try_state::<crate::project::ProjectState>()
        .ok_or_else(|| "no_project_open".to_string())
        .and_then(|project| project.require_store())
    {
        Ok(store) => store,
        Err(_) => {
            warn("store_unavailable");
            return None;
        }
    };
    let source = Path::new(CHANGES_DIRNAME).join(stream_id).join(change);
    if store.file_info(store.path().join(&source)).is_err() {
        logging::log_info(
            app,
            &BUFFER,
            &[Domain::Backend],
            "openspec archive found no change records to move",
            log_fields! {
                "stream_id" => stream_id.to_string(),
                "change" => change.to_string(),
                "archivedAs" => archived_as.to_string(),
            },
        );
        return None;
    }
    let target = store_target(&store, stream_id, archived_as);
    match store.move_under(store.path(), &source, &target) {
        Ok(()) => {
            logging::log_info(
                app,
                &BUFFER,
                &[Domain::Backend],
                "openspec archive moved the change records",
                log_fields! {
                    "stream_id" => stream_id.to_string(),
                    "change" => change.to_string(),
                    "archivedAs" => archived_as.to_string(),
                    "suffixed" => target.file_name().is_some_and(|n| n != archived_as),
                    "durationMs" => started.elapsed().as_millis() as u64,
                },
            );
            Some(target)
        }
        Err(error) => {
            warn(reason_of(&error));
            None
        }
    }
}
